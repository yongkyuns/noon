use std::{collections::HashSet, ops::Range};

use crate::{
    FramePreparer, MegaPathBatch, PreparedFrame, PreparedRenderChunk, PreparedSlot, RenderStats,
    VisibleProjectionKey,
};
use noon_runtime::{FrameChanges, FrameState};

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
    pub(crate) const RENDER_ORDER_CHUNK_SIZE: usize = 64;

    /// Install the runtime-derived painter permutation without relocating or
    /// re-realizing stable object slots. Callers need invoke this only for the
    /// initial frame or a publication carrying a painter-order change.
    pub fn set_painter_order(&mut self, frame: &FrameState, order: &[u32]) {
        debug_assert!(order
            .iter()
            .all(|&object_index| (object_index as usize) < frame.objects.len()));
        self.painter_order_indices.clear();
        self.painter_order_indices.extend_from_slice(order);
        self.painter_order_installed = true;
    }

    /// Return to storage order when a parent switches to compact scratch rows.
    pub(crate) fn clear_painter_order(&mut self) {
        self.painter_order_indices.clear();
        self.painter_order_installed = false;
    }

    /// Update only the changed portion of the runtime-derived painter permutation.
    pub fn set_painter_order_range(
        &mut self,
        frame: &FrameState,
        order: &[u32],
        range: Range<usize>,
    ) {
        self.painter_order_installed = true;
        debug_assert!(range.start <= order.len());
        debug_assert!(order[range.start..range.end.min(order.len())]
            .iter()
            .all(|&object_index| (object_index as usize) < frame.objects.len()));
        let old_end = range.end.min(self.painter_order_indices.len());
        let new_end = range.end.min(order.len());
        self.painter_order_indices.splice(
            range.start.min(old_end)..old_end,
            order[range.start..new_end].iter().copied(),
        );
        debug_assert_eq!(self.painter_order_indices.len(), order.len());
    }
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
    pub(crate) fn append_ordered_render_slot(&mut self, slot: PreparedSlot) {
        push_slot_batches(&mut self.render_batches, slot);
    }

    pub(crate) fn append_ordered_reveal_head(&mut self, line_index: usize) {
        push_batch(&mut self.render_batches, RenderPrimitive::Line, line_index);
    }

    pub(crate) fn rebuild_ordered_render_batches(&mut self) {
        self.render_batches.clear();

        if self.painter_order_installed {
            for &object_index in &self.painter_order_indices {
                if let Some(slot) = self.slots.get(object_index as usize).copied() {
                    push_slot_batches(&mut self.render_batches, slot);
                }
            }
            return;
        }

        // Low-level scratch frames without a runtime permutation use storage order.
        for slot in self.slots.iter().copied() {
            push_slot_batches(&mut self.render_batches, slot);
        }
    }

    /// Rebuild fixed-size painter partitions intersecting `range`.
    ///
    /// Stored batches stop at partition boundaries so an adjacent reorder remains
    /// proportional to its affected partition. The draw iterator lazily rejoins
    /// compatible boundary batches without touching immutable geometry or mega-path
    /// streams.
    pub(crate) fn rebuild_render_order_chunks(&mut self, range: Option<Range<usize>>) {
        let position_count = if self.painter_order_installed {
            self.painter_order_indices.len()
        } else {
            self.slots.len()
        };
        let chunk_count = position_count.div_ceil(Self::RENDER_ORDER_CHUNK_SIZE);
        let rebuild_all = range.is_none();
        if !rebuild_all && chunk_count < self.render_chunks.len() {
            for chunk in &self.render_chunks[chunk_count..] {
                self.render_order_batch_count -= chunk.render_batches.len();
                self.render_order_mega_batch_count -= chunk.mega_path_batches.len();
                self.render_order_mega_path_count -= chunk
                    .mega_path_batches
                    .iter()
                    .map(|batch| batch.path_count)
                    .sum::<usize>();
            }
            for &(ordinary, mega) in
                &self.render_chunk_boundary_merges[chunk_count.saturating_sub(1)..]
            {
                self.render_order_boundary_merge_count -= usize::from(ordinary);
                self.render_order_mega_boundary_merge_count -= usize::from(mega);
            }
        }
        self.render_chunks
            .resize_with(chunk_count, PreparedRenderChunk::default);
        self.render_chunks.truncate(chunk_count);
        self.render_chunk_boundary_merges
            .resize(chunk_count.saturating_sub(1), (false, false));
        if position_count == 0 {
            self.render_chunks_active = range.is_some();
            self.render_order_batch_count = 0;
            self.render_order_mega_batch_count = 0;
            self.render_order_mega_path_count = 0;
            self.render_order_boundary_merge_count = 0;
            self.render_order_mega_boundary_merge_count = 0;
            return;
        }

        let activate_chunks = range.is_some();
        let affected = range.unwrap_or(0..position_count);
        let first_chunk = affected.start.min(position_count - 1) / Self::RENDER_ORDER_CHUNK_SIZE;
        let last_position = affected.end.max(affected.start + 1).min(position_count) - 1;
        let last_chunk = last_position / Self::RENDER_ORDER_CHUNK_SIZE;
        if rebuild_all {
            self.render_order_batch_count = 0;
            self.render_order_mega_batch_count = 0;
            self.render_order_mega_path_count = 0;
            self.render_order_boundary_merge_count = 0;
            self.render_order_mega_boundary_merge_count = 0;
        }
        for chunk_index in first_chunk..=last_chunk {
            let start = chunk_index * Self::RENDER_ORDER_CHUNK_SIZE;
            let end = (start + Self::RENDER_ORDER_CHUNK_SIZE).min(position_count);
            let mut raw = Vec::with_capacity((end - start) * 2);
            for position in start..end {
                let object_index = self
                    .painter_order_indices
                    .get(position)
                    .map_or(position, |&index| index as usize);
                if let Some(slot) = self.slots.get(object_index).copied() {
                    push_slot_batches(&mut raw, slot);
                }
            }
            let mut render_batches = Vec::new();
            let mut mega_path_batches = Vec::new();
            project_mega_render_batches(self, &raw, &mut render_batches, &mut mega_path_batches);
            if !rebuild_all {
                let old = &self.render_chunks[chunk_index];
                self.render_order_batch_count -= old.render_batches.len();
                self.render_order_mega_batch_count -= old.mega_path_batches.len();
                self.render_order_mega_path_count -= old
                    .mega_path_batches
                    .iter()
                    .map(|batch| batch.path_count)
                    .sum::<usize>();
            }
            self.render_order_batch_count += render_batches.len();
            self.render_order_mega_batch_count += mega_path_batches.len();
            self.render_order_mega_path_count += mega_path_batches
                .iter()
                .map(|batch| batch.path_count)
                .sum::<usize>();
            self.render_chunks[chunk_index] = PreparedRenderChunk {
                render_batches,
                mega_path_batches,
            };
            self.render_order_positions_visited += end - start;
            self.render_order_chunks_rebuilt += 1;
        }
        let first_boundary = first_chunk.saturating_sub(1);
        let last_boundary = last_chunk.min(chunk_count.saturating_sub(2));
        let has_boundary = chunk_count >= 2 && first_boundary <= last_boundary;
        if !rebuild_all && has_boundary {
            for boundary in first_boundary..=last_boundary {
                let (ordinary, mega) = self.render_chunk_boundary_merges[boundary];
                self.render_order_boundary_merge_count -= usize::from(ordinary);
                self.render_order_mega_boundary_merge_count -= usize::from(mega);
            }
        }
        if has_boundary {
            for boundary in first_boundary..=last_boundary {
                let merge = render_chunk_boundary_merge(
                    &self.render_chunks[boundary],
                    &self.render_chunks[boundary + 1],
                );
                self.render_chunk_boundary_merges[boundary] = merge;
                self.render_order_boundary_merge_count += usize::from(merge.0);
                self.render_order_mega_boundary_merge_count += usize::from(merge.1);
            }
        }
        if activate_chunks {
            self.render_chunks_active = true;
        }
    }
}

