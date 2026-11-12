//! PoE2's bounded, known-offset-first layout search used at attach/refresh.
//! Both area and player-state readers use exactly the same candidate ordering.
pub(super) fn prioritized_offsets(
    center: usize,
    start: usize,
    end: usize,
    step: usize,
) -> Vec<usize> {
    let mut offsets = Vec::new();
    if center >= start && center <= end {
        offsets.push(center);
    }

    let mut distance = step;
    while center.saturating_sub(distance) >= start || center.saturating_add(distance) <= end {
        if let Some(lower) = center.checked_sub(distance) {
            if lower >= start && lower <= end {
                offsets.push(lower);
            }
        }
        if let Some(upper) = center.checked_add(distance) {
            if upper >= start && upper <= end {
                offsets.push(upper);
            }
        }
        distance = match distance.checked_add(step) {
            Some(next) => next,
            None => break,
        };
    }

    offsets
}
