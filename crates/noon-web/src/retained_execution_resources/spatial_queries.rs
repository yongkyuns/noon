//! Derived broad-phase queries over the worker's already validated frame.
//! Native/direct-WASM borrow the session index; the genuine receiver reuses the
//! same Rust implementation and existing frame indices, with no semantic IDs.
use super::*;
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex, SpatialIndexUpdateStats};

impl InstalledRetainedExecutionMirror {
    pub fn last_spatial_query_update_stats(&self) -> SpatialIndexUpdateStats {
        self.spatial_update_stats
    }

    pub(super) fn update_spatial_query_index(&mut self, changes: &FrameChanges, snapshot: bool) {
        self.spatial_update_stats = SpatialIndexUpdateStats::default();
        if !self.wire.has_painter_prefix_sources() {
            self.spatial_index = None;
            return;
        }
        let fresh = self.spatial_index.is_none();
        let index = self
            .spatial_index
            .get_or_insert_with(ExecutionSpatialIndex::default);
        let frame = self.wire.frame().expect("committed worker frame");
        // These are the existing validated frame/transport positions, not a new
        // allocator. ObjectId generations remain in each spatial entry.
        let slot = |row: u32| ExecutionSlotId::new(row, 0);
        if fresh || snapshot || changes.is_all() {
            self.spatial_update_stats = index.rebuild(
                frame,
                self.resources.geometries(),
                self.wire
                    .painter_order()
                    .iter()
                    .map(|&row| (slot(row), row as usize)),
            );
            return;
        }
        for &row in changes.object_indices() {
            let Some(state) = frame.objects.get(row) else {
                continue;
            };
            if frame.is_present(row) {
                self.spatial_update_stats
                    .merge_from(index.upsert_frame_slot(
                        frame,
                        self.resources.geometries(),
                        slot(row as u32),
                        row,
                        self.wire.painter_rank_for_frame_index(row).unwrap_or(row) as u64,
                    ));
            } else {
                self.spatial_update_stats
                    .merge_from(index.remove_object(state.id));
            }
        }
        if let Some(range) = changes.painter_order_range() {
            for rank in range {
                let Some(&row) = self.wire.painter_order().get(rank) else {
                    continue;
                };
                self.spatial_update_stats
                    .merge_from(index.upsert_frame_slot(
                        frame,
                        self.resources.geometries(),
                        slot(row),
                        row as usize,
                        rank as u64,
                    ));
            }
        }
    }
}