fn render_chunk_boundary_merge(
    left: &PreparedRenderChunk,
    right: &PreparedRenderChunk,
) -> (bool, bool) {
    let (Some(left_batch), Some(right_batch)) =
        (left.render_batches.last(), right.render_batches.first())
    else {
        return (false, false);
    };
    match (&left_batch.primitive, &right_batch.primitive) {
        (
            RenderPrimitive::MegaPath { batch: left_index },
            RenderPrimitive::MegaPath { batch: right_index },
        ) => {
            let merge = left.mega_path_batches[*left_index].index_range.end
                == right.mega_path_batches[*right_index].index_range.start;
            (merge, merge)
        }
        _ => {
            let merge = left_batch.primitive == right_batch.primitive
                && left_batch.instance_range.end == right_batch.instance_range.start;
            (merge, false)
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
        render_chunks: &[],
        render_chunks_active: false,
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
        slots: &preparer.slots,
        complete_submission: false,
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
            z_index: 0.0,
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
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
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
    fn runtime_painter_order_preserves_instance_storage() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(2.0, 2.0)),
            object(2, GeometryRef::circle(0.5)),
        ]);
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(&frame, &[1, 0, 2]);
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

/// Packed primitive kind for one identity-free derived display occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivedDisplayPrimitive {
    Circle,
    Rectangle,
    Line,
    Path { batch: usize },
}

