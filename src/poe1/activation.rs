//! A short input-settling guard, not a gameplay flask cooldown. The game
//! decides whether an active effect can be applied. We only bridge the gap
//! between SendInput and the next visible charge/effect state update.
use std::time::{Duration, Instant};

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, SLOT_COUNT};

pub const SETTLING_GUARD: Duration = Duration::from_millis(350);

#[derive(Debug, Default)]
pub struct ActivationGuard {
    last_sent: [Option<Instant>; SLOT_COUNT],
}

impl ActivationGuard {
    pub fn reset(&mut self) {
        self.last_sent.fill(None);
    }

    pub fn mark(&mut self, slot: usize, now: Instant) {
        if let Some(at) = self.last_sent.get_mut(slot) {
            *at = Some(now);
        }
    }

    /// Pretend a just-activated flask's effect is temporarily active until the
    /// game has had time to report its real state. Only blocks the same type;
    /// using Quicksilver never delays an unrelated Life or Mana flask.
    pub fn apply_to(&self, now: Instant, flasks: &FlaskSnapshot, effects: &mut EffectsSnapshot) {
        for (index, at) in self.last_sent.iter().enumerate() {
            if at.is_some_and(|when| now.saturating_duration_since(when) < SETTLING_GUARD)
                && flasks.slots[index].is_some()
            {
                effects.slots_active[index] = true;
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
        guard.mark(1, now);
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
        guard.mark(1, now);
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
    fn resets_are_deterministic() {
        let (_, _, flasks, mut effects) = setup();
        let mut guard = ActivationGuard::default();
        let now = Instant::now();
        guard.mark(1, now);
        guard.reset();
        guard.apply_to(now, &flasks, &mut effects);
        assert!(!effects.slots_active.iter().any(|active| *active));
    }
}
