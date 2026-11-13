//! PoE1 policy: one demand-driven game snapshot, then independent
//! flask/skill candidates. Skill-specific rules never live in this scheduler.
use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

use crate::action::{dispatch, ActionRequest, Dispatch, DispatchError};
use crate::config::{Config, ConfigManager, ConfigReload};
use crate::engine::{GameAutomationPolicy, TickOutcome};
use crate::input::{is_process_foreground, KeyBinding};
use crate::process::Process;
use crate::session::GameSession;

use super::activation::ActivationGuard;
use super::bindings::{BindingManager, BindingState};
use super::decision::{
    blocks_grace_period, choose_action_rotating_guarded, enabled_demand, needs_nearby_guarded,
};
use super::flasks::FlaskSnapshot;
use super::movement::{RunTracker, RUN_ANIMATION_ID};
use super::nearby::NearbySnapshot;
use super::rotation::FlaskRotation;
use super::skill_rules::{
    configured_rules, evaluate, nearby_conditions_met, observations_allow, preconditions_met,
    requires_nearby, Condition, RuleDecision, SkillAttemptGuard, SkillRule,
};
use super::skills::SkillId;
use super::state::{Preflight, TickState};
use super::types::FlaskType;

pub fn run(process: &Process, session: GameSession, manager: &mut ConfigManager) -> io::Result<()> {
    let policy = Policy::new(manager.current())?;
    crate::engine::run(process, session, manager, policy)
}

struct Policy {
    bindings: BindingManager,
    activation_guard: ActivationGuard,
    rotation: FlaskRotation,
    run_tracker: RunTracker,
    skill_rules: Vec<SkillRule>,
    skill_attempts: BTreeMap<SkillId, SkillAttemptGuard>,
    flask_read_warning: bool,
}

struct FlaskCandidate {
    slot: usize,
    flasks: FlaskSnapshot,
    proximity: Option<NearbySnapshot>,
    quicksilver_ready: bool,
    moving_ms: u64,
}

impl Policy {
    fn new(config: &Config) -> io::Result<Self> {
        Ok(Self {
            bindings: BindingManager::load()?,
            activation_guard: ActivationGuard::default(),
            rotation: FlaskRotation::default(),
            run_tracker: RunTracker::default(),
            skill_rules: configured_rules(&config.poe1.skills)?,
            skill_attempts: BTreeMap::new(),
            flask_read_warning: false,
        })
    }

    fn guard_ready(&self, skill: SkillId, now: Instant) -> bool {
        self.skill_attempts
            .get(&skill)
            .is_none_or(|guard| guard.ready(now))
    }

    fn mark_skill(&mut self, skill: SkillId, sent_at: Instant) {
        self.skill_attempts.entry(skill).or_default().mark(sent_at);
    }

    fn defer_skill(&mut self, skill: SkillId, started: Instant) {
        self.skill_attempts.entry(skill).or_default().defer(started);
    }

    fn flask_key(&self, slot: usize) -> io::Result<KeyBinding> {
        match self.bindings.current() {
            BindingState::Ready(keys) => Ok(keys[slot]),
            BindingState::Unavailable(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PoE1 flask bindings changed during decision",
            )),
        }
    }
}

impl GameAutomationPolicy for Policy {
    fn banner(&self, config: &Config) {
        println!("Connected to Path of Exile 1.");
        println!("Settings:\n{}", config.poe1_summary());
        println!("{}", binding_label(self.bindings.current()));
        warn_key_conflicts(&self.skill_rules, self.bindings.current());
        println!("Starts DISARMED. Press F11 while PoE1 is focused to arm/disarm. Ctrl+C exits.");
    }

    fn on_config_reload(&mut self, event: ConfigReload, config: &Config) {
        match event {
            ConfigReload::Reloaded => {
                // ConfigManager validated these presets before applying the reload.
                if let Ok(rules) = configured_rules(&config.poe1.skills) {
                    self.skill_rules = rules;
                }
                self.run_tracker.reset();
                self.rotation.reset();
                println!("PoE1 config reloaded:\n{}", config.poe1_summary());
                warn_key_conflicts(&self.skill_rules, self.bindings.current());
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
            warn_key_conflicts(&self.skill_rules, self.bindings.current());
        }
    }

    fn bindings_pause(&self) -> Option<&'static str> {
        if !self.skill_rules.is_empty() {
            return None;
        }
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
        self.flask_read_warning = false;
    }

