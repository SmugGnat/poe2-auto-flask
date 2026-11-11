//! Low-cost PoE1 motion gate. GameHelper reads Actor.AnimationId at 0x248
//! and its league-start Quicksilver rule uses Animation.Run with Wait(1).
//! Only a continuously observed Run animation can satisfy this optional gate.
use std::io;
use std::time::{Duration, Instant};

use crate::entity::resolve_component;
use crate::process::Process;

use super::{invalid, Probe};

const ACTOR_ANIMATION_ID: usize = 0x248;
pub const RUN_ANIMATION_ID: i32 = 4;
pub const SAMPLE_MAX_GAP: Duration = Duration::from_millis(350);

#[derive(Debug, Default)]
pub struct MotionCache {
    actor: Option<usize>,
}

impl MotionCache {
    pub fn clear(&mut self) {
        self.actor = None;
    }

    pub fn sample(&mut self, process: &Process, probe: &Probe) -> io::Result<i32> {
        let actor = match self.actor {
            Some(actor) => actor,
            None => {
                let address = resolve_component(process, probe.local_player, "Actor")
                    .ok_or_else(|| invalid("PoE1 player Actor component unavailable"))?;
                self.actor = Some(address);
                address
            }
        };
        let id = process.read_i32(actor + ACTOR_ANIMATION_ID)?;
        if !(0..=1024).contains(&id) {
            return Err(invalid("PoE1 Actor animation ID out of range"));
        }
        Ok(id)
    }
}

/// Track successive Run samples, never treating a single frame as sustained motion.
/// Resets after an interruption or large sampling gap (focus loss / area load / pause).
#[derive(Debug, Default)]
pub struct RunTracker {
    started_at: Option<Instant>,
    previous_sample_at: Option<Instant>,
}

impl RunTracker {
    pub fn reset(&mut self) {
        self.started_at = None;
        self.previous_sample_at = None;
    }

    pub fn observe(&mut self, animation_id: Option<i32>, now: Instant, minimum: Duration) -> bool {
        if minimum.is_zero() {
            self.reset();
            return true;
        }
        if animation_id != Some(RUN_ANIMATION_ID) {
            self.reset();
            return false;
        }

        let gap_too_large = self.previous_sample_at.is_some_and(|previous| {
            now.checked_duration_since(previous)
                .is_none_or(|elapsed| elapsed > SAMPLE_MAX_GAP)
        });
        if self.started_at.is_none() || gap_too_large {
            self.started_at = Some(now);
        }
        self.previous_sample_at = Some(now);
        self.started_at
            .is_some_and(|start| now.duration_since(start) >= minimum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_running_reaches_threshold_then_resets_on_idle() {
        let t = Instant::now();
        let mut tracker = RunTracker::default();
        let min = Duration::from_millis(750);
        assert!(!tracker.observe(Some(RUN_ANIMATION_ID), t, min));
        assert!(!tracker.observe(Some(RUN_ANIMATION_ID), t + Duration::from_millis(300), min));
        assert!(!tracker.observe(Some(RUN_ANIMATION_ID), t + Duration::from_millis(600), min));
        assert!(tracker.observe(Some(RUN_ANIMATION_ID), t + Duration::from_millis(800), min));
        assert!(!tracker.observe(Some(0), t + Duration::from_millis(900), min));
        assert!(!tracker.observe(Some(RUN_ANIMATION_ID), t + Duration::from_millis(1000), min));
    }

    #[test]
    fn gap_and_unreadable_state_fail_closed() {
        let t = Instant::now();
        let mut tracker = RunTracker::default();
        let minimum = Duration::from_millis(500);
        assert!(!tracker.observe(Some(4), t, minimum));
        assert!(!tracker.observe(Some(4), t + Duration::from_millis(450), minimum));
        assert!(!tracker.observe(Some(4), t + Duration::from_millis(800), minimum));
        assert!(!tracker.observe(None, t + Duration::from_millis(900), minimum));
        assert!(!tracker.observe(Some(4), t + Duration::from_millis(1000), minimum));
    }

    #[test]
    fn disabled_gate_preserves_existing_behavior() {
        let mut tracker = RunTracker::default();
        assert!(tracker.observe(None, Instant::now(), Duration::ZERO));
    }
}
