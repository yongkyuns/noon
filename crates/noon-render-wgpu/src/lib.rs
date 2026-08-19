//! CPU-side preparation for Noon's wgpu renderer.
//!
//! This layer defines deterministic packed instance records and batches analytic
//! primitives before they are uploaded to wgpu. The same preparation path is
//! used by native and browser backends.

#![forbid(unsafe_code)]

mod gpu;

pub use gpu::*;

use bytemuck::{Pod, Zeroable};
use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use std::ops::Range;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PackedTransform {
    pub translation: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub padding: f32,
}

impl From<Transform2D> for PackedTransform {
    fn from(value: Transform2D) -> Self {
        Self {
            translation: [value.translation.x, value.translation.y],
            scale: [value.scale.x, value.scale.y],
            rotation: value.rotation,
            padding: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PackedStyle {
    pub fill: [f32; 4],
    pub stroke: [f32; 4],
    pub stroke_width: f32,
    pub opacity: f32,
    pub fill_enabled: u32,
    pub stroke_enabled: u32,
}

impl From<Style> for PackedStyle {
    fn from(value: Style) -> Self {
        let (fill, fill_enabled) = pack_optional_color(value.fill);
        let (stroke, stroke_enabled) = pack_optional_color(value.stroke);
        Self {
            fill,
            stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled,
            stroke_enabled,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CircleInstance {
    pub transform: PackedTransform,
    pub style: PackedStyle,
    pub radius: f32,
    pub padding: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct RectangleInstance {
    pub transform: PackedTransform,
    pub style: PackedStyle,
    pub size: [f32; 2],
    pub padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct LineInstance {
    pub transform: PackedTransform,
    pub style: PackedStyle,
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub batch_count: usize,
    pub instance_count: usize,
    pub unsupported_count: usize,
    pub capacity_growths: usize,
    pub instances_repacked: usize,
    pub dirty_instance_count: usize,
}

#[derive(Debug)]
pub struct PreparedFrame<'a> {
    pub time: f64,
    pub circle_ids: &'a [ObjectId],
    pub circles: &'a [CircleInstance],
    pub rectangle_ids: &'a [ObjectId],
    pub rectangles: &'a [RectangleInstance],
    pub line_ids: &'a [ObjectId],
    pub lines: &'a [LineInstance],
    pub unsupported: &'a [ObjectId],
    pub circle_dirty_ranges: &'a [Range<usize>],
    pub rectangle_dirty_ranges: &'a [Range<usize>],
    pub line_dirty_ranges: &'a [Range<usize>],
    pub stats: RenderStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreparedSlot {
    Circle(usize),
    Rectangle(usize),
    Line(usize),
    Unsupported(usize),
}

#[derive(Debug, Default)]
pub struct FramePreparer {
    circle_ids: Vec<ObjectId>,
    circles: Vec<CircleInstance>,
    rectangle_ids: Vec<ObjectId>,
    rectangles: Vec<RectangleInstance>,
    line_ids: Vec<ObjectId>,
    lines: Vec<LineInstance>,
    unsupported: Vec<ObjectId>,
    slots: Vec<PreparedSlot>,
    circle_dirty_ranges: Vec<Range<usize>>,
    rectangle_dirty_ranges: Vec<Range<usize>>,
    line_dirty_ranges: Vec<Range<usize>>,
    initialized: bool,
}

impl FramePreparer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prepare<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        self.rebuild(frame)
    }

    /// Updates cached instance records using the runtime's consumed change set.
    ///
    /// Structural changes and seeks rebuild all records. Forward animation and
    /// value patches repack only the object indices named by `changes`.
    pub fn prepare_incremental<'a>(
        &'a mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> PreparedFrame<'a> {
        if !self.initialized
            || changes.is_all()
            || self.slots.len() != frame.objects.len()
            || !changes
                .object_indices()
                .iter()
                .all(|&index| self.slot_matches(frame, index))
        {
            return self.rebuild(frame);
        }

        self.clear_dirty_ranges();
        let mut instances_repacked = 0;
        for &object_index in changes.object_indices() {
            let object = &frame.objects[object_index];
            match self.slots[object_index] {
                PreparedSlot::Circle(index) => {
                    let packed = pack_circle(object);
                    instances_repacked += 1;
                    if self.circles[index] != packed {
                        self.circles[index] = packed;
                        push_dirty_range(&mut self.circle_dirty_ranges, index);
                    }
                }
                PreparedSlot::Rectangle(index) => {
                    let packed = pack_rectangle(object);
                    instances_repacked += 1;
                    if self.rectangles[index] != packed {
                        self.rectangles[index] = packed;
                        push_dirty_range(&mut self.rectangle_dirty_ranges, index);
                    }
                }
                PreparedSlot::Line(index) => {
                    let packed = pack_line(object);
                    instances_repacked += 1;
                    if self.lines[index] != packed {
                        self.lines[index] = packed;
                        push_dirty_range(&mut self.line_dirty_ranges, index);
                    }
                }
                PreparedSlot::Unsupported(_) => {}
            }
        }

        self.prepared_frame(frame.time, 0, instances_repacked)
    }

    fn rebuild<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        let capacities_before = self.capacities();

        self.circle_ids.clear();
        self.circles.clear();
        self.rectangle_ids.clear();
        self.rectangles.clear();
        self.line_ids.clear();
        self.lines.clear();
        self.unsupported.clear();
        self.slots.clear();
        self.clear_dirty_ranges();

        for object in &frame.objects {
            match object.geometry {
                GeometryRef::Circle { .. } => {
                    self.slots.push(PreparedSlot::Circle(self.circles.len()));
                    self.circle_ids.push(object.id);
                    self.circles.push(pack_circle(object));
                }
                GeometryRef::Rectangle { .. } => {
                    self.slots
                        .push(PreparedSlot::Rectangle(self.rectangles.len()));
                    self.rectangle_ids.push(object.id);
                    self.rectangles.push(pack_rectangle(object));
                }
                GeometryRef::Line { .. } => {
                    self.slots.push(PreparedSlot::Line(self.lines.len()));
                    self.line_ids.push(object.id);
                    self.lines.push(pack_line(object));
                }
                GeometryRef::External(_) => {
                    self.slots
                        .push(PreparedSlot::Unsupported(self.unsupported.len()));
                    self.unsupported.push(object.id);
                }
            }
        }

        if !self.circles.is_empty() {
            self.circle_dirty_ranges.push(0..self.circles.len());
        }
        if !self.rectangles.is_empty() {
            self.rectangle_dirty_ranges.push(0..self.rectangles.len());
        }
        if !self.lines.is_empty() {
            self.line_dirty_ranges.push(0..self.lines.len());
        }
        self.initialized = true;

        let capacities_after = self.capacities();
        let capacity_growths = capacities_before
            .into_iter()
            .zip(capacities_after)
            .filter(|(before, after)| after > before)
            .count();

        self.prepared_frame(
            frame.time,
            capacity_growths,
            self.circles.len() + self.rectangles.len() + self.lines.len(),
        )
    }

    fn prepared_frame(
        &self,
        time: f64,
        capacity_growths: usize,
        instances_repacked: usize,
    ) -> PreparedFrame<'_> {
        let batch_count = usize::from(!self.circles.is_empty())
            + usize::from(!self.rectangles.is_empty())
            + usize::from(!self.lines.is_empty());
        let dirty_instance_count = dirty_len(&self.circle_dirty_ranges)
            + dirty_len(&self.rectangle_dirty_ranges)
            + dirty_len(&self.line_dirty_ranges);
        PreparedFrame {
            time,
            circle_ids: &self.circle_ids,
            circles: &self.circles,
            rectangle_ids: &self.rectangle_ids,
            rectangles: &self.rectangles,
            line_ids: &self.line_ids,
            lines: &self.lines,
            unsupported: &self.unsupported,
            circle_dirty_ranges: &self.circle_dirty_ranges,
            rectangle_dirty_ranges: &self.rectangle_dirty_ranges,
            line_dirty_ranges: &self.line_dirty_ranges,
            stats: RenderStats {
                batch_count,
                instance_count: self.circles.len() + self.rectangles.len() + self.lines.len(),
                unsupported_count: self.unsupported.len(),
                capacity_growths,
                instances_repacked,
                dirty_instance_count,
            },
        }
    }

    fn clear_dirty_ranges(&mut self) {
        self.circle_dirty_ranges.clear();
        self.rectangle_dirty_ranges.clear();
        self.line_dirty_ranges.clear();
    }

    fn slot_matches(&self, frame: &FrameState, object_index: usize) -> bool {
        let Some(object) = frame.objects.get(object_index) else {
            return false;
        };
        match self.slots.get(object_index) {
            Some(PreparedSlot::Circle(index)) => {
                matches!(object.geometry, GeometryRef::Circle { .. })
                    && self.circle_ids.get(*index) == Some(&object.id)
            }
            Some(PreparedSlot::Rectangle(index)) => {
                matches!(object.geometry, GeometryRef::Rectangle { .. })
                    && self.rectangle_ids.get(*index) == Some(&object.id)
            }
            Some(PreparedSlot::Line(index)) => {
                matches!(object.geometry, GeometryRef::Line { .. })
                    && self.line_ids.get(*index) == Some(&object.id)
            }
            Some(PreparedSlot::Unsupported(index)) => {
                matches!(object.geometry, GeometryRef::External(_))
                    && self.unsupported.get(*index) == Some(&object.id)
            }
            None => false,
        }
    }

    fn capacities(&self) -> [usize; 11] {
        [
            self.circle_ids.capacity(),
            self.circles.capacity(),
            self.rectangle_ids.capacity(),
            self.rectangles.capacity(),
            self.line_ids.capacity(),
            self.lines.capacity(),
            self.unsupported.capacity(),
            self.slots.capacity(),
            self.circle_dirty_ranges.capacity(),
            self.rectangle_dirty_ranges.capacity(),
            self.line_dirty_ranges.capacity(),
        ]
    }
}

fn pack_circle(object: &FrameObjectState) -> CircleInstance {
    let GeometryRef::Circle { radius } = object.geometry else {
        unreachable!("circle slot must retain circle geometry")
    };
    CircleInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        radius,
        padding: [0.0; 3],
    }
}

fn pack_rectangle(object: &FrameObjectState) -> RectangleInstance {
    let GeometryRef::Rectangle { size } = object.geometry else {
        unreachable!("rectangle slot must retain rectangle geometry")
    };
    RectangleInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        size: [size.x, size.y],
        padding: [0.0; 2],
    }
}

fn pack_line(object: &FrameObjectState) -> LineInstance {
    let GeometryRef::Line { start, end } = object.geometry else {
        unreachable!("line slot must retain line geometry")
    };
    LineInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
}

fn push_dirty_range(ranges: &mut Vec<Range<usize>>, index: usize) {
    if let Some(last) = ranges.last_mut() {
        if last.end == index {
            last.end += 1;
            return;
        }
    }
    ranges.push(index..index + 1);
}

fn dirty_len(ranges: &[Range<usize>]) -> usize {
    ranges.iter().map(Range::len).sum()
}

fn pack_optional_color(color: Option<Color>) -> ([f32; 4], u32) {
    match color {
        Some(color) => ([color.red, color.green, color.blue, color.alpha], 1),
        None => ([0.0; 4], 0),
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Color, GeometryId, Vec2};
    use noon_runtime::FrameObjectState;

    use super::*;

    fn object(id: u64, geometry: GeometryRef) -> FrameObjectState {
        FrameObjectState {
            id: ObjectId::new(id),
            geometry,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
        }
    }

    fn frame(objects: Vec<FrameObjectState>) -> FrameState {
        FrameState {
            time: 1.25,
            objects,
        }
    }

    #[test]
    fn packed_instance_layout_is_stable() {
        assert_eq!(std::mem::size_of::<PackedTransform>(), 24);
        assert_eq!(std::mem::size_of::<PackedStyle>(), 48);
        assert_eq!(std::mem::size_of::<CircleInstance>(), 88);
        assert_eq!(std::mem::size_of::<RectangleInstance>(), 88);
        assert_eq!(std::mem::size_of::<LineInstance>(), 88);
    }

    #[test]
    fn one_hundred_thousand_circles_form_one_batch() {
        let objects = (0..100_000_u64)
            .map(|id| object(id, GeometryRef::circle(1.0)))
            .collect();
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.batch_count, 1);
        assert_eq!(prepared.stats.instance_count, 100_000);
        assert_eq!(prepared.circles.len(), 100_000);
        assert!(prepared.rectangles.is_empty());
        assert_eq!(prepared.circle_ids[99_999], ObjectId::new(99_999));
    }

    #[test]
    fn mixed_primitives_batch_by_pipeline_not_object() {
        let mut objects = Vec::with_capacity(30_000);
        for id in 0..10_000_u64 {
            objects.push(object(id, GeometryRef::circle(1.0)));
        }
        for id in 10_000..20_000_u64 {
            objects.push(object(id, GeometryRef::rectangle(2.0, 3.0)));
        }
        for id in 20_000..30_000_u64 {
            objects.push(object(
                id,
                GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
            ));
        }
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.instance_count, 30_000);
        assert_eq!(prepared.stats.batch_count, 3);
        assert_eq!(prepared.circles.len(), 10_000);
        assert_eq!(prepared.rectangles.len(), 10_000);
        assert_eq!(prepared.lines.len(), 10_000);
    }

