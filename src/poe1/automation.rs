//! PoE1 flask *policy* for the shared automation engine. Memory reads
//! are delegated to the validated GameSession's PoE1 adapter, not performed
//! by the scheduler. The tested Life/Mana/utility decision rules are unchanged.
use std::io;
use std::time::{Duration, Instant};

use crate::action::{dispatch, ActionRequest, Dispatch, DispatchError};
use crate::config::{Config, ConfigManager, ConfigReload};
use crate::engine::{GameAutomationPolicy, TickOutcome};
use crate::input::is_process_foreground;
use crate::process::Process;
use crate::session::GameSession;

use super::activation::ActivationGuard;
use super::bindings::{BindingManager, BindingState};
use super::decision::{
    blocks_grace_period, choose_action_rotating_guarded, enabled_demand, needs_nearby_guarded,
};
use super::movement::{RunTracker, RUN_ANIMATION_ID};
use super::rotation::FlaskRotation;
use super::types::FlaskType;

pub fn run(process: &Process, session: GameSession, manager: &mut ConfigManager) -> io::Result<()> {
    let policy = Policy::new()?;
    crate::engine::run(process, session, manager, policy)
}

struct Policy {
    bindings: BindingManager,
    activation_guard: ActivationGuard,
    rotation: FlaskRotation,
    run_tracker: RunTracker,
}

impl Policy {
    fn new() -> io::Result<Self> {
        Ok(Self {
            bindings: BindingManager::load()?,
            activation_guard: ActivationGuard::default(),
            rotation: FlaskRotation::default(),
            run_tracker: RunTracker::default(),
        })
    }
}

impl GameAutomationPolicy for Policy {
    fn banner(&self, config: &Config) {
        println!("Connected to Path of Exile 1.");
        println!("{}", config.poe1_summary());
        println!("{}", binding_label(self.bindings.current()));
        println!("Starts DISARMED. Press F11 while PoE1 is focused to arm/disarm. Ctrl+C exits.");
    }

    fn on_config_reload(&mut self, event: ConfigReload, config: &Config) {
        match event {
            ConfigReload::Reloaded => {
                self.run_tracker.reset();
                self.rotation.reset();
                println!("PoE1 config reloaded: {}", config.poe1_summary());
            }
            ConfigReload::Invalid(error) | ConfigReload::ReadError(error) => {
                eprintln!("PoE1 config read failed, keeping last valid config: {error}");
            }
            ConfigReload::Unchanged | ConfigReload::Removed => {}
        }
    }

    fn check_bindings(&mut self) {
        if self.bindings.reload() {
            println!("{}", binding_label(self.bindings.current()));
        }
    }

