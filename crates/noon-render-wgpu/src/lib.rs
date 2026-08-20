//! CPU-side preparation for Noon's wgpu renderer.
//!
//! This layer defines deterministic packed instance records and batches analytic
//! primitives before they are uploaded to wgpu. The same preparation path is
//! used by native and browser backends.

#![forbid(unsafe_code)]

mod gpu;

pub use gpu::*;

use bytemuck::{Pod, Zeroable};
use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D, VectorPath};
use noon_geometry::{PathSurface, TessellatedPath};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use std::ops::Range;

// `f32` represents every integer through 2^24 exactly. Keeping the encoded
// progress in this exact domain avoids endpoint wraparound at reveal == 1.0
// while retaining far more precision than pixel-scale path clipping needs.
const PATH_PROGRESS_MAX: u32 = 16_777_215;

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

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PathInstance {
    pub transform: PackedTransform,
    pub style: PackedStyle,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PathVertex {
    pub position: [f32; 2],
    /// Low bit is surface (0 fill, 1 stroke); the next 24 bits are normalized
    /// ordered path progress. Keeping this packed preserves the existing GPU
    /// vertex stride while adding reveal metadata.
    pub surface: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathBatch {
    pub index_range: Range<u32>,
    pub instance_range: Range<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub batch_count: usize,
    pub instance_count: usize,
    pub unsupported_count: usize,
    pub capacity_growths: usize,
    pub instances_repacked: usize,
    pub dirty_instance_count: usize,
    pub geometry_cache_misses: usize,
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
    pub path_ids: &'a [ObjectId],
    pub paths: &'a [PathInstance],
    pub path_vertices: &'a [PathVertex],
    pub path_indices: &'a [u32],
    pub path_batches: &'a [PathBatch],
    pub unsupported: &'a [ObjectId],
    pub circle_dirty_ranges: &'a [Range<usize>],
    pub rectangle_dirty_ranges: &'a [Range<usize>],
    pub line_dirty_ranges: &'a [Range<usize>],
    pub path_dirty_ranges: &'a [Range<usize>],
    pub path_geometry_dirty: bool,
    pub stats: RenderStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreparedSlot {
    Circle(usize),
    Rectangle(usize),
    Line(usize),
    Path { index: usize, batch: usize },
    Unsupported(usize),
}

#[derive(Clone, Debug)]
struct CachedPathMesh {
    path: VectorPath,
    stroke_width_bits: u32,
    mesh: TessellatedPath,
}

#[derive(Debug)]
struct PathGroup {
    cache_index: usize,
    ids: Vec<ObjectId>,
    instances: Vec<PathInstance>,
}

#[derive(Debug, Default)]
pub struct FramePreparer {
    circle_ids: Vec<ObjectId>,
    circles: Vec<CircleInstance>,
    rectangle_ids: Vec<ObjectId>,
    rectangles: Vec<RectangleInstance>,
    line_ids: Vec<ObjectId>,
    lines: Vec<LineInstance>,
    path_ids: Vec<ObjectId>,
    paths: Vec<PathInstance>,
    path_vertices: Vec<PathVertex>,
    path_indices: Vec<u32>,
    path_batches: Vec<PathBatch>,
    path_batch_cache_indices: Vec<usize>,
    path_mesh_cache: Vec<CachedPathMesh>,
    unsupported: Vec<ObjectId>,
    slots: Vec<PreparedSlot>,
    circle_dirty_ranges: Vec<Range<usize>>,
    rectangle_dirty_ranges: Vec<Range<usize>>,
    line_dirty_ranges: Vec<Range<usize>>,
    path_dirty_ranges: Vec<Range<usize>>,
    path_geometry_dirty: bool,
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
                PreparedSlot::Path { index, .. } => {
                    let packed = pack_path(object, frame.reveal(object_index));
                    instances_repacked += 1;
                    if self.paths[index] != packed {
                        self.paths[index] = packed;
                        push_dirty_range(&mut self.path_dirty_ranges, index);
                    }
                }
                PreparedSlot::Unsupported(_) => {}
            }
        }

        self.prepared_frame(frame.time, 0, instances_repacked, 0)
    }

    fn rebuild<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        let capacities_before = self.capacities();

        self.circle_ids.clear();
        self.circles.clear();
        self.rectangle_ids.clear();
        self.rectangles.clear();
        self.line_ids.clear();
        self.lines.clear();
        self.path_ids.clear();
        self.paths.clear();
        self.path_batches.clear();
        self.path_batch_cache_indices.clear();
        self.unsupported.clear();
        self.slots.clear();
        self.clear_dirty_ranges();

        let mut path_groups = Vec::<PathGroup>::new();
        let mut geometry_cache_misses = 0;
        for (object_index, object) in frame.objects.iter().enumerate() {
            match &object.geometry {
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
                GeometryRef::VectorPath(path) => {
                    let cache_index = match self.cache_path_mesh(path, object.style.stroke_width) {
                        Ok((index, cache_miss)) => {
                            geometry_cache_misses += usize::from(cache_miss);
                            index
                        }
                        Err(_) => {
                            self.slots
                                .push(PreparedSlot::Unsupported(self.unsupported.len()));
                            self.unsupported.push(object.id);
                            continue;
                        }
                    };
                    let batch = path_groups
                        .iter()
                        .position(|group| group.cache_index == cache_index)
                        .unwrap_or_else(|| {
                            path_groups.push(PathGroup {
                                cache_index,
                                ids: Vec::new(),
                                instances: Vec::new(),
                            });
                            path_groups.len() - 1
                        });
                    let index = path_groups[batch].instances.len();
                    path_groups[batch].ids.push(object.id);
                    path_groups[batch]
                        .instances
                        .push(pack_path(object, frame.reveal(object_index)));
                    self.slots.push(PreparedSlot::Path { index, batch });
                }
                GeometryRef::External(_) => {
                    self.slots
                        .push(PreparedSlot::Unsupported(self.unsupported.len()));
                    self.unsupported.push(object.id);
                }
            }
        }

        let mut next_vertices = Vec::new();
        let mut next_indices = Vec::new();
        let mut group_offsets = Vec::with_capacity(path_groups.len());
        for group in path_groups {
            let instance_start = self.paths.len();
            group_offsets.push(instance_start);
            self.path_ids.extend(group.ids);
            self.paths.extend(group.instances);

            let mesh = &self.path_mesh_cache[group.cache_index].mesh;
            let vertex_start = u32::try_from(next_vertices.len())
                .expect("path vertex count exceeds renderer limits");
            let index_start = u32::try_from(next_indices.len())
                .expect("path index count exceeds renderer limits");
            next_vertices.extend(mesh.vertices.iter().map(|vertex| PathVertex {
                position: [vertex.position.x, vertex.position.y],
                surface: pack_path_surface(vertex.surface, vertex.path_progress),
            }));
            next_indices.extend(mesh.indices.iter().map(|index| {
                index
                    .checked_add(vertex_start)
                    .expect("path index exceeds renderer limits")
            }));
            let index_end = u32::try_from(next_indices.len())
                .expect("path index count exceeds renderer limits");
            let instance_end = u32::try_from(self.paths.len())
                .expect("path instance count exceeds renderer limits");
            self.path_batches.push(PathBatch {
                index_range: index_start..index_end,
                instance_range: u32::try_from(instance_start)
                    .expect("path instance count exceeds renderer limits")
                    ..instance_end,
            });
            self.path_batch_cache_indices.push(group.cache_index);
        }
        for slot in &mut self.slots {
            if let PreparedSlot::Path { index, batch } = slot {
                *index += group_offsets[*batch];
            }
        }
        self.path_geometry_dirty =
            self.path_vertices != next_vertices || self.path_indices != next_indices;
        self.path_vertices = next_vertices;
        self.path_indices = next_indices;

        if !self.circles.is_empty() {
            self.circle_dirty_ranges.push(0..self.circles.len());
        }
        if !self.rectangles.is_empty() {
            self.rectangle_dirty_ranges.push(0..self.rectangles.len());
        }
        if !self.lines.is_empty() {
            self.line_dirty_ranges.push(0..self.lines.len());
        }
        if !self.paths.is_empty() {
            self.path_dirty_ranges.push(0..self.paths.len());
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
            self.circles.len() + self.rectangles.len() + self.lines.len() + self.paths.len(),
            geometry_cache_misses,
        )
    }

    fn prepared_frame(
        &self,
        time: f64,
        capacity_growths: usize,
        instances_repacked: usize,
        geometry_cache_misses: usize,
    ) -> PreparedFrame<'_> {
        let batch_count = usize::from(!self.circles.is_empty())
            + usize::from(!self.rectangles.is_empty())
            + usize::from(!self.lines.is_empty());
        let batch_count = batch_count
            + self
                .path_batches
                .iter()
                .filter(|batch| !batch.index_range.is_empty())
                .count();
        let dirty_instance_count = dirty_len(&self.circle_dirty_ranges)
            + dirty_len(&self.rectangle_dirty_ranges)
            + dirty_len(&self.line_dirty_ranges);
        let dirty_instance_count = dirty_instance_count + dirty_len(&self.path_dirty_ranges);
        PreparedFrame {
            time,
            circle_ids: &self.circle_ids,
            circles: &self.circles,
            rectangle_ids: &self.rectangle_ids,
            rectangles: &self.rectangles,
            line_ids: &self.line_ids,
            lines: &self.lines,
            path_ids: &self.path_ids,
            paths: &self.paths,
            path_vertices: &self.path_vertices,
            path_indices: &self.path_indices,
            path_batches: &self.path_batches,
            unsupported: &self.unsupported,
            circle_dirty_ranges: &self.circle_dirty_ranges,
            rectangle_dirty_ranges: &self.rectangle_dirty_ranges,
            line_dirty_ranges: &self.line_dirty_ranges,
            path_dirty_ranges: &self.path_dirty_ranges,
            path_geometry_dirty: self.path_geometry_dirty,
            stats: RenderStats {
                batch_count,
                instance_count: self.circles.len()
                    + self.rectangles.len()
                    + self.lines.len()
                    + self.paths.len(),
                unsupported_count: self.unsupported.len(),
                capacity_growths,
                instances_repacked,
                dirty_instance_count,
                geometry_cache_misses,
            },
        }
    }

    fn clear_dirty_ranges(&mut self) {
        self.circle_dirty_ranges.clear();
        self.rectangle_dirty_ranges.clear();
        self.line_dirty_ranges.clear();
        self.path_dirty_ranges.clear();
        self.path_geometry_dirty = false;
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
            Some(PreparedSlot::Path { index, batch }) => {
                let GeometryRef::VectorPath(path) = &object.geometry else {
                    return false;
                };
                let Some(cache_index) = self.path_batch_cache_indices.get(*batch) else {
                    return false;
                };
                let cache = &self.path_mesh_cache[*cache_index];
                self.path_ids.get(*index) == Some(&object.id)
                    && cache.path == *path
                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()
            }
            Some(PreparedSlot::Unsupported(index)) => {
                matches!(object.geometry, GeometryRef::External(_))
                    && self.unsupported.get(*index) == Some(&object.id)
            }
            None => false,
        }
    }

    fn capacities(&self) -> [usize; 19] {
        [
            self.circle_ids.capacity(),
            self.circles.capacity(),
            self.rectangle_ids.capacity(),
            self.rectangles.capacity(),
            self.line_ids.capacity(),
            self.lines.capacity(),
            self.path_ids.capacity(),
            self.paths.capacity(),
            self.path_vertices.capacity(),
            self.path_indices.capacity(),
            self.path_batches.capacity(),
            self.path_batch_cache_indices.capacity(),
            self.path_mesh_cache.capacity(),
            self.unsupported.capacity(),
            self.slots.capacity(),
            self.circle_dirty_ranges.capacity(),
            self.rectangle_dirty_ranges.capacity(),
            self.line_dirty_ranges.capacity(),
            self.path_dirty_ranges.capacity(),
        ]
    }

    fn cache_path_mesh(
        &mut self,
        path: &VectorPath,
        stroke_width: f32,
    ) -> Result<(usize, bool), noon_geometry::GeometryError> {
        let stroke_width_bits = stroke_width.to_bits();
        if let Some(index) = self
            .path_mesh_cache
            .iter()
            .position(|entry| entry.path == *path && entry.stroke_width_bits == stroke_width_bits)
        {
            return Ok((index, false));
        }
        let mesh = noon_geometry::tessellate(path, stroke_width)?;
        self.path_mesh_cache.push(CachedPathMesh {
            path: path.clone(),
            stroke_width_bits,
            mesh,
        });
        Ok((self.path_mesh_cache.len() - 1, true))
    }

    pub fn cached_path_mesh_count(&self) -> usize {
        self.path_mesh_cache.len()
    }
}

