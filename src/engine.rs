//! Shared automation lifecycle. This module schedules work and enforces
//! arming/focus/reconnect boundaries; it never interprets game memory and
//! never decides which flask should be used.
//!
//! Individual game policies request validated state through GameSession and
//! retain their own decision rules, safety guards, bindings, and input logic.
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{Config, ConfigManager, ConfigReload};
use crate::input::{is_process_foreground, master_toggle_down};
use crate::process::Process;
use crate::session::GameSession;

const POLL: Duration = Duration::from_millis(100);
const CONFIG_RELOAD: Duration = Duration::from_secs(1);
const BINDING_RELOAD: Duration = Duration::from_secs(2);
const PROCESS_CHECK: Duration = Duration::from_secs(1);
const REFRESH_RETRY: Duration = Duration::from_millis(250);

/// Outcome of one game-policy decision tick. Parsing and deciding are kept
/// outside the engine. Only the policy may authorize a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickOutcome {
    Status(&'static str),
    /// A read became stale; invalidate and reconnect the session.
    Refresh,
    /// Keep the last status when a game intentionally tolerates transient
    /// read failures (PoE2's existing three-sample policy).
    Unchanged,
}

pub trait GameAutomationPolicy {
    /// Preserve each game's existing startup and config/binding messages.
    fn banner(&self, config: &Config);
    fn on_config_reload(&mut self, event: ConfigReload, config: &Config);
    fn check_bindings(&mut self);
    /// A reason to pause, if the selected game's current bindings are unsafe.
    fn bindings_pause(&self) -> Option<&'static str>;

    /// These hooks reset only game-specific state, not the shared session.
    fn on_arm_change(&mut self, armed: bool);
    fn on_inactive(&mut self);
    fn on_focus_lost(&mut self);
    fn after_refresh(&mut self);
    /// Resolve game-specific pointers after the shared GameSession refresh.
    fn refresh(&mut self, process: &Process, session: &GameSession) -> io::Result<()>;

    fn tick(
        &mut self,
        process: &Process,
        session: &mut GameSession,
        config: &Config,
        started: Instant,
    ) -> io::Result<TickOutcome>;

    fn focus_status(&self) -> &'static str;
    fn waiting_status(&self) -> &'static str;
}

/// Shared F11 edge detector. Holding the key does not toggle repeatedly;
/// pressing it outside the selected game's foreground window is ignored.
#[derive(Default)]
struct ArmLatch {
    armed: bool,
    last_down: bool,
}

impl ArmLatch {
    fn observe(&mut self, down: bool, focused: bool) -> Option<bool> {
        let pressed = down && !self.last_down && focused;
        self.last_down = down;
        if pressed {
            self.armed = !self.armed;
            Some(self.armed)
        } else {
            None
        }
    }
}