/// One lookup row into the transient derived-instance buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedDerivedDisplaySlot {
    pub anchor_object_index: u32,
    pub occurrence_index: u32,
    pub primitive: DerivedDisplayPrimitive,
    pub instance_index: usize,
}

/// Painter-level merge descriptor. Stable entries retain their execution slot;
/// derived entries retain only their plan-local occurrence ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPainterItem {
    Stable { object_index: u32 },
    Derived { occurrence_index: u32 },
}

/// Transient packed geometry for one renderer publication.
///
/// These arrays deliberately contain no `ObjectId` side tables. They are rebuilt
/// from the publication's identity-free derived rows and discarded independently of
/// the stable `FramePreparer` storage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedDerivedDisplay {
    pub circles: Vec<crate::CircleInstance>,
    pub rectangles: Vec<crate::RectangleInstance>,
    pub lines: Vec<crate::LineInstance>,
    pub paths: Vec<crate::PathInstance>,
    pub path_vertices: Vec<crate::PathVertex>,
    pub path_indices: Vec<u32>,
    pub path_batches: Vec<crate::PathBatch>,
    pub slots: Vec<PreparedDerivedDisplaySlot>,
    pub painter_items: Vec<DisplayPainterItem>,
}

impl PreparedDerivedDisplay {
    pub fn slot_for_occurrence(&self, occurrence_index: u32) -> Option<PreparedDerivedDisplaySlot> {
        self.slots
            .iter()
            .copied()
            .find(|slot| slot.occurrence_index == occurrence_index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivedDisplayRenderError {
    MissingAnchorInPainterOrder(u32),
    ZIndexDiffersFromAnchor(u32),
    DerivedObjectNotPresent(u32),
    UnsupportedContent(u32),
    UnsupportedGeometry(u32),
}

impl std::fmt::Display for DerivedDisplayRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::MissingAnchorInPainterOrder(anchor) => write!(
                formatter,
                "derived display anchor {anchor} is absent from the stable painter order"
            ),
            Self::ZIndexDiffersFromAnchor(occurrence) => write!(
                formatter,
                "derived display occurrence {occurrence} changes z-index relative to its source anchor"
            ),
            Self::DerivedObjectNotPresent(occurrence) => write!(
                formatter,
                "derived display occurrence {occurrence} is not present"
            ),
            Self::UnsupportedContent(occurrence) => write!(
                formatter,
                "derived display occurrence {occurrence} requires non-geometry content packing"
            ),
            Self::UnsupportedGeometry(occurrence) => write!(
                formatter,
                "derived display occurrence {occurrence} could not be prepared as transient geometry"
            ),
        }
    }
}

