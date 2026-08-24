//! CPU-side preparation for Noon's wgpu renderer.
//!
//! This layer defines deterministic packed instance records and batches analytic
//! primitives before they are uploaded to wgpu. The same preparation path is
//! used by native and browser backends.

#![forbid(unsafe_code)]

mod gpu;
mod render_order;
mod reveal;

pub use gpu::*;
pub use render_order::*;

use bytemuck::{Pod, Zeroable};
use noon_core::{
    Color, GeometryRef, ObjectId, PathCommand, StrokeCap, StrokeJoin, Style, Transform2D, Vec2,
    VectorPath,
};
use noon_geometry::{PathSurface, TessellatedPath};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use reveal::{analytic_reveal_key, temporary_reveal_path, AnalyticRevealKey};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    ops::Range,
};

// `f32` represents every integer through 2^24 exactly. Keeping the encoded
// progress in this exact domain avoids endpoint wraparound at reveal == 1.0
// while retaining far more precision than pixel-scale path clipping needs.
const PATH_PROGRESS_MAX: u32 = 16_777_215;
const DEFAULT_PATH_MESH_CACHE_LIMIT: usize = 256;

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
    /// x = reveal, y = morph progress. Both are normalized and independent.
    pub path_params: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PathVertex {
    pub position: [f32; 2],
    pub target_position: [f32; 2],
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
    pub render_batches: &'a [OrderedRenderBatch],
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
    Absent,
    Circle(usize),
    Rectangle(usize),
    Line(usize),
    Path {
        index: usize,
        batch: usize,
        analytic_reveal: Option<AnalyticRevealKey>,
        reveal_head: Option<usize>,
    },
    Unsupported(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PathMeshKey {
    path_hash: u64,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
}

#[derive(Clone, Debug)]
struct CachedPathMesh {
    path: VectorPath,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
    mesh: TessellatedPath,
    last_used: u64,
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
    render_batches: Vec<OrderedRenderBatch>,
    render_order_keys: Vec<RenderOrderKey>,
    path_batch_cache_indices: Vec<usize>,
    path_mesh_cache: Vec<CachedPathMesh>,
    path_mesh_lookup: HashMap<PathMeshKey, Vec<usize>>,
    path_mesh_cache_limit: Option<usize>,
    path_mesh_clock: u64,
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
                PreparedSlot::Absent => {}
                PreparedSlot::Circle(index) => {
                    let packed = pack_circle(object, frame.reveal(object_index));
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
                    let packed = pack_line(object, frame.reveal(object_index));
                    instances_repacked += 1;
                    if self.lines[index] != packed {
                        self.lines[index] = packed;
                        push_dirty_range(&mut self.line_dirty_ranges, index);
                    }
                }
                PreparedSlot::Path {
                    index,
                    batch,
                    reveal_head,
                    ..
                } => {
                    let reveal = frame.reveal(object_index);
                    let packed = pack_path(object, reveal, frame.morph(object_index));
                    instances_repacked += 1;
                    if self.paths[index] != packed {
                        self.paths[index] = packed;
                        push_dirty_range(&mut self.path_dirty_ranges, index);
                    }
                    if let Some(head_index) = reveal_head {
                        let cache_index = self.path_batch_cache_indices[batch];
                        let packed_head = pack_path_reveal_head(
                            object,
                            &self.path_mesh_cache[cache_index].mesh,
                            reveal,
                        );
                        instances_repacked += 1;
                        if self.lines[head_index] != packed_head {
                            self.lines[head_index] = packed_head;
                            push_dirty_range(&mut self.line_dirty_ranges, head_index);
                        }
                    }
                }
                PreparedSlot::Unsupported(_) => {}
            }
        }

        normalize_dirty_ranges(&mut self.circle_dirty_ranges);
        normalize_dirty_ranges(&mut self.rectangle_dirty_ranges);
        normalize_dirty_ranges(&mut self.line_dirty_ranges);
        normalize_dirty_ranges(&mut self.path_dirty_ranges);

        self.prepared_frame(frame.time, 0, instances_repacked, 0)
    }

    fn rebuild<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        self.prune_path_mesh_cache(frame);
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
        self.render_batches.clear();
        self.path_batch_cache_indices.clear();
        self.unsupported.clear();
        self.slots.clear();
        self.clear_dirty_ranges();

        let mut path_groups = Vec::<PathGroup>::new();
        let mut path_group_lookup = HashMap::<usize, usize>::new();
        let mut geometry_cache_misses = 0;
        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                self.slots.push(PreparedSlot::Absent);
                continue;
            }
            let render_geometry = frame.render_geometry(object_index);
            let temporary_reveal =
                temporary_reveal_path(render_geometry, frame.reveal(object_index));
            let path = temporary_reveal
                .as_ref()
                .map(|(_, path)| path)
                .or(match render_geometry {
                    GeometryRef::VectorPath(path) => Some(path),
                    _ => None,
                });
            if let Some(path) = path {
                let cache_index = match self.cache_path_mesh(path, object.style) {
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
                let batch = match path_group_lookup.get(&cache_index).copied() {
                    Some(batch) => batch,
                    None => {
                        let batch = path_groups.len();
                        path_groups.push(PathGroup {
                            cache_index,
                            ids: Vec::new(),
                            instances: Vec::new(),
                        });
                        path_group_lookup.insert(cache_index, batch);
                        batch
                    }
                };
                let reveal = frame.reveal(object_index);
                let index = path_groups[batch].instances.len();
                path_groups[batch].ids.push(object.id);
                path_groups[batch].instances.push(pack_path(
                    object,
                    reveal,
                    frame.morph(object_index),
                ));
                let reveal_head = if should_create_path_reveal_head(object, reveal) {
                    let head_index = self.lines.len();
                    self.line_ids.push(object.id);
                    self.lines.push(pack_path_reveal_head(
                        object,
                        &self.path_mesh_cache[cache_index].mesh,
                        reveal,
                    ));
                    Some(head_index)
                } else {
                    None
                };
                self.slots.push(PreparedSlot::Path {
                    index,
                    batch,
                    analytic_reveal: temporary_reveal.as_ref().map(|(key, _)| *key),
                    reveal_head,
                });
                continue;
            }

            match render_geometry {
                GeometryRef::Circle { .. } => {
                    self.slots.push(PreparedSlot::Circle(self.circles.len()));
                    self.circle_ids.push(object.id);
                    self.circles
                        .push(pack_circle(object, frame.reveal(object_index)));
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
                    self.lines
                        .push(pack_line(object, frame.reveal(object_index)));
                }
                GeometryRef::VectorPath(_) => {
                    unreachable!("vector path must enter the path preparation branch")
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
                target_position: [vertex.target_position.x, vertex.target_position.y],
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
            if let PreparedSlot::Path { index, batch, .. } = slot {
                *index += group_offsets[*batch];
            }
        }
        self.rebuild_ordered_render_batches();
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
        let batch_count = self.render_batches.len();
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
            render_batches: &self.render_batches,
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
        let Some(slot) = self.slots.get(object_index) else {
            return false;
        };
        if !frame.is_present(object_index) {
            return matches!(slot, PreparedSlot::Absent);
        }
        if matches!(slot, PreparedSlot::Absent) {
            return false;
        }
        let render_geometry = frame.render_geometry(object_index);
        match slot {
            PreparedSlot::Absent => false,
            PreparedSlot::Circle(index) => {
                matches!(render_geometry, GeometryRef::Circle { .. })
                    && self.circle_ids.get(*index) == Some(&object.id)
            }
            PreparedSlot::Rectangle(index) => {
                matches!(render_geometry, GeometryRef::Rectangle { .. })
                    && self.rectangle_ids.get(*index) == Some(&object.id)
            }
            PreparedSlot::Line(index) => {
                matches!(render_geometry, GeometryRef::Line { .. })
                    && self.line_ids.get(*index) == Some(&object.id)
            }
            PreparedSlot::Path {
                index,
                batch,
                analytic_reveal,
                reveal_head,
            } => {
                let Some(cache_index) = self.path_batch_cache_indices.get(*batch) else {
                    return false;
                };
                let cache = &self.path_mesh_cache[*cache_index];
                let geometry_matches = match analytic_reveal {
                    Some(expected) => {
                        frame.reveal(object_index) < 1.0
                            && analytic_reveal_key(render_geometry) == Some(*expected)
                    }
                    None => {
                        let GeometryRef::VectorPath(path) = render_geometry else {
                            return false;
                        };
                        cache.path == *path
                    }
                };
                let reveal_head_available = reveal_head.is_some()
                    || !should_create_path_reveal_head(object, frame.reveal(object_index));
                self.path_ids.get(*index) == Some(&object.id)
                    && geometry_matches
                    && reveal_head_available
                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()
                    && cache.stroke_join == object.style.stroke_join
                    && cache.stroke_cap == object.style.stroke_cap
                    && cache.fill_enabled == object.style.fill.is_some()
            }
            PreparedSlot::Unsupported(index) => {
                matches!(render_geometry, GeometryRef::External(_))
                    && self.unsupported.get(*index) == Some(&object.id)
            }
        }
    }

    fn capacities(&self) -> [usize; 20] {
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
            self.path_mesh_lookup.capacity(),
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
        style: Style,
    ) -> Result<(usize, bool), noon_geometry::GeometryError> {
        let stroke_width_bits = style.stroke_width.to_bits();
        let fill_enabled = style.fill.is_some();
        let key = path_mesh_key(
            path,
            stroke_width_bits,
            style.stroke_join,
            style.stroke_cap,
            fill_enabled,
        );
        let existing = self.path_mesh_lookup.get(&key).and_then(|candidates| {
            candidates.iter().copied().find(|&index| {
                let entry = &self.path_mesh_cache[index];
                entry.path == *path
                    && entry.stroke_width_bits == stroke_width_bits
                    && entry.stroke_join == style.stroke_join
                    && entry.stroke_cap == style.stroke_cap
                    && entry.fill_enabled == fill_enabled
            })
        });
        if let Some(index) = existing {
            self.mark_path_mesh_used(index);
            return Ok((index, false));
        }

        let mesh = noon_geometry::tessellate_styled_with_fill(
            path,
            style.stroke_width,
            style.stroke_join,
            style.stroke_cap,
            fill_enabled,
        )?;
        let index = self.path_mesh_cache.len();
        let last_used = self.next_path_mesh_use();
        self.path_mesh_cache.push(CachedPathMesh {
            path: path.clone(),
            stroke_width_bits,
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            fill_enabled,
            mesh,
            last_used,
        });
        self.path_mesh_lookup.entry(key).or_default().push(index);
        Ok((index, true))
    }

    fn next_path_mesh_use(&mut self) -> u64 {
        self.path_mesh_clock = self.path_mesh_clock.saturating_add(1);
        self.path_mesh_clock
    }

    fn mark_path_mesh_used(&mut self, index: usize) {
        let last_used = self.next_path_mesh_use();
        self.path_mesh_cache[index].last_used = last_used;
    }

    fn prune_path_mesh_cache(&mut self, frame: &FrameState) {
        let limit = self.path_mesh_cache_limit();
        if self.path_mesh_cache.len() <= limit {
            return;
        }

        // Analytic Create uses temporary path meshes that are not represented by
        // `frame.render_geometry`. Defer eviction while any such outline is active
        // so a full rebuild cannot evict and immediately retessellate visible work.
        // Once all analytic reveals complete, normal bounded LRU pruning resumes.
        if frame.objects.iter().enumerate().any(|(object_index, _)| {
            frame.is_present(object_index)
                && frame.reveal(object_index) < 1.0
                && analytic_reveal_key(frame.render_geometry(object_index)).is_some()
        }) {
            return;
        }

        let mut keep = vec![false; self.path_mesh_cache.len()];
        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                continue;
            }
            let GeometryRef::VectorPath(path) = frame.render_geometry(object_index) else {
                continue;
            };
            let stroke_width_bits = object.style.stroke_width.to_bits();
            let fill_enabled = object.style.fill.is_some();
            let key = path_mesh_key(
                path,
                stroke_width_bits,
                object.style.stroke_join,
                object.style.stroke_cap,
                fill_enabled,
            );
            if let Some(candidates) = self.path_mesh_lookup.get(&key) {
                if let Some(index) = candidates.iter().copied().find(|&index| {
                    let entry = &self.path_mesh_cache[index];
                    entry.path == *path
                        && entry.stroke_width_bits == stroke_width_bits
                        && entry.stroke_join == object.style.stroke_join
                        && entry.stroke_cap == object.style.stroke_cap
                        && entry.fill_enabled == fill_enabled
                }) {
                    keep[index] = true;
                }
            }
        }

        let pinned_count = keep.iter().filter(|&&pinned| pinned).count();
        let stale_budget = limit.saturating_sub(pinned_count);
        let mut order: Vec<usize> = (0..self.path_mesh_cache.len())
            .filter(|&index| !keep[index])
            .collect();
        order.sort_unstable_by(|&left, &right| {
            self.path_mesh_cache[right]
                .last_used
                .cmp(&self.path_mesh_cache[left].last_used)
                .then_with(|| right.cmp(&left))
        });
        for index in order.into_iter().take(stale_budget) {
            keep[index] = true;
        }
        if keep.iter().all(|&retained| retained) {
            return;
        }

        let old_cache = std::mem::take(&mut self.path_mesh_cache);
        self.path_mesh_lookup.clear();
        for (old_index, entry) in old_cache.into_iter().enumerate() {
            if !keep[old_index] {
                continue;
            }
            let key = path_mesh_key(
                &entry.path,
                entry.stroke_width_bits,
                entry.stroke_join,
                entry.stroke_cap,
                entry.fill_enabled,
            );
            let new_index = self.path_mesh_cache.len();
            self.path_mesh_cache.push(entry);
            self.path_mesh_lookup
                .entry(key)
                .or_default()
                .push(new_index);
        }
    }

    /// Sets the target number of path meshes retained across full rebuilds.
    ///
    /// Incoming-frame meshes are pinned before stale LRU eviction, so a prepared
    /// frame may contain more meshes than this limit without forcing retessellation
    /// or invalidating its path-batch cache indices.
    pub fn set_path_mesh_cache_limit(&mut self, limit: usize) {
        self.path_mesh_cache_limit = Some(limit);
    }

    pub fn path_mesh_cache_limit(&self) -> usize {
        self.path_mesh_cache_limit
            .unwrap_or(DEFAULT_PATH_MESH_CACHE_LIMIT)
    }

    pub fn cached_path_mesh_count(&self) -> usize {
        self.path_mesh_cache.len()
    }
}

