//! Bounded evidence that a displayed point lies outside subsequent picking damage.
//!
//! Each spatial synchronization checkpoints one batch of old/new conservative
//! bounds. Dense updates coalesce coverage within that batch, so dirty-row count
//! cannot expire the preceding snapshot. This journal stores no geometry,
//! candidates, targets or frame snapshots. Missing history rejects the proof.

use std::collections::VecDeque;

use noon_core::{Rect, Vec2};

// Eight dirty synchronizations, independent of display cadence. This is the same
// fixed budget as the worker's retained receipts, but a batch is not a displayed
// frame and the journal also serves native/direct hosts.
const MAX_DAMAGE_BATCHES: usize = 8;
const MAX_DAMAGE_REGIONS_PER_BATCH: usize = 128;

#[derive(Clone, Copy, Debug)]
struct DamageRegion {
    before: Rect,
    after: Rect,
}

#[derive(Clone, Debug)]
struct DamageBatch {
    stamp: u64,
    regions: VecDeque<DamageRegion>,
    invalid: bool,
}

/// Constant-bounded picking invalidation history owned by the execution session.
///
/// A stamp denotes the state after a complete synchronization batch. It does not
/// identify a frame, authorize input, or replace the original publication/view.
/// The caller must also prove compatible runtime, scene, execution and view state.
#[derive(Clone, Debug)]
pub(in crate::execution_session) struct PointerFrameDamage {
    current: Option<u64>,
    floor: u64,
    batches: VecDeque<DamageBatch>,
}

/// The exclusive journal borrow prevents captures between rows of one batch.
pub(in crate::execution_session) struct PointerDamageBatch<'a> {
    batch: Option<&'a mut DamageBatch>,
}

impl Default for PointerFrameDamage {
    fn default() -> Self {
        Self {
            current: Some(0),
            floor: 0,
            batches: VecDeque::with_capacity(MAX_DAMAGE_BATCHES),
        }
    }
}

impl PointerFrameDamage {
    pub(in crate::execution_session) const fn stamp(&self) -> Option<u64> {
        self.current
    }

    /// Begin one nonempty spatial synchronization. Only complete batches expire;
    /// no number of dirty rows within this batch can discard earlier history.
    pub(in crate::execution_session) fn begin_batch(&mut self) -> PointerDamageBatch<'_> {
        let Some(stamp) = self.advance() else {
            return PointerDamageBatch { batch: None };
        };
        let batch = if self.batches.len() == MAX_DAMAGE_BATCHES {
            let mut retired = self.batches.pop_front().expect("full damage journal");
            // A snapshot taken after this retired batch still has complete
            // subsequent history. Older snapshots no longer have that proof.
            self.floor = self.floor.max(retired.stamp);
            retired.stamp = stamp;
            retired.regions.clear();
            retired.invalid = false;
            retired
        } else {
            DamageBatch {
                stamp,
                regions: VecDeque::with_capacity(MAX_DAMAGE_REGIONS_PER_BATCH),
                invalid: false,
            }
        };
        self.batches.push_back(batch);
        PointerDamageBatch {
            batch: self.batches.back_mut(),
        }
    }

    /// Retire all earlier stamps when a local envelope cannot prove compatibility.
    /// A later capture may establish a new baseline. Exhaustion is permanent for
    /// this journal, so a wrapped sequence can never revive an old snapshot.
    pub(in crate::execution_session) fn invalidate(&mut self) {
        if let Some(stamp) = self.advance() {
            self.floor = stamp;
            // Earlier batches cannot prove anything past this barrier. Retain
            // their allocations for reuse by subsequent synchronizations.
        }
    }

    /// Prove that no retained change after `stamp` covers this exact scene point.
    /// This tests at most 8 * 128 envelope pairs, independent of scene size or age.
    pub(in crate::execution_session) fn unchanged_since(
        &self,
        stamp: Option<u64>,
        point: Vec2,
    ) -> bool {
        let (Some(stamp), Some(current)) = (stamp, self.current) else {
            return false;
        };
        if !valid_point(point) || stamp < self.floor || stamp > current {
            return false;
        }
        self.batches
            .iter()
            .filter(|batch| batch.stamp > stamp)
            .all(|batch| {
                !batch.invalid
                    && batch.regions.iter().all(|region| {
                        !contains(region.before, point) && !contains(region.after, point)
                    })
            })
    }

    fn advance(&mut self) -> Option<u64> {
        self.current = self.current.and_then(|current| current.checked_add(1));
        if self.current.is_none() {
            self.floor = u64::MAX;
            self.batches.clear();
        }
        self.current
    }
}