    fn refresh(&mut self, _process: &Process, _session: &GameSession) -> io::Result<()> {
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
        let mut state = match TickState::sample(session, process, config.general.enable_in_hideout)
        {
            Ok(value) => value,
            Err(error) => return Ok(TickOutcome::RefreshFailed(error.to_string())),
        };
        if !state.area_allowed {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("PAUSED (town or hideout)"));
        }
        if state.vitals.health.current <= 0 {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("PAUSED (dead)"));
        }

        let flask_demand = matches!(self.bindings.current(), BindingState::Ready(_))
            && enabled_demand(config, state.vitals);
        let skill_demand = self.skill_rules.iter().any(|rule| {
            self.guard_ready(rule.id, started) && preconditions_met(rule, state.vitals)
        });
        if !flask_demand && !skill_demand {
            self.run_tracker.reset();
            return Ok(TickOutcome::Status("ARMED"));
        }

        // Shared state readers run once per tick, regardless of which policy
        // requests a flask, a skill, or both.
        match state.chat(session, process) {
            Ok(true) => return Ok(TickOutcome::Status("PAUSED (chat or focus)")),
            Ok(false) => {}
            Err(error) => return Ok(TickOutcome::RefreshFailed(error.to_string())),
        }
        let effects = match state.effects(session, process) {
            Ok(value) => value,
            Err(error) => return Ok(TickOutcome::RefreshFailed(error.to_string())),
        };
        if blocks_grace_period(effects) {
            return Ok(TickOutcome::Status("ARMED"));
        }

        let flask = if flask_demand {
            match self.select_flask(process, session, config, started, &mut state) {
                Ok(value) => {
                    if self.flask_read_warning {
                        println!("PoE1 flask reader recovered.");
                        self.flask_read_warning = false;
                    }
                    value
                }
                Err(error) => {
                    // Interrupted player identity, memory I/O failures and all
                    // unclassified errors remain session-wide refresh events.
                    // Only a flask-local malformed inventory may be skipped,
                    // and only after a fresh player/vitals verification.
                    if !can_skip_flask_read_error(&error, skill_demand) {
                        return Ok(TickOutcome::RefreshFailed(error.to_string()));
                    }
                    if let Err(error) = session.poe1_verify_vitals(process) {
                        return Ok(TickOutcome::RefreshFailed(error.to_string()));
                    }
                    if !self.flask_read_warning {
                        eprintln!(
                            "PoE1 flask data unavailable ({error}); flask input suppressed, skill rules remain active."
                        );
                        self.flask_read_warning = true;
                    }
                    None
                }
            }
        } else {
            self.run_tracker.reset();
            None
        };

        // Only rules with passing cheap conditions enter the Actor request.
        // A single scan resolves readiness for *all* requested skills.
        let mut candidates = Vec::new();
        let observed_effects = state.effects(session, process)?.clone();
        for rule in &self.skill_rules {
            if !self.guard_ready(rule.id, started)
                || !observations_allow(rule, state.vitals, &observed_effects)
            {
                continue;
            }
            if let BindingState::Ready(flask_keys) = self.bindings.current() {
                if flask_keys.contains(&rule.key) {
                    continue;
                }
            }
            // A proximity-gated skill fails closed when the census is
            // unavailable. TickState reuses a flask's existing census.
            if requires_nearby(rule) {
                let nearby = state.nearby(session, process).ok();
                if !nearby_conditions_met(rule, nearby) {
                    continue;
                }
            }
            candidates.push(rule.clone());
        }
        let requested: Vec<SkillId> = candidates
            .iter()
            .filter(|rule| requires_skill_readiness(rule))
            .map(|rule| rule.id)
            .collect();
        let statuses = if requested.is_empty() {
            None
        } else {
            match state.skills(session, process, &requested) {
                Ok(value) => Some(value.clone()),
                Err(error) => {
                    // All affected skill rules fail closed; flask candidates
                    // remain valid and do not get delayed by this failure.
                    eprintln!("PoE1 skill snapshot unavailable: {error}");
                    for id in &requested {
                        self.defer_skill(*id, started);
                    }
                    None
                }
            }
        };
        let mut skills = Vec::new();
        for rule in candidates {
            let readiness = statuses.as_ref().and_then(|all| all.get(&rule.id)).copied();
            if evaluate(
                &rule,
                state.vitals,
                &observed_effects,
                state.observed_nearby(),
                readiness,
            ) == RuleDecision::Eligible
            {
                skills.push(rule);
            }
        }

        if flask.is_none() && skills.is_empty() {
            return Ok(TickOutcome::Status("ARMED"));
        }

        // A single fresh state check protects *both* candidates. Each
        // dispatched key still performs its own immediate focus checks.
        let mut preflight = match Preflight::sample(session, process) {
            Ok(value) => value,
            Err(error) => return Ok(TickOutcome::RefreshFailed(error.to_string())),
        };
        if preflight.chat || preflight.vitals.health.current <= 0 {
            return Ok(TickOutcome::Status("PAUSED (chat or focus)"));
        }
        if blocks_grace_period(&preflight.effects) {
            return Ok(TickOutcome::Status("ARMED"));
        }

        if let Some(candidate) = flask {
            let outcome =
                self.send_flask(process, session, config, started, &preflight, candidate)?;
            if outcome != TickOutcome::Status("ARMED") {
                // Focus loss and stale shared state legitimately halt the tick.
                return Ok(outcome);
            }
        }
        let requested: Vec<SkillId> = skills
            .iter()
            .filter(|rule| requires_skill_readiness(rule))
            .map(|rule| rule.id)
            .collect();
        let fresh_statuses = if requested.is_empty() {
            None
        } else {
            match preflight.skills(session, process, &requested) {
                Ok(value) => Some(value.clone()),
                Err(error) => {
                    eprintln!("PoE1 skill preflight unavailable: {error}");
                    for id in &requested {
                        self.defer_skill(*id, started);
                    }
                    None
                }
            }
        };
        for rule in skills {
            let readiness = fresh_statuses
                .as_ref()
                .and_then(|all| all.get(&rule.id))
                .copied();
            if evaluate(
                &rule,
                preflight.vitals,
                &preflight.effects,
                state.observed_nearby(),
                readiness,
            ) != RuleDecision::Eligible
            {
                continue;
            }
            match dispatch(process, ActionRequest::skill(rule.key), || Ok(Some(()))) {
                Ok(Dispatch::Sent { sent_at, .. }) => {
                    self.mark_skill(rule.id, sent_at);
                    println!("{} -> {}", rule.id.label(), rule.key.label());
                }
                Ok(Dispatch::Skipped) if !is_process_foreground(process.pid()) => {
                    return Ok(TickOutcome::Status("PAUSED (chat or focus)"));
                }
                Ok(Dispatch::Skipped) => {}
                Err(DispatchError::Validation(error)) => {
                    return Ok(TickOutcome::RefreshFailed(error.to_string()));
                }
                Err(DispatchError::Input(error)) => {
                    self.defer_skill(rule.id, started);
                    eprintln!("PoE1 {} input failed: {error}", rule.id.label());
                }
            }
        }
        Ok(TickOutcome::Status("ARMED"))
    }
}

