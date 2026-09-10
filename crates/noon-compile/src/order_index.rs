//! Shared stable-slot ordering algorithms for compiled and effective runtime indices.
use std::{cmp::Ordering, ops::Range};

/// Move one live row and repair inverse ranks only across the affected interval.
/// `order` and `ranks` must describe the same stable-slot permutation.
pub fn move_order_row(
    order: &mut [u32],
    ranks: &mut [Option<u32>],
    position: usize,
    destination: usize,
) -> Range<usize> {
    if position == destination {
        return position..position;
    }
    let first = position.min(destination);
    let last = position.max(destination);
    if position < destination {
        order[first..=last].rotate_left(1);
    } else {
        order[first..=last].rotate_right(1);
    }
    for rank in first..=last {
        ranks[order[rank] as usize] = Some(rank as u32);
    }
    first..last + 1
}

/// Restore a changed row in a sorted stable-slot index using O(log N) comparisons
/// and O(affected interval) rank writes. The key provider owns the actual values;
/// the index is disposable derived ordering, never property state.
pub fn reposition_order_row(
    order: &mut [u32],
    ranks: &mut [Option<u32>],
    index: u32,
    compare: impl Fn(u32, u32) -> Ordering,
) -> Range<usize> {
    let position = ranks[index as usize].expect("live row has an inverse rank") as usize;
    let mut low = 0;
    let mut high = order.len() - 1;
    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = order[middle + usize::from(middle >= position)];
        if compare(candidate, index) == Ordering::Less {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    move_order_row(order, ranks, position, low)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reordering_repairs_only_crossed_ranks_and_equal_keys_are_stable() {
        let mut order = vec![0, 1, 2, 3, 4];
        let mut ranks = (0..5).map(Some).collect::<Vec<_>>();
        let keys = [0, 1, 4, 3, 4];
        assert_eq!(
            reposition_order_row(&mut order, &mut ranks, 2, |a, b| keys[a as usize]
                .cmp(&keys[b as usize])
                .then(a.cmp(&b))),
            2..4
        );
        assert_eq!(order, [0, 1, 3, 2, 4]);
        assert_eq!(ranks, [Some(0), Some(1), Some(3), Some(2), Some(4)]);
        assert!(
            reposition_order_row(&mut order, &mut ranks, 2, |a, b| keys[a as usize]
                .cmp(&keys[b as usize])
                .then(a.cmp(&b)))
            .is_empty()
        );
    }
}