    #[test]
    fn packing_preserves_transform_and_style() {
        let mut state = object(7, GeometryRef::circle(2.5));
        state.transform = Transform2D {
            translation: Vec2::new(4.0, -3.0),
            rotation: 0.75,
            scale: Vec2::new(2.0, 0.5),
        };
        state.style = Style {
            fill: Some(Color::rgba(0.1, 0.2, 0.3, 0.4)),
            stroke: Some(Color::rgb(0.8, 0.7, 0.6)),
            stroke_width: 3.0,
            opacity: 0.5,
        };
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        let instance = prepared.circles[0];

        assert_eq!(instance.transform.translation, [4.0, -3.0]);
        assert_eq!(instance.transform.scale, [2.0, 0.5]);
        assert_eq!(instance.transform.rotation, 0.75);
        assert_eq!(instance.radius, 2.5);
        assert_eq!(instance.style.fill, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(instance.style.stroke, [0.8, 0.7, 0.6, 1.0]);
        assert_eq!(instance.style.fill_enabled, 1);
        assert_eq!(instance.style.stroke_enabled, 1);
        assert_eq!(instance.style.stroke_width, 3.0);
        assert_eq!(instance.style.opacity, 0.5);
    }

    #[test]
    fn repeated_preparation_reuses_allocated_capacity() {
        let frame = frame(
            (0..10_000_u64)
                .map(|id| object(id, GeometryRef::circle(1.0)))
                .collect(),
        );
        let mut preparer = FramePreparer::new();

        assert!(preparer.prepare(&frame).stats.capacity_growths > 0);
        assert_eq!(preparer.prepare(&frame).stats.capacity_growths, 0);
    }

    #[test]
    fn line_packing_preserves_semantic_endpoints_and_style() {
        let mut state = object(
            8,
            GeometryRef::line(Vec2::new(-2.0, 1.5), Vec2::new(3.0, -0.5)),
        );
        state.style = Style {
            fill: None,
            stroke: Some(Color::rgb(0.2, 0.8, 0.4)),
            stroke_width: 0.125,
            opacity: 0.75,
        };
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        let instance = prepared.lines[0];

        assert_eq!(prepared.line_ids, &[ObjectId::new(8)]);
        assert_eq!(instance.start, [-2.0, 1.5]);
        assert_eq!(instance.end, [3.0, -0.5]);
        assert_eq!(instance.style.stroke, [0.2, 0.8, 0.4, 1.0]);
        assert_eq!(instance.style.stroke_enabled, 1);
        assert_eq!(instance.style.stroke_width, 0.125);
        assert_eq!(instance.style.opacity, 0.75);
    }

    #[test]
    fn unsupported_geometry_is_reported_explicitly() {
        let frame = frame(vec![object(42, GeometryRef::External(GeometryId::new(3)))]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.instance_count, 0);
        assert_eq!(prepared.stats.unsupported_count, 1);
        assert_eq!(prepared.unsupported, &[ObjectId::new(42)]);
    }

    #[test]
    fn unchanged_incremental_frame_skips_all_repacking() {
        let frame = frame(
            (0..100_000_u64)
                .map(|id| object(id, GeometryRef::circle(1.0)))
                .collect(),
        );
        let mut preparer = FramePreparer::new();
        assert_eq!(preparer.prepare(&frame).stats.instances_repacked, 100_000);

        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::default());

        assert_eq!(prepared.stats.instances_repacked, 0);
        assert_eq!(prepared.stats.dirty_instance_count, 0);
        assert!(prepared.circle_dirty_ranges.is_empty());
    }

