//! Pure decision logic: no memory reads and no keyboard input.
use crate::config::Config;
use crate::game_state::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::config::NearbyRule;
use super::flasks::{FlaskSnapshot, FLASK_SLOT_COUNT};
use super::nearby::NearbySnapshot;
use super::rotation::FlaskRotation;
use super::types::{FlaskType, UniqueFlask, UTILITY_TYPES};

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
    // The Writhing Jar is a hybrid base, but its worm-spawning use is independent
    // of missing Life or Mana. Generic hybrid recovery is intentionally excluded.
    let jar = UniqueFlask::WrithingJar;
    if config.poe1.unique.enabled(jar)
        && config
            .poe1
            .unique_conditions
            .get(&jar)
            .and_then(|conditions| conditions.nearby)
            .filter(|rule| rule.enabled)
            .is_none_or(|rule| nearby_matches(&rule, nearby))
    {
        if let Some(slot) = best_unique_slot(jar, flasks, effects, rotation) {
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
                if !nearby_matches(rule, nearby) {
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

fn nearby_matches(rule: &NearbyRule, nearby: Option<&NearbySnapshot>) -> bool {
    nearby.is_some_and(|census| {
        let counts = census.counts_at(f32::from(rule.radius));
        counts[rule.minimum_rarity.first_index()..]
            .iter()
            .sum::<usize>()
            >= usize::from(rule.minimum_count)
    })
}

/// Return true only when a usable equipped optional flask needs a nearby scan.
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
        flasks.slots[slot].as_ref().is_some_and(|flask| {
            matches!(flask.flask_type, FlaskType::Life | FlaskType::Mana)
                || flask.unique == Some(UniqueFlask::WrithingJar)
        })
    }) {
        return false;
    }
    let jar = UniqueFlask::WrithingJar;
    if config.poe1.unique.enabled(jar)
        && config
            .poe1
            .unique_conditions
            .get(&jar)
            .and_then(|conditions| conditions.nearby)
            .is_some_and(|rule| rule.enabled)
        && best_unique_slot(jar, flasks, effects, None).is_some()
    {
        return true;
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

fn best_unique_slot(
    kind: UniqueFlask,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
    rotation: Option<&FlaskRotation>,
) -> Option<usize> {
    best_eligible_slot(FlaskType::Hybrid, flasks, rotation, |slot, flask| {
        flask.unique == Some(kind) && !effects.slots_active[slot]
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
    (0..FLASK_SLOT_COUNT)
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
        || config.poe1.unique.writhing_jar
        || UTILITY_TYPES
            .into_iter()
            .any(|kind| config.poe1.utility.enabled(kind))
}

pub fn blocks_grace_period(effects: &EffectsSnapshot) -> bool {
    effects.effects.iter().any(|e| e.name == "grace_period")
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
