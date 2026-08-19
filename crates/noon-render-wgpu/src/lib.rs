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
use noon_runtime::FrameState;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub batch_count: usize,
    pub instance_count: usize,
    pub unsupported_count: usize,
    pub capacity_growths: usize,
}

#[derive(Debug)]
pub struct PreparedFrame<'a> {
    pub time: f64,
    pub circle_ids: &'a [ObjectId],
    pub circles: &'a [CircleInstance],
    pub rectangle_ids: &'a [ObjectId],
    pub rectangles: &'a [RectangleInstance],
    pub unsupported: &'a [ObjectId],
    pub stats: RenderStats,
}

#[derive(Debug, Default)]
pub struct FramePreparer {
    circle_ids: Vec<ObjectId>,
    circles: Vec<CircleInstance>,
    rectangle_ids: Vec<ObjectId>,
    rectangles: Vec<RectangleInstance>,
    unsupported: Vec<ObjectId>,
}

impl FramePreparer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prepare<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        let capacities_before = self.capacities();

        self.circle_ids.clear();
        self.circles.clear();
        self.rectangle_ids.clear();
        self.rectangles.clear();
        self.unsupported.clear();

        for object in &frame.objects {
            match object.geometry {
                GeometryRef::Circle { radius } => {
                    self.circle_ids.push(object.id);
                    self.circles.push(CircleInstance {
                        transform: object.transform.into(),
                        style: object.style.into(),
                        radius,
                        padding: [0.0; 3],
                    });
                }
                GeometryRef::Rectangle { size } => {
                    self.rectangle_ids.push(object.id);
                    self.rectangles.push(RectangleInstance {
                        transform: object.transform.into(),
                        style: object.style.into(),
                        size: [size.x, size.y],
                        padding: [0.0; 2],
                    });
                }
                GeometryRef::External(_) => self.unsupported.push(object.id),
            }
        }

        let capacities_after = self.capacities();
        let capacity_growths = capacities_before
            .into_iter()
            .zip(capacities_after)
            .filter(|(before, after)| after > before)
            .count();

        let mut batch_count = 0;
        if !self.circles.is_empty() {
            batch_count += 1;
        }
        if !self.rectangles.is_empty() {
            batch_count += 1;
        }

        PreparedFrame {
            time: frame.time,
            circle_ids: &self.circle_ids,
            circles: &self.circles,
            rectangle_ids: &self.rectangle_ids,
            rectangles: &self.rectangles,
            unsupported: &self.unsupported,
            stats: RenderStats {
                batch_count,
                instance_count: self.circles.len() + self.rectangles.len(),
                unsupported_count: self.unsupported.len(),
                capacity_growths,
            },
        }
    }

    fn capacities(&self) -> [usize; 5] {
        [
            self.circle_ids.capacity(),
            self.circles.capacity(),
            self.rectangle_ids.capacity(),
            self.rectangles.capacity(),
            self.unsupported.capacity(),
        ]
    }
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
        let mut objects = Vec::with_capacity(20_000);
        for id in 0..10_000_u64 {
            objects.push(object(id, GeometryRef::circle(1.0)));
        }
        for id in 10_000..20_000_u64 {
            objects.push(object(id, GeometryRef::rectangle(2.0, 3.0)));
        }
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.instance_count, 20_000);
        assert_eq!(prepared.stats.batch_count, 2);
        assert_eq!(prepared.circles.len(), 10_000);
        assert_eq!(prepared.rectangles.len(), 10_000);
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
    fn unsupported_geometry_is_reported_explicitly() {
        let frame = frame(vec![object(42, GeometryRef::External(GeometryId::new(3)))]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.instance_count, 0);
        assert_eq!(prepared.stats.unsupported_count, 1);
        assert_eq!(prepared.unsupported, &[ObjectId::new(42)]);
    }
}