fn path_mesh_key(
    path: &VectorPath,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
) -> PathMeshKey {
    let mut hasher = DefaultHasher::new();
    hash_vector_path(path, &mut hasher);
    PathMeshKey {
        path_hash: hasher.finish(),
        stroke_width_bits,
        stroke_join,
        stroke_cap,
        fill_enabled,
    }
}

fn hash_vector_path(path: &VectorPath, hasher: &mut impl Hasher) {
    path.commands().len().hash(hasher);
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                0_u8.hash(hasher);
                hash_vec2(to, hasher);
            }
            PathCommand::LineTo { to } => {
                1_u8.hash(hasher);
                hash_vec2(to, hasher);
            }
            PathCommand::QuadraticTo { control, to } => {
                2_u8.hash(hasher);
                hash_vec2(control, hasher);
                hash_vec2(to, hasher);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                3_u8.hash(hasher);
                hash_vec2(control1, hasher);
                hash_vec2(control2, hasher);
                hash_vec2(to, hasher);
            }
            PathCommand::Close => 4_u8.hash(hasher),
        }
    }
    match path.morph_target() {
        Some(target) => {
            1_u8.hash(hasher);
            hash_vector_path(target, hasher);
        }
        None => 0_u8.hash(hasher),
    }
}

fn hash_vec2(value: noon_core::Vec2, hasher: &mut impl Hasher) {
    value.x.to_bits().hash(hasher);
    value.y.to_bits().hash(hasher);
}