impl PointerDamageBatch<'_> {
    /// Record both endpoint envelopes, including when their coordinates match.
    /// Fill, reveal, content and other picking inputs can change within identical
    /// bounds. Invalid or unknown bounds must never be treated as empty damage.
    pub(in crate::execution_session) fn record(&mut self, before: Rect, after: Rect) {
        if !valid_bounds(before) || !valid_bounds(after) {
            self.invalidate();
            return;
        }
        let Some(batch) = self.batch.as_deref_mut() else {
            return;
        };
        if batch.invalid {
            return;
        }
        if batch.regions.len() == MAX_DAMAGE_REGIONS_PER_BATCH {
            let oldest = batch.regions.pop_front().expect("full damage batch");
            let next = batch.regions.front_mut().expect("remaining damage region");
            // Coalescing only widens damage and never crosses a captured stamp.
            // Keep the old/new envelopes separate rather than sweeping between
            // them. The wider coverage may conservatively reject an unchanged
            // gap, but can never admit a point covered by an original envelope.
            next.before = union(next.before, oldest.before);
            next.after = union(next.after, oldest.after);
        }
        batch.regions.push_back(DamageRegion { before, after });
    }

    /// Unknown coverage makes every earlier snapshot incompatible. A capture
    /// after this complete batch can still establish a new baseline.
    pub(in crate::execution_session) fn invalidate(&mut self) {
        if let Some(batch) = self.batch.as_deref_mut() {
            batch.invalid = true;
            batch.regions.clear();
        }
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect::new(
        Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
        Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
    )
}

