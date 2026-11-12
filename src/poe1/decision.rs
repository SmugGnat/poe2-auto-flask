//! Pure decision logic: no memory reads and no keyboard input.
use crate::config::Config;
use crate::game_state::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, SLOT_COUNT};
use super::nearby::NearbySnapshot;
use super::rotation::FlaskRotation;
use super::types::{FlaskType, UTILITY_TYPES};

#[cfg(test)]
pub fn choose_action(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
) -> Option<usize> {
    choose_action_with_nearby(config, vitals, flasks, effects, quicksilver_ready, None)
}

/// Any enabled proximity rule requires a valid sampled census. Other utility
/// types and emergency recovery never depend on nearby data.
#[cfg(test)]
pub fn choose_action_with_nearby(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
    nearby: Option<&NearbySnapshot>,
) -> Option<usize> {
    choose_action_internal(
        config,
        vitals,
        flasks,
        effects,
        quicksilver_ready,
        nearby,
        None,
        true,
    )
}

/// Test-compatible stateful variant; the production loop uses the guarded form.
#[cfg(test)]
pub fn choose_action_rotating_with_nearby(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
    nearby: Option<&NearbySnapshot>,
    rotation: &FlaskRotation,
) -> Option<usize> {
    choose_action_internal(
        config,
        vitals,
        flasks,
        effects,
        quicksilver_ready,
        nearby,
        Some(rotation),
        true,
    )
}

/// Automation version: an outstanding unacknowledged Life input may not
/// bypass even a genuine ongoing recovery buff.
#[allow(clippy::too_many_arguments)]
pub fn choose_action_rotating_guarded(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
    nearby: Option<&NearbySnapshot>,
    rotation: &FlaskRotation,
    life_emergency_ready: bool,
) -> Option<usize> {
    choose_action_internal(
        config,
        vitals,
        flasks,
        effects,
        quicksilver_ready,
        nearby,
        Some(rotation),
        life_emergency_ready,
    )
}

#[allow(clippy::too_many_arguments)]
fn choose_action_internal(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
    nearby: Option<&NearbySnapshot>,
    rotation: Option<&FlaskRotation>,
    life_emergency_ready: bool,
) -> Option<usize> {
    // Life and mana have priority over optional utility maintenance.
    let health = vitals.health;
    let ci_like = health.total == 1 && vitals.energy_shield.is_some_and(|es| es.total > 0);
    if !ci_like
        && config.health.enabled
        && health.current > 0
        && health
            .percentage()
            .is_some_and(|percent| percent <= config.health.threshold_percent)
    {
        if let Some(slot) = best_life_slot(
            flasks,
            effects,
            rotation,
            health,
            config.health.threshold_percent,
            life_emergency_ready,
        ) {
            return Some(slot);
        }
    }
    if config.mana.enabled
        && vitals.mana.is_some_and(|mana| {
            mana.usable_max() > 0
                && mana
                    .percentage()
                    .is_some_and(|pct| pct <= config.mana.threshold_percent)
        })
    {
        if let Some(slot) = best_slot(FlaskType::Mana, flasks, effects, rotation) {
            return Some(slot);
        }
    }
    for kind in UTILITY_TYPES {
        if kind == FlaskType::Quicksilver
            && config.poe1.quicksilver_min_run_ms != 0
            && !quicksilver_ready
        {
            continue;
        }
        if config.poe1.utility.enabled(kind) {
            if let Some(rule) = config.poe1.nearby.get(&kind).filter(|rule| rule.enabled) {
                let matches = nearby.is_some_and(|census| {
                    let counts = census.counts_at(f32::from(rule.radius));
                    counts[rule.minimum_rarity.first_index()..]
                        .iter()
                        .sum::<usize>()
                        >= usize::from(rule.minimum_count)
                });
                if !matches {
                    continue;
                }
            }
            if let Some(slot) = best_slot(kind, flasks, effects, rotation) {
                return Some(slot);
            }
        }
    }
    None
}