fn pack_circle(object: &FrameObjectState) -> CircleInstance {
    let GeometryRef::Circle { radius } = &object.geometry else {
        unreachable!("circle slot must retain circle geometry")
    };
    CircleInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        radius: *radius,
        padding: [0.0; 3],
    }
}

fn pack_rectangle(object: &FrameObjectState) -> RectangleInstance {
    let GeometryRef::Rectangle { size } = &object.geometry else {
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
    let GeometryRef::Line { start, end } = &object.geometry else {
        unreachable!("line slot must retain line geometry")
    };
    LineInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
}

fn pack_path(object: &FrameObjectState, reveal: f32) -> PathInstance {
    debug_assert!(matches!(object.geometry, GeometryRef::VectorPath(_)));
    let mut style: PackedStyle = object.style.into();
    // Path stroke width is baked into the cached mesh, so this otherwise-unused
    // GPU field carries normalized reveal without growing the instance stride.
    style.stroke_width = reveal.clamp(0.0, 1.0);
    PathInstance {
        transform: object.transform.into(),
        style,
    }
}

fn pack_path_surface(surface: PathSurface, progress: f32) -> u32 {
    let progress = (progress.clamp(0.0, 1.0) * PATH_PROGRESS_MAX as f32).round() as u32;
    (progress << 1)
        | match surface {
            PathSurface::Fill => 0,
            PathSurface::Stroke => 1,
        }
}

#[cfg(test)]
fn unpack_path_progress(surface: u32) -> f32 {
    (surface >> 1) as f32 / PATH_PROGRESS_MAX as f32
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
    use noon_core::{Color, GeometryId, Vec2, VectorPath};
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
        let reveals = vec![1.0; objects.len()];
        FrameState {
            time: 1.25,
            objects,
            reveals,
        }
    }

    #[test]
    fn packed_instance_layout_is_stable() {
        assert_eq!(std::mem::size_of::<PackedTransform>(), 24);
        assert_eq!(std::mem::size_of::<PackedStyle>(), 48);
        assert_eq!(std::mem::size_of::<CircleInstance>(), 88);
        assert_eq!(std::mem::size_of::<RectangleInstance>(), 88);
        assert_eq!(std::mem::size_of::<LineInstance>(), 88);
        assert_eq!(std::mem::size_of::<PathInstance>(), 72);
        assert_eq!(std::mem::size_of::<PathVertex>(), 12);
    }

    fn curved_path() -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(-1.0, -0.5))
            .quadratic_to(Vec2::new(0.0, 1.5), Vec2::new(1.0, -0.5))
            .cubic_to(
                Vec2::new(0.5, -1.0),
                Vec2::new(-0.5, -1.0),
                Vec2::new(-1.0, -0.5),
            )
            .close()
    }

    #[test]
    fn identical_paths_share_cached_mesh_and_instance_batch() {
        let geometry = GeometryRef::path(curved_path());
        let mut first = object(1, geometry.clone());
        let mut second = object(2, geometry);
        first.style.stroke = Some(Color::WHITE);
        first.style.stroke_width = 0.15;
        second.style = first.style;
        let frame = frame(vec![first, second]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.stats.geometry_cache_misses, 1);
        assert_eq!(prepared.stats.batch_count, 1);
        assert_eq!(prepared.stats.instance_count, 2);
        assert_eq!(prepared.path_batches.len(), 1);
        assert_eq!(prepared.path_batches[0].instance_range, 0..2);
        assert!(!prepared.path_vertices.is_empty());
        assert!(!prepared.path_indices.is_empty());
        assert!(prepared.path_geometry_dirty);
        assert_eq!(preparer.cached_path_mesh_count(), 1);
    }

    #[test]
    fn prepared_path_vertices_preserve_ordered_reveal_progress() {
        let mut state = object(
            7,
            GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::new(0.0, 0.0))
                    .line_to(Vec2::new(3.0, 4.0)),
            ),
        );
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);

        let stroke_progresses: Vec<f32> = prepared
            .path_vertices
            .iter()
            .filter(|vertex| vertex.surface & 1 == 1)
            .map(|vertex| unpack_path_progress(vertex.surface))
            .collect();
        assert!(stroke_progresses.contains(&0.0));
        assert!(stroke_progresses
            .iter()
            .any(|progress| (*progress - 1.0).abs() < 1e-6));
        assert!(stroke_progresses
            .iter()
            .all(|progress| (0.0..=1.0).contains(progress)));
    }

    #[test]
    fn path_transform_and_color_changes_do_not_retessellate() {
        let mut state = object(7, GeometryRef::path(curved_path()));
        state.style.stroke = Some(Color::BLACK);
        state.style.stroke_width = 0.2;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        frame.objects[0].transform.translation = Vec2::new(2.0, -3.0);
        frame.objects[0].style.fill = Some(Color::rgb(0.2, 0.5, 0.8));
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.path_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_dirty_ranges[0], 0..1);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        frame.objects[0].style.stroke_width = 0.4;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(prepared.stats.geometry_cache_misses, 1);
        assert!(prepared.path_geometry_dirty);
        assert_eq!(preparer.cached_path_mesh_count(), 2);
    }

    #[test]
    fn path_reveal_changes_only_dirty_the_instance_record() {
        let mut state = object(7, GeometryRef::path(curved_path()));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        frame.reveals[0] = 0.35;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.path_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_dirty_ranges[0], 0..1);
        assert_eq!(prepared.paths[0].style.stroke_width, 0.35);
        assert_eq!(preparer.cached_path_mesh_count(), 1);
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
