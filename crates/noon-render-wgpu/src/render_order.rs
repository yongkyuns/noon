use std::ops::Range;

use crate::{FramePreparer, PreparedSlot};
use noon_runtime::FrameState;

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
            live: true,
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
}