fn pack_style(object: &FrameObjectState) -> PackedStyle {
    let mut style: PackedStyle = object.style.into();
    style.opacity *= object.appearance.clamp(0.0, 1.0);
    style
}

fn pack_circle(object: &FrameObjectState, reveal: f32) -> CircleInstance {
    let GeometryRef::Circle { radius } = &object.geometry else {
        unreachable!("circle slot must retain circle geometry")
    };
    CircleInstance {
        transform: object.transform.into(),
        style: pack_style(object),
        radius: *radius,
        padding: [reveal.clamp(0.0, 1.0), 0.0, 0.0],
    }
}

fn pack_rectangle(object: &FrameObjectState) -> RectangleInstance {
    let GeometryRef::Rectangle { size } = &object.geometry else {
        unreachable!("rectangle slot must retain rectangle geometry")
    };
    RectangleInstance {
        transform: object.transform.into(),
        style: pack_style(object),
        size: [size.x, size.y],
        padding: [0.0; 2],
    }
}

fn pack_line(object: &FrameObjectState, reveal: f32) -> LineInstance {
    let GeometryRef::Line { start, end } = &object.geometry else {
        unreachable!("line slot must retain line geometry")
    };
    let mut transform: PackedTransform = object.transform.into();
    transform.padding = reveal.clamp(0.0, 1.0);
    LineInstance {
        transform,
        style: pack_style(object),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
}

fn should_create_path_reveal_head(object: &FrameObjectState, reveal: f32) -> bool {
    reveal < 1.0
        && object.style.stroke_cap == StrokeCap::Round
        && object.style.stroke_width > 0.0
        && (object.style.stroke.is_some() || object.style.fill.is_some())
}

fn pack_path_reveal_head(
    object: &FrameObjectState,
    mesh: &TessellatedPath,
    reveal: f32,
) -> LineInstance {
    let reveal = reveal.clamp(0.0, 1.0);
    let point = mesh.reveal_head_position(reveal).unwrap_or(Vec2::ZERO);
    let mut transform: PackedTransform = object.transform.into();
    transform.padding = 1.0;
    let mut style = pack_style(object);
    style.fill = [0.0; 4];
    style.fill_enabled = 0;
    if let Some(color) = object.style.stroke.or(object.style.fill) {
        style.stroke = [color.red, color.green, color.blue, color.alpha];
        style.stroke_enabled = 1;
    } else {
        style.stroke = [0.0; 4];
        style.stroke_enabled = 0;
    }
    let active = reveal > 0.0 && reveal < 1.0;
    style.opacity *= f32::from(active);
    if object.style.stroke.is_none() {
        style.opacity *= 1.0 - smoothstep(0.75, 1.0, reveal);
    }
    LineInstance {
        transform,
        style,
        start: [point.x, point.y],
        end: [point.x, point.y],
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return f32::from(value >= edge1);
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn pack_path(object: &FrameObjectState, reveal: f32, morph: f32) -> PathInstance {
    PathInstance {
        transform: object.transform.into(),
        style: pack_style(object),
        path_params: [reveal.clamp(0.0, 1.0), morph.clamp(0.0, 1.0)],
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

fn normalize_dirty_ranges(ranges: &mut Vec<Range<usize>>) {
    if ranges.len() < 2 {
        return;
    }
    ranges.sort_unstable_by_key(|range| range.start);
    let mut write = 0;
    for read in 1..ranges.len() {
        if ranges[read].start <= ranges[write].end {
            ranges[write].end = ranges[write].end.max(ranges[read].end);
        } else {
            write += 1;
            ranges[write] = ranges[read].clone();
        }
    }
    ranges.truncate(write + 1);
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
            appearance: 1.0,
        }
    }

    fn frame(objects: Vec<FrameObjectState>) -> FrameState {
        let presences = vec![true; objects.len()];
        let reveals = vec![1.0; objects.len()];
        let morphs = vec![0.0; objects.len()];
        let render_geometries = vec![None; objects.len()];
        FrameState {
            time: 1.25,
            objects,
            presences,
            reveals,
            morphs,
            render_geometries,
        }
    }

    #[test]
    fn filled_morph_reuses_geometry_after_cold_prepare() {
        let source = curved_path();
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.3))
            .line_to(Vec2::new(0.38, 0.42))
            .line_to(Vec2::new(1.2, 0.4))
            .line_to(Vec2::new(0.5, -0.18))
            .line_to(Vec2::new(0.74, -1.05))
            .line_to(Vec2::new(0.0, -0.52))
            .line_to(Vec2::new(-0.74, -1.05))
            .line_to(Vec2::new(-0.5, -0.18))
            .line_to(Vec2::new(-1.2, 0.4))
            .line_to(Vec2::new(-0.38, 0.42))
            .close();
        let geometry = GeometryRef::path(source.with_morph_target(target));
        let mut path = object(7, geometry.clone());
        path.style.fill = Some(Color::WHITE);
        path.style.stroke = Some(Color::BLACK);
        path.style.stroke_width = 0.08;
        let mut initial = frame(vec![path.clone()]);
        initial.render_geometries[0] = Some(geometry.clone());
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&initial);
        assert_eq!(cold.stats.geometry_cache_misses, 1);
        assert!(cold
            .path_vertices
            .iter()
            .any(|vertex| vertex.surface & 1 == 0));
        let vertices = cold.path_vertices.to_vec();
        let indices = cold.path_indices.to_vec();

        let mut advanced = initial.clone();
        advanced.morphs[0] = 0.5;
        let changes = FrameChanges::objects(vec![0]);
        let steady = preparer.prepare_incremental(&advanced, &changes);
        assert_eq!(steady.stats.geometry_cache_misses, 0);
        assert!(!steady.path_geometry_dirty);
        assert_eq!(steady.path_vertices, vertices);
        assert_eq!(steady.path_indices, indices);
        assert_eq!(steady.path_dirty_ranges.len(), 1);
        assert_eq!(steady.path_dirty_ranges[0].start, 0);
        assert_eq!(steady.path_dirty_ranges[0].end, 1);
    }

    #[test]
    fn fill_presence_is_part_of_path_mesh_cache_identity() {
        let geometry = GeometryRef::path(curved_path());
        let mut path = object(17, geometry);
        path.style.fill = None;
        path.style.stroke = Some(Color::WHITE);
        path.style.stroke_width = 0.08;
        let initial = frame(vec![path]);
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&initial);
        assert_eq!(cold.stats.geometry_cache_misses, 1);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        let mut filled = initial.clone();
        filled.objects[0].style.fill = Some(Color::WHITE);
        let changes = FrameChanges::objects(vec![0]);
        let rebuilt = preparer.prepare_incremental(&filled, &changes);
        assert_eq!(rebuilt.stats.geometry_cache_misses, 1);
        assert!(rebuilt.path_geometry_dirty);
        assert!(rebuilt
            .path_vertices
            .iter()
            .any(|vertex| vertex.surface & 1 == 0));
        assert_eq!(preparer.cached_path_mesh_count(), 2);
    }

    #[test]
    fn packed_instance_layout_is_stable() {
        assert_eq!(std::mem::size_of::<PackedTransform>(), 24);
        assert_eq!(std::mem::size_of::<PackedStyle>(), 48);
        assert_eq!(std::mem::size_of::<CircleInstance>(), 88);
        assert_eq!(std::mem::size_of::<RectangleInstance>(), 88);
        assert_eq!(std::mem::size_of::<LineInstance>(), 88);
        assert_eq!(std::mem::size_of::<PathInstance>(), 80);
        assert_eq!(std::mem::size_of::<PathVertex>(), 20);
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
    fn path_reveal_reuses_cached_geometry_and_moves_only_instance_and_head() {
        let mut state = object(7, GeometryRef::path(curved_path()));
        state.style.fill = None;
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
        let mut frame = frame(vec![state]);
        frame.reveals[0] = 0.2;
        let mut preparer = FramePreparer::new();
        let cold = preparer.prepare(&frame);
        assert_eq!(cold.paths.len(), 1);
        assert_eq!(cold.lines.len(), 1);
        assert_eq!(cold.lines[0].start, cold.lines[0].end);
        let head_before = cold.lines[0].start;
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        frame.reveals[0] = 0.35;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.instances_repacked, 2);
        assert_eq!(prepared.stats.dirty_instance_count, 2);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.path_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_dirty_ranges[0], 0..1);
        assert_eq!(prepared.line_dirty_ranges.len(), 1);
        assert_eq!(prepared.line_dirty_ranges[0], 0..1);
        assert_eq!(prepared.paths[0].path_params[0], 0.35);
        assert_ne!(prepared.lines[0].start, head_before);
        assert_eq!(prepared.lines[0].start, prepared.lines[0].end);
        assert_eq!(preparer.cached_path_mesh_count(), 1);
    }

    #[test]
    fn circle_create_stays_on_the_analytic_fast_path() {
        let mut state = object(7, GeometryRef::circle(1.25));
        state.style.fill = Some(Color::WHITE);
        state.style.stroke = Some(Color::BLACK);
        state.style.stroke_width = 0.08;
        let mut frame = frame(vec![state]);
        frame.reveals[0] = 0.25;
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&frame);
        assert_eq!(cold.circles.len(), 1);
        assert!(cold.paths.is_empty());
        assert_eq!(cold.circles[0].padding[0], 0.25);
        assert_eq!(cold.stats.geometry_cache_misses, 0);

        frame.reveals[0] = 0.6;
        let steady = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(steady.circles.len(), 1);
        assert!(steady.paths.is_empty());
        assert_eq!(steady.circles[0].padding[0], 0.6);
        assert_eq!(steady.stats.geometry_cache_misses, 0);
        assert_eq!(steady.stats.instances_repacked, 1);
        assert!(!steady.path_geometry_dirty);

        frame.reveals[0] = 1.0;
        let complete = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(complete.circles.len(), 1);
        assert!(complete.paths.is_empty());
        assert_eq!(complete.circles[0].padding[0], 1.0);
        assert_eq!(complete.stats.instance_count, 1);
    }

    #[test]
    fn circle_and_line_create_stay_analytic_while_rectangle_uses_a_path() {
        let mut circle = object(1, GeometryRef::circle(1.0));
        let mut rectangle = object(2, GeometryRef::rectangle(2.0, 1.0));
        let mut line = object(
            3,
            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        );
        for state in [&mut circle, &mut rectangle, &mut line] {
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.05;
        }
        let mut frame = frame(vec![circle, rectangle, line]);
        frame.reveals.fill(0.5);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.circles.len(), 1);
        assert_eq!(prepared.circles[0].padding[0], 0.5);
        assert!(prepared.rectangles.is_empty());
        assert_eq!(prepared.lines.len(), 2);
        assert_eq!(prepared.lines[1].transform.padding, 0.5);
        assert_eq!(prepared.paths.len(), 1);
        assert_eq!(prepared.stats.instance_count, 4);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 1);

        frame.reveals[2] = 0.8;
        let advanced = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2]));
        assert_eq!(advanced.lines.len(), 2);
        assert_eq!(advanced.lines[1].transform.padding, 0.8);
        assert_eq!(advanced.stats.geometry_cache_misses, 0);
        assert_eq!(advanced.stats.instances_repacked, 1);
        assert_eq!(advanced.line_dirty_ranges.len(), 1);
        assert_eq!(advanced.line_dirty_ranges[0], 1..2);
        assert!(!advanced.path_geometry_dirty);
    }

    #[test]
    fn path_morph_changes_only_dirty_the_instance_record() {
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0))
            .with_morph_target(target);
        let mut state = object(7, GeometryRef::path(source));
        // This regression is specifically for the established stroke-only morph
        // path. `Style::default()` carries a fill, which now has real topology
        // semantics and would intentionally reject this open contour.
        state.style.fill = None;
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);

        frame.morphs[0] = 0.6;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.paths[0].path_params, [1.0, 0.6]);
    }

    fn stress_morph_geometry(variant: usize) -> GeometryRef {
        let scale = 0.8 + variant as f32 * 0.03;
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, scale))
            .line_to(Vec2::new(scale, 0.0))
            .line_to(Vec2::new(0.0, -scale))
            .line_to(Vec2::new(-scale, 0.0))
            .close();
        GeometryRef::path(curved_path().with_morph_target(target))
    }

    #[test]
    fn six_hundred_morphs_reuse_twelve_meshes_and_coalesce_uploads() {
        const OBJECT_COUNT: usize = 600;
        const VARIANT_COUNT: usize = 12;
        let geometries: Vec<_> = (0..VARIANT_COUNT).map(stress_morph_geometry).collect();
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let mut state = object(index as u64, geometries[index % VARIANT_COUNT].clone());
                // Keep the 600-object stress regression scoped to stroke morphing;
                // filled morphs have their own topology/cache tests.
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.02;
                state
            })
            .collect();
        let mut frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.stats.instance_count, OBJECT_COUNT);
        assert_eq!(prepared.stats.geometry_cache_misses, VARIANT_COUNT);
        // Mesh/cache batching remains at 12 variants, while exact transparent painter order
        // requires one ordered draw batch per alternating object until mega-mesh packing lands.
        assert_eq!(prepared.stats.batch_count, OBJECT_COUNT);
        assert_eq!(prepared.path_batches.len(), VARIANT_COUNT);
        assert_eq!(prepared.paths.len(), OBJECT_COUNT);
        assert_eq!(preparer.cached_path_mesh_count(), VARIANT_COUNT);

        frame.morphs.fill(0.5);
        let changes = FrameChanges::objects((0..OBJECT_COUNT).collect());
        let prepared = preparer.prepare_incremental(&frame, &changes);

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.instances_repacked, OBJECT_COUNT);
        assert_eq!(prepared.stats.dirty_instance_count, OBJECT_COUNT);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.path_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_dirty_ranges[0], 0..OBJECT_COUNT);
        assert_eq!(preparer.cached_path_mesh_count(), VARIANT_COUNT);
    }

    #[test]
    fn two_thousand_revealed_paths_share_one_mesh_without_per_frame_tessellation() {
        const OBJECT_COUNT: usize = 2_000;
        let geometry =
            GeometryRef::path(VectorPath::new().move_to(Vec2::new(-2.4, -1.0)).cubic_to(
                Vec2::new(-1.2, -2.0),
                Vec2::new(1.2, 0.0),
                Vec2::new(2.4, -1.0),
            ));
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let mut state = object(index as u64, geometry.clone());
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.05;
                state
            })
            .collect();
        let mut frame = frame(objects);
        frame.reveals.fill(0.25);
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&frame);
        assert_eq!(cold.stats.geometry_cache_misses, 1);
        assert_eq!(cold.paths.len(), OBJECT_COUNT);
        assert_eq!(cold.lines.len(), OBJECT_COUNT);
        assert_eq!(cold.path_batches.len(), 1);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        frame.reveals.fill(0.65);
        let changes = FrameChanges::objects((0..OBJECT_COUNT).collect());
        let steady = preparer.prepare_incremental(&frame, &changes);
        assert_eq!(steady.stats.geometry_cache_misses, 0);
        assert_eq!(steady.stats.instances_repacked, OBJECT_COUNT * 2);
        assert!(!steady.path_geometry_dirty);
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
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
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
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
        };
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        let instance = prepared.lines[0];

        assert_eq!(prepared.line_ids, &[ObjectId::new(8)]);
        assert_eq!(instance.start, [-2.0, 1.5]);
        assert_eq!(instance.end, [3.0, -0.5]);
        assert_eq!(instance.transform.padding, 1.0);
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
    fn absent_objects_keep_semantic_slots_without_gpu_instances() {
        let mut frame = frame(vec![
            object(1, GeometryRef::circle(1.0)),
            object(2, GeometryRef::circle(2.0)),
        ]);
        let mut preparer = FramePreparer::new();
        assert_eq!(preparer.prepare(&frame).stats.instance_count, 2);

        frame.presences[0] = false;
        let hidden = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(hidden.stats.instance_count, 1);
        assert_eq!(hidden.circle_ids, &[ObjectId::new(2)]);
        assert_eq!(hidden.stats.unsupported_count, 0);

        frame.presences[0] = true;
        let restored = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(restored.stats.instance_count, 2);
        assert_eq!(restored.circle_ids, &[ObjectId::new(1), ObjectId::new(2)]);
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
