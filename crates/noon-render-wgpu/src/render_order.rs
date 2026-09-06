use std::{collections::HashSet, ops::Range};

use crate::{
    FramePreparer, MegaPathBatch, PreparedFrame, PreparedSlot, RenderStats, VisibleProjectionKey,
};
use noon_runtime::{FrameChanges, FrameState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderOrderKey {
    pub z_index: i32,
    pub insertion_order: u64,
}

impl RenderOrderKey {
    pub const fn new(z_index: i32, insertion_order: u64) -> Self {
        Self {
            z_index,
            insertion_order,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderPrimitive {
    Circle,
    Rectangle,
    Line,
    /// Index into `PreparedFrame::path_batches` identifying the mesh/index range.
    Path {
        batch: usize,
    },
    /// Index into `PreparedFrame::mega_path_batches` for a painter-ordered
    /// packed draw containing one or more unique path meshes.
    MegaPath {
        batch: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedRenderBatch {
    pub primitive: RenderPrimitive,
    pub instance_range: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderOrderError {
    KeyCountMismatch { objects: usize, keys: usize },
}

impl std::fmt::Display for RenderOrderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyCountMismatch { objects, keys } => write!(
                formatter,
                "render order key count {keys} does not match scene object count {objects}"
            ),
        }
    }
}

impl std::error::Error for RenderOrderError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisibleRenderError {
    ObjectIndexOutOfRange { index: usize, objects: usize },
    DuplicateObjectIndex(usize),
}

/// Cumulative candidate-sized work used to derive geometry draw submissions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisibleRenderProjectionStats {
    pub projections: u64,
    pub candidates_projected: u64,
    pub render_batches_projected: u64,
}

impl VisibleRenderProjectionStats {
    fn record(&mut self, candidates: usize, render_batches: usize) {
        self.projections = self.projections.saturating_add(1);
        self.candidates_projected = self.candidates_projected.saturating_add(candidates as u64);
        self.render_batches_projected = self
            .render_batches_projected
            .saturating_add(render_batches as u64);
    }
}

impl std::fmt::Display for VisibleRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ObjectIndexOutOfRange { index, objects } => write!(
                formatter,
                "visible render object index {index} is outside frame object count {objects}"
            ),
            Self::DuplicateObjectIndex(index) => {
                write!(formatter, "duplicate visible render object index {index}")
            }
        }
    }
}

impl std::error::Error for VisibleRenderError {}

