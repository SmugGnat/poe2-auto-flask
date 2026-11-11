//! PoE1 offset recovery is intentionally narrow and only runs while
//! attaching / refreshing a game session. Never search from the 100ms tick.
//! Do not infer offsets from "readable pointers" without independent checks.
use super::offsets::{AREA_PLAYER_INFO, IN_GAME_AREA, IN_GAME_WORLD};

const AREA_WINDOW: usize = 0x20;
const PLAYER_INFO_WINDOW: usize = 0x20;
const WORLD_WINDOW: usize = 0x20;
const VITAL_WINDOW: usize = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub area: usize,
    pub world: usize,
    pub player_info: usize,
    pub vital_delta: isize,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            area: IN_GAME_AREA,
            world: IN_GAME_WORLD,
            player_info: AREA_PLAYER_INFO,
            vital_delta: 0,
        }
    }
}

/// Nearest-first, fixed-size range around a known offset. No heap allocation,
/// exponential probing, or searching arbitrary process memory.
pub fn nearby_offsets(center: usize, window: usize, step: usize) -> impl Iterator<Item = usize> {
    (0..=window / step).flat_map(move |distance| {
        let delta = distance * step;
        let lower = center.checked_sub(delta);
        let upper = (distance != 0).then(|| center.checked_add(delta)).flatten();
        [lower, upper].into_iter().flatten()
    })
}

pub fn area_offsets() -> impl Iterator<Item = usize> {
    nearby_offsets(IN_GAME_AREA, AREA_WINDOW, 8)
}

pub fn player_info_offsets() -> impl Iterator<Item = usize> {
    nearby_offsets(AREA_PLAYER_INFO, PLAYER_INFO_WINDOW, 8)
}

pub fn world_offsets() -> impl Iterator<Item = usize> {
    nearby_offsets(IN_GAME_WORLD, WORLD_WINDOW, 8)
}

pub fn vital_deltas() -> impl Iterator<Item = isize> {
    nearby_offsets(VITAL_WINDOW, VITAL_WINDOW, 4)
        .map(|offset| offset as isize - VITAL_WINDOW as isize)
}

/// Pure candidate resolver. If two distinct candidates satisfy every strict
/// validator, do not guess which one is the actual layout.
#[cfg(test)]
pub fn unique_valid<T: Copy + Eq>(
    candidates: impl IntoIterator<Item = T>,
    mut validate: impl FnMut(T) -> bool,
) -> Option<T> {
    let mut found = None;
    for candidate in candidates {
        if !validate(candidate) {
            continue;
        }
        if let Some(previous) = found {
            if previous != candidate {
                return None;
            }
        } else {
            found = Some(candidate);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_windows_are_bounded_and_prioritize_known_offsets() {
        assert_eq!(area_offsets().count(), 9);
        assert_eq!(player_info_offsets().count(), 9);
        assert_eq!(world_offsets().count(), 9);
        assert_eq!(vital_deltas().count(), 17);
        assert_eq!(area_offsets().next(), Some(IN_GAME_AREA));
        assert_eq!(vital_deltas().take(5).collect::<Vec<_>>(), [0, -4, 4, -8, 8]);
        assert!(area_offsets().all(|offset| offset.abs_diff(IN_GAME_AREA) <= AREA_WINDOW));
        assert!(player_info_offsets().all(|offset| offset % 8 == 0));
        assert!(world_offsets().all(|offset| offset % 8 == 0));
    }

    #[test]
    fn duplicate_or_ambiguous_candidates_never_authorize_a_repair() {
        assert_eq!(unique_valid([10, 11, 12], |value| value == 11), Some(11));
        assert_eq!(unique_valid([10, 11, 12], |value| value >= 11), None);
        assert_eq!(unique_valid([10, 11, 12], |_| false), None);
        assert_eq!(unique_valid([10, 11, 11], |value| value == 11), Some(11));
    }
}
