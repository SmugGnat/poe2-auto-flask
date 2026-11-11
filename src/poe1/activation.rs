//! PoE1 input settling: 350ms per utility slot; recovery uses state feedback.
//! Life/Mana share a one-tick guard across all duplicate slots. Once the game
//! reports spent charges, a resource increase, or a fresh resource loss, the
//! ordinary vitals/effects decision is allowed to run again without a fixed
//! 900ms cooldown. No additional process memory reads are performed here.
use std::time::{Duration, Instant};

use crate::poe2::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, SLOT_COUNT};
use super::types::FlaskType;

pub const SETTLING_GUARD: Duration = Duration::from_millis(350);
pub const RECOVERY_SETTLING_GUARD: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
struct RecoveryAttempt {
    slot: usize,
    sent_at: Instant,
    charges_before: i32,
    resource_before: i32,
    last_resource: i32,
}

#[derive(Debug, Default)]
pub struct ActivationGuard {
    last_sent: [Option<(FlaskType, Instant)>; SLOT_COUNT],
    life: Option<RecoveryAttempt>,
    mana: Option<RecoveryAttempt>,
}

impl ActivationGuard {
    pub fn reset(&mut self) {
        self.last_sent.fill(None);
        self.life = None;
        self.mana = None;
    }
    /// Queried after apply_to() has consumed any acknowledged recovery.
    /// An unacknowledged input cannot use the active-Life emergency bypass.
    pub fn life_input_pending(&self) -> bool {
        self.life.is_some()
    }

    /// Record exactly the sampled, pre-send charges and most recently verified
    /// vitals. Only called after SendInput succeeded.
    pub fn mark(
        &mut self,
        slot: usize,
        kind: FlaskType,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
    ) {
        let Some(entry) = self.last_sent.get_mut(slot) else {
            return;
        };
        *entry = Some((kind, now));
        if matches!(kind, FlaskType::Life | FlaskType::Mana) {
            let charges_before = flasks.slots[slot]
                .as_ref()
                .filter(|flask| flask.flask_type == kind)
                .map(|flask| flask.current_charges);
            if let (Some(charges_before), Some(resource_before)) =
                (charges_before, resource_current(vitals, kind))
            {
                let attempt = RecoveryAttempt {
                    slot,
                    sent_at: now,
                    charges_before,
                    resource_before,
                    last_resource: resource_before,
                };
                match kind {
                    FlaskType::Life => self.life = Some(attempt),
                    FlaskType::Mana => self.mana = Some(attempt),
                    _ => (),
                }
            }
        }
    }

    /// Adds temporary virtual effects only to the current decision snapshot.
    /// Active in-game effects are never removed or overwritten.
    pub fn apply_to(
        &mut self,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
        effects: &mut EffectsSnapshot,
    ) {
        // Utility key settling stays per-slot and must never delay Life/Mana.
        for (slot, last) in self.last_sent.iter().enumerate() {
            if let Some((kind, sent_at)) = last {
                if !matches!(kind, FlaskType::Life | FlaskType::Mana)
                    && now.saturating_duration_since(*sent_at) < SETTLING_GUARD
                    && flasks.slots[slot]
                        .as_ref()
                        .is_some_and(|flask| flask.flask_type == *kind)
                {
                    effects.slots_active[slot] = true;
                }
            }
        }

        Self::apply_recovery(
            &mut self.life,
            FlaskType::Life,
            now,
            vitals,
            flasks,
            effects,
        );
        Self::apply_recovery(
            &mut self.mana,
            FlaskType::Mana,
            now,
            vitals,
            flasks,
            effects,
        );
    }

    fn apply_recovery(
        pending: &mut Option<RecoveryAttempt>,
        kind: FlaskType,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
        effects: &mut EffectsSnapshot,
    ) {
        let Some(attempt) = pending.as_mut() else {
            return;
        };
        let resource_now = resource_current(vitals, kind);
        let charges_now = flasks.slots[attempt.slot]
            .as_ref()
            .filter(|flask| flask.flask_type == kind)
            .map(|flask| flask.current_charges);

        // The one-tick guard bridges SendInput to the next usable sample.
        // A decrease in charges confirms consumption even if instant healing
        // was offset by damage. An increase in the resource confirms recovery.
        // Fresh damage / mana loss is a separate reason to try again.
        let response = charges_now.is_some_and(|charges| charges < attempt.charges_before)
            || resource_now.is_some_and(|value| value > attempt.resource_before);
        let fresh_loss = resource_now.is_some_and(|value| value < attempt.last_resource);
        if now.saturating_duration_since(attempt.sent_at) >= RECOVERY_SETTLING_GUARD
            && (response || fresh_loss)
        {
            *pending = None;
            return;
        }
        if let Some(value) = resource_now {
            attempt.last_resource = value;
        }

        // Any same-type slot blocks the entire type, including other equipped
        // instant flasks. It never blocks the opposite recovery resource.
        if let Some(slot) = flasks
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|flask| flask.flask_type == kind))
        {
            effects.slots_active[slot] = true;
        }
    }
}

