//! Shared, allocation-free action dispatch. A policy chooses the action,
//! provides its game-specific final validation, and owns all cooldown/rotation
//! bookkeeping. Only a successful SendInput returns a sent timestamp.
//!
//! Dispatch performs no game-memory reads itself. The validation callback
//! can request only the state required by the selected game's policy.
use std::io;
use std::time::Instant;

use crate::input::{is_process_foreground, send_key, KeyBinding};
use crate::process::Process;

/// Maximum number of flask slots any supported game can request.
const MAX_FLASK_SLOTS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Flask { slot: usize },
    Skill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionRequest {
    pub kind: ActionKind,
    pub binding: KeyBinding,
}

impl ActionRequest {
    pub fn flask(slot: usize, binding: KeyBinding) -> Self {
        Self {
            kind: ActionKind::Flask { slot },
            binding,
        }
    }

    pub fn skill(binding: KeyBinding) -> Self {
        Self {
            kind: ActionKind::Skill,
            binding,
        }
    }
}

#[derive(Debug)]
pub enum Dispatch<T> {
    Skipped,
    Sent { validated: T, sent_at: Instant },
}

#[derive(Debug)]
pub enum DispatchError {
    /// A game-specific memory or safety read became invalid before input.
    Validation(io::Error),
    /// The underlying input operation failed, or the request was malformed.
    Input(io::Error),
}

impl DispatchError {
    pub fn into_io(self) -> io::Error {
        match self {
            Self::Validation(error) | Self::Input(error) => error,
        }
    }
}

/// Focus is checked before AND after policy validation, so a slow memory
/// sample cannot authorize sending to a window that lost focus meanwhile.
/// The policy decides whether to re-sample memory, effects or chat.
pub fn dispatch<T>(
    process: &Process,
    action: ActionRequest,
    validate: impl FnOnce() -> io::Result<Option<T>>,
) -> Result<Dispatch<T>, DispatchError> {
    dispatch_with(
        action,
        || is_process_foreground(process.pid()),
        validate,
        send_key,
    )
}

