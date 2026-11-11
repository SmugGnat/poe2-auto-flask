//! Short input-settling protection, not a game-mechanic flask cooldown.
//! Instant Life/Mana recovery often has no sustained effect to detect, so
//! the safety window for recovery is longer and shared across duplicate slots.
use std::time::{Duration, Instant};

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, SLOT_COUNT};
use super::types::FlaskType;

pub const SETTLING_GUARD: Duration = Duration::from_millis(350);
/// Recheck actual vitals and charge/effect state after this window before
/// allowing another same-resource use. Never blocks unrelated utility flasks.
pub const RECOVERY_SETTLING_GUARD: Duration = Duration::from_millis(900);

#[derive(Debug, Default)]
pub struct ActivationGuard {
    last_sent: [Option<(FlaskType, Instant)>; SLOT_COUNT],
    last_life: Option<Instant>,
    last_mana: Option<Instant>,
}

impl ActivationGuard {
    pub fn reset(&mut self) {
        self.last_sent.fill(None);
        self.last_life = None;
        self.last_mana = None;
    }

    /// Mark only after the input function reports success.
    pub fn mark(&mut self, slot: usize, kind: FlaskType, now: Instant) {
        if let Some(entry) = self.last_sent.get_mut(slot) {
            *entry = Some((kind, now));
            match kind {
                FlaskType::Life => self.last_life = Some(now),
                FlaskType::Mana => self.last_mana = Some(now),
                _ => (),
            }
        }
    }

    /// Represent an unconfirmed key as a temporarily active effect. The engine
    /// already blocks other equipped flasks of that same type when one is active.
    /// Recovery is guarded across *all* same-type slots even if instant healing
    /// produced no persistent buff. Utility settling never postpones Life/Mana.
    pub fn apply_to(&self, now: Instant, flasks: &FlaskSnapshot, effects: &mut EffectsSnapshot) {
        for (index, last) in self.last_sent.iter().enumerate() {
            if let Some((kind, at)) = last {
                if now.saturating_duration_since(*at) < SETTLING_GUARD
                    && flasks.slots[index]
                        .as_ref()
                        .is_some_and(|flask| flask.flask_type == *kind)
                {
                    effects.slots_active[index] = true;
                }
            }
        }
        for (kind, last) in [
            (FlaskType::Life, self.last_life),
            (FlaskType::Mana, self.last_mana),
        ] {
            if last.is_some_and(|at| now.saturating_duration_since(at) < RECOVERY_SETTLING_GUARD) {
                // The first equipped matching slot is enough to make the
                // same-type decision gate block every duplicate of that type.
                if let Some(slot) = flasks
                    .slots
                    .iter()
                    .position(|f| f.as_ref().is_some_and(|flask| flask.flask_type == kind))
                {
                    effects.slots_active[slot] = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::poe1::decision::choose_action;
    use crate::poe1::flasks::FlaskSlot;
    use crate::poe1::types::FlaskType;
    use crate::poe2::{LiveVitals, VitalPool};

    fn slot(kind: FlaskType) -> Option<FlaskSlot> {
        Some(FlaskSlot {
            item_path: String::new(),
            flask_type: kind,
            current_charges: 80,
            raw_per_use: 15,
            effective_per_use: Some(15),
        })
    }
    fn setup() -> (Config, LiveVitals, FlaskSnapshot, EffectsSnapshot) {
        let mut config = Config::default();
        config.poe1.utility.quicksilver = true;
        let mut vitals = LiveVitals {
            health: VitalPool {
                current: 100,
                total: 100,
                reserved_flat: 0,
                reserved_fraction: 0,
            },
            mana: None,
            energy_shield: None,
        };
        let mut flasks = FlaskSnapshot {
            slots: std::array::from_fn(|_| None),
        };
        flasks.slots[1] = slot(FlaskType::Quicksilver);
        flasks.slots[3] = slot(FlaskType::Life);
        let effects = EffectsSnapshot {
            slots_active: [false; SLOT_COUNT],
            effects: Vec::new(),
        };
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
        vitals.health.current = 25;
        (config, vitals, flasks, effects)
    }

    #[test]
    fn utility_input_never_blocks_emergency_life_action() {
        let (config, vitals, flasks, mut effects) = setup();
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(1, FlaskType::Quicksilver, now);
        guard.apply_to(now + Duration::from_millis(100), &flasks, &mut effects);
        assert!(effects.slots_active[1]);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(3)
        );
    }

    #[test]
    fn settling_guard_blocks_duplicates_only_until_visibility_delay() {
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        config.health.enabled = false;
        vitals.health.current = 100;
        flasks.slots[4] = slot(FlaskType::Quicksilver);
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(1, FlaskType::Quicksilver, now);
        guard.apply_to(now + Duration::from_millis(200), &flasks, &mut effects);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        effects.slots_active.fill(false);
        guard.apply_to(now + Duration::from_millis(351), &flasks, &mut effects);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
    }

    #[test]
    fn instant_life_use_blocks_both_life_slots_until_vitals_can_settle() {
        let (config, vitals, mut flasks, mut effects) = setup();
        flasks.slots[4] = slot(FlaskType::Life);
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(3, FlaskType::Life, now);
        guard.apply_to(now + Duration::from_millis(200), &flasks, &mut effects);
        assert!(effects.slots_active[3]);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        effects.slots_active.fill(false);
        guard.apply_to(now + Duration::from_millis(700), &flasks, &mut effects);
        assert!(effects.slots_active[3]);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        // At 900ms, the decision loop re-reads actual vitals/charges/effects.
        effects.slots_active.fill(false);
        guard.apply_to(now + RECOVERY_SETTLING_GUARD, &flasks, &mut effects);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(3)
        );
    }

    #[test]
    fn life_settling_does_not_block_mana_or_an_enabled_utility() {
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(3, FlaskType::Life, now);
        flasks.slots[0] = slot(FlaskType::Mana);
        vitals.mana = Some(VitalPool {
            current: 10,
            total: 100,
            reserved_flat: 0,
            reserved_fraction: 0,
        });
        guard.apply_to(now + Duration::from_millis(100), &flasks, &mut effects);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(0)
        );
        config.mana.enabled = false;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
    }

    #[test]
    fn resets_are_deterministic() {
        let (_, _, flasks, mut effects) = setup();
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(1, FlaskType::Quicksilver, now);
        guard.reset();
        guard.apply_to(now, &flasks, &mut effects);
        assert!(!effects.slots_active.iter().any(|active| *active));
    }
}