impl FramePreparer {
    /// Prepare one incremental frame while limiting ordered draw submission to the
    /// supplied retained-visibility candidates.
    ///
    /// `visible_object_indices` must already be in back-to-front painter order, as
    /// returned by the retained viewport query. The ordinary incremental preparation
    /// still updates packed instance and mesh caches for dirty objects; this method
    /// projects only candidate-sized draw descriptors while the canonical all-live
    /// painter batches remain untouched inside the retained preparer.
    pub fn prepare_incremental_visible<'a>(
        &'a mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        visible_object_indices: &[usize],
    ) -> Result<PreparedFrame<'a>, VisibleRenderError> {
        let mut seen = HashSet::with_capacity(visible_object_indices.len());
        for &object_index in visible_object_indices {
            if object_index >= frame.objects.len() {
                return Err(VisibleRenderError::ObjectIndexOutOfRange {
                    index: object_index,
                    objects: frame.objects.len(),
                });
            }
            if !seen.insert(object_index) {
                return Err(VisibleRenderError::DuplicateObjectIndex(object_index));
            }
        }

        let mut stats = self.prepare_incremental(frame, changes).stats;

        if !self.visible_projection_matches(visible_object_indices) {
            self.visible_projection_key.clear();
            for &object_index in visible_object_indices {
                let slot = self.slots[object_index];
                let (mega_path_segment, mega_path_detached) = match slot {
                    PreparedSlot::Path { batch, .. } => (
                        self.mega_path_segments.get(batch).cloned().flatten(),
                        self.mega_path_detached.get(batch).copied().unwrap_or(true),
                    ),
                    _ => (None, false),
                };
                self.visible_projection_key.push(VisibleProjectionKey {
                    object_index,
                    slot,
                    mega_path_segment,
                    mega_path_detached,
                });
            }

            let mut raw_render_batches = std::mem::take(&mut self.visible_raw_render_batches);
            raw_render_batches.clear();
            for &object_index in visible_object_indices {
                push_slot_batches(&mut raw_render_batches, self.slots[object_index]);
            }
            let mut render_batches = std::mem::take(&mut self.visible_render_batches);
            let mut mega_path_batches = std::mem::take(&mut self.visible_mega_path_batches);
            project_mega_render_batches(
                self,
                &raw_render_batches,
                &mut render_batches,
                &mut mega_path_batches,
            );
            self.visible_raw_render_batches = raw_render_batches;
            self.visible_render_batches = render_batches;
            self.visible_mega_path_batches = mega_path_batches;
            self.visible_projection_ready = true;
            self.visible_projection_stats.record(
                visible_object_indices.len(),
                self.visible_render_batches.len(),
            );
        }

        stats.batch_count = self.visible_render_batches.len();
        stats.mega_path_count = self
            .visible_mega_path_batches
            .iter()
            .map(|batch| batch.path_count)
            .sum();
        stats.mega_path_batch_count = self.visible_mega_path_batches.len();

        Ok(projected_frame(
            self,
            &self.visible_render_batches,
            &self.visible_mega_path_batches,
            frame.time,
            stats,
        ))
    }

    /// Cumulative descriptor projection work. An unchanged candidate view whose
    /// exact slot/mega topology is unchanged reuses the prior projection.
    pub const fn visible_projection_stats(&self) -> VisibleRenderProjectionStats {
        self.visible_projection_stats
    }

    fn visible_projection_matches(&self, visible_object_indices: &[usize]) -> bool {
        self.visible_projection_ready
            && self.visible_projection_key.len() == visible_object_indices.len()
            && self
                .visible_projection_key
                .iter()
                .zip(visible_object_indices)
                .all(|(cached, &object_index)| {
                    if cached.object_index != object_index
                        || cached.slot != self.slots[object_index]
                    {
                        return false;
                    }
                    match cached.slot {
                        PreparedSlot::Path { batch, .. } => {
                            cached.mega_path_segment
                                == self.mega_path_segments.get(batch).cloned().flatten()
                                && cached.mega_path_detached
                                    == self.mega_path_detached.get(batch).copied().unwrap_or(true)
                        }
                        _ => cached.mega_path_segment.is_none() && !cached.mega_path_detached,
                    }
                })
    }
}

impl FramePreparer {
    /// Install explicit semantic z/painter keys for subsequent preparations.
    ///
    /// The current runtime has not yet migrated #62 presentation metadata into
    /// `FrameObjectState`, so this narrow adapter lets that migration land later
    /// without changing the renderer ordering algorithm again. When no keys are
    /// supplied, object-vector order remains the stable painter order.
    pub fn set_render_order_keys(
        &mut self,
        frame: &FrameState,
        keys: &[RenderOrderKey],
    ) -> Result<(), RenderOrderError> {
        if keys.len() != frame.objects.len() {
            return Err(RenderOrderError::KeyCountMismatch {
                objects: frame.objects.len(),
                keys: keys.len(),
            });
        }
        if self.render_order_keys != keys {
            self.render_order_keys.clear();
            self.render_order_keys.extend_from_slice(keys);
            self.initialized = false;
        }
        Ok(())
    }

    pub fn clear_render_order_keys(&mut self) {
        if !self.render_order_keys.is_empty() {
            self.render_order_keys.clear();
            self.initialized = false;
        }
    }

    pub(crate) fn append_ordered_render_slot(&mut self, slot: PreparedSlot) {
        debug_assert!(
            self.render_order_keys.is_empty(),
            "explicit render-order keys require structural rebuild",
        );
        push_slot_batches(&mut self.render_batches, slot);
    }

    pub(crate) fn append_ordered_reveal_head(&mut self, line_index: usize) {
        debug_assert!(
            self.render_order_keys.is_empty(),
            "explicit render-order keys require structural rebuild",
        );
        push_batch(&mut self.render_batches, RenderPrimitive::Line, line_index);
    }