fn resource_current(vitals: LiveVitals, kind: FlaskType) -> Option<i32> {
    match kind {
        FlaskType::Life => Some(vitals.health.current),
        FlaskType::Mana => vitals.mana.map(|mana| mana.current),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::poe1::decision::choose_action;
    use crate::poe1::flasks::FlaskSlot;
    use crate::poe1::recovery::RecoveryKind;
    use crate::poe2::VitalPool;

    fn slot(kind: FlaskType, charges: i32) -> Option<FlaskSlot> {
        Some(FlaskSlot {
            item_path: String::new(),
            flask_type: kind,
            current_charges: charges,
            raw_per_use: 15,
            effective_per_use: Some(15),
            recovery: RecoveryKind::Gradual,
        })
    }

    fn setup() -> (Config, LiveVitals, FlaskSnapshot, EffectsSnapshot) {
        let mut config = Config::default();
        config.poe1.utility.quicksilver = false;
        let pool = |current| VitalPool {
            current,
            total: 100,
            reserved_flat: 0,
            reserved_fraction: 0,
        };
        let vitals = LiveVitals {
            health: pool(25),
            mana: Some(pool(25)),
            energy_shield: None,
        };
        let mut flasks = FlaskSnapshot {
            slots: std::array::from_fn(|_| None),
        };
        flasks.slots[1] = slot(FlaskType::Quicksilver, 80);
        flasks.slots[3] = slot(FlaskType::Life, 80);
        let effects = EffectsSnapshot {
            slots_active: [false; SLOT_COUNT],
            effects: Vec::new(),
        };
        (config, vitals, flasks, effects)
    }

    fn choose(
        config: &Config,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
        guard: &mut ActivationGuard,
        now: Instant,
    ) -> Option<usize> {
        let mut effects = EffectsSnapshot {
            slots_active: [false; SLOT_COUNT],
            effects: Vec::new(),
        };
        guard.apply_to(now, vitals, flasks, &mut effects);
        choose_action(config, vitals, flasks, &effects, true)
    }

    #[test]
    fn unchanged_instant_life_does_not_repeat_even_after_old_900ms_window() {
        let (config, vitals, flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        assert_eq!(choose(&config, vitals, &flasks, &mut guard, sent), None);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(200)
            ),
            None
        );
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_secs(5)
            ),
            None
        );
    }

    #[test]
    fn confirmed_spent_charges_allows_next_needed_life_use_after_one_tick() {
        let (config, vitals, mut flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        flasks.slots[3] = slot(FlaskType::Life, 65);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(60)
            ),
            None
        );
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(100)
            ),
            Some(3)
        );
    }

    #[test]
    fn confirmed_healing_but_still_below_threshold_allows_next_use() {
        let (config, mut vitals, flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        vitals.health.current = 35;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(100)
            ),
            Some(3)
        );
    }

    #[test]
    fn fresh_damage_allows_another_life_use_without_fixed_delay() {
        let (config, mut vitals, flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(100)
            ),
            None
        );
        vitals.health.current = 10;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(200)
            ),
            Some(3)
        );
    }

    #[test]
    fn separate_large_hits_can_trigger_again_as_soon_as_the_next_tick() {
        let (config, mut vitals, flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        vitals.health.current = 90;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(100)
            ),
            None
        );
        vitals.health.current = 20;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(200)
            ),
            Some(3)
        );
    }

    #[test]
    fn mana_has_the_same_state_aware_rules_as_life() {
        let (mut config, mut vitals, mut flasks, _) = setup();
        config.health.enabled = false;
        flasks.slots[0] = slot(FlaskType::Mana, 60);
        vitals.mana.as_mut().unwrap().current = 15;
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(0, FlaskType::Mana, sent, vitals, &flasks);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(150)
            ),
            None
        );
        vitals.mana.as_mut().unwrap().current = 5;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(200)
            ),
            Some(0)
        );
        guard.mark(
            0,
            FlaskType::Mana,
            sent + Duration::from_millis(200),
            vitals,
            &flasks,
        );
        flasks.slots[0] = slot(FlaskType::Mana, 45);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(300)
            ),
            Some(0)
        );
    }

    #[test]
    fn one_instant_life_use_blocks_both_life_slots_but_not_mana() {
        let (mut config, mut vitals, mut flasks, _) = setup();
        flasks.slots[4] = slot(FlaskType::Life, 80);
        flasks.slots[0] = slot(FlaskType::Mana, 60);
        vitals.mana.as_mut().unwrap().current = 10;
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(50)
            ),
            Some(0)
        );
        config.mana.enabled = false;
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(200)
            ),
            None
        );
    }

    #[test]
    fn utility_guard_still_blocks_duplicate_quicksilvers_for_350ms() {
        let (mut config, mut vitals, mut flasks, _) = setup();
        config.health.enabled = false;
        config.mana.enabled = false;
        config.poe1.utility.quicksilver = true;
        vitals.health.current = 100;
        flasks.slots[4] = slot(FlaskType::Quicksilver, 60);
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(1, FlaskType::Quicksilver, sent, vitals, &flasks);
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(250)
            ),
            None
        );
        assert_eq!(
            choose(
                &config,
                vitals,
                &flasks,
                &mut guard,
                sent + Duration::from_millis(351)
            ),
            Some(1)
        );
    }

    #[test]
    fn successful_recovery_does_not_override_a_real_active_effect() {
        let (config, vitals, mut flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        flasks.slots[3] = slot(FlaskType::Life, 65);
        let mut effects = EffectsSnapshot {
            slots_active: [false, false, false, true, false],
            effects: Vec::new(),
        };
        guard.apply_to(
            sent + Duration::from_millis(120),
            vitals,
            &flasks,
            &mut effects,
        );
        assert!(effects.slots_active[3]);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn reset_clears_unacknowledged_resource_attempt() {
        let (config, vitals, flasks, _) = setup();
        let mut guard = ActivationGuard::default();
        let sent = Instant::now();
        guard.mark(3, FlaskType::Life, sent, vitals, &flasks);
        guard.reset();
        assert_eq!(choose(&config, vitals, &flasks, &mut guard, sent), Some(3));
    }
}
