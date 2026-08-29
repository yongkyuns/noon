use noon_core::{ObjectId, Rect, Vec2};
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex};

const REUSE_CYCLES: u32 = 1_000;
const PHYSICAL_SLOT: u32 = 7;

#[test]
fn stale_slot_generations_cannot_remove_or_alias_the_reused_spatial_leaf() {
    let mut index = ExecutionSpatialIndex::default();
    let bounds = Rect::new(Vec2::new(-0.5, -0.5), Vec2::new(0.5, 0.5));
    let mut current = ExecutionSlotId::new(PHYSICAL_SLOT, 0);

    index.upsert_bounds(current, ObjectId::new(0), bounds, 0);

    for generation in 1..=REUSE_CYCLES {
        let stale = current;
        let removed = index.remove_slot(stale);
        assert_eq!(removed.leaves_removed, 1);

        current = ExecutionSlotId::new(PHYSICAL_SLOT, generation);
        let inserted = index.upsert_bounds(
            current,
            ObjectId::new(u64::from(generation)),
            bounds,
            u64::from(generation),
        );
        assert_eq!(inserted.leaves_upserted, 1);

        assert_eq!(index.remove_slot(stale).leaves_removed, 0);
        assert!(!index.contains_slot(stale));
        assert!(index.contains_slot(current));
        assert_eq!(index.len(), 1);

        let hit = index.hit_test(Vec2::ZERO);
        assert_eq!(hit.slots(), &[current]);
        assert_eq!(hit.stats().full_scan_fallbacks, 0);
    }
}