impl std::error::Error for DerivedDisplayRenderError {}

/// Pack analytic derived display rows and merge their occurrence order immediately
/// after each stable source anchor without modifying the stable frame preparer.
///
/// Vector paths reuse the renderer's shared tessellation semantics and ordinary path
/// pipeline without receiving stable identity. External geometry and text still fail
/// closed rather than fabricating a stable `ObjectId` merely to reuse retained slots.
pub fn prepare_derived_display(
    publication: &noon_runtime::RendererPublication<'_>,
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    prepare_derived_display_inner(publication, None, None)
}

/// Prepare only derived occurrences whose real source anchor participates in this
/// viewport projection. Stable and derived painter ordering still comes from the
/// authoritative publication order rather than candidate order.
pub fn prepare_derived_display_visible(
    publication: &noon_runtime::RendererPublication<'_>,
    visible_object_indices: &[usize],
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    let visible = visible_object_indices
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    prepare_derived_display_inner(publication, Some(&visible), None)
}

pub(crate) fn prepare_derived_display_visible_cached(
    publication: &noon_runtime::RendererPublication<'_>,
    visible_object_indices: &[usize],
    path_preparer: &mut crate::FramePreparer,
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    let visible = visible_object_indices
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    prepare_derived_display_inner(publication, Some(&visible), Some(path_preparer))
}

fn prepare_derived_display_inner(
    publication: &noon_runtime::RendererPublication<'_>,
    visible: Option<&std::collections::HashSet<usize>>,
    mut path_preparer: Option<&mut crate::FramePreparer>,
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    let mut by_anchor =
        std::collections::BTreeMap::<u32, Vec<&noon_runtime::DerivedDisplayObject>>::new();
    for object in publication.derived_display_objects() {
        if visible
            .is_some_and(|visible| !visible.contains(&(object.anchor_object_index() as usize)))
        {
            continue;
        }
        let anchor = object.anchor_object_index();
        let state = object.state();
        if state.z_index != publication.frame().objects[anchor as usize].z_index {
            return Err(DerivedDisplayRenderError::ZIndexDiffersFromAnchor(
                object.occurrence_index(),
            ));
        }
        by_anchor.entry(anchor).or_default().push(object);
    }
    for objects in by_anchor.values_mut() {
        objects.sort_unstable_by_key(|object| object.occurrence_index());
    }

    let mut prepared = PreparedDerivedDisplay::default();
    let mut seen_anchors = HashSet::with_capacity(by_anchor.len());
    for &object_index in publication.painter_order() {
        if visible.is_some_and(|visible| !visible.contains(&(object_index as usize))) {
            continue;
        }
        prepared
            .painter_items
            .push(DisplayPainterItem::Stable { object_index });
        let Some(objects) = by_anchor.get(&object_index) else {
            continue;
        };
        seen_anchors.insert(object_index);
        for &object in objects {
            pack_derived_display_object(object, &mut prepared, path_preparer.as_deref_mut())?;
            prepared.painter_items.push(DisplayPainterItem::Derived {
                occurrence_index: object.occurrence_index(),
            });
        }
    }
    if let Some(&anchor) = by_anchor
        .keys()
        .find(|anchor| !seen_anchors.contains(anchor))
    {
        return Err(DerivedDisplayRenderError::MissingAnchorInPainterOrder(
            anchor,
        ));
    }
    Ok(prepared)
}

