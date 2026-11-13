//! Rate-limited diagnostics for game-state retries. A failed read never
//! authorizes input; this only explains persistent validation failures.
use std::time::{Duration, Instant};

pub const WARNING_GRACE: Duration = Duration::from_secs(15);
const CHANGED_REASON_COOLDOWN: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct FailureMonitor {
    since: Option<Instant>,
    last_warning: Option<Instant>,
    last_reason: Option<String>,
}

impl FailureMonitor {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn warn(&mut self, stage: &str, reason: &str) {
        if self.should_warn(Instant::now(), reason) {
            eprintln!(
                "WARNING: {stage} game-state validation failed for {}s: {reason}",
                WARNING_GRACE.as_secs()
            );
            eprintln!(
                "         If already in a playable area, game offsets/structures may need updating. Automation remains paused."
            );
        }
    }

    fn should_warn(&mut self, now: Instant, reason: &str) -> bool {
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) < WARNING_GRACE {
            return false;
        }
        if self.last_reason.as_deref() == Some(reason)
            || self
                .last_warning
                .is_some_and(|previous| now.duration_since(previous) < CHANGED_REASON_COOLDOWN)
        {
            return false;
        }
        self.last_warning = Some(now);
        self.last_reason = Some(reason.to_owned());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_transitions_do_not_warn_but_persistent_failures_do() {
        let start = Instant::now();
        let mut monitor = FailureMonitor::default();
        assert!(!monitor.should_warn(start, "player unavailable"));
        assert!(!monitor.should_warn(start + WARNING_GRACE - Duration::from_millis(1), "player unavailable"));
        assert!(monitor.should_warn(start + WARNING_GRACE, "player unavailable"));
        assert!(!monitor.should_warn(start + WARNING_GRACE + Duration::from_secs(60), "player unavailable"));
    }

    #[test]
    fn changed_errors_are_rate_limited_and_recovery_resets_the_grace_period() {
        let start = Instant::now();
        let mut monitor = FailureMonitor::default();
        assert!(!monitor.should_warn(start, "old layout"));
        assert!(monitor.should_warn(start + WARNING_GRACE, "old layout"));
        assert!(!monitor.should_warn(start + WARNING_GRACE + Duration::from_secs(1), "new layout"));
        assert!(monitor.should_warn(start + WARNING_GRACE + CHANGED_REASON_COOLDOWN, "new layout"));
        monitor.clear();
        assert!(!monitor.should_warn(start + WARNING_GRACE + CHANGED_REASON_COOLDOWN + Duration::from_secs(1), "new layout"));
    }
}
