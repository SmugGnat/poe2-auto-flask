//! Skill-preset definitions and pure condition evaluation for PoE1.
//! No game-memory calls, input dispatch or scheduling belong here.
use std::io;
use std::time::{Duration, Instant};

use crate::game_state::LiveVitals;
use crate::input::KeyBinding;

use super::buffs::EffectsSnapshot;
use super::config::Poe1SkillsConfig;
use super::nearby::NearbySnapshot;
use super::skills::{SkillId, SkillReadiness};

pub const MOLTEN_SHELL_BUFF: &str = "molten_shell_shield";
pub const BLOOD_RAGE_BUFF: &str = "blood_rage";
const MIN_ATTEMPT_GAP: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    LifeAtOrBelow(u8),
    LifeAtOrAbove(u8),
    BuffAbsent(&'static str),
    NearbyMonsters { radius: u16, minimum_count: u16 },
    SkillUsable,
}

#[derive(Debug, Clone)]
pub struct SkillRule {
    pub id: SkillId,
    pub key: KeyBinding,
    pub conditions: Vec<Condition>,
}

/// Presets define their own conditions. A future warcry need not have LifeAtOrBelow.
pub fn configured_rules(config: &Poe1SkillsConfig) -> io::Result<Vec<SkillRule>> {
    let mut rules = Vec::new();
    let molten = &config.molten_shell;
    if molten.enabled {
        rules.push(SkillRule {
            id: SkillId::MoltenShell,
            key: molten.binding()?,
            conditions: vec![
                Condition::LifeAtOrBelow(molten.threshold_percent),
                Condition::BuffAbsent(MOLTEN_SHELL_BUFF),
                Condition::SkillUsable,
            ],
        });
    }
    let blood = &config.blood_rage;
    if blood.enabled {
        let mut conditions = vec![
            Condition::LifeAtOrAbove(blood.min_life_percent),
            Condition::BuffAbsent(BLOOD_RAGE_BUFF),
        ];
        if blood.require_nearby_monsters {
            conditions.push(Condition::NearbyMonsters {
                radius: blood.nearby_radius,
                minimum_count: blood.nearby_minimum_count,
            });
        }
        conditions.push(Condition::SkillUsable);
        rules.push(SkillRule {
            id: SkillId::BloodRage,
            key: blood.binding()?,
            conditions,
        });
    }
    Ok(rules)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    InvalidVitals,
    LifeAboveThreshold,
    LifeBelowThreshold,
    AlreadyActive,
    NotEnoughNearbyMonsters,
    SkillUnavailable,
    SkillNotUsable,
    Eligible,
}

pub fn preconditions_met(rule: &SkillRule, vitals: LiveVitals) -> bool {
    rule.conditions.iter().all(|condition| match condition {
        Condition::LifeAtOrBelow(threshold) => life_at_or_below(vitals, *threshold),
        Condition::LifeAtOrAbove(threshold) => life_at_or_above(vitals, *threshold),
        Condition::BuffAbsent(_) | Condition::NearbyMonsters { .. } | Condition::SkillUsable => {
            true
        }
    })
}

/// Only observations already in this tick's snapshot are checked here.
/// Expensive Actor skill reads are not requested if these conditions fail.
pub fn observations_allow(rule: &SkillRule, vitals: LiveVitals, effects: &EffectsSnapshot) -> bool {
    let max = vitals.health.usable_max();
    if vitals.health.current <= 0 || max <= 0 || vitals.health.current > max {
        return false;
    }
    rule.conditions.iter().all(|condition| match *condition {
        Condition::LifeAtOrBelow(threshold) => life_at_or_below(vitals, threshold),
        Condition::LifeAtOrAbove(threshold) => life_at_or_above(vitals, threshold),
        Condition::NearbyMonsters { .. } => true,
        Condition::BuffAbsent(name) => !effects.effects.iter().any(|effect| {
            effect.name == name && (!effect.time_left.is_finite() || effect.time_left >= 0.0)
        }),
        Condition::SkillUsable => true,
    })
}

/// Evaluate proximity conditions from the existing cached monster census.
/// Unknown/unavailable nearby state never authorizes a gated skill.
pub fn nearby_conditions_met(rule: &SkillRule, nearby: Option<&NearbySnapshot>) -> bool {
    rule.conditions.iter().all(|condition| match *condition {
        Condition::NearbyMonsters {
            radius,
            minimum_count,
        } => {
            (1..=150).contains(&radius)
                && (1..=512).contains(&minimum_count)
                && nearby.is_some_and(|snapshot| {
                    snapshot
                        .counts_at(f32::from(radius))
                        .into_iter()
                        .sum::<usize>()
                        >= usize::from(minimum_count)
                })
        }
        _ => true,
    })
}

pub fn requires_nearby(rule: &SkillRule) -> bool {
    rule.conditions
        .iter()
        .any(|condition| matches!(condition, Condition::NearbyMonsters { .. }))
}