    pub(crate) fn rebuild_ordered_render_batches(&mut self) {
        self.render_batches.clear();

        if self.render_order_keys.len() == self.slots.len() {
            // Explicit z-order is comparatively uncommon and genuinely needs a
            // sortable indirection. Keep that allocation isolated to this path.
            let mut object_indices = (0..self.slots.len()).collect::<Vec<_>>();
            object_indices.sort_by_key(|&index| self.render_order_keys[index]);
            for object_index in object_indices {
                push_slot_batches(&mut self.render_batches, self.slots[object_index]);
            }
            return;
        }

        // Default painter order is already the semantic object-vector order.
        // Walking slots directly avoids allocating/filling a temporary index
        // vector on every full preparation or structural rebuild.
        for slot in self.slots.iter().copied() {
            push_slot_batches(&mut self.render_batches, slot);
        }
    }
}

fn project_mega_render_batches(
    preparer: &FramePreparer,
    ordered: &[OrderedRenderBatch],
    render_batches: &mut Vec<OrderedRenderBatch>,
    mega_path_batches: &mut Vec<MegaPathBatch>,
) {
    render_batches.clear();
    mega_path_batches.clear();
    let mut active_mega = None::<usize>;

    for ordered_batch in ordered.iter().cloned() {
        let RenderPrimitive::Path {
            batch: path_batch_index,
        } = ordered_batch.primitive
        else {
            active_mega = None;
            render_batches.push(ordered_batch);
            continue;
        };

        let segment = preparer
            .mega_path_segments
            .get(path_batch_index)
            .and_then(|segment| segment.as_ref())
            .filter(|_| {
                !preparer
                    .mega_path_detached
                    .get(path_batch_index)
                    .copied()
                    .unwrap_or(true)
            })
            .cloned();
        let Some(segment) = segment else {
            active_mega = None;
            render_batches.push(ordered_batch);
            continue;
        };

        if let Some(mega_index) = active_mega {
            let mega = &mut mega_path_batches[mega_index];
            if mega.index_range.end == segment.start {
                mega.index_range.end = segment.end;
                mega.path_count += 1;
                let ordered = render_batches
                    .last_mut()
                    .expect("active visible mega batch must have an ordered batch");
                ordered.instance_range.end += 1;
                continue;
            }
        }

        let mega_index = mega_path_batches.len();
        mega_path_batches.push(MegaPathBatch {
            index_range: segment,
            path_count: 1,
        });
        render_batches.push(OrderedRenderBatch {
            primitive: RenderPrimitive::MegaPath { batch: mega_index },
            instance_range: 0..1,
        });
        active_mega = Some(mega_index);
    }
}

fn projected_frame<'a>(
    preparer: &'a FramePreparer,
    render_batches: &'a [OrderedRenderBatch],
    mega_path_batches: &'a [MegaPathBatch],
    time: f64,
    stats: RenderStats,
) -> PreparedFrame<'a> {
    PreparedFrame {
        time,
        circle_ids: &preparer.circle_ids,
        circles: &preparer.circles,
        rectangle_ids: &preparer.rectangle_ids,
        rectangles: &preparer.rectangles,
        line_ids: &preparer.line_ids,
        lines: &preparer.lines,
        path_ids: &preparer.path_ids,
        paths: &preparer.paths,
        path_vertices: &preparer.path_vertices,
        path_indices: &preparer.path_indices,
        path_batches: &preparer.path_batches,
        mega_path_indices: &preparer.mega_path_indices,
        mega_path_vertex_instances: &preparer.mega_path_vertex_instances,
        mega_path_batches,
        render_batches,
        unsupported: &preparer.unsupported,
        circle_dirty_ranges: &preparer.circle_dirty_ranges,
        rectangle_dirty_ranges: &preparer.rectangle_dirty_ranges,
        line_dirty_ranges: &preparer.line_dirty_ranges,
        path_dirty_ranges: &preparer.path_dirty_ranges,
        path_vertex_dirty_ranges: &preparer.path_vertex_dirty_ranges,
        path_index_dirty_ranges: &preparer.path_index_dirty_ranges,
        mega_path_instance_dirty_ranges: &preparer.mega_path_instance_dirty_ranges,
        mega_path_index_dirty_ranges: &preparer.mega_path_index_dirty_ranges,
        mega_path_index_dirty: preparer.mega_path_index_dirty,
        path_geometry_dirty: preparer.path_geometry_dirty,
        stats,
    }
}

