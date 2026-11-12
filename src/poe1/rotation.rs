//! Deterministic, zero-allocation round-robin history across five physical slots.
//! A type never needs its own configured slot. The least recently used eligible
//! slot wins; unseen slots tie-break using remaining full uses and charge count.
use super::flasks::FLASK_SLOT_COUNT;
use super::types::FlaskType;

#[derive(Debug, Default)]
pub struct FlaskRotation {
    next_sequence: u64,
    last_used: [Option<(FlaskType, u64)>; FLASK_SLOT_COUNT],
}

impl FlaskRotation {
    pub fn reset(&mut self) {
        self.next_sequence = 0;
        self.last_used.fill(None);
    }

    /// Call only after successful SendInput.
    pub fn record(&mut self, slot: usize, kind: FlaskType) {
        if slot < FLASK_SLOT_COUNT {
            self.next_sequence = self.next_sequence.wrapping_add(1);
            if self.next_sequence == 0 {
                // Extremely unlikely wraparound, but preserve ordering invariant.
                self.reset();
                self.next_sequence = 1;
            }
            self.last_used[slot] = Some((kind, self.next_sequence));
        }
    }

    /// Zero denotes never used (highest priority), otherwise an increasing age.
    pub fn last_sequence(&self, slot: usize, kind: FlaskType) -> u64 {
        self.last_used
            .get(slot)
            .and_then(|entry| *entry)
            .filter(|(used_type, _)| *used_type == kind)
            .map_or(0, |(_, sequence)| sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_each_slot_and_type_without_allocating() {
        let mut r = FlaskRotation::default();
        assert_eq!(r.last_sequence(0, FlaskType::Life), 0);
        r.record(0, FlaskType::Life);
        assert_eq!(r.last_sequence(0, FlaskType::Life), 1);
        assert_eq!(r.last_sequence(0, FlaskType::Mana), 0);
        r.record(4, FlaskType::Life);
        assert_eq!(r.last_sequence(4, FlaskType::Life), 2);
        r.record(0, FlaskType::Quicksilver);
        assert_eq!(r.last_sequence(0, FlaskType::Life), 0);
        assert_eq!(r.last_sequence(0, FlaskType::Quicksilver), 3);
        r.reset();
        assert_eq!(r.last_sequence(4, FlaskType::Life), 0);
    }
}
