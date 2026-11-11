//! Pure decision logic: no memory reads and no keyboard input.
use crate::config::Config;
use crate::poe2::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, FlaskSlot, SLOT_COUNT};
use super::types::FlaskType;

pub fn choose_action(
    config: &Config,
    vitals: LiveVitals,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
) -> Option<usize> {
    // Life and mana have priority over optional utility maintenance.
    let health = vitals.health;
    let ci_like = health.total == 1
        && vitals.energy_shield.is_some_and(|es| es.total > 0);
    if !ci_like
        && config.health.enabled
        && health.current > 0
        && health.percentage().is_some_and(|percent| percent <= config.health.threshold_percent)
    {
        if let Some(slot) = best_slot(FlaskType::Life, flasks, effects) {
            return Some(slot);
        }
    }
    if config.mana.enabled
        && vitals.mana.is_some_and(|mana| {
            mana.usable_max() > 0
                && mana.percentage().is_some_and(|pct| pct <= config.mana.threshold_percent)
        })
    {
        if let Some(slot) = best_slot(FlaskType::Mana, flasks, effects) {
            return Some(slot);
        }
    }
    for kind in [
        FlaskType::Quicksilver, FlaskType::Granite,
        FlaskType::Quartz, FlaskType::Diamond,
    ] {
        if config.poe1.utility.enabled(kind) {
            if let Some(slot) = best_slot(kind, flasks, effects) {
                return Some(slot);
            }
        }
    }
    None
}

/// Duplicates: choose most whole uses remaining, then most charges,
/// then the lowest slot. A currently active flask blocks *all* of its type.
fn best_slot(
    kind: FlaskType,
    flasks: &FlaskSnapshot,
    effects: &EffectsSnapshot,
) -> Option<usize> {
    if flasks.slots.iter().enumerate().any(|(slot, state)| {
        state.as_ref().is_some_and(|s| s.flask_type == kind && effects.slots_active[slot])
    }) {
        return None;
    }

    (0..SLOT_COUNT)
        .filter_map(|slot| {
            let state = flasks.slots[slot].as_ref()?;
            if state.flask_type != kind { return None; }
            let uses = state.remaining_uses()?;
            if uses < 1 { return None; }
            Some((slot, uses, state.current_charges))
        })
        .max_by_key(|(slot, uses, charges)| (*uses, *charges, std::cmp::Reverse(*slot)))
        .map(|(slot, _, _)| slot)
}

pub fn enabled_demand(config: &Config, vitals: LiveVitals) -> bool {
    let health = vitals.health;
    let ci_like = health.total == 1
        && vitals.energy_shield.is_some_and(|es| es.total > 0);
    (!ci_like && config.health.enabled && health.current > 0
        && health.percentage().is_some_and(|pct| pct <= config.health.threshold_percent))
        || (config.mana.enabled && vitals.mana.is_some_and(|mana| {
            mana.usable_max() > 0
                && mana.percentage().is_some_and(|pct| pct <= config.mana.threshold_percent)
        }))
        || config.poe1.utility.quicksilver
        || config.poe1.utility.granite
        || config.poe1.utility.quartz
        || config.poe1.utility.diamond
}

pub fn blocks_grace_period(effects: &EffectsSnapshot) -> bool {
    effects.effects.iter().any(|e| e.name == "grace_period")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poe2::VitalPool;

    fn slot(kind: FlaskType, charges: i32, cost: Option<i32>) -> Option<FlaskSlot> {
        Some(FlaskSlot {
            item_path: "Metadata/Items/Flasks/Test".into(),
            flask_type: kind,
            current_charges: charges,
            raw_per_use: 15,
            effective_per_use: cost,
        })
    }
    fn setup() -> (Config, LiveVitals, FlaskSnapshot, EffectsSnapshot) {
        let config = Config::default();
        let vital = |current, total| VitalPool { current, total, reserved_flat: 0, reserved_fraction: 0 };
        (
            config,
            LiveVitals { health: vital(40,100), mana: Some(vital(80,100)), energy_shield: None },
            FlaskSnapshot { slots: std::array::from_fn(|_| None) },
            EffectsSnapshot { slots_active: [false; SLOT_COUNT], effects: Vec::new() },
        )
    }

    #[test]
    fn duplicate_life_flasks_prefer_more_uses() {
        let (config,vitals,mut flasks,effects)=setup();
        flasks.slots[0]=slot(FlaskType::Life,45,Some(15));
        flasks.slots[1]=slot(FlaskType::Life,60,Some(15));
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),Some(1));
    }

    #[test]
    fn equal_uses_prefer_charges_then_first_slot() {
        let (config,vitals,mut flasks,effects)=setup();
        flasks.slots[0]=slot(FlaskType::Life,45,Some(15));
        flasks.slots[1]=slot(FlaskType::Life,48,Some(15));
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),Some(1));
        flasks.slots[0]=slot(FlaskType::Life,48,Some(15));
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),Some(0));
    }

    #[test]
    fn active_duplicate_blocks_entire_type() {
        let (config,vitals,mut flasks,mut effects)=setup();
        flasks.slots[0]=slot(FlaskType::Life,45,Some(15));
        flasks.slots[1]=slot(FlaskType::Life,60,Some(15));
        effects.slots_active[0]=true;
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),None);
    }

    #[test]
    fn no_effective_cost_never_authorizes_input() {
        let (config,vitals,mut flasks,effects)=setup();
        flasks.slots[0]=slot(FlaskType::Life,45,None);
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),None);
    }

    #[test]
    fn utility_matches_type_not_slot_and_remains_opt_in() {
        let (mut config,mut vitals,mut flasks,effects)=setup();
        vitals.health.current=100;
        flasks.slots[3]=slot(FlaskType::Quicksilver,50,Some(30));
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),None);
        config.poe1.utility.quicksilver=true;
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),Some(3));
    }

    #[test]
    fn mana_threshold_uses_unreserved_pool() {
        let (config,mut vitals,mut flasks,effects)=setup();
        let mana=vitals.mana.as_mut().unwrap();
        mana.total=1234; mana.reserved_flat=1102; mana.current=30;
        flasks.slots[1]=slot(FlaskType::Mana,40,Some(15));
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),Some(1));
    }

    #[test]
    fn rejects_dead_or_ci_like_health() {
        let (config,mut vitals,mut flasks,effects)=setup();
        flasks.slots[0]=slot(FlaskType::Life,45,Some(15));
        vitals.health.current=0;
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),None);
        vitals.health.total=1;
        vitals.health.current=1;
        vitals.energy_shield=Some(VitalPool {
            total: 2000, current: 1000, reserved_flat: 0, reserved_fraction: 0,
        });
        assert_eq!(choose_action(&config,vitals,&flasks,&effects),None);
    }
}
