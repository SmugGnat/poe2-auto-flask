//! Cross-game, read-only values. These represent what automation needs,
//! not the in-process memory layout of either Path of Exile client.
//! Each game adapter independently validates and constructs these values.
use std::io;
use crate::process::Process;

const MAX_VITAL_TOTAL: i32 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitalPool {
    pub current: i32,
    /// Raw VitalStruct total before reservations are removed.
    pub total: i32,
    pub reserved_flat: i32,
    /// Reservation fraction in basis points: 10_000 = 100%.
    pub reserved_fraction: i32,
}

impl VitalPool {
    pub fn usable_max(self) -> i32 {
        let reserved_percent =
            ((self.total as i64 * self.reserved_fraction as i64) + 9_999) / 10_000;
        (self.total as i64 - reserved_percent - self.reserved_flat as i64).clamp(0, i32::MAX as i64)
            as i32
    }

    pub fn percentage(self) -> Option<f32> {
        let usable_max = self.usable_max();
        (usable_max > 0).then_some(100.0 * self.current as f32 / usable_max as f32)
    }

    pub(crate) fn looks_valid(self) -> bool {
        if self.total <= 0 || self.total > MAX_VITAL_TOTAL {
            return false;
        }
        if self.current < -self.total || self.current > self.total + 1 {
            return false;
        }
        if self.reserved_flat < 0 || self.reserved_flat > self.total {
            return false;
        }
        (0..=10_000).contains(&self.reserved_fraction)
    }

    pub(crate) fn looks_valid_or_zero(self) -> bool {
        (self.total == 0
            && self.current == 0
            && self.reserved_flat == 0
            && (0..=10_000).contains(&self.reserved_fraction))
            || self.looks_valid()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveVitals {
    pub health: VitalPool,
    pub mana: Option<VitalPool>,
    pub energy_shield: Option<VitalPool>,
}

/// The common state sampled in a decision tick. Game-specific objects
/// (flask identities, effects, entities, animation, etc.) remain owned by
/// their individual adapters until there is a real shared consumer.
#[derive(Debug, Default)]
pub struct GameSnapshot {
    pub vitals: Option<LiveVitals>,
}

impl GameSnapshot {
    pub fn begin_tick(&mut self) {
        *self = Self::default();
    }
}

/// Small, statically dispatched adapter contract. Memory reads live in the
/// game adapters; the shared session only requests validated values.
pub trait GameStateReader {
    fn refresh(&mut self, process: &Process) -> io::Result<()>;
    fn read_vitals(&self, process: &Process) -> io::Result<LiveVitals>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_invalidates_across_ticks() {
        let mut snap = GameSnapshot {
            vitals: Some(LiveVitals {
                health: VitalPool {
                    current: 20, total: 100, reserved_flat: 0, reserved_fraction: 0
                },
                mana: None,
                energy_shield: None,
            }),
        };
        assert_eq!(snap.vitals.unwrap().health.current, 20);
        snap.begin_tick();
        assert!(snap.vitals.is_none());
    }

    #[test]
    fn shared_percentages_use_unreserved_pool() {
        let life = VitalPool {
            current: 300, total: 1000, reserved_flat: 200, reserved_fraction: 1000
        };
        assert_eq!(life.usable_max(), 700);
        assert_eq!(life.percentage(), Some(300.0 / 700.0 * 100.0));
    }
}