/// Return true only when a usable equipped utility needs a nearby scan.
/// Fast-path Life/Mana without scanning so emergency recovery is never delayed.
#[cfg(test)]
pub fn needs_nearby(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
) -> bool {
    needs_nearby_guarded(config, vitals, flasks, effects, quicksilver_ready, true)
}

pub fn needs_nearby_guarded(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    quicksilver_ready: bool,
    life_emergency_ready: bool,
) -> bool {
    if choose_action_internal(
        config,
        vitals,
        flasks,
        effects,
        quicksilver_ready,
        None,
        None,
        life_emergency_ready,
    )
    .is_some_and(|slot| {
        flasks.slots[slot]
            .as_ref()
            .is_some_and(|flask| matches!(flask.flask_type, FlaskType::Life | FlaskType::Mana))
    }) {
        return false;
    }
    UTILITY_TYPES.into_iter().any(|kind| {
        config.poe1.utility.enabled(kind)
            && config
                .poe1
                .nearby
                .get(&kind)
                .is_some_and(|rule| rule.enabled)
            && !(kind == FlaskType::Quicksilver
                && config.poe1.quicksilver_min_run_ms != 0
                && !quicksilver_ready)
            && best_slot(kind, flasks, effects, None).is_some()
    })
}

/// Rotate all eligible Life flasks normally, but when a genuine gradual
/// recovery is already active reserve fully instant recovery for emergencies.
/// Virtual settling flags cannot authorize emergency bypass.
fn best_life_slot(
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    rotation: Option<&FlaskRotation>,
    health: crate::game_state::VitalPool,
    threshold: f32,
    life_emergency_ready: bool,
) -> Option<usize> {
    let any_active = flasks.slots.iter().enumerate().any(|(index, flask)| {
        flask
            .as_ref()
            .is_some_and(|f| f.flask_type == FlaskType::Life)
            && effects.slots_active[index]
    });
    if !any_active {
        return best_eligible_slot(FlaskType::Life, flasks, rotation, |_, flask| {
            flask.recovery.eligible_for_normal_life(health)
        });
    }
    if !life_emergency_ready
        || health
            .percentage()
            .is_none_or(|pct| pct > (threshold / 2.0).min(25.0))
    {
        return None;
    }
    let real_life_active = effects.effects.iter().any(|buff| {
        buff.flask_slot
            .and_then(|slot| flasks.slots.get(slot))
            .and_then(Option::as_ref)
            .is_some_and(|flask| flask.flask_type == FlaskType::Life)
    });
    if !real_life_active {
        return None;
    }
    best_eligible_slot(FlaskType::Life, flasks, rotation, |slot, flask| {
        !effects.slots_active[slot] && flask.recovery.instant_at_life(health)
    })
}

/// When more than one same-type flask has enough charges, prefer the least
/// recently used physical slot. On first use, prefer the most whole charges
/// remaining; ties go to the lowest slot. Never override an active effect.
fn best_slot(
    kind: FlaskType,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    rotation: Option<&FlaskRotation>,
) -> Option<usize> {
    if flasks.slots.iter().enumerate().any(|(slot, state)| {
        state
            .as_ref()
            .is_some_and(|f| f.flask_type == kind && effects.slots_active[slot])
    }) {
        return None;
    }
    best_eligible_slot(kind, flasks, rotation, |_, _| true)
}

fn best_eligible_slot(
    kind: FlaskType,
    flasks: &FlaskSnapshot,
    rotation: Option<&FlaskRotation>,
    eligible: impl Fn(usize, &super::flasks::FlaskSlot) -> bool,
) -> Option<usize> {
    (0..SLOT_COUNT)
        .filter_map(|slot| {
            let state = flasks.slots[slot].as_ref()?;
            if state.flask_type != kind || !eligible(slot, state) {
                return None;
            }
            let uses = state.remaining_uses()?;
            if uses < 1 {
                return None;
            }
            let last_used = rotation.map_or(0, |r| r.last_sequence(slot, kind));
            Some((slot, last_used, uses, state.current_charges))
        })
        .max_by_key(|(slot, last_used, uses, charges)| {
            (
                std::cmp::Reverse(*last_used),
                *uses,
                *charges,
                std::cmp::Reverse(*slot),
            )
        })
        .map(|(slot, _, _, _)| slot)
}