fn pack_derived_display_object(
    object: &noon_runtime::DerivedDisplayObject,
    prepared: &mut PreparedDerivedDisplay,
    path_preparer: Option<&mut crate::FramePreparer>,
) -> Result<(), DerivedDisplayRenderError> {
    let state = object.state();
    let occurrence = object.occurrence_index();
    if !state.presence {
        return Err(DerivedDisplayRenderError::DerivedObjectNotPresent(
            occurrence,
        ));
    }
    let geometry = state
        .effective_render_geometry()
        .ok_or(DerivedDisplayRenderError::UnsupportedContent(occurrence))?;
    let render_transform = state.effective_render_transform();
    let transform: crate::PackedTransform = render_transform.into();
    let style = pack_derived_display_style(state);
    let (primitive, instance_index) = match geometry {
        noon_core::GeometryRef::Circle { radius } => {
            let index = prepared.circles.len();
            prepared.circles.push(crate::CircleInstance {
                transform,
                style,
                radius: *radius,
                padding: [state.reveal.clamp(0.0, 1.0), 0.0, 0.0],
            });
            (DerivedDisplayPrimitive::Circle, index)
        }
        noon_core::GeometryRef::Rectangle { size } => {
            let index = prepared.rectangles.len();
            prepared.rectangles.push(crate::RectangleInstance {
                transform,
                style,
                size: [size.x, size.y],
                padding: [0.0; 2],
            });
            (DerivedDisplayPrimitive::Rectangle, index)
        }
        noon_core::GeometryRef::Line { start, end } => {
            let index = prepared.lines.len();
            let mut transform = transform;
            transform.padding = state.reveal.clamp(0.0, 1.0);
            let mut style = style;
            style.stroke_enabled |= match state.style.stroke_cap {
                noon_core::StrokeCap::Round => 0,
                noon_core::StrokeCap::Butt => 1 << 2,
                noon_core::StrokeCap::Square => 2 << 2,
            };
            prepared.lines.push(crate::LineInstance {
                transform,
                style,
                start: [start.x, start.y],
                end: [end.x, end.y],
            });
            (DerivedDisplayPrimitive::Line, index)
        }
        noon_core::GeometryRef::VectorPath(path) => {
            if let Some(path_preparer) = path_preparer {
                let (mesh, _) = path_preparer
                    .cached_path_mesh(path, state.style, render_transform)
                    .map_err(|_| DerivedDisplayRenderError::UnsupportedGeometry(occurrence))?;
                pack_derived_path_mesh(prepared, mesh, state, render_transform, style)
            } else {
                let mesh = crate::tessellate_path_mesh(path, state.style, render_transform)
                    .map_err(|_| DerivedDisplayRenderError::UnsupportedGeometry(occurrence))?;
                pack_derived_path_mesh(prepared, &mesh, state, render_transform, style)
            }
        }
        noon_core::GeometryRef::External(_) => {
            return Err(DerivedDisplayRenderError::UnsupportedGeometry(occurrence));
        }
    };
    prepared.slots.push(PreparedDerivedDisplaySlot {
        anchor_object_index: object.anchor_object_index(),
        occurrence_index: occurrence,
        primitive,
        instance_index,
    });
    Ok(())
}

fn pack_derived_path_mesh(
    prepared: &mut PreparedDerivedDisplay,
    mesh: &noon_geometry::TessellatedPath,
    state: &noon_runtime::DerivedDisplayObjectState,
    render_transform: noon_core::Transform2D,
    style: crate::PackedStyle,
) -> (DerivedDisplayPrimitive, usize) {
    let vertex_start = u32::try_from(prepared.path_vertices.len())
        .expect("transient path vertex count exceeds renderer limits");
    prepared
        .path_vertices
        .extend(mesh.vertices.iter().map(|vertex| crate::PathVertex {
            position: [vertex.position.x, vertex.position.y],
            target_position: [vertex.target_position.x, vertex.target_position.y],
            surface: crate::pack_path_surface(vertex.surface, vertex.path_progress),
        }));
    let index_start = u32::try_from(prepared.path_indices.len())
        .expect("transient path index count exceeds renderer limits");
    prepared
        .path_indices
        .extend(mesh.indices.iter().map(|index| {
            index
                .checked_add(vertex_start)
                .expect("transient path index exceeds renderer limits")
        }));
    let index_end = u32::try_from(prepared.path_indices.len())
        .expect("transient path index count exceeds renderer limits");
    let index = prepared.paths.len();
    prepared.paths.push(crate::PathInstance {
        transform: crate::packed_path_transform(state.style, render_transform),
        style,
        path_params: [state.reveal.clamp(0.0, 1.0), state.morph.clamp(0.0, 1.0)],
    });
    let instance_start =
        u32::try_from(index).expect("transient path instance count exceeds renderer limits");
    let batch = prepared.path_batches.len();
    prepared.path_batches.push(crate::PathBatch {
        index_range: index_start..index_end,
        instance_range: instance_start..instance_start + 1,
    });
    (DerivedDisplayPrimitive::Path { batch }, index)
}