fn dispatch_with<T>(
    action: ActionRequest,
    focused: impl Fn() -> bool,
    validate: impl FnOnce() -> io::Result<Option<T>>,
    send: impl FnOnce(KeyBinding) -> io::Result<()>,
) -> Result<Dispatch<T>, DispatchError> {
    match action.kind {
        ActionKind::Flask { slot } if slot < MAX_FLASK_SLOTS => {}
        ActionKind::Skill => {}
        ActionKind::Flask { .. } => {
            return Err(DispatchError::Input(io::Error::new(
                io::ErrorKind::InvalidInput,
                "flask action slot is outside the supported range",
            )));
        }
    }
    if !focused() {
        return Ok(Dispatch::Skipped);
    }
    let Some(validated) = validate().map_err(DispatchError::Validation)? else {
        return Ok(Dispatch::Skipped);
    };
    if !focused() {
        return Ok(Dispatch::Skipped);
    }
    send(action.binding).map_err(DispatchError::Input)?;
    Ok(Dispatch::Sent {
        validated,
        sent_at: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::input::KeyModifier;

    fn key() -> KeyBinding {
        KeyBinding::new(b'1' as u16, KeyModifier::None)
    }

    #[test]
    fn successful_dispatch_returns_context_and_send_timestamp() {
        let sends = Cell::new(0);
        let started = Instant::now();
        let result = dispatch_with(
            ActionRequest::flask(4, key()),
            || true,
            || Ok(Some(42u32)),
            |_| {
                sends.set(sends.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        match result {
            Dispatch::Sent { validated, sent_at } => {
                assert_eq!(validated, 42);
                assert!(sent_at >= started);
            }
            Dispatch::Skipped => panic!("valid action was skipped"),
        }
        assert_eq!(sends.get(), 1);
    }

    #[test]
    fn unfocused_game_never_runs_validation_or_sends() {
        let checks = Cell::new(0);
        let result = dispatch_with(
            ActionRequest::flask(0, key()),
            || false,
            || {
                checks.set(checks.get() + 1);
                Ok(Some(()))
            },
            |_| panic!("must not send when game is unfocused"),
        )
        .unwrap();
        assert!(matches!(result, Dispatch::Skipped));
        assert_eq!(checks.get(), 0);
    }

    #[test]
    fn denied_policy_action_never_sends() {
        let result = dispatch_with(
            ActionRequest::flask(1, key()),
            || true,
            || Ok::<_, io::Error>(None::<()>),
            |_| panic!("must not send when policy denies"),
        )
        .unwrap();
        assert!(matches!(result, Dispatch::Skipped));
    }

    #[test]
    fn losing_focus_during_validation_prevents_input() {
        let calls = Cell::new(0);
        let result = dispatch_with(
            ActionRequest::flask(0, key()),
            || {
                calls.set(calls.get() + 1);
                calls.get() == 1
            },
            || Ok(Some(())),
            |_| panic!("must not send after focus loss"),
        )
        .unwrap();
        assert!(matches!(result, Dispatch::Skipped));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn validation_failure_is_distinct_from_input_failure() {
        let result = dispatch_with(
            ActionRequest::flask(0, key()),
            || true,
            || Err::<Option<()>, _>(io::Error::new(io::ErrorKind::Interrupted, "stale state")),
            |_| panic!("must not send on validation error"),
        );
        assert!(matches!(result, Err(DispatchError::Validation(_))));

        let result = dispatch_with(
            ActionRequest::flask(0, key()),
            || true,
            || Ok(Some(())),
            |_| Err(io::Error::other("SendInput failed")),
        );
        assert!(matches!(result, Err(DispatchError::Input(_))));
    }

    #[test]
    fn flask_and_skill_both_dispatch_without_consuming_the_other() {
        let sent = Cell::new(0);
        for action in [ActionRequest::flask(0, key()), ActionRequest::skill(key())] {
            let result = dispatch_with(
                action,
                || true,
                || Ok(Some(())),
                |_| {
                    sent.set(sent.get() + 1);
                    Ok(())
                },
            )
            .unwrap();
            assert!(matches!(result, Dispatch::Sent { .. }));
        }
        assert_eq!(sent.get(), 2);
        let mut called = false;
        let ignored = dispatch_with(
            ActionRequest::flask(0, key()),
            || true,
            || Ok::<_, io::Error>(None::<()>),
            |_| {
                called = true;
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(ignored, Dispatch::Skipped));
        assert!(!called);
        let permitted = dispatch_with(
            ActionRequest::skill(key()),
            || true,
            || Ok(Some(())),
            |_| {
                called = true;
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(permitted, Dispatch::Sent { .. }));
        assert!(called);
    }

    #[test]
    fn two_independent_skills_and_a_flask_can_dispatch_in_one_decision_pass() {
        let sends = Cell::new(0);
        for action in [
            ActionRequest::flask(1, key()),
            ActionRequest::skill(KeyBinding::new(b'R' as u16, KeyModifier::None)),
            ActionRequest::skill(KeyBinding::new(b'T' as u16, KeyModifier::None)),
        ] {
            let result = dispatch_with(
                action,
                || true,
                || Ok(Some(())),
                |_| {
                    sends.set(sends.get() + 1);
                    Ok(())
                },
            )
            .unwrap();
            assert!(matches!(result, Dispatch::Sent { .. }));
        }
        assert_eq!(sends.get(), 3);
    }

    #[test]
    fn malformed_flask_slot_never_runs_validation_or_sends() {
        let result: Result<Dispatch<()>, DispatchError> = dispatch_with(
            ActionRequest::flask(5, key()),
            || true,
            || panic!("invalid slot must not be validated"),
            |_| panic!("invalid slot must not send"),
        );
        assert!(matches!(result, Err(DispatchError::Input(_))));
    }
}