pub fn evaluate(
    rule: &SkillRule,
    vitals: LiveVitals,
    effects: &EffectsSnapshot,
    nearby: Option<&NearbySnapshot>,
    readiness: Option<SkillReadiness>,
) -> RuleDecision {
    let max = vitals.health.usable_max();
    let current = vitals.health.current;
    if current <= 0 || max <= 0 || current > max {
        return RuleDecision::InvalidVitals;
    }
    for condition in &rule.conditions {
        match *condition {
            Condition::LifeAtOrBelow(threshold) => {
                if !(1..=99).contains(&threshold) {
                    return RuleDecision::InvalidVitals;
                }
                if !life_at_or_below(vitals, threshold) {
                    return RuleDecision::LifeAboveThreshold;
                }
            }
            Condition::LifeAtOrAbove(threshold) => {
                if !(1..=100).contains(&threshold) {
                    return RuleDecision::InvalidVitals;
                }
                if !life_at_or_above(vitals, threshold) {
                    return RuleDecision::LifeBelowThreshold;
                }
            }
            Condition::NearbyMonsters { .. } => {
                if !nearby_conditions_met(rule, nearby) {
                    return RuleDecision::NotEnoughNearbyMonsters;
                }
            }
            Condition::BuffAbsent(name) => {
                if effects.effects.iter().any(|effect| {
                    effect.name == name
                        && (!effect.time_left.is_finite() || effect.time_left >= 0.0)
                }) {
                    return RuleDecision::AlreadyActive;
                }
            }
            Condition::SkillUsable => match readiness {
                Some(SkillReadiness::Usable) => {}
                Some(SkillReadiness::NotUsable) => return RuleDecision::SkillNotUsable,
                Some(SkillReadiness::Ambiguous | SkillReadiness::NotFound) | None => {
                    return RuleDecision::SkillUnavailable;
                }
            },
        }
    }
    RuleDecision::Eligible
}

fn life_at_or_below(vitals: LiveVitals, threshold: u8) -> bool {
    let max = vitals.health.usable_max();
    let current = vitals.health.current;
    (1..=99).contains(&threshold)
        && current > 0
        && max > 0
        && current <= max
        && i64::from(current) * 100 <= i64::from(max) * i64::from(threshold)
}

fn life_at_or_above(vitals: LiveVitals, threshold: u8) -> bool {
    let max = vitals.health.usable_max();
    let current = vitals.health.current;
    (1..=100).contains(&threshold)
        && current > 0
        && max > 0
        && current <= max
        && i64::from(current) * 100 >= i64::from(max) * i64::from(threshold)
}