fn pack_derived_display_style(
    state: &noon_runtime::DerivedDisplayObjectState,
) -> crate::PackedStyle {
    let mut style: crate::PackedStyle = state.style.into();
    style.opacity *= state.appearance.clamp(0.0, 1.0);
    style
}

#[cfg(test)]
mod derived_display_tests {
    use noon_compile::{CompiledObject, CompiledScene};
    use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2};
    use noon_runtime::{DerivedDisplayObject, DerivedDisplayObjectState, SceneInstance};

    use super::*;

    fn runtime(geometries: Vec<GeometryRef>) -> SceneInstance {
        let objects = geometries
            .into_iter()
            .enumerate()
            .map(|(index, geometry)| {
                CompiledObject::new(
                    ObjectId::new(index as u64 + 1),
                    geometry,
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .collect();
        SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap())
    }

    fn state(geometry: GeometryRef) -> DerivedDisplayObjectState {
        DerivedDisplayObjectState {
            z_index: 0.0,
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        }
    }

    #[test]
    fn derived_circle_is_packed_without_object_id_storage() {
        let mut runtime = runtime(vec![GeometryRef::circle(1.0)]);
        let mut derived_state = state(GeometryRef::circle(0.5));
        derived_state.appearance = 0.25;
        derived_state.reveal = 0.5;
        derived_state.transform.translation = Vec2::new(2.0, -1.0);
        let derived = [DerivedDisplayObject::new(0, 7, derived_state)];
        let publication = runtime
            .take_renderer_publication()
            .with_derived_display_objects(&derived)
            .unwrap();

        let prepared = prepare_derived_display(&publication).unwrap();
        assert_eq!(prepared.circles.len(), 1);
        assert_eq!(prepared.circles[0].transform.translation, [2.0, -1.0]);
        assert_eq!(prepared.circles[0].style.opacity, 0.25);
        assert_eq!(prepared.circles[0].padding[0], 0.5);
        assert_eq!(
            prepared.slot_for_occurrence(7),
            Some(PreparedDerivedDisplaySlot {
                anchor_object_index: 0,
                occurrence_index: 7,
                primitive: DerivedDisplayPrimitive::Circle,
                instance_index: 0,
            })
        );
    }

    #[test]
    fn painter_projection_inserts_copies_after_stable_anchor_in_occurrence_order() {
        let mut runtime = runtime(vec![
            GeometryRef::circle(1.0),
            GeometryRef::rectangle(1.0, 1.0),
        ]);
        let derived = [
            DerivedDisplayObject::new(0, 4, state(GeometryRef::circle(0.4))),
            DerivedDisplayObject::new(0, 2, state(GeometryRef::circle(0.2))),
            DerivedDisplayObject::new(1, 8, state(GeometryRef::rectangle(0.5, 0.5))),
        ];
        let publication = runtime
            .take_renderer_publication()
            .with_derived_display_objects(&derived)
            .unwrap();

        let prepared = prepare_derived_display(&publication).unwrap();
        assert_eq!(
            prepared.painter_items,
            vec![
                DisplayPainterItem::Stable { object_index: 0 },
                DisplayPainterItem::Derived {
                    occurrence_index: 2
                },
                DisplayPainterItem::Derived {
                    occurrence_index: 4
                },
                DisplayPainterItem::Stable { object_index: 1 },
                DisplayPainterItem::Derived {
                    occurrence_index: 8
                },
            ]
        );
    }

    #[test]
    fn transient_renderer_packs_path_without_synthetic_id() {
        let mut runtime = runtime(vec![GeometryRef::circle(1.0)]);
        let path = noon_core::VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.5))
            .line_to(Vec2::new(0.5, -0.5))
            .line_to(Vec2::new(0.0, 0.5))
            .close();
        let mut path_state = state(GeometryRef::path(path));
        path_state.style.fill = Some(noon_core::Color::WHITE);
        path_state.style.stroke = None;
        path_state.morph = 0.25;
        let derived = [DerivedDisplayObject::new(0, 3, path_state)];
        let publication = runtime
            .take_renderer_publication()
            .with_derived_display_objects(&derived)
            .unwrap();

        let prepared = prepare_derived_display(&publication).unwrap();
        assert_eq!(prepared.paths.len(), 1);
        assert!(!prepared.path_vertices.is_empty());
        assert!(!prepared.path_indices.is_empty());
        assert_eq!(prepared.path_batches.len(), 1);
        assert_eq!(prepared.paths[0].path_params, [1.0, 0.25]);
        assert_eq!(
            prepared.slot_for_occurrence(3),
            Some(PreparedDerivedDisplaySlot {
                anchor_object_index: 0,
                occurrence_index: 3,
                primitive: DerivedDisplayPrimitive::Path { batch: 0 },
                instance_index: 0,
            })
        );
    }

    #[test]
    fn cached_transient_path_reuses_frame_preparer_mesh_without_repacking_stable_path() {
        let stable_path = noon_core::VectorPath::new()
            .move_to(Vec2::new(-1.0, -0.5))
            .line_to(Vec2::new(0.0, 0.75))
            .line_to(Vec2::new(1.0, -0.5))
            .close();
        let transient_path = noon_core::VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.25))
            .line_to(Vec2::new(0.0, 0.5))
            .line_to(Vec2::new(0.5, -0.25))
            .close();
        let mut runtime = runtime(vec![GeometryRef::path(stable_path)]);
        let mut path_state = state(GeometryRef::path(transient_path));
        path_state.style.fill = Some(noon_core::Color::WHITE);
        path_state.style.stroke = None;
        let derived = [DerivedDisplayObject::new(0, 7, path_state)];
        let publication = runtime
            .take_renderer_publication()
            .with_derived_display_objects(&derived)
            .unwrap();
        let mut preparer = crate::FramePreparer::new();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        {
            let stable = preparer.prepare(publication.frame());
            assert_eq!(stable.stats.full_rebuilds, 1);
        }
        assert_eq!(preparer.path_mesh_cache_len(), 1);

        let first =
            prepare_derived_display_visible_cached(&publication, &[0], &mut preparer).unwrap();
        assert_eq!(preparer.path_mesh_cache_len(), 2);
        let second =
            prepare_derived_display_visible_cached(&publication, &[0], &mut preparer).unwrap();
        assert_eq!(preparer.path_mesh_cache_len(), 2);
        assert_eq!(first.path_vertices, second.path_vertices);
        assert_eq!(first.path_indices, second.path_indices);

        let stable = preparer
            .prepare_incremental(publication.frame(), &noon_runtime::FrameChanges::default());
        assert_eq!(stable.stats.full_rebuilds, 0);
        assert_eq!(stable.stats.instances_repacked, 0);
        assert_eq!(stable.stats.geometry_cache_misses, 0);
    }

    #[test]
    fn painter_anchor_requires_source_z_index_and_stable_order_membership() {
        let mut runtime = runtime(vec![GeometryRef::circle(1.0)]);
        let mut different_z = state(GeometryRef::circle(0.5));
        different_z.z_index = 1.0;
        let derived = [DerivedDisplayObject::new(0, 9, different_z)];
        let publication = runtime
            .take_renderer_publication()
            .with_derived_display_objects(&derived)
            .unwrap();

        assert_eq!(
            prepare_derived_display(&publication).unwrap_err(),
            DerivedDisplayRenderError::ZIndexDiffersFromAnchor(9)
        );
    }
}
