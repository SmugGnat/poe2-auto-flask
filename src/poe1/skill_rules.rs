//! Read-only PoE1 skill-rule prototype. No rule here sends input.
use crate::game_state::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::skills::SkillReadiness;

/// The first preset being validated against current PoE1 game state.
pub const MOLTEN_SHELL_BUFF: &str = "molten_shell_shield";
pub const DEFAULT_LIFE_THRESHOLD_PERCENT: u8 = 95;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoltenShellDecision {
    InvalidVitals,
    LifeAboveThreshold,
    AlreadyActive,
    SkillUnavailable,
    SkillNotUsable,
    Eligible,
}

pub fn evaluate_molten_shell(
    vitals: LiveVitals,
    effects: &EffectsSnapshot,
    readiness: Option<SkillReadiness>,
    threshold: u8,
) -> MoltenShellDecision {
    let max = vitals.health.usable_max();
    let current = vitals.health.current;
    if current <= 0 || max <= 0 || current > max || threshold > 100 {
        return MoltenShellDecision::InvalidVitals;
    }
    if i64::from(current) * 100 > i64::from(max) * i64::from(threshold) {
        return MoltenShellDecision::LifeAboveThreshold;
    }
    // The buff name alone is insufficient to infer the skill or its cooldown.
    // A malformed/unknown remaining duration must never authorize an action.
    if effects.effects.iter().any(|effect| {
        effect.name == MOLTEN_SHELL_BUFF
            && (!effect.time_left.is_finite() || effect.time_left >= 0.0)
    }) {
        return MoltenShellDecision::AlreadyActive;
    }
    match readiness {
        Some(SkillReadiness::Usable) => MoltenShellDecision::Eligible,
        Some(SkillReadiness::NotUsable) => MoltenShellDecision::SkillNotUsable,
        Some(SkillReadiness::NotFound) | None => MoltenShellDecision::SkillUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::{LiveVitals, VitalPool};
    use crate::poe1::buffs::Effect;

    fn vitals(current: i32) -> LiveVitals {
        LiveVitals {
            health: VitalPool {
                current,
                total: 100,
                reserved_flat: 0,
                reserved_fraction: 0,
            },
            mana: None,
            energy_shield: None,
        }
    }

    fn empty() -> EffectsSnapshot {
        EffectsSnapshot {
            slots_active: [false; 5],
            effects: Vec::new(),
        }
    }

    #[test]
    fn requires_hp_threshold_and_verified_skill_readiness() {
        assert_eq!(
            evaluate_molten_shell(vitals(96), &empty(), Some(SkillReadiness::Usable), 95),
            MoltenShellDecision::LifeAboveThreshold
        );
        assert_eq!(
            evaluate_molten_shell(vitals(95), &empty(), Some(SkillReadiness::Usable), 95),
            MoltenShellDecision::Eligible
        );
        assert_eq!(
            evaluate_molten_shell(vitals(50), &empty(), None, 95),
            MoltenShellDecision::SkillUnavailable
        );
        assert_eq!(
            evaluate_molten_shell(vitals(50), &empty(), Some(SkillReadiness::NotUsable), 95),
            MoltenShellDecision::SkillNotUsable
        );
    }

    #[test]
    fn active_buff_and_invalid_hp_never_become_eligible() {
        let mut state = empty();
        state.effects.push(Effect {
            name: MOLTEN_SHELL_BUFF.into(),
            flask_slot: None,
            buff_type: 0,
            time_left: 2.0,
        });
        assert_eq!(
            evaluate_molten_shell(vitals(20), &state, Some(SkillReadiness::Usable), 95),
            MoltenShellDecision::AlreadyActive
        );
        assert_eq!(
            evaluate_molten_shell(vitals(0), &empty(), Some(SkillReadiness::Usable), 95),
            MoltenShellDecision::InvalidVitals
        );
        assert_eq!(
            evaluate_molten_shell(vitals(80), &empty(), Some(SkillReadiness::Usable), 101),
            MoltenShellDecision::InvalidVitals
        );
    }
}