fn valid_point(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn valid_bounds(bounds: Rect) -> bool {
    valid_point(bounds.min)
        && valid_point(bounds.max)
        && bounds.min.x <= bounds.max.x
        && bounds.min.y <= bounds.max.y
}

fn contains(bounds: Rect, point: Vec2) -> bool {
    bounds.min.x <= point.x
        && point.x <= bounds.max.x
        && bounds.min.y <= point.y
        && point.y <= bounds.max.y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32) -> Rect {
        Rect::new(Vec2::new(x, y), Vec2::new(x + 1.0, y + 1.0))
    }

    #[test]
    fn clean_and_distant_changes_preserve_point_local_compatibility() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        let point = Vec2::new(0.5, 0.5);
        assert!(damage.unchanged_since(displayed, point));

        damage
            .begin_batch()
            .record(bounds(10.0, 10.0), bounds(11.0, 10.0));
        assert!(damage.unchanged_since(displayed, point));
        assert!(!damage.unchanged_since(displayed, Vec2::new(10.5, 10.5)));
    }

    #[test]
    fn unchanged_bounds_still_retire_points_inside_changed_content() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        let region = bounds(0.0, 0.0);
        damage.begin_batch().record(region, region);

        assert!(!damage.unchanged_since(displayed, Vec2::new(0.5, 0.5)));
        assert!(damage.unchanged_since(displayed, Vec2::new(2.0, 0.5)));
        assert!(damage.unchanged_since(damage.stamp(), Vec2::new(0.5, 0.5)));
    }

    #[test]
    fn moving_rows_damage_old_and_new_closed_envelopes() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        let before = bounds(-4.0, 0.0);
        let after = bounds(3.0, 0.0);
        damage.begin_batch().record(before, after);

        for point in [
            before.min,
            before.max,
            Vec2::new(-3.5, 0.5),
            after.min,
            after.max,
            Vec2::new(3.5, 0.5),
        ] {
            assert!(!damage.unchanged_since(displayed, point));
        }
        // The journal compares captured endpoint views, not an unpresented
        // continuous sweep between them. Every intervening captured view must
        // be recorded separately by the session.
        assert!(damage.unchanged_since(displayed, Vec2::new(0.0, 0.5)));
    }

    #[test]
    fn every_change_after_the_captured_stamp_participates() {
        let mut damage = PointerFrameDamage::default();
        let first = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(0.0, 0.0), bounds(1.0, 0.0));
        let second = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(10.0, 0.0), bounds(11.0, 0.0));
        damage
            .begin_batch()
            .record(bounds(20.0, 0.0), bounds(21.0, 0.0));

        assert!(!damage.unchanged_since(first, Vec2::new(0.5, 0.5)));
        assert!(damage.unchanged_since(second, Vec2::new(0.5, 0.5)));
        assert!(!damage.unchanged_since(second, Vec2::new(10.5, 0.5)));
        assert!(!damage.unchanged_since(second, Vec2::new(21.5, 0.5)));
        assert!(damage.unchanged_since(first, Vec2::new(30.0, 0.5)));
    }

    #[test]
    fn dense_batches_keep_bounded_conservative_coverage_of_every_endpoint() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        let changes: Vec<_> = (0..1024)
            .map(|row| {
                (
                    bounds(-100.0 - row as f32 * 2.0, 0.0),
                    bounds(100.0 + row as f32 * 2.0, 0.0),
                )
            })
            .collect();
        {
            let mut batch = damage.begin_batch();
            for &(before, after) in &changes {
                batch.record(before, after);
            }
        }

        assert_eq!(damage.stamp(), Some(1));
        assert_eq!(damage.batches.len(), 1);
        assert_eq!(
            damage.batches[0].regions.len(),
            MAX_DAMAGE_REGIONS_PER_BATCH
        );
        for (before, after) in changes {
            for point in [before.min, before.max, after.min, after.max] {
                assert!(!damage.unchanged_since(displayed, point));
            }
        }
        // The two endpoint sets remain separate even after coalescing. Neither
        // dirty-row count nor an artificial old-to-new sweep rejects this gap.
        assert!(damage.unchanged_since(displayed, Vec2::new(0.0, 0.5)));
        assert!(damage.unchanged_since(damage.stamp(), Vec2::new(100.0, 0.5)));
    }

    #[test]
    fn coalescing_never_imports_damage_from_before_a_captured_stamp() {
        let mut damage = PointerFrameDamage::default();
        let initial = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(0.0, 0.0), bounds(1.0, 0.0));
        let middle = damage.stamp();
        {
            let mut batch = damage.begin_batch();
            for row in 0..512 {
                let x = 10.0 + row as f32 * 2.0;
                batch.record(bounds(x, 0.0), bounds(x + 1.0, 0.0));
            }
        }

        assert!(!damage.unchanged_since(initial, Vec2::new(0.5, 0.5)));
        assert!(damage.unchanged_since(middle, Vec2::new(0.5, 0.5)));
        assert!(!damage.unchanged_since(middle, Vec2::new(10.5, 0.5)));
    }

    #[test]
    fn eviction_rejects_expired_stamps_even_at_untouched_points() {
        let mut damage = PointerFrameDamage::default();
        let expired = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(0.0, 0.0), bounds(0.0, 0.0));
        let oldest_complete = damage.stamp();
        for _ in 1..MAX_DAMAGE_BATCHES {
            damage
                .begin_batch()
                .record(bounds(10.0, 0.0), bounds(10.0, 0.0));
        }
        assert!(damage.unchanged_since(expired, Vec2::new(100.0, 100.0)));
        damage
            .begin_batch()
            .record(bounds(10.0, 0.0), bounds(10.0, 0.0));

        assert_eq!(damage.batches.len(), MAX_DAMAGE_BATCHES);
        assert!(!damage.unchanged_since(expired, Vec2::new(100.0, 100.0)));
        assert!(damage.unchanged_since(oldest_complete, Vec2::new(0.5, 0.5)));
        assert!(!damage.unchanged_since(oldest_complete, Vec2::new(10.5, 0.5)));
        damage
            .begin_batch()
            .record(bounds(10.0, 0.0), bounds(10.0, 0.0));
        assert!(!damage.unchanged_since(oldest_complete, Vec2::new(0.5, 0.5)));
    }

    #[test]
    fn invalidation_retires_all_old_stamps_and_allows_a_new_baseline() {
        let mut damage = PointerFrameDamage::default();
        let initial = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(0.0, 0.0), bounds(1.0, 0.0));
        let before_invalidation = damage.stamp();
        damage.invalidate();
        let fresh = damage.stamp();

        assert!(!damage.unchanged_since(initial, Vec2::new(100.0, 100.0)));
        assert!(!damage.unchanged_since(before_invalidation, Vec2::new(100.0, 100.0)));
        assert!(damage.unchanged_since(fresh, Vec2::ZERO));
        damage
            .begin_batch()
            .record(bounds(10.0, 0.0), bounds(11.0, 0.0));
        assert!(damage.unchanged_since(fresh, Vec2::ZERO));
        assert!(!damage.unchanged_since(fresh, Vec2::new(10.5, 0.5)));
    }

    #[test]
    fn an_invalid_batch_stays_invalid_but_allows_an_immediate_new_baseline() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        {
            let mut batch = damage.begin_batch();
            batch.record(bounds(10.0, 0.0), bounds(11.0, 0.0));
            batch.invalidate();
            batch.record(bounds(20.0, 0.0), bounds(21.0, 0.0));
        }
        let fresh = damage.stamp();

        assert!(!damage.unchanged_since(displayed, Vec2::ZERO));
        assert!(damage.unchanged_since(fresh, Vec2::ZERO));
        damage
            .begin_batch()
            .record(bounds(30.0, 0.0), bounds(31.0, 0.0));
        assert!(!damage.unchanged_since(displayed, Vec2::ZERO));
        assert!(damage.unchanged_since(fresh, Vec2::ZERO));
        assert!(!damage.unchanged_since(fresh, Vec2::new(30.5, 0.5)));
    }

    #[test]
    fn recycling_batches_cannot_move_the_floor_back_across_invalidation() {
        let mut damage = PointerFrameDamage::default();
        for _ in 0..MAX_DAMAGE_BATCHES {
            damage
                .begin_batch()
                .record(bounds(10.0, 0.0), bounds(11.0, 0.0));
        }
        let before_invalidation = damage.stamp();
        damage.invalidate();
        let fresh = damage.stamp();
        for _ in 0..MAX_DAMAGE_BATCHES {
            damage
                .begin_batch()
                .record(bounds(20.0, 0.0), bounds(21.0, 0.0));
            assert!(!damage.unchanged_since(before_invalidation, Vec2::ZERO));
            assert!(damage.unchanged_since(fresh, Vec2::ZERO));
        }
        damage
            .begin_batch()
            .record(bounds(20.0, 0.0), bounds(21.0, 0.0));
        assert!(!damage.unchanged_since(fresh, Vec2::ZERO));
    }

    #[test]
    fn invalid_bounds_cause_global_invalidation_in_either_endpoint() {
        let valid = bounds(0.0, 0.0);
        for invalid in [
            Rect::new(Vec2::new(f32::NAN, 0.0), Vec2::ONE),
            Rect::new(Vec2::ZERO, Vec2::new(1.0, f32::NAN)),
            Rect::new(Vec2::new(f32::NEG_INFINITY, 0.0), Vec2::ONE),
            Rect::new(Vec2::ZERO, Vec2::new(1.0, f32::INFINITY)),
            Rect::new(Vec2::new(2.0, 0.0), Vec2::ONE),
            Rect::new(Vec2::new(0.0, 2.0), Vec2::ONE),
        ] {
            for (before, after) in [(invalid, valid), (valid, invalid)] {
                let mut damage = PointerFrameDamage::default();
                let displayed = damage.stamp();
                damage.begin_batch().record(before, after);
                assert!(!damage.unchanged_since(displayed, Vec2::new(100.0, 100.0)));
                assert!(damage.unchanged_since(damage.stamp(), Vec2::ZERO));
            }
        }
    }

    #[test]
    fn invalid_points_missing_stamps_and_future_stamps_never_prove_compatibility() {
        let damage = PointerFrameDamage::default();
        for point in [
            Vec2::new(f32::NAN, 0.0),
            Vec2::new(0.0, f32::NAN),
            Vec2::new(f32::INFINITY, 0.0),
            Vec2::new(0.0, f32::NEG_INFINITY),
        ] {
            assert!(!damage.unchanged_since(damage.stamp(), point));
        }
        assert!(!damage.unchanged_since(None, Vec2::ZERO));
        assert!(!damage.unchanged_since(Some(1), Vec2::ZERO));
        assert!(!damage.unchanged_since(Some(u64::MAX), Vec2::ZERO));
    }

    #[test]
    fn finite_degenerate_bounds_still_cover_their_exact_point() {
        let mut damage = PointerFrameDamage::default();
        let displayed = damage.stamp();
        let point = Vec2::new(f32::MAX, f32::MIN);
        damage
            .begin_batch()
            .record(Rect::new(point, point), Rect::new(point, point));

        assert!(!damage.unchanged_since(displayed, point));
        assert!(damage.unchanged_since(displayed, Vec2::ZERO));
    }

    #[test]
    fn batch_sequence_exhaustion_permanently_disables_proofs() {
        let mut damage = PointerFrameDamage {
            current: Some(u64::MAX - 1),
            floor: u64::MAX - 1,
            ..PointerFrameDamage::default()
        };
        let before_last = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(0.0, 0.0), bounds(1.0, 0.0));
        assert_eq!(damage.stamp(), Some(u64::MAX));
        assert!(damage.unchanged_since(before_last, Vec2::new(100.0, 100.0)));
        let last = damage.stamp();
        damage
            .begin_batch()
            .record(bounds(10.0, 0.0), bounds(11.0, 0.0));

        assert_eq!(damage.stamp(), None);
        assert!(!damage.unchanged_since(before_last, Vec2::new(100.0, 100.0)));
        assert!(!damage.unchanged_since(last, Vec2::new(100.0, 100.0)));
        damage.invalidate();
        damage
            .begin_batch()
            .record(bounds(20.0, 0.0), bounds(21.0, 0.0));
        assert_eq!(damage.stamp(), None);
        assert!(!damage.unchanged_since(None, Vec2::ZERO));
    }

    #[test]
    fn invalidation_cannot_wrap_the_last_valid_stamp() {
        let mut damage = PointerFrameDamage {
            current: Some(u64::MAX),
            floor: u64::MAX,
            ..PointerFrameDamage::default()
        };
        let displayed = damage.stamp();
        damage.invalidate();

        assert_eq!(damage.stamp(), None);
        assert!(!damage.unchanged_since(displayed, Vec2::ZERO));
    }
}
