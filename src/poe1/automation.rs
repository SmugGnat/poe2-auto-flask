//! Independent PoE1 automation policy. Starts disarmed; never reuses stale
//! state to authorize input. PoE2's existing engine is unchanged.
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{ConfigManager, ConfigReload};
use crate::input::{is_process_foreground, master_toggle_down, send_flask_key};
use crate::process::Process;
use crate::session::GameSession;

use super::activation::ActivationGuard;
use super::bindings::{BindingManager, BindingState};
use super::decision::{
    blocks_grace_period, choose_action_rotating_with_nearby, enabled_demand, needs_nearby,
};
use super::movement::{RunTracker, RUN_ANIMATION_ID};
use super::rotation::FlaskRotation;
use super::types::FlaskType;

const POLL: Duration = Duration::from_millis(100);
const RELOAD_INTERVAL: Duration = Duration::from_secs(1);
const BINDING_INTERVAL: Duration = Duration::from_secs(2);
const REFRESH_INTERVAL: Duration = Duration::from_millis(250);

pub fn run(
    process: &Process,
    mut session: GameSession,
    manager: &mut ConfigManager,
) -> io::Result<()> {
    let mut bindings = BindingManager::load()?;
    let mut armed = false;
    let mut was_down = false;
    let mut activation_guard = ActivationGuard::default();
    let mut rotation = FlaskRotation::default();
    let mut run_tracker = RunTracker::default();
    let mut next_refresh = Instant::now();
    let mut next_config_reload = Instant::now() + RELOAD_INTERVAL;
    let mut next_binding_reload = Instant::now() + BINDING_INTERVAL;
    let mut needs_refresh = false;
    let mut status: Option<&'static str> = None;

    println!("Connected to Path of Exile 1.");
    println!("{}", manager.current().poe1_summary());
    println!("{}", binding_label(bindings.current()));
    println!("Starts DISARMED. Press F11 while PoE1 is focused to arm/disarm. Ctrl+C exits.");
    show(&mut status, "DISARMED");

    loop {
        let started = Instant::now();
        if !process.is_alive().unwrap_or(false) {
            return Ok(());
        }
        if started >= next_config_reload {
            next_config_reload = started + RELOAD_INTERVAL;
            match manager.check_for_reload() {
                ConfigReload::Reloaded => {
                    run_tracker.reset();
                    rotation.reset();
                    println!("PoE1 config reloaded: {}", manager.current().poe1_summary())
                }
                ConfigReload::Invalid(error) | ConfigReload::ReadError(error) => {
                    eprintln!("PoE1 config read failed, keeping last valid config: {error}")
                }
                ConfigReload::Unchanged | ConfigReload::Removed => (),
            }
        }
        if started >= next_binding_reload {
            next_binding_reload = started + BINDING_INTERVAL;
            if bindings.reload() {
                println!("{}", binding_label(bindings.current()));
            }
        }

        let focus = is_process_foreground(process.pid());
        let down = master_toggle_down();
        if down && !was_down && focus {
            run_tracker.reset();
            armed = !armed;
            activation_guard.reset();
            rotation.reset();
            needs_refresh = armed;
            next_refresh = started;
            show(
                &mut status,
                if armed {
                    "WAITING FOR POE1 GAME STATE"
                } else {
                    "DISARMED"
                },
            );
        }
        was_down = down;

        if !armed {
            run_tracker.reset();
            nap(started);
            continue;
        }
        if !focus {
            // Always refresh after focus returns, so no pointer/cache gathered
            // during an unfocused UI state can authorize an action.
            needs_refresh = true;
            run_tracker.reset();
            show(&mut status, "PAUSED (PoE1 not focused)");
            nap(started);
            continue;
        }

        let keys = match bindings.current() {
            BindingState::Ready(keys) => *keys,
            BindingState::Unavailable(_) => {
                run_tracker.reset();
                show(&mut status, "PAUSED (PoE1 keybindings unavailable)");
                nap(started);
                continue;
            }
        };

        if needs_refresh {
            if started < next_refresh {
                nap(started);
                continue;
            }
            match session.refresh(process) {
                Ok(()) => {
                    needs_refresh = false;
                    activation_guard.reset();
                    rotation.reset();
                    run_tracker.reset();
                }
                Err(_) => {
                    next_refresh = started + REFRESH_INTERVAL;
                    show(&mut status, "WAITING FOR POE1 GAME STATE");
                    nap(started);
                    continue;
                }
            }
        }

        session.begin_tick();
        let sampled = (|| {
            let vitals = session.vitals(process)?;
            let area = session.poe1_area(process)?;
            Ok::<_, io::Error>((
                vitals,
                area.allows_auto_flask(manager.current().general.enable_in_hideout),
            ))
        })();
        let (vitals, area_allowed) = match sampled {
            Ok(values) => values,
            Err(_) => {
                needs_refresh = true;
                next_refresh = started + REFRESH_INTERVAL;
                show(&mut status, "WAITING FOR POE1 GAME STATE");
                nap(started);
                continue;
            }
        };
        if !area_allowed {
            run_tracker.reset();
            show(&mut status, "PAUSED (town or hideout)");
            nap(started);
            continue;
        }
        if vitals.health.current <= 0 {
            run_tracker.reset();
            show(&mut status, "PAUSED (dead)");
            nap(started);
            continue;
        }
        if !enabled_demand(manager.current(), vitals) {
            run_tracker.reset();
            show(&mut status, "ARMED");
            nap(started);
            continue;
        }

        // Optional Quicksilver-only movement confirmation. One cheap Actor
        // animation read per active tick, shared through GameSession.
        // Read failures disable the Quicksilver action, never Life/Mana.
        let moving_ms = manager.current().poe1.quicksilver_min_run_ms;
        let quicksilver_ready = if moving_ms > 0 && manager.current().poe1.utility.quicksilver {
            let animation_id = session.poe1_animation(process).ok();
            run_tracker.observe(animation_id, started, Duration::from_millis(moving_ms))
        } else {
            run_tracker.reset();
            true
        };

        let state = (|| {
            let chat = session.poe1_chat_active(process)?;
            if chat {
                return Ok::<_, io::Error>(None);
            }
            let mut effects = session.poe1_effects(process)?.clone();
            if blocks_grace_period(&effects) {
                return Ok(None);
            }
            let flasks = session.poe1_flasks(process)?.clone();
            activation_guard.apply_to(started, vitals, &flasks, &mut effects);
            // Only scan monsters if an enabled, usable utility needs it.
            // One cached census is shared across every equipped utility type.
            // Failed scans disable nearby-gated utilities, never Life/Mana.
            let proximity = if needs_nearby(
                manager.current(),
                vitals,
                &flasks,
                &effects,
                quicksilver_ready,
            ) {
                session.poe1_nearby(process).ok()
            } else {
                None
            };
            let chosen = choose_action_rotating_with_nearby(
                manager.current(),
                vitals,
                &flasks,
                &effects,
                quicksilver_ready,
                proximity.as_ref(),
                &rotation,
            );
            Ok(chosen.map(|slot| (slot, flasks, proximity)))
        })();

        match state {
            Ok(Some((slot, flasks, proximity))) => {
                // Validate again immediately before any input: game state,
                // threshold, focus and chat may have changed during reading.
                let revalidated = session.poe1_verify_vitals(process).and_then(|v| {
                    let chat = session.poe1_chat_active(process)?;
                    Ok((v, chat))
                });
                match revalidated {
                    Ok((new_vitals, false))
                        if is_process_foreground(process.pid())
                            && new_vitals.health.current > 0 =>
                    {
                        // If Quicksilver requires sustained motion, confirm the
                        // animation is *still* Run just before issuing the key.
                        let quicksilver_motion_ok = if moving_ms > 0
                            && flasks.slots[slot]
                                .as_ref()
                                .is_some_and(|flask| flask.flask_type == FlaskType::Quicksilver)
                        {
                            quicksilver_ready
                                && session.poe1_verify_animation(process).ok()
                                    == Some(RUN_ANIMATION_ID)
                        } else {
                            true
                        };
                        if !quicksilver_motion_ok {
                            run_tracker.reset();
                        }
                        let mut effects = session.poe1_effects(process)?.clone();
                        activation_guard.apply_to(started, new_vitals, &flasks, &mut effects);
                        if quicksilver_motion_ok
                            && choose_action_rotating_with_nearby(
                                manager.current(),
                                new_vitals,
                                &flasks,
                                &effects,
                                quicksilver_ready,
                                proximity.as_ref(),
                                &rotation,
                            ) == Some(slot)
                            && !blocks_grace_period(&effects)
                        {
                            if let Some(flask) = &flasks.slots[slot] {
                                send_flask_key(keys[slot])?;
                                let sent_at = Instant::now();
                                activation_guard.mark(slot, flask.flask_type, sent_at, new_vitals, &flasks);
                                rotation.record(slot, flask.flask_type);
                                println!(
                                    "{} flask (slot {}) -> {} | charges {} | effective cost {}",
                                    flask.flask_type.name(),
                                    slot + 1,
                                    keys[slot].label(),
                                    flask.current_charges,
                                    flask.effective_per_use.unwrap_or_default()
                                );
                                show(&mut status, "ARMED");
                            }
                        }
                    }
                    Ok(_) => show(&mut status, "PAUSED (chat or focus)"),
                    Err(_) => {
                        needs_refresh = true;
                        next_refresh = started + REFRESH_INTERVAL;
                        show(&mut status, "WAITING FOR POE1 GAME STATE");
                    }
                }
            }
            Ok(None) => show(&mut status, "ARMED"),
            Err(error) => {
                eprintln!("PoE1 state unavailable: {error}");
                needs_refresh = true;
                next_refresh = started + REFRESH_INTERVAL;
                show(&mut status, "WAITING FOR POE1 GAME STATE");
            }
        }

        nap(started);
    }
}

fn show(status: &mut Option<&'static str>, next: &'static str) {
    if *status != Some(next) {
        println!("Status: {next}");
        *status = Some(next);
    }
}
fn nap(started: Instant) {
    if let Some(wait) = POLL.checked_sub(started.elapsed()) {
        thread::sleep(wait);
    }
}
fn binding_label(state: &BindingState) -> String {
    match state {
        BindingState::Ready(keys) => format!(
            "PoE1 flask keys: {}",
            keys.iter()
                .enumerate()
                .map(|(i, k)| format!("{}={}", i + 1, k.label()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        BindingState::Unavailable(error) => format!("PoE1 bindings unavailable: {error}"),
    }
}