impl Policy {
    fn select_flask(
        &mut self,
        process: &Process,
        session: &mut GameSession,
        config: &Config,
        started: Instant,
        state: &mut TickState,
    ) -> io::Result<Option<FlaskCandidate>> {
        // Only Quicksilver needs animation; do not read it for other flasks.
        let moving_ms = config.poe1.quicksilver_min_run_ms;
        let quicksilver_ready = if moving_ms > 0 && config.poe1.utility.quicksilver {
            let animation = state.animation(session, process).ok();
            self.run_tracker
                .observe(animation, started, Duration::from_millis(moving_ms))
        } else {
            self.run_tracker.reset();
            true
        };
        let flasks = state.flasks(session, process)?.clone();
        let mut effects = state.effects(session, process)?.clone();
        self.activation_guard
            .apply_to(started, state.vitals, &flasks, &mut effects);
        let proximity = if needs_nearby_guarded(
            config,
            state.vitals,
            &flasks,
            &effects,
            quicksilver_ready,
            !self.activation_guard.life_input_pending(),
        ) {
            state.nearby(session, process).ok().cloned()
        } else {
            None
        };
        let chosen = choose_action_rotating_guarded(
            config,
            state.vitals,
            &flasks,
            &effects,
            quicksilver_ready,
            proximity.as_ref(),
            &self.rotation,
            !self.activation_guard.life_input_pending(),
        );
        Ok(chosen.map(|slot| FlaskCandidate {
            slot,
            flasks,
            proximity,
            quicksilver_ready,
            moving_ms,
        }))
    }

