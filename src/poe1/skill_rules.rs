//! Skill-preset definitions and pure condition evaluation for PoE1.
//! No game-memory calls, input dispatch or scheduling belong here.
use std::collections::BTreeMap;
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
/// Active effects published in PoEDB's Steelskin and Bladefall of Trarthus data.
pub const STEELSKIN_BUFF: &str = "quick_guard";
/// Current damage-reduction effect; the older immunity effect is immortal_call.
pub const IMMORTAL_CALL_BUFF: &str = "mortal_call";
pub const BLADEFALL_OF_TRARTHUS_BUFF: &str = "righteous_blades";
/// Focused buff definition in the game data, distinct from the has_focus stat.
pub const FOCUS_BUFF: &str = "focus";
/// Verified in the player's PoE1 Buffs snapshot during live Spirit Offering.
pub const SPIRIT_OFFERING_BUFF: &str = "offering_spirit";
const MIN_ATTEMPT_GAP: Duration = Duration::from_secs(1);
const CONVOCATION_RECAST_GAP: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    LifeOrShieldAtOrBelow { life: u8, shield: u8 },
    LifeAtOrAbove(u8),
    BuffAbsent(&'static str),
    NearbyMonsters { radius: u16, minimum_count: u16 },
    NearbyCorpses { radius: u16, minimum_count: u16 },
    SkillUsable,
}

#[derive(Debug, Clone)]
pub struct SkillRule {
    pub id: SkillId,
    pub key: KeyBinding,
    pub conditions: Vec<Condition>,
}

