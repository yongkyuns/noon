use noon_core::{ObjectId, Rect, Vec2};
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex, SpatialIndexConfig};

const OBJECT_COUNT: usize = 2_048;
const QUERY_COUNT: usize = 256;

#[derive(Clone, Copy, Debug)]
struct ReferenceEntry {
    slot: ExecutionSlotId,
    bounds: Rect,
    painter_order: u64,
}

#[test]
fn indexed_queries_match_bruteforce_reference_corpus() {
    let config = SpatialIndexConfig {
        cell_size: 1.0,
        max_cells_per_object: 16,
        max_cells_per_query: 4_096,
    };
    let mut index = ExecutionSpatialIndex::new(config);
    let mut reference = Vec::with_capacity(OBJECT_COUNT);

    for object_index in 0..OBJECT_COUNT {
        let slot = ExecutionSlotId::new(object_index as u32, 0);
        let bounds = corpus_bounds(object_index);
        let painter_order = ((object_index * 7_919) % 4_096) as u64;
        index.upsert_bounds(
            slot,
            ObjectId::new(object_index as u64),
            bounds,
            painter_order,
        );
        reference.push(Some(ReferenceEntry {
            slot,
            bounds,
            painter_order,
        }));
    }

    assert_queries_match(&index, &reference);

    // Exercise incremental refits and removals before repeating the same independent
    // oracle comparison. Some entries deliberately exceed max_cells_per_object so
    // both ordinary cell residency and global-entry residency participate.
    for object_index in (0..OBJECT_COUNT).step_by(23) {
        let Some(entry) = reference[object_index].as_mut() else {
            continue;
        };
        let shift = Vec2::new(6.5 + (object_index % 5) as f32, -4.25);
        entry.bounds = Rect::new(entry.bounds.min + shift, entry.bounds.max + shift);
        entry.painter_order = entry.painter_order.saturating_add(8_192);
        index.upsert_bounds(
            entry.slot,
            ObjectId::new(object_index as u64),
            entry.bounds,
            entry.painter_order,
        );
    }
    for object_index in (11..OBJECT_COUNT).step_by(31) {
        let Some(entry) = reference[object_index].take() else {
            continue;
        };
        index.remove_slot(entry.slot);
    }

    assert_queries_match(&index, &reference);
}

fn assert_queries_match(index: &ExecutionSpatialIndex, reference: &[Option<ReferenceEntry>]) {
    for query_index in 0..QUERY_COUNT {
        let point = query_point(query_index);
        let indexed = index.hit_test(point);
        let expected = brute_force(reference, Rect::new(point, point), true);
        assert_eq!(
            indexed.slots(),
            expected,
            "point query {query_index} diverged from brute-force oracle"
        );
        assert_eq!(indexed.stats().full_scan_fallbacks, 0);

        let bounds = query_bounds(query_index);
        let indexed = index.query_rect(bounds);
        let expected = brute_force(reference, bounds, false);
        assert_eq!(
            indexed.slots(),
            expected,
            "viewport query {query_index} diverged from brute-force oracle"
        );
        assert_eq!(indexed.stats().full_scan_fallbacks, 0);
    }
}

fn brute_force(
    reference: &[Option<ReferenceEntry>],
    query: Rect,
    topmost_first: bool,
) -> Vec<ExecutionSlotId> {
    let mut matches = reference
        .iter()
        .flatten()
        .copied()
        .filter(|entry| intersects(entry.bounds, query))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        left.painter_order
            .cmp(&right.painter_order)
            .then_with(|| left.slot.cmp(&right.slot))
    });
    if topmost_first {
        matches.reverse();
    }
    matches.into_iter().map(|entry| entry.slot).collect()
}

fn corpus_bounds(index: usize) -> Rect {
    let x = ((index * 37) % 128) as f32 * 1.75 - 112.0;
    let y = ((index * 53) % 96) as f32 * 1.5 - 72.0;
    if index.is_multiple_of(97) {
        // Deliberately force global residency without making normal queries fall back.
        return Rect::new(Vec2::new(x - 3.0, y - 3.0), Vec2::new(x + 3.0, y + 3.0));
    }
    let width = 0.2 + ((index * 13) % 11) as f32 * 0.17;
    let height = 0.25 + ((index * 17) % 9) as f32 * 0.19;
    Rect::new(Vec2::new(x, y), Vec2::new(x + width, y + height))
}

fn query_point(index: usize) -> Vec2 {
    Vec2::new(
        ((index * 29) % 180) as f32 - 90.0 + 0.375,
        ((index * 47) % 120) as f32 - 60.0 + 0.625,
    )
}

fn query_bounds(index: usize) -> Rect {
    let center = query_point(index);
    let half_width = 0.5 + (index % 7) as f32 * 0.35;
    let half_height = 0.5 + (index % 5) as f32 * 0.4;
    Rect::new(
        Vec2::new(center.x - half_width, center.y - half_height),
        Vec2::new(center.x + half_width, center.y + half_height),
    )
}

fn intersects(left: Rect, right: Rect) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
}