fn push_slot_batches(batches: &mut Vec<OrderedRenderBatch>, slot: PreparedSlot) {
    match slot {
        PreparedSlot::Absent | PreparedSlot::Unsupported(_) => {}
        PreparedSlot::Circle(index) => {
            push_batch(batches, RenderPrimitive::Circle, index);
        }
        PreparedSlot::Rectangle(index) => {
            push_batch(batches, RenderPrimitive::Rectangle, index);
        }
        PreparedSlot::Line(index) => {
            push_batch(batches, RenderPrimitive::Line, index);
        }
        PreparedSlot::Path {
            index,
            batch,
            reveal_head,
            ..
        } => {
            push_batch(batches, RenderPrimitive::Path { batch }, index);
            // The animated reveal head belongs to this object's painter
            // position and should sit immediately above its path body.
            if let Some(line_index) = reveal_head {
                push_batch(batches, RenderPrimitive::Line, line_index);
            }
        }
    }
}

fn push_batch(batches: &mut Vec<OrderedRenderBatch>, primitive: RenderPrimitive, index: usize) {
    let start = u32::try_from(index).expect("render instance count exceeds u32 limits");
    let end = start
        .checked_add(1)
        .expect("render instance count exceeds u32 limits");
    if let Some(last) = batches.last_mut() {
        if last.primitive == primitive && last.instance_range.end == start {
            last.instance_range.end = end;
            return;
        }
    }
    batches.push(OrderedRenderBatch {
        primitive,
        instance_range: start..end,
    });
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
    use noon_runtime::{FrameObjectState, FrameState};

    use super::*;

    fn object(id: u64, geometry: GeometryRef) -> FrameObjectState {
        FrameObjectState {
            id: ObjectId::new(id),
            content: noon_core::ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::default(),
            style: Style::default(),
            appearance: 1.0,
        }
    }

    fn frame(objects: Vec<FrameObjectState>) -> FrameState {
        let count = objects.len();
        FrameState {
            time: 0.0,
            objects,
            presences: vec![true; count],
            reveals: vec![1.0; count],
            morphs: vec![0.0; count],
            render_geometries: vec![None; count],
            render_transforms: vec![None; count],
        }
    }

    #[test]
    fn mixed_analytic_primitives_keep_painter_order() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        assert_eq!(
            prepared.render_batches,
            &[
                OrderedRenderBatch {
                    primitive: RenderPrimitive::Circle,
                    instance_range: 0..1,
                },
                OrderedRenderBatch {
                    primitive: RenderPrimitive::Rectangle,
                    instance_range: 0..1,
                },
                OrderedRenderBatch {
                    primitive: RenderPrimitive::Circle,
                    instance_range: 1..2,
                },
            ]
        );
    }

    #[test]
    fn contiguous_same_type_instances_still_batch() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::circle(0.8)),
            object(2, GeometryRef::circle(0.6)),
        ]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.render_batches.len(), 1);
        assert_eq!(prepared.render_batches[0].instance_range, 0..3);
    }

    #[test]
    fn explicit_z_keys_reorder_without_changing_instance_storage() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();
        preparer
            .set_render_order_keys(
                &frame,
                &[
                    RenderOrderKey::new(5, 0),
                    RenderOrderKey::new(-1, 1),
                    RenderOrderKey::new(5, 2),
                ],
            )
            .unwrap();
        let prepared = preparer.prepare(&frame);
        assert_eq!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::Rectangle
        );
        assert_eq!(
            prepared.render_batches[1].primitive,
            RenderPrimitive::Circle
        );
        assert_eq!(prepared.render_batches[1].instance_range, 0..2);
    }

    #[test]
    fn key_count_must_match_scene() {
        let frame = frame(vec![object(0, GeometryRef::circle(1.0))]);
        let mut preparer = FramePreparer::new();
        assert!(matches!(
            preparer.set_render_order_keys(&frame, &[]),
            Err(RenderOrderError::KeyCountMismatch {
                objects: 1,
                keys: 0
            })
        ));
    }

    #[test]
    fn visible_preparation_filters_draw_batches_without_repacking_storage() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer
            .prepare_incremental_visible(&frame, &FrameChanges::all(), &[0, 2])
            .unwrap();

        assert_eq!(prepared.circles.len(), 2);
        assert_eq!(prepared.rectangles.len(), 1);
        assert_eq!(prepared.stats.instance_count, 3);
        assert_eq!(prepared.stats.batch_count, 1);
        assert_eq!(
            prepared.render_batches,
            &[OrderedRenderBatch {
                primitive: RenderPrimitive::Circle,
                instance_range: 0..2,
            }]
        );
    }

    #[test]
    fn visible_preparation_preserves_candidate_painter_order() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer
            .prepare_incremental_visible(&frame, &FrameChanges::all(), &[1, 2])
            .unwrap();

        assert_eq!(prepared.render_batches.len(), 2);
        assert_eq!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::Rectangle
        );
        assert_eq!(
            prepared.render_batches[1].primitive,
            RenderPrimitive::Circle
        );
        assert_eq!(prepared.render_batches[1].instance_range, 1..2);
    }

    #[test]
    fn empty_visibility_keeps_packed_instances_but_submits_no_draws() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
        ]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer
            .prepare_incremental_visible(&frame, &FrameChanges::all(), &[])
            .unwrap();

        assert_eq!(prepared.stats.instance_count, 2);
        assert_eq!(prepared.stats.batch_count, 0);
        assert!(prepared.render_batches.is_empty());
    }

    #[test]
    fn invalid_visibility_is_rejected_before_preparation() {
        let frame = frame(vec![object(0, GeometryRef::circle(1.0))]);
        let mut preparer = FramePreparer::new();

        assert!(matches!(
            preparer.prepare_incremental_visible(&frame, &FrameChanges::all(), &[1]),
            Err(VisibleRenderError::ObjectIndexOutOfRange {
                index: 1,
                objects: 1
            })
        ));
        assert!(matches!(
            preparer.prepare_incremental_visible(&frame, &FrameChanges::all(), &[0, 0]),
            Err(VisibleRenderError::DuplicateObjectIndex(0))
        ));
    }

    #[test]
    fn visibility_projection_does_not_poison_unculled_submission() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();

        let visible = preparer
            .prepare_incremental_visible(&frame, &FrameChanges::all(), &[0])
            .unwrap();
        assert_eq!(visible.render_batches.len(), 1);
        assert_eq!(visible.stats.batch_count, 1);

        let full = preparer.prepare_incremental(&frame, &FrameChanges::default());
        assert_eq!(full.render_batches.len(), 3);
        assert_eq!(full.stats.batch_count, 3);
        assert_eq!(full.render_batches[0].primitive, RenderPrimitive::Circle);
        assert_eq!(full.render_batches[1].primitive, RenderPrimitive::Rectangle);
        assert_eq!(full.render_batches[2].primitive, RenderPrimitive::Circle);
    }

    #[test]
    fn unchanged_candidates_reuse_geometry_projection_across_clean_and_offscreen_changes() {
        let mut frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
        ]);
        let mut preparer = FramePreparer::new();

        preparer
            .prepare_incremental_visible(&frame, &FrameChanges::all(), &[0])
            .unwrap();
        let projected_once = preparer.visible_projection_stats();
        assert_eq!(projected_once.projections, 1);
        assert_eq!(projected_once.candidates_projected, 1);

        preparer
            .prepare_incremental_visible(&frame, &FrameChanges::default(), &[0])
            .unwrap();
        assert_eq!(preparer.visible_projection_stats(), projected_once);

        frame.objects[1].transform.translation = noon_core::Vec2::new(20.0, 0.0);
        let prepared = preparer
            .prepare_incremental_visible(&frame, &FrameChanges::objects(vec![1]), &[0])
            .unwrap();
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(preparer.visible_projection_stats(), projected_once);

        frame.objects[0].content =
            noon_core::ObjectContentRef::Geometry(GeometryRef::rectangle(3.0, 3.0));
        {
            let prepared = preparer
                .prepare_incremental_visible(&frame, &FrameChanges::objects(vec![0]), &[0])
                .unwrap();
            assert_eq!(prepared.render_batches.len(), 1);
            assert_eq!(
                prepared.render_batches[0].primitive,
                RenderPrimitive::Rectangle
            );
        }
        assert_eq!(preparer.visible_projection_stats().projections, 2);
    }
}