    fn bindings_pause(&self) -> Option<&'static str> {
        match self.bindings.current() {
            BindingState::Ready(_) => None,
            BindingState::Unavailable(_) => Some("PAUSED (PoE1 keybindings unavailable)"),
        }
    }

    fn on_arm_change(&mut self, _armed: bool) {
        self.run_tracker.reset();
        self.activation_guard.reset();
        self.rotation.reset();
    }

    fn on_inactive(&mut self) {
        self.run_tracker.reset();
    }

    fn on_focus_lost(&mut self) {
        self.run_tracker.reset();
    }

    fn after_refresh(&mut self) {
        self.activation_guard.reset();
        self.rotation.reset();
        self.run_tracker.reset();
    }

    fn refresh(&mut self, _process: &Process, _session: &GameSession) -> io::Result<()> {
        // All PoE1 pointer/cache refresh work belongs to GameSession's adapter.
        Ok(())
    }

    fn focus_status(&self) -> &'static str {
        "PAUSED (PoE1 not focused)"
    }

    fn waiting_status(&self) -> &'static str {
        "WAITING FOR POE1 GAME STATE"
    }

    fn tick(
        &mut self,
        process: &Process,
        session: &mut GameSession,
        config: &Config,
        started: Instant,
    ) -> io::Result<TickOutcome> {
        // A separate validated game-state read (and optional area read) is
        // requested on demand. The engine itself never interprets these.
        let sampled = (|| {
            let vitals = session.vitals(process)?;
            let area = session.poe1_area(process)?;
            Ok::<_, io::Error>((
                vitals,
                area.allows_auto_flask(config.general.enable_in_hideout),
            ))
        })();
        let (vitals, area_allowed) = match sampled {
            Ok(values) => values,
            Err(_) => return Ok(TickOutcome::Refresh),
        };
        if !area_allowed {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("PAUSED (town or hideout)"));
        }
        if vitals.health.current <= 0 {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("PAUSED (dead)"));
        }
        if !enabled_demand(config, vitals) {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("ARMED"));
        }

        // Optional Quicksilver-only movement confirmation. Only a game
        // policy knows when animation data is actually needed.
        let moving_ms = config.poe1.quicksilver_min_run_ms;
        let quicksilver_ready = if moving_ms > 0 && config.poe1.utility.quicksilver {
            let animation_id = session.poe1_animation(process).ok();
            self.run_tracker
                .observe(animation_id, started, Duration::from_millis(moving_ms))
        } else {
            self.run_tracker.reset();
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
            self.activation_guard
                .apply_to(started, vitals, &flasks, &mut effects);
            // Only scan monsters if an enabled, usable utility needs it.
            // One cached census is shared across every equipped utility type.
            // Failed scans disable nearby-gated utilities, never Life/Mana.
            let proximity = if needs_nearby_guarded(
                config,
                vitals,
                &flasks,
                &effects,
                quicksilver_ready,
                !self.activation_guard.life_input_pending(),
            ) {
                session.poe1_nearby(process).ok()
            } else {
                None
            };
            let chosen = choose_action_rotating_guarded(
                config,
                vitals,
                &flasks,
                &effects,
                quicksilver_ready,
                proximity.as_ref(),
                &self.rotation,
                !self.activation_guard.life_input_pending(),
            );
            Ok(chosen.map(|slot| (slot, flasks, proximity)))
        })();

        match state {
            Ok(Some((slot, flasks, proximity))) => {
                // The shared executor checks foreground before and after
                // game-specific validation, then owns the only SendInput.
                // No input, cooldown or rotation bookkeeping is performed
                // when the policy denies an action or a read becomes stale.
                let keys = self.ready_keys()?;
                let action = ActionRequest::flask(slot, keys[slot]);
                let mut paused = false;
                let dispatched = dispatch(process, action, || {
                    // Keep the proven PoE1 pre-send checks: live player
                    // identity/vitals, chat, Quicksilver animation and
                    // re-running the current flask decision/active effects.
                    let new_vitals = session.poe1_verify_vitals(process)?;
                    let chat = session.poe1_chat_active(process)?;
                    if chat || new_vitals.health.current <= 0 {
                        paused = true;
                        return Ok(None);
                    }

                    let quicksilver_motion_ok = if moving_ms > 0
                        && flasks.slots[slot]
                            .as_ref()
                            .is_some_and(|flask| flask.flask_type == FlaskType::Quicksilver)
                    {
                        quicksilver_ready
                            && session.poe1_verify_animation(process).ok() == Some(RUN_ANIMATION_ID)
                    } else {
                        true
                    };
                    if !quicksilver_motion_ok {
                        self.run_tracker.reset();
                    }
                    let mut effects = session.poe1_effects(process)?.clone();
                    self.activation_guard
                        .apply_to(started, new_vitals, &flasks, &mut effects);
                    let still_selected = choose_action_rotating_guarded(
                        config,
                        new_vitals,
                        &flasks,
                        &effects,
                        quicksilver_ready,
                        proximity.as_ref(),
                        &self.rotation,
                        !self.activation_guard.life_input_pending(),
                    ) == Some(slot);
                    if quicksilver_motion_ok && still_selected && !blocks_grace_period(&effects) {
                        Ok(Some(new_vitals))
                    } else {
                        Ok(None)
                    }
                });

                match dispatched {
                    Ok(Dispatch::Sent {
                        validated: new_vitals,
                        sent_at,
                    }) => {
                        if let Some(flask) = &flasks.slots[slot] {
                            self.activation_guard.mark(
                                slot,
                                flask.flask_type,
                                sent_at,
                                new_vitals,
                                &flasks,
                            );
                            self.rotation.record(slot, flask.flask_type);
                            println!(
                                "{} flask (slot {}) -> {} | charges {} | effective cost {}",
                                flask.flask_type.name(),
                                slot + 1,
                                keys[slot].label(),
                                flask.current_charges,
                                flask.effective_per_use.unwrap_or_default()
                            );
                        }
                    }
                    Ok(Dispatch::Skipped) if paused || !is_process_foreground(process.pid()) => {
                        return Ok(TickOutcome::Status("PAUSED (chat or focus)"));
                    }
                    Ok(Dispatch::Skipped) => {}
                    Err(DispatchError::Validation(_)) => return Ok(TickOutcome::Refresh),
                    Err(DispatchError::Input(error)) => return Err(error),
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("PoE1 state unavailable: {error}");
                return Ok(TickOutcome::Refresh);
            }
        }
        Ok(TickOutcome::Status("ARMED"))
    }
}

impl Policy {
    fn ready_keys(&self) -> io::Result<[crate::input::KeyBinding; 5]> {
        match self.bindings.current() {
            BindingState::Ready(keys) => Ok(*keys),
            BindingState::Unavailable(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PoE1 bindings changed during decision",
            )),
        }
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