    #[test]
    fn changed_objects_repack_and_dirty_only_their_packed_ranges() {
        let mut frame = frame(vec![
            object(1, GeometryRef::circle(1.0)),
            object(2, GeometryRef::rectangle(2.0, 3.0)),
            object(3, GeometryRef::circle(4.0)),
        ]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        frame.objects[2].transform.translation = Vec2::new(3.0, 4.0);

        let changes = FrameChanges::objects(vec![2]);
        let prepared = preparer.prepare_incremental(&frame, &changes);

        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert_eq!(prepared.circle_dirty_ranges.len(), 1);
        assert_eq!(prepared.circle_dirty_ranges[0], 1..2);
        assert!(prepared.rectangle_dirty_ranges.is_empty());
        assert_eq!(prepared.circles[1].transform.translation, [3.0, 4.0]);
    }

    #[test]
    fn incompatible_incremental_layout_falls_back_to_full_rebuild() {
        let mut frame = frame(vec![object(1, GeometryRef::circle(1.0))]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        frame.objects[0].geometry = GeometryRef::rectangle(2.0, 3.0);

        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(prepared.circles.is_empty());
        assert_eq!(prepared.rectangle_dirty_ranges.len(), 1);
        assert_eq!(prepared.rectangle_dirty_ranges[0], 0..1);
    }
}
