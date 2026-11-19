//! Shared automation lifecycle. This module schedules work and enforces
//! arming/focus/reconnect boundaries; it never interprets game memory and
//! never decides which flask should be used.
//!
//! Individual game policies request validated state through GameSession and
//! retain their own decision rules, safety guards, bindings, and input logic.
use std::io;
use std::time::{Duration, Instant};

use crate::config::{Config, ConfigManager, ConfigReload};
use crate::diagnostics::FailureMonitor;
use crate::input::{is_process_foreground, AutomationKey};
use crate::process::Process;
use crate::session::GameSession;

const POLL: Duration = Duration::from_millis(100);
const CONFIG_RELOAD: Duration = Duration::from_secs(1);
const BINDING_RELOAD: Duration = Duration::from_secs(2);
const PROCESS_CHECK: Duration = Duration::from_secs(1);
const REFRESH_RETRY: Duration = Duration::from_millis(250);

/// Outcome of one game-policy decision tick. Parsing and deciding are kept
/// outside the engine. Only the policy may authorize a key press.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TickOutcome {
    Status(&'static str),
    /// A read failed; invalidate and reconnect while retaining its reason.
    RefreshFailed(String),
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
    fn bindings_pause(&self, toggle: AutomationKey) -> Option<&'static str>;

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

/// Shared shortcut edge detector. Holding the key does not toggle repeatedly;
/// pressing it outside the selected game's foreground window is ignored.
#[derive(Default)]
struct ArmLatch {
    armed: bool,
    last_down: bool,
}

impl ArmLatch {
    /// Consume any key held during a profile or shortcut change.
    fn disable(&mut self, down: bool) -> bool {
        let was_armed = self.armed;
        self.armed = false;
        self.last_down = down;
        was_armed
    }

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
    control: &mut crate::runtime::BackendControl,
) -> io::Result<()> {
    policy.banner(config_manager.current());
    let mut toggle = config_manager.automation_key();
    print_toggle_key(toggle);
    let mut latch = ArmLatch {
        last_down: toggle.is_down(),
        ..ArmLatch::default()
    };
    let mut status = None;
    let mut failures = FailureMonitor::default();
    let mut needs_refresh = false;
    let mut next_refresh = Instant::now();
    let mut next_config_reload = Instant::now() + CONFIG_RELOAD;
    let mut next_binding_reload = Instant::now() + BINDING_RELOAD;
    let mut next_process_check = Instant::now();
    show(&mut status, "DISABLED");

    loop {
        let started = Instant::now();
        if control.is_stopping() {
            policy.on_arm_change(false);
            return Ok(());
        }
        if started >= next_process_check {
            next_process_check = started + PROCESS_CHECK;
            if !process.is_alive().unwrap_or(false) {
                return Ok(());
            }
        }

        if started >= next_config_reload {
            next_config_reload = started + CONFIG_RELOAD;
            let event = config_manager.check_for_reload();
            let new_toggle = config_manager.automation_key();
            let toggle_changed = new_toggle != toggle;
            toggle = new_toggle;
            if toggle_changed || matches!(&event, ConfigReload::ProfileSwitched) {
                if latch.disable(toggle.is_down()) {
                    policy.on_arm_change(false);
                }
                needs_refresh = false;
                failures.clear();
                if toggle_changed {
                    show(&mut status, "DISABLED (automation key changed)");
                    print_toggle_key(toggle);
                } else {
                    show(&mut status, "DISABLED (profile switched)");
                }
            }
            match &event {
                ConfigReload::Invalid(error) | ConfigReload::ReadError(error) => {
                    control.config_error(error.clone());
                }
                ConfigReload::Removed => {
                    control.config_error("Configuration file removed".to_owned())
                }
                ConfigReload::Reloaded | ConfigReload::ProfileSwitched => {
                    control.applied(config_manager)
                }
                ConfigReload::Unchanged if toggle_changed => control.applied(config_manager),
                ConfigReload::Unchanged => {}
            }
            policy.on_config_reload(event, config_manager.current());
        }
        if started >= next_binding_reload {
            next_binding_reload = started + BINDING_RELOAD;
            policy.check_bindings();
        }

        let focused = is_process_foreground(process.pid());
        if let Some(armed) = latch.observe(toggle.is_down(), focused) {
            policy.on_arm_change(armed);
            needs_refresh = armed;
            next_refresh = started;
            show(
                &mut status,
                if armed {
                    policy.waiting_status()
                } else {
                    "DISABLED"
                },
            );
        }
        if !latch.armed {
            failures.clear();
            policy.on_inactive();
            nap(started, control, latch.armed, focused, status);
            continue;
        }
        if !focused {
            failures.clear();
            needs_refresh = true;
            policy.on_focus_lost();
            show(&mut status, policy.focus_status());
            nap(started, control, latch.armed, focused, status);
            continue;
        }
        if let Some(reason) = policy.bindings_pause(toggle) {
            failures.clear();
            policy.on_inactive();
            show(&mut status, reason);
            nap(started, control, latch.armed, focused, status);
            continue;
        }
        if needs_refresh {
            if started < next_refresh {
                nap(started, control, latch.armed, focused, status);
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
                Err(error) => {
                    failures.warn("Reconnect", &error.to_string());
                    if !process.is_alive().unwrap_or(false) {
                        return Ok(());
                    }
                    next_refresh = started + REFRESH_RETRY;
                    show(&mut status, policy.waiting_status());
                    nap(started, control, latch.armed, focused, status);
                    continue;
                }
            }
        }

        session.begin_tick();
        let outcome = policy.tick(process, &mut session, config_manager.current(), started)?;
        match outcome {
            TickOutcome::Status(next) => {
                failures.clear();
                show(&mut status, next);
            }
            TickOutcome::RefreshFailed(reason) => {
                failures.warn("Live", &reason);
                needs_refresh = true;
                next_refresh = started + REFRESH_RETRY;
                show(&mut status, policy.waiting_status());
            }
            TickOutcome::Unchanged => {}
        }
        nap(started, control, latch.armed, focused, status);
    }
}

fn print_toggle_key(toggle: AutomationKey) {
    println!(
        "Press {} while the game is focused to enable or disable automation. Ctrl+C exits.",
        toggle.label()
    );
}

fn show(current: &mut Option<&'static str>, next: &'static str) {
    if *current != Some(next) {
        println!("Status: {next}");
        *current = Some(next);
    }
}

fn nap(
    started: Instant,
    control: &mut crate::runtime::BackendControl,
    enabled: bool,
    focused: bool,
    status: Option<&str>,
) {
    control.automation(enabled, focused, status.unwrap_or("DISABLED"));
    if let Some(wait) = POLL.checked_sub(started.elapsed()) {
        control.wait(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::{ArmLatch, TickOutcome};

    #[test]
    fn reported_enabled_state_follows_the_latch_through_focus_pauses() {
        use crate::runtime::{BackendControl, Connection};
        use std::time::Instant;

        let (mut control, events, _commands) = BackendControl::supervised();
        control.connection(Connection::Connected, None, "DISABLED");
        let mut latch = ArmLatch::default();
        let mut report = |latch: &ArmLatch, focused, reason| {
            super::nap(
                Instant::now() - super::POLL,
                &mut control,
                latch.armed,
                focused,
                Some(reason),
            );
            events.try_iter().last().unwrap()
        };
        assert!(!report(&latch, true, "DISABLED").enabled);
        assert_eq!(latch.observe(true, true), Some(true));
        assert!(report(&latch, true, "ENABLED").enabled);
        let paused = report(&latch, false, "PAUSED (PoE1 not focused)");
        assert!(paused.enabled);
        assert!(!paused.focused);
        assert_eq!(latch.observe(false, true), None);
        assert_eq!(latch.observe(true, true), Some(false));
        assert!(!report(&latch, true, "DISABLED").enabled);
    }

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
    fn profile_switch_disarms_without_accidental_rearm() {
        let mut latch = ArmLatch::default();
        assert_eq!(latch.observe(true, true), Some(true));
        assert!(latch.armed);
        assert!(latch.disable(true));
        assert!(!latch.armed);
        assert_eq!(latch.observe(true, true), None);
        assert_eq!(latch.observe(false, true), None);
        assert_eq!(latch.observe(true, true), Some(true));
    }

    #[test]
    fn launch_is_disarmed_and_unfocused_key_does_not_arm_on_refocus() {
        let mut latch = ArmLatch::default();
        assert!(!latch.armed);

        // An unfocused shortcut press is consumed, not replayed on focus gain.
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
        assert_ne!(
            TickOutcome::Unchanged,
            TickOutcome::RefreshFailed("stale state".to_owned())
        );
        assert_eq!(
            TickOutcome::Status("ENABLED"),
            TickOutcome::Status("ENABLED")
        );
    }
}