/// The only auto-mode polling loop for PoE1 and PoE2. No game-specific reads
/// or game-specific input decisions belong here.
pub fn run<P: GameAutomationPolicy>(
    process: &Process,
    mut session: GameSession,
    config_manager: &mut ConfigManager,
    mut policy: P,
) -> io::Result<()> {
    policy.banner(config_manager.current());
    let mut latch = ArmLatch::default();
    let mut status = None;
    let mut needs_refresh = false;
    let mut next_refresh = Instant::now();
    let mut next_config_reload = Instant::now() + CONFIG_RELOAD;
    let mut next_binding_reload = Instant::now() + BINDING_RELOAD;
    let mut next_process_check = Instant::now();
    show(&mut status, "DISARMED");

    loop {
        let started = Instant::now();
        if started >= next_process_check {
            next_process_check = started + PROCESS_CHECK;
            if !process.is_alive().unwrap_or(false) {
                return Ok(());
            }
        }

        if started >= next_config_reload {
            next_config_reload = started + CONFIG_RELOAD;
            let event = config_manager.check_for_reload();
            policy.on_config_reload(event, config_manager.current());
        }
        if started >= next_binding_reload {
            next_binding_reload = started + BINDING_RELOAD;
            policy.check_bindings();
        }

        let focused = is_process_foreground(process.pid());
        if let Some(armed) = latch.observe(master_toggle_down(), focused) {
            policy.on_arm_change(armed);
            needs_refresh = armed;
            next_refresh = started;
            show(
                &mut status,
                if armed {
                    policy.waiting_status()
                } else {
                    "DISARMED"
                },
            );
        }
        if !latch.armed {
            policy.on_inactive();
            nap(started);
            continue;
        }
        if !focused {
            needs_refresh = true;
            policy.on_focus_lost();
            show(&mut status, policy.focus_status());
            nap(started);
            continue;
        }
        if let Some(reason) = policy.bindings_pause() {
            policy.on_inactive();
            show(&mut status, reason);
            nap(started);
            continue;
        }
        if needs_refresh {
            if started < next_refresh {
                nap(started);
                continue;
            }
            // Refresh the session before its policy-specific derived probes.
            // A failure cannot authorize stale memory-backed input.
            let refreshed = session
                .refresh(process)
                .and_then(|()| policy.refresh(process, &session));
            match refreshed {
                Ok(()) => {
                    needs_refresh = false;
                    policy.after_refresh();
                }
                Err(_) => {
                    if !process.is_alive().unwrap_or(false) {
                        return Ok(());
                    }
                    next_refresh = started + REFRESH_RETRY;
                    show(&mut status, policy.waiting_status());
                    nap(started);
                    continue;
                }
            }
        }

        session.begin_tick();
        let outcome = policy.tick(process, &mut session, config_manager.current(), started)?;
        match outcome {
            TickOutcome::Status(next) => show(&mut status, next),
            TickOutcome::Refresh => {
                needs_refresh = true;
                next_refresh = started + REFRESH_RETRY;
                show(&mut status, policy.waiting_status());
            }
            TickOutcome::Unchanged => {}
        }
        nap(started);
    }
}

fn show(current: &mut Option<&'static str>, next: &'static str) {
    if *current != Some(next) {
        println!("Status: {next}");
        *current = Some(next);
    }
}

fn nap(started: Instant) {
    if let Some(wait) = POLL.checked_sub(started.elapsed()) {
        thread::sleep(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::{ArmLatch, TickOutcome};

    #[test]
    fn key_toggles_only_on_foreground_rising_edge() {
        let mut latch = ArmLatch::default();
        assert_eq!(latch.observe(true, false), None);
        assert_eq!(latch.observe(true, true), None);
        assert_eq!(latch.observe(false, true), None);
        assert_eq!(latch.observe(true, true), Some(true));
        assert_eq!(latch.observe(true, true), None);
        assert_eq!(latch.observe(false, false), None);
        assert_eq!(latch.observe(true, true), Some(false));
    }

    #[test]
    fn launch_is_disarmed_and_unfocused_key_does_not_arm_on_refocus() {
        let mut latch = ArmLatch::default();
        assert!(!latch.armed);

        // An unfocused F11 press is consumed, not replayed on focus gain.
        assert_eq!(latch.observe(true, false), None);
        assert_eq!(latch.observe(true, true), None);
        assert!(!latch.armed);

        assert_eq!(latch.observe(false, true), None);
        assert_eq!(latch.observe(true, true), Some(true));
        assert!(latch.armed);
    }

    #[test]
    fn focus_loss_does_not_replay_held_toggle_or_implicitly_disarm() {
        let mut latch = ArmLatch::default();
        assert_eq!(latch.observe(true, true), Some(true));
        assert!(latch.armed);

        // The engine pauses while unfocused; the latch itself only changes
        // state on a fresh foreground rising edge.
        assert_eq!(latch.observe(true, false), None);
        assert_eq!(latch.observe(true, true), None);
        assert!(latch.armed);

        assert_eq!(latch.observe(false, false), None);
        assert_eq!(latch.observe(true, false), None);
        assert_eq!(latch.observe(true, true), None);
        assert!(latch.armed);

        assert_eq!(latch.observe(false, true), None);
        assert_eq!(latch.observe(true, true), Some(false));
        assert!(!latch.armed);
    }

    #[test]
    fn policy_can_retry_without_clearing_state_or_request_refresh() {
        assert_ne!(TickOutcome::Unchanged, TickOutcome::Refresh);
        assert_eq!(TickOutcome::Status("ARMED"), TickOutcome::Status("ARMED"));
    }
}
