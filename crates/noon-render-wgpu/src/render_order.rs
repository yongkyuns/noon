use std::{collections::HashSet, ops::Range};

use crate::{FramePreparer, PreparedFrame, PreparedSlot};
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

    /// Prepare one incremental frame while limiting ordered draw submission to the
    /// supplied retained-visibility candidates.
    ///
    /// `visible_object_indices` must already be in back-to-front painter order, as
    /// returned by the retained viewport query. The ordinary incremental preparation
    /// still updates packed instance and mesh caches for dirty objects; this method
    /// only rebuilds the lightweight ordered draw descriptors from visible slots, so
    /// offscreen objects remain resident and can re-enter without a cache rebuild.
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

        let stats = self.prepare_incremental(frame, changes).stats;
        self.render_batches.clear();
        for &object_index in visible_object_indices {
            push_slot_batches(&mut self.render_batches, self.slots[object_index]);
        }
        self.rebuild_mega_render_batches();

        Ok(self.prepared_frame(
            frame.time,
            stats.capacity_growths,
            stats.instances_repacked,
            stats.geometry_cache_misses,
            stats.path_vertices_repacked,
            stats.path_indices_repacked,
            stats.structural_slots_added,
            stats.structural_slots_retired,
            stats.full_rebuilds,
        ))
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
            geometry,
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
        assert_eq!(prepared.render_batches[1].primitive, RenderPrimitive::Circle);
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
}