    fn send_flask(
        &mut self,
        process: &Process,
        session: &mut GameSession,
        config: &Config,
        started: Instant,
        preflight: &Preflight,
        candidate: FlaskCandidate,
    ) -> io::Result<TickOutcome> {
        let slot = candidate.slot;
        let key = self.flask_key(slot)?;
        let quicksilver_motion_ok = if candidate.moving_ms > 0
            && candidate.flasks.slots[slot]
                .as_ref()
                .is_some_and(|flask| flask.flask_type == FlaskType::Quicksilver)
        {
            candidate.quicksilver_ready
                && session.poe1_verify_animation(process).ok() == Some(RUN_ANIMATION_ID)
        } else {
            true
        };
        if !quicksilver_motion_ok {
            self.run_tracker.reset();
        }
        let mut effects = preflight.effects.clone();
        self.activation_guard
            .apply_to(started, preflight.vitals, &candidate.flasks, &mut effects);
        let still_selected = choose_action_rotating_guarded(
            config,
            preflight.vitals,
            &candidate.flasks,
            &effects,
            candidate.quicksilver_ready,
            candidate.proximity.as_ref(),
            &self.rotation,
            !self.activation_guard.life_input_pending(),
        ) == Some(slot);
        if !quicksilver_motion_ok || !still_selected {
            return Ok(TickOutcome::Status("ARMED"));
        }

        match dispatch(process, ActionRequest::flask(slot, key), || {
            Ok(Some(preflight.vitals))
        }) {
            Ok(Dispatch::Sent { validated, sent_at }) => {
                if let Some(flask) = &candidate.flasks.slots[slot] {
                    self.activation_guard.mark(
                        slot,
                        flask.flask_type,
                        sent_at,
                        validated,
                        &candidate.flasks,
                    );
                    self.rotation.record(slot, flask.flask_type);
                    println!(
                        "{} flask (slot {}) -> {} | charges {} | effective cost {}",
                        flask.flask_type.name(),
                        slot + 1,
                        key.label(),
                        flask.current_charges,
                        flask.effective_per_use.unwrap_or_default()
                    );
                }
            }
            Ok(Dispatch::Skipped) if !is_process_foreground(process.pid()) => {
                return Ok(TickOutcome::Status("PAUSED (chat or focus)"));
            }
            Ok(Dispatch::Skipped) => {}
            Err(DispatchError::Validation(error)) => {
                return Ok(TickOutcome::RefreshFailed(error.to_string()));
            }
            Err(DispatchError::Input(error)) => return Err(error),
        }
        Ok(TickOutcome::Status("ARMED"))
    }
}

/// Only malformed flask-inventory observations may be isolated from an
/// otherwise healthy player/session. A fresh identity check is required at
/// the call site; I/O failures and identity transitions trigger Refresh.
fn can_skip_flask_read_error(error: &io::Error, skill_demand: bool) -> bool {
    skill_demand && error.kind() == io::ErrorKind::InvalidData
}

/// Report conflicting keys only when configuration/bindings actually change,
/// never every 100 ms. Conflicting skills are already denied at selection.
fn conflicting_skill_bindings(
    rules: &[SkillRule],
    bindings: &BindingState,
) -> Vec<(SkillId, usize, KeyBinding)> {
    let BindingState::Ready(keys) = bindings else {
        return Vec::new();
    };
    rules
        .iter()
        .flat_map(|rule| {
            keys.iter().enumerate().filter_map(move |(slot, binding)| {
                (*binding == rule.key).then_some((rule.id, slot + 1, rule.key))
            })
        })
        .collect()
}

fn warn_key_conflicts(rules: &[SkillRule], bindings: &BindingState) {
    for (skill, slot, key) in conflicting_skill_bindings(rules, bindings) {
        eprintln!(
            "PoE1 key conflict: {} uses {} (flask slot {slot}); suppressing skill input until the bindings change.",
            skill.label(),
            key.label()
        );
    }
}

fn requires_skill_readiness(rule: &SkillRule) -> bool {
    rule.conditions
        .iter()
        .any(|condition| matches!(condition, Condition::SkillUsable))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::KeyModifier;

    #[test]
    fn local_flask_error_is_skippable_only_when_skill_policy_needs_it() {
        let inventory = io::Error::new(io::ErrorKind::InvalidData, "malformed flask inventory");
        let player_changed = io::Error::new(io::ErrorKind::Interrupted, "player changed");
        let io_error = io::Error::other("unreadable process memory");
        assert!(can_skip_flask_read_error(&inventory, true));
        assert!(!can_skip_flask_read_error(&inventory, false));
        assert!(!can_skip_flask_read_error(&player_changed, true));
        assert!(!can_skip_flask_read_error(&io_error, true));
    }

    #[test]
    fn detects_skill_to_flask_conflicts_without_blocking_unrelated_skills() {
        let config = Config::default();
        let mut settings = config.poe1.skills.clone();
        settings.molten_shell.enabled = true;
        settings.molten_shell.key = "R".into();
        settings.blood_rage.enabled = true;
        settings.blood_rage.key = "T".into();
        let rules = configured_rules(&settings).unwrap();
        let mut keys = [KeyBinding::new(b'1' as u16, KeyModifier::None); 5];
        keys[2] = KeyBinding::new(b'R' as u16, KeyModifier::None);
        let warnings = conflicting_skill_bindings(&rules, &BindingState::Ready(keys));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].0, SkillId::MoltenShell);
        assert_eq!(warnings[0].1, 3);
        assert!(
            conflicting_skill_bindings(&rules, &BindingState::Unavailable("unknown".into()))
                .is_empty()
        );
    }
}