#[derive(Debug, Default)]
pub struct SkillAttemptGuard {
    last_sent: Option<Instant>,
    retry_after: Option<Instant>,
}
impl SkillAttemptGuard {
    pub fn ready(&self, now: Instant) -> bool {
        self.last_sent
            .is_none_or(|sent| now.saturating_duration_since(sent) >= MIN_ATTEMPT_GAP)
            && self.retry_after.is_none_or(|at| now >= at)
    }
    pub fn mark(&mut self, sent_at: Instant) {
        self.last_sent = Some(sent_at);
        self.retry_after = None;
    }
    pub fn defer(&mut self, now: Instant) {
        self.retry_after = Some(now + MIN_ATTEMPT_GAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::VitalPool;
    use crate::poe1::buffs::Effect;
    use crate::poe1::config::Poe1SkillsConfig;

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
    fn rule() -> SkillRule {
        SkillRule {
            id: SkillId::MoltenShell,
            key: KeyBinding::new(81, crate::input::KeyModifier::None),
            conditions: vec![
                Condition::LifeAtOrBelow(95),
                Condition::BuffAbsent(MOLTEN_SHELL_BUFF),
                Condition::SkillUsable,
            ],
        }
    }
    #[test]
    fn presets_are_opt_in_and_use_distinct_rule_definitions() {
        assert!(configured_rules(&Poe1SkillsConfig::default())
            .unwrap()
            .is_empty());
        let mut cfg = Poe1SkillsConfig::default();
        cfg.molten_shell.enabled = true;
        let rules = configured_rules(&cfg).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, SkillId::MoltenShell);
        assert_eq!(rules[0].conditions.len(), 3);
    }
    #[test]
    fn evaluates_life_buff_and_readiness_independently() {
        let r = rule();
        assert_eq!(
            evaluate(&r, vitals(96), &empty(), None, Some(SkillReadiness::Usable)),
            RuleDecision::LifeAboveThreshold
        );
        assert_eq!(
            evaluate(&r, vitals(95), &empty(), None, Some(SkillReadiness::Usable)),
            RuleDecision::Eligible
        );
        assert_eq!(
            evaluate(&r, vitals(50), &empty(), None, None),
            RuleDecision::SkillUnavailable
        );
        assert_eq!(
            evaluate(
                &r,
                vitals(50),
                &empty(),
                None,
                Some(SkillReadiness::NotUsable)
            ),
            RuleDecision::SkillNotUsable
        );
        let mut state = empty();
        state.effects.push(Effect {
            name: MOLTEN_SHELL_BUFF.into(),
            flask_slot: None,
            buff_type: 0,
            time_left: 2.0,
        });
        assert_eq!(
            evaluate(&r, vitals(20), &state, None, Some(SkillReadiness::Usable)),
            RuleDecision::AlreadyActive
        );
        assert_eq!(
            evaluate(&r, vitals(0), &empty(), None, Some(SkillReadiness::Usable)),
            RuleDecision::InvalidVitals
        );
    }
    #[test]
    fn rules_without_life_threshold_are_supported() {
        let mut r = rule();
        r.conditions = vec![
            Condition::BuffAbsent(MOLTEN_SHELL_BUFF),
            Condition::SkillUsable,
        ];
        assert!(preconditions_met(&r, vitals(100)));
        assert_eq!(
            evaluate(
                &r,
                vitals(100),
                &empty(),
                None,
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::Eligible
        );
    }
    #[test]
    fn cheap_state_conditions_skip_unneeded_actor_skill_reads() {
        let r = rule();
        assert!(!preconditions_met(&r, vitals(96)));
        let mut effects = empty();
        effects.effects.push(Effect {
            name: MOLTEN_SHELL_BUFF.into(),
            flask_slot: None,
            buff_type: 0,
            time_left: 2.0,
        });
        assert!(!observations_allow(&r, vitals(50), &effects));
        assert!(observations_allow(&r, vitals(50), &empty()));
        assert!(!observations_allow(&r, vitals(100), &empty()));
    }

    #[test]
    fn blood_rage_nearby_and_life_conditions_are_independent() {
        use super::super::nearby::{NearbyMonster, Rarity};
        let mut settings = Poe1SkillsConfig::default();
        settings.blood_rage.enabled = true;
        let rule = configured_rules(&settings).unwrap().remove(0);
        assert_eq!(rule.id, SkillId::BloodRage);
        assert!(!preconditions_met(&rule, vitals(79)));
        assert!(preconditions_met(&rule, vitals(80)));
        let snapshot = NearbySnapshot {
            nearby: vec![NearbyMonster {
                rarity: Rarity::Normal,
                distance_squared: 20.0 * 20.0,
            }],
            ..NearbySnapshot::default()
        };
        assert!(!nearby_conditions_met(&rule, None));
        assert!(nearby_conditions_met(&rule, Some(&snapshot)));
        assert_eq!(
            evaluate(
                &rule,
                vitals(79),
                &empty(),
                Some(&snapshot),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::LifeBelowThreshold
        );
        assert_eq!(
            evaluate(
                &rule,
                vitals(80),
                &empty(),
                None,
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::NotEnoughNearbyMonsters
        );
        assert_eq!(
            evaluate(
                &rule,
                vitals(80),
                &empty(),
                Some(&snapshot),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::Eligible
        );
        let mut buffed = empty();
        buffed.effects.push(Effect {
            name: BLOOD_RAGE_BUFF.into(),
            flask_slot: None,
            buff_type: 0,
            time_left: 8.0,
        });
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &buffed,
                Some(&snapshot),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::AlreadyActive
        );
        settings.blood_rage.require_nearby_monsters = false;
        let without_nearby = configured_rules(&settings).unwrap().remove(0);
        assert!(!requires_nearby(&without_nearby));
        assert_eq!(
            evaluate(
                &without_nearby,
                vitals(80),
                &empty(),
                None,
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::Eligible
        );
    }

    #[test]
    fn two_skill_presets_produce_different_condition_sets() {
        let mut config = Poe1SkillsConfig::default();
        config.molten_shell.enabled = true;
        config.blood_rage.enabled = true;
        let rules = configured_rules(&config).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(!requires_nearby(&rules[0]));
        assert!(requires_nearby(&rules[1]));
    }

    #[test]
    fn ambiguous_actor_readiness_never_authorizes_any_skill_rule() {
        let mut settings = Poe1SkillsConfig::default();
        settings.blood_rage.enabled = true;
        let rule = configured_rules(&settings).unwrap().remove(0);
        // Disable the optional proximity predicate to isolate readiness.
        let mut rule = rule;
        rule.conditions
            .retain(|condition| !matches!(condition, Condition::NearbyMonsters { .. }));
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &empty(),
                None,
                Some(SkillReadiness::Ambiguous)
            ),
            RuleDecision::SkillUnavailable
        );
    }

    #[test]
    fn attempt_guard_is_independent_of_other_actions() {
        let mut guard = SkillAttemptGuard::default();
        let at = Instant::now();
        assert!(guard.ready(at));
        guard.mark(at);
        assert!(!guard.ready(at + Duration::from_millis(100)));
        assert!(guard.ready(at + Duration::from_secs(1)));
        guard.defer(at + Duration::from_secs(1));
        assert!(!guard.ready(at + Duration::from_millis(1500)));
        assert!(guard.ready(at + Duration::from_secs(2)));
    }
}