/// Presets define their own conditions; not every skill needs a resource gate.
pub fn configured_rules(config: &Poe1SkillsConfig) -> io::Result<Vec<SkillRule>> {
    let mut rules = Vec::new();
    let molten = &config.molten_shell;
    if molten.enabled {
        rules.push(SkillRule {
            id: SkillId::MoltenShell,
            key: molten.binding()?,
            conditions: guard_conditions(molten.threshold_percent, molten.shield_threshold_percent),
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
    let offering = &config.spirit_offering;
    if offering.enabled {
        rules.push(SkillRule {
            id: SkillId::SpiritOffering,
            key: offering.binding()?,
            conditions: vec![
                Condition::BuffAbsent(SPIRIT_OFFERING_BUFF),
                Condition::NearbyCorpses {
                    radius: offering.nearby_radius,
                    minimum_count: offering.nearby_minimum_count,
                },
                Condition::SkillUsable,
            ],
        });
    }
    let steel = &config.steelskin;
    if steel.enabled {
        rules.push(SkillRule {
            id: SkillId::Steelskin,
            key: steel.binding()?,
            conditions: guard_conditions(steel.threshold_percent, steel.shield_threshold_percent),
        });
    }
    let call = &config.immortal_call;
    if call.enabled {
        rules.push(SkillRule {
            id: SkillId::ImmortalCall,
            key: call.binding()?,
            conditions: guard_conditions(call.threshold_percent, call.shield_threshold_percent),
        });
    }
    let bladefall = &config.bladefall_of_trarthus;
    if bladefall.enabled {
        rules.push(SkillRule {
            id: SkillId::BladefallOfTrarthus,
            key: bladefall.binding()?,
            conditions: vec![
                Condition::BuffAbsent(BLADEFALL_OF_TRARTHUS_BUFF),
                Condition::SkillUsable,
            ],
        });
    }
    let convocation = &config.convocation;
    if convocation.enabled {
        rules.push(SkillRule {
            id: SkillId::Convocation,
            key: convocation.binding()?,
            conditions: vec![Condition::SkillUsable],
        });
    }
    let focus = &config.focus;
    if focus.enabled {
        rules.push(SkillRule {
            id: SkillId::Focus,
            key: focus.binding()?,
            conditions: vec![
                Condition::LifeOrShieldAtOrBelow {
                    life: focus.threshold_percent,
                    shield: focus.shield_threshold_percent,
                },
                Condition::BuffAbsent(FOCUS_BUFF),
                Condition::SkillUsable,
            ],
        });
    }
    Ok(rules)
}

fn guard_conditions(life: u8, shield: u8) -> Vec<Condition> {
    vec![
        Condition::LifeOrShieldAtOrBelow { life, shield },
        Condition::BuffAbsent(MOLTEN_SHELL_BUFF),
        Condition::BuffAbsent(STEELSKIN_BUFF),
        Condition::BuffAbsent(IMMORTAL_CALL_BUFF),
        Condition::SkillUsable,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    InvalidVitals,
    GuardThresholdsNotMet,
    LifeBelowThreshold,
    AlreadyActive,
    NotEnoughNearbyMonsters,
    NotEnoughNearbyCorpses,
    SkillUnavailable,
    SkillNotUsable,
    Eligible,
}

pub fn preconditions_met(rule: &SkillRule, vitals: LiveVitals) -> bool {
    rule.conditions.iter().all(|condition| match condition {
        Condition::LifeOrShieldAtOrBelow { life, shield } => {
            guard_trigger(vitals, *life, *shield).is_some()
        }
        Condition::LifeAtOrAbove(threshold) => life_at_or_above(vitals, *threshold),
        Condition::BuffAbsent(_)
        | Condition::NearbyMonsters { .. }
        | Condition::NearbyCorpses { .. }
        | Condition::SkillUsable => true,
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
        Condition::LifeOrShieldAtOrBelow { life, shield } => {
            guard_trigger(vitals, life, shield).is_some()
        }
        Condition::LifeAtOrAbove(threshold) => life_at_or_above(vitals, threshold),
        Condition::NearbyMonsters { .. } | Condition::NearbyCorpses { .. } => true,
        Condition::BuffAbsent(name) => !effects
            .effects
            .iter()
            .any(|effect| buff_present(effect, name)),
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
        Condition::NearbyCorpses {
            radius,
            minimum_count,
        } => {
            (1..=150).contains(&radius)
                && (1..=512).contains(&minimum_count)
                && nearby.is_some_and(|snapshot| {
                    snapshot.corpses_at(f32::from(radius)) >= usize::from(minimum_count)
                })
        }
        _ => true,
    })
}

pub fn requires_nearby(rule: &SkillRule) -> bool {
    rule.conditions.iter().any(|condition| {
        matches!(
            condition,
            Condition::NearbyMonsters { .. } | Condition::NearbyCorpses { .. }
        )
    })
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
            Condition::LifeAtOrAbove(threshold) => {
                if !(1..=100).contains(&threshold) {
                    return RuleDecision::InvalidVitals;
                }
                if !life_at_or_above(vitals, threshold) {
                    return RuleDecision::LifeBelowThreshold;
                }
            }
            Condition::LifeOrShieldAtOrBelow { life, shield } => {
                if life > 99 || shield > 99 || (life == 0 && shield == 0) {
                    return RuleDecision::InvalidVitals;
                }
                if guard_trigger(vitals, life, shield).is_none() {
                    return RuleDecision::GuardThresholdsNotMet;
                }
            }
            Condition::NearbyMonsters { .. } => {
                if !nearby_conditions_met(rule, nearby) {
                    return RuleDecision::NotEnoughNearbyMonsters;
                }
            }
            Condition::NearbyCorpses { .. } => {
                if !nearby_conditions_met(rule, nearby) {
                    return RuleDecision::NotEnoughNearbyCorpses;
                }
            }
            Condition::BuffAbsent(name) => {
                if effects
                    .effects
                    .iter()
                    .any(|effect| buff_present(effect, name))
                {
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

/// These effects block input while their buff entry is present.
/// Other timed buff rules retain their original duration semantics.
fn buff_present(effect: &super::buffs::Effect, name: &str) -> bool {
    effect.name == name
        && (matches!(
            name,
            SPIRIT_OFFERING_BUFF | BLADEFALL_OF_TRARTHUS_BUFF | FOCUS_BUFF
        ) || !effect.time_left.is_finite()
            || effect.time_left >= 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuardTrigger {
    Life,
    Shield,
}

fn guard_trigger(vitals: LiveVitals, life: u8, shield: u8) -> Option<GuardTrigger> {
    let health = vitals.health;
    if life > 99
        || shield > 99
        || !health.looks_valid()
        || health.current <= 0
        || health.current > health.usable_max()
    {
        return None;
    }
    if life_at_or_below(vitals, life) {
        return Some(GuardTrigger::Life);
    }
    let pool = vitals.energy_shield?;
    let max = pool.usable_max();
    (shield > 0
        && pool.looks_valid()
        && max > 0
        && pool.current >= 0
        && pool.current <= max
        && i64::from(pool.current) * 100 <= i64::from(max) * i64::from(shield))
    .then_some(GuardTrigger::Shield)
}

/// Prefer a Life-triggered Guard over a Shield-triggered Guard using fresh
/// preflight vitals. Only Guard positions change; unrelated skills keep order.
pub fn prioritize_guard_skills(rules: &mut [SkillRule], vitals: LiveVitals) {
    let triggered_by_life = |rule: &SkillRule| {
        rule.conditions.iter().any(|condition| match *condition {
            Condition::LifeOrShieldAtOrBelow { life, shield } => {
                guard_trigger(vitals, life, shield) == Some(GuardTrigger::Life)
            }
            _ => false,
        })
    };
    for index in 0..rules.len() {
        if !rules[index].id.is_guard() || !triggered_by_life(&rules[index]) {
            continue;
        }
        // Move this Life candidate past preceding Shield candidates, using
        // only Guard positions. Preserve the order within each trigger group.
        let mut current = index;
        while let Some(previous) = (0..current).rfind(|&i| rules[i].id.is_guard()) {
            if triggered_by_life(&rules[previous]) {
                break;
            }
            rules.swap(previous, current);
            current = previous;
        }
    }
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
    next_cast_at: Option<Instant>,
    retry_after: Option<Instant>,
}

/// Guard skills share a cooldown. Recheck this after each successful send so
/// two candidates from one preflight cannot send both guard keys in one tick.
pub fn skill_attempt_ready(
    attempts: &BTreeMap<SkillId, SkillAttemptGuard>,
    skill: SkillId,
    now: Instant,
) -> bool {
    if skill.is_guard() {
        attempts
            .iter()
            .filter(|(id, _)| id.is_guard())
            .all(|(_, guard)| guard.ready(now))
    } else {
        attempts.get(&skill).is_none_or(|guard| guard.ready(now))
    }
}

impl SkillAttemptGuard {
    pub fn ready(&self, now: Instant) -> bool {
        self.next_cast_at.is_none_or(|at| now >= at) && self.retry_after.is_none_or(|at| now >= at)
    }
    pub fn mark(&mut self, skill: SkillId, sent_at: Instant) {
        let gap = if skill == SkillId::Convocation {
            CONVOCATION_RECAST_GAP
        } else {
            MIN_ATTEMPT_GAP
        };
        self.next_cast_at = Some(sent_at + gap);
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
    fn shield_vitals(life: i32, shield: i32, max_shield: i32) -> LiveVitals {
        LiveVitals {
            energy_shield: Some(VitalPool {
                current: shield,
                total: max_shield,
                reserved_flat: 0,
                reserved_fraction: 0,
            }),
            ..vitals(life)
        }
    }

    fn guard_rules(life: u8, shield: u8) -> Vec<SkillRule> {
        let mut config = Poe1SkillsConfig::default();
        config.molten_shell.enabled = true;
        config.molten_shell.threshold_percent = life;
        config.molten_shell.shield_threshold_percent = shield;
        config.steelskin.enabled = true;
        config.steelskin.threshold_percent = life;
        config.steelskin.shield_threshold_percent = shield;
        config.immortal_call.enabled = true;
        config.immortal_call.threshold_percent = life;
        config.immortal_call.shield_threshold_percent = shield;
        configured_rules(&config).unwrap()
    }

    #[test]
    fn guards_trigger_on_either_resource_with_life_first_at_all_evaluation_stages() {
        let rules = guard_rules(80, 95);
        for (life, shield, trigger) in [
            (100, 100, None),
            (81, 96, None),
            (100, 95, Some(GuardTrigger::Shield)),
            (100, 0, Some(GuardTrigger::Shield)),
            (80, 100, Some(GuardTrigger::Life)),
            (80, 95, Some(GuardTrigger::Life)),
            (1, 0, Some(GuardTrigger::Life)),
        ] {
            let state = shield_vitals(life, shield, 100);
            assert_eq!(guard_trigger(state, 80, 95), trigger);
            for rule in &rules {
                assert_eq!(preconditions_met(rule, state), trigger.is_some());
                assert_eq!(observations_allow(rule, state, &empty()), trigger.is_some());
                assert_eq!(
                    evaluate(rule, state, &empty(), None, Some(SkillReadiness::Usable)),
                    if trigger.is_some() {
                        RuleDecision::Eligible
                    } else {
                        RuleDecision::GuardThresholdsNotMet
                    }
                );
            }
        }
    }

    #[test]
    fn absent_invalid_or_disabled_shield_never_creates_a_guard_trigger() {
        for state in [
            vitals(100),
            shield_vitals(100, 0, 0),
            shield_vitals(100, -1, 100),
            shield_vitals(100, 101, 100),
            shield_vitals(100, 0, -100),
            shield_vitals(100, 0, crate::game_state::MAX_VITAL_TOTAL + 1),
            LiveVitals {
                energy_shield: Some(VitalPool {
                    reserved_flat: 100,
                    ..shield_vitals(100, 0, 100).energy_shield.unwrap()
                }),
                ..vitals(100)
            },
        ] {
            assert_eq!(guard_trigger(state, 80, 95), None);
            assert_eq!(
                guard_trigger(
                    LiveVitals {
                        health: vitals(80).health,
                        ..state
                    },
                    80,
                    95
                ),
                Some(GuardTrigger::Life)
            );
        }
        let depleted = shield_vitals(100, 0, 100);
        assert_eq!(guard_trigger(depleted, 80, 0), None);
        assert_eq!(guard_trigger(depleted, 80, 1), Some(GuardTrigger::Shield));
        // A tiny pool is valid, but the user can turn its condition off.
        assert_eq!(
            guard_trigger(shield_vitals(100, 0, 1), 80, 95),
            Some(GuardTrigger::Shield)
        );
        assert_eq!(guard_trigger(shield_vitals(100, 0, 1), 80, 0), None);
        for state in [
            shield_vitals(0, 0, 100),
            shield_vitals(-1, 0, 100),
            shield_vitals(101, 0, 100),
        ] {
            for rule in guard_rules(80, 95) {
                assert!(!preconditions_met(&rule, state));
                assert!(!observations_allow(&rule, state, &empty()));
                assert_eq!(
                    evaluate(&rule, state, &empty(), None, Some(SkillReadiness::Usable)),
                    RuleDecision::InvalidVitals
                );
            }
        }
    }

    #[test]
    fn shield_then_life_keeps_guard_effect_readiness_and_shared_attempt_gates() {
        let rules = guard_rules(80, 95);
        let now = Instant::now();
        let mut attempts = BTreeMap::new();
        let mut sent = Vec::new();
        // Both Guard presets qualify, but only the first can send.
        for rule in &rules {
            if skill_attempt_ready(&attempts, rule.id, now)
                && evaluate(
                    rule,
                    shield_vitals(100, 90, 100),
                    &empty(),
                    None,
                    Some(SkillReadiness::Usable),
                ) == RuleDecision::Eligible
            {
                sent.push(rule.id);
                let mut attempt = SkillAttemptGuard::default();
                attempt.mark(rule.id, now);
                attempts.insert(rule.id, attempt);
            }
        }
        assert_eq!(sent.len(), 1);
        let life_fell = shield_vitals(70, 90, 100);
        for rule in &rules {
            assert_eq!(
                evaluate(
                    rule,
                    life_fell,
                    &empty(),
                    None,
                    Some(SkillReadiness::Usable)
                ),
                RuleDecision::Eligible
            );
            assert!(!skill_attempt_ready(
                &attempts,
                rule.id,
                now + Duration::from_millis(100)
            ));
            assert!(skill_attempt_ready(
                &attempts,
                rule.id,
                now + Duration::from_secs(1)
            ));
            for state in [life_fell, shield_vitals(100, 90, 100)] {
                for name in [MOLTEN_SHELL_BUFF, STEELSKIN_BUFF, IMMORTAL_CALL_BUFF] {
                    let mut effects = empty();
                    effects.effects.push(Effect {
                        name: name.into(),
                        flask_slot: None,
                        buff_type: 2,
                        time_left: 1.0,
                    });
                    assert!(!observations_allow(rule, state, &effects));
                    assert_eq!(
                        evaluate(rule, state, &effects, None, Some(SkillReadiness::Usable)),
                        RuleDecision::AlreadyActive
                    );
                }
                for readiness in [
                    None,
                    Some(SkillReadiness::NotUsable),
                    Some(SkillReadiness::NotFound),
                    Some(SkillReadiness::Ambiguous),
                ] {
                    assert_ne!(
                        evaluate(rule, state, &empty(), None, readiness),
                        RuleDecision::Eligible
                    );
                }
            }
        }
        // The same tick where both thresholds match still has one rule per Guard.
        assert_eq!(
            guard_trigger(shield_vitals(80, 80, 100), 90, 90),
            Some(GuardTrigger::Life)
        );
    }

    #[test]
    fn life_guard_priority_preserves_other_skills_and_stable_ties() {
        let mut config = Poe1SkillsConfig::default();
        config.molten_shell.enabled = true;
        config.molten_shell.threshold_percent = 50;
        config.molten_shell.shield_threshold_percent = 95;
        config.steelskin.enabled = true;
        config.steelskin.threshold_percent = 90;
        config.steelskin.shield_threshold_percent = 95;
        config.immortal_call.enabled = true;
        config.immortal_call.threshold_percent = 90;
        config.immortal_call.shield_threshold_percent = 95;
        config.blood_rage.enabled = true;
        config.spirit_offering.enabled = true;
        config.convocation.enabled = true;
        let original = configured_rules(&config).unwrap();
        let ids = |rules: &[SkillRule]| rules.iter().map(|rule| rule.id).collect::<Vec<_>>();
        for state in [shield_vitals(50, 80, 100), shield_vitals(100, 80, 100)] {
            let mut rules = original.clone();
            prioritize_guard_skills(&mut rules, state);
            assert_eq!(ids(&rules), ids(&original));
        }
        let mut rules = original.clone();
        prioritize_guard_skills(&mut rules, shield_vitals(80, 80, 100));
        assert_eq!(
            ids(&rules),
            [
                SkillId::Steelskin,
                SkillId::BloodRage,
                SkillId::SpiritOffering,
                SkillId::ImmortalCall,
                SkillId::MoltenShell,
                SkillId::Convocation
            ]
        );
        // A later Life candidate keeps both preceding Shield candidates in order.
        config.steelskin.threshold_percent = 50;
        let mut rules = configured_rules(&config).unwrap();
        prioritize_guard_skills(&mut rules, shield_vitals(80, 80, 100));
        assert_eq!(
            ids(&rules),
            [
                SkillId::ImmortalCall,
                SkillId::BloodRage,
                SkillId::SpiritOffering,
                SkillId::MoltenShell,
                SkillId::Steelskin,
                SkillId::Convocation
            ]
        );
    }

    fn rule() -> SkillRule {
        SkillRule {
            id: SkillId::MoltenShell,
            key: KeyBinding::new(81, crate::input::KeyModifier::None),
            conditions: vec![
                Condition::LifeOrShieldAtOrBelow {
                    life: 95,
                    shield: 0,
                },
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
        assert_eq!(rules[0].conditions.len(), 5);
    }
    #[test]
    fn evaluates_life_buff_and_readiness_independently() {
        let r = rule();
        assert_eq!(
            evaluate(&r, vitals(96), &empty(), None, Some(SkillReadiness::Usable)),
            RuleDecision::GuardThresholdsNotMet
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
    fn spirit_offering_needs_corpses_not_live_monsters() {
        use super::super::nearby::{NearbyCorpse, NearbyMonster, Rarity};

        let mut config = Poe1SkillsConfig::default();
        config.spirit_offering.enabled = true;
        let rule = configured_rules(&config).unwrap().remove(0);
        assert_eq!(rule.id, SkillId::SpiritOffering);
        assert!(requires_nearby(&rule));
        assert!(preconditions_met(&rule, vitals(100)));
        assert!(!nearby_conditions_met(&rule, None));
        let mut snapshot = NearbySnapshot {
            nearby: vec![NearbyMonster {
                rarity: Rarity::Normal,
                distance_squared: 10.0 * 10.0,
            }],
            ..NearbySnapshot::default()
        };
        assert!(!nearby_conditions_met(&rule, Some(&snapshot)));
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &empty(),
                Some(&snapshot),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::NotEnoughNearbyCorpses
        );
        snapshot.corpses.push(NearbyCorpse {
            distance_squared: 25.0 * 25.0,
        });
        assert!(nearby_conditions_met(&rule, Some(&snapshot)));
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &empty(),
                Some(&snapshot),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::Eligible
        );
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &empty(),
                Some(&snapshot),
                Some(SkillReadiness::NotUsable)
            ),
            RuleDecision::SkillNotUsable
        );
        config.spirit_offering.nearby_minimum_count = 2;
        let twice = configured_rules(&config).unwrap().remove(0);
        assert!(!nearby_conditions_met(&twice, Some(&snapshot)));
        snapshot.corpses.push(NearbyCorpse {
            distance_squared: 80.0 * 80.0,
        });
        assert!(!nearby_conditions_met(&twice, Some(&snapshot)));
    }

    #[test]
    fn spirit_offering_buff_blocks_recasts_until_it_disappears() {
        use super::super::nearby::NearbyCorpse;

        let mut config = Poe1SkillsConfig::default();
        config.spirit_offering.enabled = true;
        let rule = configured_rules(&config).unwrap().remove(0);
        assert!(rule
            .conditions
            .iter()
            .any(|condition| { matches!(condition, Condition::BuffAbsent(SPIRIT_OFFERING_BUFF)) }));
        let nearby = NearbySnapshot {
            corpses: vec![NearbyCorpse {
                distance_squared: 20.0 * 20.0,
            }],
            ..NearbySnapshot::default()
        };
        let mut effects = empty();
        let ready = Some(SkillReadiness::Usable);
        assert!(observations_allow(&rule, vitals(100), &effects));
        assert_eq!(
            evaluate(&rule, vitals(100), &effects, Some(&nearby), ready),
            RuleDecision::Eligible
        );
        effects.effects.push(Effect {
            name: SPIRIT_OFFERING_BUFF.into(),
            flask_slot: None,
            buff_type: 2,
            time_left: 6.0,
        });
        assert!(!observations_allow(&rule, vitals(100), &effects));
        assert_eq!(
            evaluate(&rule, vitals(100), &effects, Some(&nearby), ready),
            RuleDecision::AlreadyActive
        );
        // Offering buffs can have non-timed status data.
        effects.effects[0].time_left = -1.0;
        assert!(!observations_allow(&rule, vitals(100), &effects));
        assert_eq!(
            evaluate(&rule, vitals(100), &effects, Some(&nearby), ready),
            RuleDecision::AlreadyActive
        );
        // Expiration removes the buff from the player's snapshot.
        effects.effects.clear();
        assert!(observations_allow(&rule, vitals(100), &effects));
        assert_eq!(
            evaluate(&rule, vitals(100), &effects, Some(&nearby), ready),
            RuleDecision::Eligible
        );
    }

    #[test]
    fn interrupted_spirit_offering_can_retry_after_one_second_without_buff() {
        use super::super::nearby::NearbyCorpse;
        let mut config = Poe1SkillsConfig::default();
        config.spirit_offering.enabled = true;
        let rule = configured_rules(&config).unwrap().remove(0);
        let nearby = NearbySnapshot {
            corpses: vec![NearbyCorpse {
                distance_squared: 12.0 * 12.0,
            }],
            ..NearbySnapshot::default()
        };
        let mut guard = SkillAttemptGuard::default();
        let start = Instant::now();
        guard.mark(rule.id, start); // Key dispatched, but the game interrupted the cast.
        assert!(!guard.ready(start + Duration::from_millis(999)));
        assert!(guard.ready(start + Duration::from_secs(1)));
        assert_eq!(
            evaluate(
                &rule,
                vitals(100),
                &empty(),
                Some(&nearby),
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::Eligible
        );
        guard.mark(rule.id, start + Duration::from_secs(1));
        assert!(!guard.ready(start + Duration::from_millis(1500)));
        assert!(guard.ready(start + Duration::from_secs(2)));
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
        guard.mark(SkillId::BloodRage, at);
        assert!(!guard.ready(at + Duration::from_millis(100)));
        assert!(guard.ready(at + Duration::from_secs(1)));
        guard.defer(at + Duration::from_secs(1));
        assert!(!guard.ready(at + Duration::from_millis(1500)));
        assert!(guard.ready(at + Duration::from_secs(2)));
    }

    #[test]
    fn steelskin_uses_life_readiness_and_shared_guard_effect_checks() {
        let mut config = Poe1SkillsConfig::default();
        config.molten_shell.enabled = true;
        config.steelskin.enabled = true;
        let rules = configured_rules(&config).unwrap();
        let steel = rules
            .iter()
            .find(|rule| rule.id == SkillId::Steelskin)
            .unwrap();
        let molten = rules
            .iter()
            .find(|rule| rule.id == SkillId::MoltenShell)
            .unwrap();
        let ready = Some(SkillReadiness::Usable);
        assert!(!requires_nearby(steel));
        assert_eq!(
            evaluate(steel, vitals(96), &empty(), None, ready),
            RuleDecision::GuardThresholdsNotMet
        );
        assert_eq!(
            evaluate(steel, vitals(95), &empty(), None, ready),
            RuleDecision::Eligible
        );
        for status in [
            SkillReadiness::NotFound,
            SkillReadiness::Ambiguous,
            SkillReadiness::NotUsable,
        ] {
            assert_ne!(
                evaluate(steel, vitals(50), &empty(), None, Some(status)),
                RuleDecision::Eligible
            );
        }
        for name in [STEELSKIN_BUFF, MOLTEN_SHELL_BUFF, IMMORTAL_CALL_BUFF] {
            let mut effects = empty();
            effects.effects.push(Effect {
                name: name.into(),
                flask_slot: None,
                buff_type: 2,
                time_left: 1.0,
            });
            for rule in [steel, molten] {
                assert!(!observations_allow(rule, vitals(50), &effects));
                assert_eq!(
                    evaluate(rule, vitals(50), &effects, None, ready),
                    RuleDecision::AlreadyActive
                );
            }
        }
    }

    #[test]
    fn bladefall_uses_game_readiness_without_assuming_a_resource_type() {
        let mut config = Poe1SkillsConfig::default();
        config.bladefall_of_trarthus.enabled = true;
        let rule = configured_rules(&config).unwrap().remove(0);
        assert_eq!(rule.id, SkillId::BladefallOfTrarthus);
        assert!(!requires_nearby(&rule));
        let mut state = vitals(1);
        for mana in [
            None,
            Some(crate::game_state::VitalPool {
                current: 0,
                total: 100,
                reserved_flat: 0,
                reserved_fraction: 0,
            }),
        ] {
            state.mana = mana;
            assert!(preconditions_met(&rule, state));
            assert_eq!(
                evaluate(&rule, state, &empty(), None, Some(SkillReadiness::Usable)),
                RuleDecision::Eligible
            );
            assert_eq!(
                evaluate(
                    &rule,
                    state,
                    &empty(),
                    None,
                    Some(SkillReadiness::NotUsable)
                ),
                RuleDecision::SkillNotUsable
            );
        }
        for status in [
            None,
            Some(SkillReadiness::NotFound),
            Some(SkillReadiness::Ambiguous),
        ] {
            assert_eq!(
                evaluate(&rule, state, &empty(), None, status),
                RuleDecision::SkillUnavailable
            );
        }
        for time_left in [-1.0, 0.0, 5.0, f32::NAN] {
            let mut effects = empty();
            effects.effects.push(Effect {
                name: BLADEFALL_OF_TRARTHUS_BUFF.into(),
                flask_slot: None,
                buff_type: 2,
                time_left,
            });
            assert!(!observations_allow(&rule, state, &effects));
            assert_eq!(
                evaluate(&rule, state, &effects, None, Some(SkillReadiness::Usable)),
                RuleDecision::AlreadyActive
            );
            effects.effects.clear();
            assert_eq!(
                evaluate(&rule, state, &effects, None, Some(SkillReadiness::Usable)),
                RuleDecision::Eligible
            );
        }
    }

    #[test]
    fn a_guard_attempt_blocks_its_shared_cooldown_partner_only() {
        let at = Instant::now();
        for sent in [
            SkillId::MoltenShell,
            SkillId::Steelskin,
            SkillId::ImmortalCall,
        ] {
            let mut attempts = BTreeMap::new();
            let mut guard = SkillAttemptGuard::default();
            guard.mark(sent, at);
            attempts.insert(sent, guard);
            for skill in [
                SkillId::MoltenShell,
                SkillId::Steelskin,
                SkillId::ImmortalCall,
            ] {
                assert!(!skill_attempt_ready(&attempts, skill, at));
                assert!(!skill_attempt_ready(
                    &attempts,
                    skill,
                    at + Duration::from_millis(999)
                ));
                assert!(skill_attempt_ready(
                    &attempts,
                    skill,
                    at + Duration::from_secs(1)
                ));
            }
            for skill in [
                SkillId::BloodRage,
                SkillId::SpiritOffering,
                SkillId::BladefallOfTrarthus,
                SkillId::Convocation,
            ] {
                assert!(skill_attempt_ready(&attempts, skill, at));
            }
            attempts.clear();
            assert!(skill_attempt_ready(&attempts, sent, at));
        }
    }

    #[test]
    fn focus_uses_either_threshold_and_requires_fresh_readiness_and_no_buff() {
        let mut config = Poe1SkillsConfig::default();
        config.focus.enabled = true;
        config.focus.threshold_percent = 80;
        config.focus.shield_threshold_percent = 90;
        let rule = configured_rules(&config).unwrap().pop().unwrap();
        assert_eq!(rule.id, SkillId::Focus);
        assert!(!rule.id.is_guard());
        let ready = Some(SkillReadiness::Usable);
        for state in [
            vitals(80),
            shield_vitals(100, 90, 100),
            shield_vitals(80, 90, 100),
        ] {
            assert!(preconditions_met(&rule, state));
            assert_eq!(
                evaluate(&rule, state, &empty(), None, ready),
                RuleDecision::Eligible
            );
        }
        let state = shield_vitals(100, 90, 100);
        let mut active = empty();
        active.effects.push(Effect {
            name: FOCUS_BUFF.into(),
            flask_slot: None,
            buff_type: 2,
            time_left: 3.0,
        });
        assert!(!observations_allow(&rule, state, &active));
        assert_eq!(
            evaluate(&rule, state, &active, None, ready),
            RuleDecision::AlreadyActive
        );
        assert_eq!(
            evaluate(
                &rule,
                state,
                &empty(),
                None,
                Some(SkillReadiness::NotUsable)
            ),
            RuleDecision::SkillNotUsable
        );
        assert_eq!(
            evaluate(&rule, state, &empty(), None, Some(SkillReadiness::NotFound)),
            RuleDecision::SkillUnavailable
        );

        config.focus.threshold_percent = 0;
        let shield_only = configured_rules(&config).unwrap().pop().unwrap();
        assert!(preconditions_met(&shield_only, state));
        assert!(!preconditions_met(&shield_only, vitals(5)));
        assert_eq!(
            evaluate(&shield_only, state, &empty(), None, ready),
            RuleDecision::Eligible
        );
        assert_eq!(
            evaluate(
                &shield_only,
                shield_vitals(5, 91, 100),
                &empty(),
                None,
                ready
            ),
            RuleDecision::GuardThresholdsNotMet
        );
    }

    #[test]
    fn convocation_requires_three_seconds_and_game_readiness() {
        let mut config = Poe1SkillsConfig::default();
        assert!(!config.convocation.enabled);
        config.convocation.enabled = true;
        let rule = configured_rules(&config).unwrap().remove(0);
        assert_eq!(rule.id, SkillId::Convocation);
        assert!(!requires_nearby(&rule));
        let state = vitals(1);
        let mut effects = empty();
        effects.effects.push(Effect {
            name: "existing_regeneration".into(),
            flask_slot: None,
            buff_type: 2,
            time_left: 30.0,
        });
        assert!(preconditions_met(&rule, state));
        assert!(observations_allow(&rule, state, &effects));
        let at = Instant::now();
        let mut guard = SkillAttemptGuard::default();
        let mut attempts = BTreeMap::new();
        assert!(skill_attempt_ready(&attempts, rule.id, at));
        guard.mark(rule.id, at);
        attempts.insert(rule.id, guard);
        assert!(!skill_attempt_ready(
            &attempts,
            rule.id,
            at + Duration::from_millis(2999)
        ));
        assert!(skill_attempt_ready(
            &attempts,
            rule.id,
            at + Duration::from_secs(3)
        ));
        assert!(skill_attempt_ready(&attempts, SkillId::BloodRage, at));
        assert_eq!(
            evaluate(
                &rule,
                state,
                &effects,
                None,
                Some(SkillReadiness::NotUsable)
            ),
            RuleDecision::SkillNotUsable
        );
        assert_eq!(
            evaluate(&rule, state, &effects, None, Some(SkillReadiness::Usable)),
            RuleDecision::Eligible
        );
        for status in [
            None,
            Some(SkillReadiness::NotFound),
            Some(SkillReadiness::Ambiguous),
        ] {
            assert_eq!(
                evaluate(&rule, state, &effects, None, status),
                RuleDecision::SkillUnavailable
            );
        }
        assert_eq!(
            evaluate(
                &rule,
                vitals(0),
                &effects,
                None,
                Some(SkillReadiness::Usable)
            ),
            RuleDecision::InvalidVitals
        );
    }
}
