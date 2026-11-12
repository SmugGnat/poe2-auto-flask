//! Regression tests for the PoE1 activation module.
use super::*;
use crate::config::Config;
use crate::game_state::VitalPool;
use crate::poe1::decision::choose_action;
use crate::poe1::flasks::FlaskSlot;
use crate::poe1::recovery::RecoveryKind;

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