pub fn enabled_demand(config: &Config, vitals: LiveVitals) -> bool {
    let health = vitals.health;
    let ci_like = health.total == 1 && vitals.energy_shield.is_some_and(|es| es.total > 0);
    (!ci_like
        && config.health.enabled
        && health.current > 0
        && health
            .percentage()
            .is_some_and(|pct| pct <= config.health.threshold_percent))
        || (config.mana.enabled
            && vitals.mana.is_some_and(|mana| {
                mana.usable_max() > 0
                    && mana
                        .percentage()
                        .is_some_and(|pct| pct <= config.mana.threshold_percent)
            }))
        || UTILITY_TYPES
            .into_iter()
            .any(|kind| config.poe1.utility.enabled(kind))
}

pub fn blocks_grace_period(effects: &EffectsSnapshot) -> bool {
    effects.effects.iter().any(|e| e.name == "grace_period")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::VitalPool;
    use crate::poe1::flasks::FlaskSlot;
    use crate::poe1::recovery::RecoveryKind;

    fn slot(kind: FlaskType, charges: i32, cost: Option<i32>) -> Option<FlaskSlot> {
        Some(FlaskSlot {
            item_path: "Metadata/Items/Flasks/Test".into(),
            flask_type: kind,
            current_charges: charges,
            raw_per_use: 15,
            effective_per_use: cost,
            recovery: RecoveryKind::Gradual,
        })
    }
    fn setup() -> (Config, LiveVitals, FlaskSnapshot, EffectsSnapshot) {
        let config = Config::default();
        let vital = |current, total| VitalPool {
            current,
            total,
            reserved_flat: 0,
            reserved_fraction: 0,
        };
        (
            config,
            LiveVitals {
                health: vital(40, 100),
                mana: Some(vital(80, 100)),
                energy_shield: None,
            },
            FlaskSnapshot {
                slots: std::array::from_fn(|_| None),
            },
            EffectsSnapshot {
                slots_active: [false; SLOT_COUNT],
                effects: Vec::new(),
            },
        )
    }

    #[test]
    fn duplicate_life_flasks_prefer_more_uses() {
        let (config, vitals, mut flasks, effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 60, Some(15));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
    }

    #[test]
    fn equal_uses_prefer_charges_then_first_slot() {
        let (config, vitals, mut flasks, effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 48, Some(15));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
        flasks.slots[0] = slot(FlaskType::Life, 48, Some(15));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(0)
        );
    }

    #[test]
    fn active_duplicate_blocks_entire_type() {
        let (config, vitals, mut flasks, mut effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 60, Some(15));
        effects.slots_active[0] = true;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn no_effective_cost_never_authorizes_input() {
        let (config, vitals, mut flasks, effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, None);
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn utility_matches_type_not_slot_and_remains_opt_in() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        vitals.health.current = 100;
        flasks.slots[3] = slot(FlaskType::Quicksilver, 50, Some(30));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        config.poe1.utility.quicksilver = true;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(3)
        );
    }

    #[test]
    fn movement_gate_only_restricts_quicksilver_not_other_flasks() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        vitals.health.current = 100;
        config.poe1.utility.quicksilver = true;
        config.poe1.quicksilver_min_run_ms = 750;
        flasks.slots[3] = slot(FlaskType::Quicksilver, 50, Some(30));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, false),
            None
        );
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(3)
        );
        // A different enabled utility still works when standing still.
        config.poe1.utility.jade = true;
        flasks.slots[2] = slot(FlaskType::Jade, 60, Some(30));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, false),
            Some(2)
        );
    }

    #[test]
    fn jade_and_silver_recognize_type_not_inventory_slot() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        vitals.health.current = 100;
        config.poe1.utility.jade = true;
        config.poe1.utility.silver = true;
        flasks.slots[0] = slot(FlaskType::Jade, 60, Some(30));
        flasks.slots[4] = slot(FlaskType::Silver, 60, Some(40));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(0)
        );
        flasks.slots[0] = None;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(4)
        );
    }

    #[test]
    fn rarity_thresholds_include_unique_bosses() {
        use crate::poe1::config::{MinimumRarity, NearbyRule};
        use crate::poe1::nearby::{NearbyMonster, Rarity};
        use std::time::Duration;

        let (mut config, mut vitals, mut flasks, effects) = setup();
        vitals.health.current = 100;
        config.poe1.utility.granite = true;
        flasks.slots[2] = slot(FlaskType::Granite, 45, Some(30));
        config.poe1.nearby.insert(
            FlaskType::Granite,
            NearbyRule {
                enabled: true,
                radius: 60,
                minimum_count: 1,
                minimum_rarity: MinimumRarity::Rare,
            },
        );
        let mut census = NearbySnapshot {
            awake_nodes: 1,
            candidate_monsters: 1,
            scan_wall: Duration::ZERO,
            nearby: vec![NearbyMonster {
                rarity: Rarity::Unique,
                distance_squared: 25.0 * 25.0,
            }],
        };
        assert!(needs_nearby(&config, vitals, &flasks, &effects, true));
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, None),
            None
        );
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, Some(&census)),
            Some(2)
        );
        census.nearby[0].rarity = Rarity::Normal;
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, Some(&census)),
            None
        );
        config
            .poe1
            .nearby
            .get_mut(&FlaskType::Granite)
            .unwrap()
            .minimum_rarity = MinimumRarity::Normal;
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, Some(&census)),
            Some(2)
        );
    }

    #[test]
    fn count_radius_and_failed_scans_block_only_proximity_gated_flasks() {
        use crate::poe1::config::{MinimumRarity, NearbyRule};
        use crate::poe1::nearby::{NearbyMonster, Rarity};
        use std::time::Duration;

        let (mut config, mut vitals, mut flasks, effects) = setup();
        vitals.health.current = 100;
        config.poe1.utility.granite = true;
        config.poe1.utility.jade = true;
        flasks.slots[1] = slot(FlaskType::Granite, 70, Some(30));
        flasks.slots[3] = slot(FlaskType::Jade, 60, Some(30));
        config.poe1.nearby.insert(
            FlaskType::Granite,
            NearbyRule {
                enabled: true,
                radius: 60,
                minimum_count: 2,
                minimum_rarity: MinimumRarity::Normal,
            },
        );
        let census = NearbySnapshot {
            awake_nodes: 1,
            candidate_monsters: 1,
            scan_wall: Duration::ZERO,
            nearby: vec![NearbyMonster {
                rarity: Rarity::Unique,
                distance_squared: 70.0 * 70.0,
            }],
        };
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, Some(&census)),
            Some(3)
        );
        config.poe1.utility.jade = false;
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, Some(&census)),
            None
        );
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, None),
            None
        );
        // Emergency flask remains eligible even if the census is unavailable.
        vitals.health.current = 20;
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        assert!(!needs_nearby(&config, vitals, &flasks, &effects, true));
        assert_eq!(
            choose_action_with_nearby(&config, vitals, &flasks, &effects, true, None),
            Some(0)
        );
    }

    #[test]
    fn duplicate_quicksilvers_round_robin_without_slot_settings() {
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        let mut rotation = FlaskRotation::default();
        vitals.health.current = 100;
        config.poe1.utility.quicksilver = true;
        flasks.slots[1] = slot(FlaskType::Quicksilver, 45, Some(30));
        flasks.slots[4] = slot(FlaskType::Quicksilver, 90, Some(30));
        // First use chooses the better-filled flask; then switches to the
        // other eligible copy, even if it has fewer remaining whole uses.
        let choose =
            |flasks: &FlaskSnapshot, effects: &EffectsSnapshot, rotation: &FlaskRotation| {
                choose_action_rotating_with_nearby(
                    &config, vitals, flasks, effects, true, None, rotation,
                )
            };
        assert_eq!(choose(&flasks, &effects, &rotation), Some(4));
        rotation.record(4, FlaskType::Quicksilver);
        effects.slots_active[4] = true;
        assert_eq!(choose(&flasks, &effects, &rotation), None);
        effects.slots_active[4] = false;
        assert_eq!(choose(&flasks, &effects, &rotation), Some(1));
        rotation.record(1, FlaskType::Quicksilver);
        assert_eq!(choose(&flasks, &effects, &rotation), Some(4));
        // No more charges in flask 4: never select it on the next rotation.
        flasks.slots[4] = slot(FlaskType::Quicksilver, 10, Some(30));
        assert_eq!(choose(&flasks, &effects, &rotation), Some(1));
    }

    #[test]
    fn duplicate_life_flasks_rotate_when_effect_is_no_longer_active() {
        let (config, vitals, mut flasks, mut effects) = setup();
        let mut rotation = FlaskRotation::default();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 60, Some(15));
        let choose = |effects: &EffectsSnapshot, rotation: &FlaskRotation| {
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, effects, true, None, rotation,
            )
        };
        assert_eq!(choose(&effects, &rotation), Some(1));
        rotation.record(1, FlaskType::Life);
        effects.slots_active[1] = true;
        assert_eq!(choose(&effects, &rotation), None);
        effects.slots_active[1] = false;
        assert_eq!(choose(&effects, &rotation), Some(0));
        rotation.record(0, FlaskType::Life);
        assert_eq!(choose(&effects, &rotation), Some(1));
    }

    #[test]
    fn one_usable_duplicate_or_changed_slot_still_works() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        let mut rotation = FlaskRotation::default();
        vitals.health.current = 100;
        config.poe1.utility.quicksilver = true;
        flasks.slots[2] = slot(FlaskType::Quicksilver, 75, Some(30));
        flasks.slots[4] = slot(FlaskType::Quicksilver, 25, Some(30));
        rotation.record(2, FlaskType::Quicksilver);
        assert_eq!(
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, &effects, true, None, &rotation,
            ),
            Some(2)
        );
        flasks.slots[2] = None;
        flasks.slots[0] = slot(FlaskType::Quicksilver, 75, Some(30));
        assert_eq!(
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, &effects, true, None, &rotation,
            ),
            Some(0)
        );
    }

    #[test]
    fn normal_mapping_rotates_regular_and_conditional_life_at_low_life() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        config.health.threshold_percent = 50.0;
        vitals.health.current = 45;
        let mut rotation = FlaskRotation::default();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::InstantOnLowLife;
        assert_eq!(
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, &effects, true, None, &rotation,
            ),
            Some(0)
        );
        rotation.record(0, FlaskType::Life);
        assert_eq!(
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, &effects, true, None, &rotation,
            ),
            Some(1)
        );
        rotation.record(1, FlaskType::Life);
        assert_eq!(
            choose_action_rotating_with_nearby(
                &config, vitals, &flasks, &effects, true, None, &rotation,
            ),
            Some(0)
        );
    }

    #[test]
    fn instant_on_low_life_does_not_fire_at_55_percent_even_if_config_is_60() {
        let (mut config, mut vitals, mut flasks, effects) = setup();
        config.health.threshold_percent = 60.0;
        vitals.health.current = 55;
        flasks.slots[1] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::InstantOnLowLife;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(0)
        );
        flasks.slots[0] = None;
        vitals.health.current = 50;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
    }

    #[test]
    fn emergency_bypasses_real_gradual_life_recovery_only_below_floor() {
        use crate::poe1::buffs::Effect;
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        config.health.threshold_percent = 50.0;
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::InstantOnLowLife;
        effects.slots_active[0] = true;
        effects.effects.push(Effect {
            name: "flask_life".into(),
            flask_slot: Some(0),
            buff_type: 4,
            time_left: 2.0,
        });
        vitals.health.current = 40;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        vitals.health.current = 26;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        vitals.health.current = 25;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
        assert_eq!(
            choose_action_rotating_guarded(
                &config,
                vitals,
                &flasks,
                &effects,
                true,
                None,
                &FlaskRotation::default(),
                false,
            ),
            None
        );
        effects.slots_active[1] = true;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn emergency_requires_true_instant_not_partial_or_unverified_flask() {
        use crate::poe1::buffs::Effect;
        let (config, mut vitals, mut flasks, mut effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 45, Some(15));
        effects.slots_active[0] = true;
        effects.effects.push(Effect {
            name: "flask_life".into(),
            flask_slot: Some(0),
            buff_type: 4,
            time_left: 2.0,
        });
        vitals.health.current = 20;
        for kind in [RecoveryKind::Gradual, RecoveryKind::PartialInstant] {
            flasks.slots[1].as_mut().unwrap().recovery = kind;
            assert_eq!(
                choose_action(&config, vitals, &flasks, &effects, true),
                None
            );
        }
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::Instant;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
        // Virtual effects from key settling never count as a real active
        // recovery effect and cannot authorize a bypass on their own.
        effects.effects.clear();
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn outstanding_life_input_does_not_suppress_nearby_utility_scanning() {
        use crate::poe1::config::NearbyRule;
        use crate::poe1::buffs::Effect;
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        config.poe1.utility.granite = true;
        config
            .poe1
            .nearby
            .insert(FlaskType::Granite, NearbyRule::default());
        config.health.threshold_percent = 50.0;
        vitals.health.current = 20;
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Life, 45, Some(15));
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::Instant;
        flasks.slots[2] = slot(FlaskType::Granite, 60, Some(30));
        effects.slots_active[0] = true;
        effects.effects.push(Effect {
            name: "flask_life".into(),
            flask_slot: Some(0),
            buff_type: 4,
            time_left: 2.0,
        });
        assert!(!needs_nearby_guarded(
            &config, vitals, &flasks, &effects, true, true,
        ));
        assert!(needs_nearby_guarded(
            &config, vitals, &flasks, &effects, true, false,
        ));
    }

    #[test]
    fn mana_does_not_gain_life_emergency_bypass() {
        use crate::poe1::buffs::Effect;
        let (mut config, mut vitals, mut flasks, mut effects) = setup();
        config.health.enabled = false;
        vitals.mana.as_mut().unwrap().current = 10;
        flasks.slots[0] = slot(FlaskType::Mana, 45, Some(15));
        flasks.slots[1] = slot(FlaskType::Mana, 45, Some(15));
        flasks.slots[1].as_mut().unwrap().recovery = RecoveryKind::Instant;
        effects.slots_active[0] = true;
        effects.effects.push(Effect {
            name: "flask_mana".into(),
            flask_slot: Some(0),
            buff_type: 4,
            time_left: 2.0,
        });
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }

    #[test]
    fn mana_threshold_uses_unreserved_pool() {
        let (config, mut vitals, mut flasks, effects) = setup();
        let mana = vitals.mana.as_mut().unwrap();
        mana.total = 1234;
        mana.reserved_flat = 1102;
        mana.current = 30;
        flasks.slots[1] = slot(FlaskType::Mana, 40, Some(15));
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            Some(1)
        );
    }

    #[test]
    fn rejects_dead_or_ci_like_health() {
        let (config, mut vitals, mut flasks, effects) = setup();
        flasks.slots[0] = slot(FlaskType::Life, 45, Some(15));
        vitals.health.current = 0;
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
        vitals.health.total = 1;
        vitals.health.current = 1;
        vitals.energy_shield = Some(VitalPool {
            total: 2000,
            current: 1000,
            reserved_flat: 0,
            reserved_fraction: 0,
        });
        assert_eq!(
            choose_action(&config, vitals, &flasks, &effects, true),
            None
        );
    }
}
