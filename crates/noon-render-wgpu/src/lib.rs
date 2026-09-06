//! CPU-side preparation for Noon's wgpu renderer.
//!
//! This layer defines deterministic packed instance records and batches analytic
//! primitives before they are uploaded to wgpu. The same preparation path is
//! used by native and browser backends.

#![forbid(unsafe_code)]

mod gpu;
mod mega_mesh;
mod path_residency;
mod render_order;
mod reveal;

pub use gpu::*;
pub use path_residency::{PathMeshPreload, PathMeshPreloadError, PathMeshPreloadStats};
pub use render_order::*;

use bytemuck::{Pod, Zeroable};
use noon_core::{
    Color, GeometryRef, ObjectId, PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode, Style,
    Transform2D, Vec2, VectorPath,
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
        let stroke_width_mode = match value.stroke_width_mode {
            StrokeWidthMode::ScaleWithObject => 0,
            StrokeWidthMode::ScreenSpace => 2,
        };
        Self {
            fill,
            stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled,
            stroke_enabled: stroke_enabled | stroke_width_mode,
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

/// One ordered draw slice in the packed unique-path index stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MegaPathBatch {
    pub index_range: Range<u32>,
    pub path_count: usize,
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
    pub path_vertices_repacked: usize,
    pub path_indices_repacked: usize,
    /// Unique vector paths submitted through the packed mega-mesh path.
    pub mega_path_count: usize,
    /// Ordered packed draws after painter-order coalescing.
    pub mega_path_batch_count: usize,
    /// Index entries in the packed painter-order stream.
    pub mega_path_index_count: usize,
    /// Packed mega-index entries rewritten by this preparation call.
    pub mega_path_indices_repacked: usize,
    /// Per-vertex instance records rewritten for dirty unique paths.
    pub mega_path_instance_vertices_repacked: usize,
    /// Number of free vertex chunks retained until the next compaction rebuild.
    pub path_vertex_free_range_count: usize,
    /// Total unused vertex elements represented by the free chunks.
    pub path_vertex_free_element_count: usize,
    /// Number of free index chunks retained until the next compaction rebuild.
    pub path_index_free_range_count: usize,
    /// Total unused index elements represented by the free chunks.
    pub path_index_free_element_count: usize,
    /// Unique paths temporarily detached from the immutable mega stream.
    pub mega_path_detached_count: usize,
    /// Appended semantic/render slots handled without a full preparation rebuild.
    pub structural_slots_added: usize,
    /// Retired semantic/render slots handled without compacting packed storage.
    pub structural_slots_retired: usize,
    /// Full preparation rebuilds performed by this preparation call.
    pub full_rebuilds: usize,
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
    /// Painter-ordered index stream for mostly-unique path geometry.
    pub mega_path_indices: &'a [u32],
    /// Path instance attributes repeated per geometry vertex so the packed path
    /// pipeline works on both WebGPU and the WebGL2 fallback without SSBOs.
    pub mega_path_vertex_instances: &'a [PathInstance],
    pub mega_path_batches: &'a [MegaPathBatch],
    pub render_batches: &'a [OrderedRenderBatch],
    pub unsupported: &'a [ObjectId],
    pub circle_dirty_ranges: &'a [Range<usize>],
    pub rectangle_dirty_ranges: &'a [Range<usize>],
    pub line_dirty_ranges: &'a [Range<usize>],
    pub path_dirty_ranges: &'a [Range<usize>],
    /// Dirty packed path-geometry vertex ranges. Incremental path replacement
    /// writes only these ranges instead of rewriting the full mesh arena.
    pub path_vertex_dirty_ranges: &'a [Range<usize>],
    /// Dirty packed path-geometry index ranges.
    pub path_index_dirty_ranges: &'a [Range<usize>],
    pub mega_path_instance_dirty_ranges: &'a [Range<usize>],
    /// Dirty ranges in the packed painter-order mega-index stream.
    pub mega_path_index_dirty_ranges: &'a [Range<usize>],
    pub mega_path_index_dirty: bool,
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
        partial_reveal_bits: Option<u32>,
        reveal_head: Option<usize>,
    },
    Unsupported(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct PathStrokeTransformKey {
    scale_x_bits: u32,
    scale_y_bits: u32,
    rotation_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PathMeshKey {
    path_hash: u64,
    stroke_transform: PathStrokeTransformKey,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
}

#[derive(Clone, Debug)]
struct CachedPathMesh {
    path: VectorPath,
    stroke_transform: PathStrokeTransformKey,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
    mesh: TessellatedPath,
    resident: Option<path_residency::ResidentPathRanges>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PathReplacementStats {
    cache_miss: bool,
    vertices_repacked: usize,
    indices_repacked: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StructuralAppendStats {
    instances_repacked: usize,
    geometry_cache_misses: usize,
    path_vertices_repacked: usize,
    path_indices_repacked: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VisibleProjectionKey {
    object_index: usize,
    slot: PreparedSlot,
    mega_path_segment: Option<Range<u32>>,
    mega_path_detached: bool,
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
    path_batch_vertex_ranges: Vec<Range<u32>>,
    // Mixed text/geometry consumers emit their own individual path painter order;
    // they never consume the per-vertex instance stream used by packed path draws.
    individual_path_draws: bool,
    resident_vertex_count: usize,
    resident_index_count: usize,
    mega_path_indices: Vec<u32>,
    mega_path_vertex_instances: Vec<PathInstance>,
    mega_path_batches: Vec<MegaPathBatch>,
    // Stable slices in the immutable packed mega index stream. A live-edited
    // path is detached rather than forcing a whole-stream rewrite.
    mega_path_segments: Vec<Option<Range<u32>>>,
    mega_path_detached: Vec<bool>,
    render_batches: Vec<OrderedRenderBatch>,
    // Candidate-sized submission projection. Canonical packed state and painter
    // order stay resident in the fields above when the camera changes.
    visible_raw_render_batches: Vec<OrderedRenderBatch>,
    visible_render_batches: Vec<OrderedRenderBatch>,
    visible_mega_path_batches: Vec<MegaPathBatch>,
    visible_projection_ready: bool,
    visible_projection_key: Vec<VisibleProjectionKey>,
    visible_projection_stats: VisibleRenderProjectionStats,
    render_order_keys: Vec<RenderOrderKey>,
    path_batch_cache_indices: Vec<usize>,
    path_mesh_cache: Vec<CachedPathMesh>,
    path_mesh_lookup: HashMap<PathMeshKey, Vec<usize>>,
    path_mesh_cache_limit: Option<usize>,
    path_mesh_clock: u64,
    path_mesh_cache_generation: u64,
    packed_path_mesh_cache_generation: u64,
    unsupported: Vec<ObjectId>,
    slots: Vec<PreparedSlot>,
    circle_dirty_ranges: Vec<Range<usize>>,
    rectangle_dirty_ranges: Vec<Range<usize>>,
    line_dirty_ranges: Vec<Range<usize>>,
    path_dirty_ranges: Vec<Range<usize>>,
    path_vertex_dirty_ranges: Vec<Range<usize>>,
    path_index_dirty_ranges: Vec<Range<usize>>,
    // Incremental geometry edits never compact the arena: released chunks are
    // first-fit reused, while full rebuilds are the explicit compaction barrier.
    path_vertex_free_ranges: Vec<Range<u32>>,
    path_index_free_ranges: Vec<Range<u32>>,
    mega_path_instance_dirty_ranges: Vec<Range<usize>>,
    mega_path_index_dirty_ranges: Vec<Range<usize>>,
    mega_path_index_dirty: bool,
    path_geometry_dirty: bool,
    initialized: bool,
}

impl FramePreparer {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn for_individual_path_draws() -> Self {
        Self {
            individual_path_draws: true,
            ..Self::default()
        }
    }

    pub fn prepare<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        self.rebuild(frame)
    }

    /// Updates cached instance records using the runtime's consumed change set.
    ///
    /// Structural removals retire packed slots in place. Tail-appended objects,
    /// including new vector paths, extend packed storage and painter order locally.
    /// New path meshes append one arena chunk and one mega-index suffix; unrelated
    /// geometry and packed painter-order indices remain untouched.
    pub fn prepare_incremental<'a>(
        &'a mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> PreparedFrame<'a> {
        if !self.initialized || changes.is_all() || frame.objects.len() < self.slots.len() {
            return self.rebuild(frame);
        }

        let old_slot_len = self.slots.len();
        let expected_added = frame.objects.len().saturating_sub(old_slot_len);
        let added_are_tail = expected_added == changes.added_indices().len()
            && changes
                .added_indices()
                .iter()
                .copied()
                .eq(old_slot_len..frame.objects.len());
        let removed_are_existing = changes
            .removed_indices()
            .iter()
            .all(|&index| index < old_slot_len);
        let can_append = changes
            .added_indices()
            .iter()
            .all(|&index| self.can_append_structural_slot(frame, index));
        if !added_are_tail || !removed_are_existing || !can_append {
            return self.rebuild(frame);
        }

        let replacement_indices = changes
            .object_indices()
            .iter()
            .copied()
            .filter(|index| changes.added_indices().binary_search(index).is_err())
            .filter(|index| changes.removed_indices().binary_search(index).is_err())
            .filter(|&index| !self.slot_matches(frame, index))
            .collect::<Vec<_>>();
        if !replacement_indices
            .iter()
            .all(|&index| self.can_replace_unique_path_geometry(frame, index))
        {
            return self.rebuild(frame);
        }

        self.clear_dirty_ranges();
        let mut geometry_cache_misses = 0;
        let mut path_vertices_repacked = 0;
        let mut path_indices_repacked = 0;
        for object_index in replacement_indices {
            let replacement = self
                .replace_unique_path_geometry(frame, object_index)
                .expect("preflighted unique path replacement must tessellate");
            geometry_cache_misses += usize::from(replacement.cache_miss);
            path_vertices_repacked += replacement.vertices_repacked;
            path_indices_repacked += replacement.indices_repacked;
        }
        if path_vertices_repacked > 0 || path_indices_repacked > 0 {
            self.rebuild_ordered_render_batches();
            self.rebuild_mega_render_batches();
        }

        let mut instances_repacked = 0;
        for &object_index in changes.removed_indices() {
            instances_repacked += self.retire_structural_slot(object_index);
        }
        for &object_index in changes.added_indices() {
            let appended = self.append_structural_slot(frame, object_index);
            instances_repacked += appended.instances_repacked;
            geometry_cache_misses += appended.geometry_cache_misses;
            path_vertices_repacked += appended.path_vertices_repacked;
            path_indices_repacked += appended.path_indices_repacked;
        }

        for &object_index in changes.object_indices() {
            if changes.added_indices().binary_search(&object_index).is_ok()
                || changes
                    .removed_indices()
                    .binary_search(&object_index)
                    .is_ok()
            {
                continue;
            }
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
                    partial_reveal_bits,
                    reveal_head,
                    ..
                } => {
                    let reveal = frame.reveal(object_index);
                    let packed = if partial_reveal_bits.is_some() {
                        // The temporary geometry already *is* Manim's partial VMobject.
                        // Never apply the legacy path shader reveal a second time.
                        pack_path(object, frame.render_transform(object_index), 1.0, 0.0)
                    } else {
                        pack_path(
                            object,
                            frame.render_transform(object_index),
                            reveal,
                            frame.morph(object_index),
                        )
                    };
                    instances_repacked += 1;
                    if self.paths[index] != packed {
                        self.paths[index] = packed;
                        push_dirty_range(&mut self.path_dirty_ranges, index);
                        self.update_mega_path_instance(batch, packed);
                    }
                    if let Some(head_index) = reveal_head {
                        let cache_index = self.path_batch_cache_indices[batch];
                        let packed_head = pack_path_reveal_head(
                            object,
                            frame.render_transform(object_index),
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
        normalize_dirty_ranges(&mut self.path_vertex_dirty_ranges);
        normalize_dirty_ranges(&mut self.path_index_dirty_ranges);
        normalize_dirty_ranges(&mut self.mega_path_instance_dirty_ranges);
        normalize_dirty_ranges(&mut self.mega_path_index_dirty_ranges);

        self.prepared_frame(
            frame.time,
            0,
            instances_repacked,
            geometry_cache_misses,
            path_vertices_repacked,
            path_indices_repacked,
            changes.added_indices().len(),
            changes.removed_indices().len(),
            0,
        )
    }

    fn can_append_structural_slot(&self, frame: &FrameState, object_index: usize) -> bool {
        if !self.render_order_keys.is_empty() || !frame.is_present(object_index) {
            return false;
        }
        matches!(
            frame.render_geometry(object_index),
            Some(
                GeometryRef::Circle { .. }
                    | GeometryRef::Rectangle { .. }
                    | GeometryRef::Line { .. }
                    | GeometryRef::VectorPath(_)
                    | GeometryRef::External(_)
            )
        )
    }

    fn append_structural_slot(
        &mut self,
        frame: &FrameState,
        object_index: usize,
    ) -> StructuralAppendStats {
        debug_assert_eq!(object_index, self.slots.len());
        let object = &frame.objects[object_index];
        let Some(render_geometry) = frame.render_geometry(object_index) else {
            self.slots.push(PreparedSlot::Absent);
            return StructuralAppendStats::default();
        };
        let reveal = frame.reveal(object_index);
        let temporary_reveal = temporary_reveal_path(render_geometry, reveal);
        let path = temporary_reveal
            .as_ref()
            .map(|(_, path)| path)
            .or(match render_geometry {
                GeometryRef::VectorPath(path) => Some(path),
                _ => None,
            });
        if let Some(path) = path {
            return self.append_path_structural_slot(
                frame,
                object_index,
                path,
                temporary_reveal.as_ref().map(|(key, _)| *key),
            );
        }

        let slot = match render_geometry {
            GeometryRef::Circle { .. } => {
                let index = self.circles.len();
                self.circle_ids.push(object.id);
                self.circles.push(pack_circle(object, reveal));
                push_dirty_range(&mut self.circle_dirty_ranges, index);
                PreparedSlot::Circle(index)
            }
            GeometryRef::Rectangle { .. } => {
                let index = self.rectangles.len();
                self.rectangle_ids.push(object.id);
                self.rectangles.push(pack_rectangle(object));
                push_dirty_range(&mut self.rectangle_dirty_ranges, index);
                PreparedSlot::Rectangle(index)
            }
            GeometryRef::Line { .. } => {
                let index = self.lines.len();
                self.line_ids.push(object.id);
                self.lines.push(pack_line(object, reveal));
                push_dirty_range(&mut self.line_dirty_ranges, index);
                PreparedSlot::Line(index)
            }
            GeometryRef::External(_) => {
                let index = self.unsupported.len();
                self.unsupported.push(object.id);
                PreparedSlot::Unsupported(index)
            }
            GeometryRef::VectorPath(_) => unreachable!("vector path must enter append path branch"),
        };
        self.slots.push(slot);
        self.append_ordered_render_slot(slot);
        StructuralAppendStats {
            instances_repacked: usize::from(!matches!(slot, PreparedSlot::Unsupported(_))),
            ..StructuralAppendStats::default()
        }
    }

    fn append_path_structural_slot(
        &mut self,
        frame: &FrameState,
        object_index: usize,
        path: &VectorPath,
        analytic_reveal: Option<AnalyticRevealKey>,
    ) -> StructuralAppendStats {
        let object = &frame.objects[object_index];
        let reveal = frame.reveal(object_index);
        let partial_reveal_bits = analytic_reveal.map(|_| reveal.clamp(0.0, 1.0).to_bits());
        let (cache_index, cache_miss) =
            match self.cache_path_mesh(path, object.style, frame.render_transform(object_index)) {
                Ok(value) => value,
                Err(_) => {
                    let slot = PreparedSlot::Unsupported(self.unsupported.len());
                    self.unsupported.push(object.id);
                    self.slots.push(slot);
                    self.append_ordered_render_slot(slot);
                    return StructuralAppendStats::default();
                }
            };

        let packed = if partial_reveal_bits.is_some() {
            pack_path(object, frame.render_transform(object_index), 1.0, 0.0)
        } else {
            pack_path(
                object,
                frame.render_transform(object_index),
                reveal,
                frame.morph(object_index),
            )
        };
        let path_index = self.paths.len();
        self.path_ids.push(object.id);
        self.paths.push(packed);
        push_dirty_range(&mut self.path_dirty_ranges, path_index);

        let reveal_head_instance =
            if partial_reveal_bits.is_none() && should_create_path_reveal_head(object, reveal) {
                Some(pack_path_reveal_head(
                    object,
                    frame.render_transform(object_index),
                    &self.path_mesh_cache[cache_index].mesh,
                    reveal,
                ))
            } else {
                None
            };
        let reveal_head = reveal_head_instance.map(|instance| {
            let line_index = self.lines.len();
            self.line_ids.push(object.id);
            self.lines.push(instance);
            push_dirty_range(&mut self.line_dirty_ranges, line_index);
            line_index
        });

        let shared_geometry = self
            .path_batch_cache_indices
            .iter()
            .position(|&existing| existing == cache_index);
        let (vertex_range, index_range, vertices_repacked, indices_repacked, mega_eligible) =
            if let Some(ranges) = &self.path_mesh_cache[cache_index].resident {
                (ranges.vertices.clone(), ranges.indices.clone(), 0, 0, false)
            } else if let Some(batch) = shared_geometry {
                (
                    self.path_batch_vertex_ranges[batch].clone(),
                    self.path_batches[batch].index_range.clone(),
                    0,
                    0,
                    false,
                )
            } else {
                let (packed_vertices, local_indices) = {
                    let mesh = &self.path_mesh_cache[cache_index].mesh;
                    (
                        mesh.vertices
                            .iter()
                            .map(|vertex| PathVertex {
                                position: [vertex.position.x, vertex.position.y],
                                target_position: [
                                    vertex.target_position.x,
                                    vertex.target_position.y,
                                ],
                                surface: pack_path_surface(vertex.surface, vertex.path_progress),
                            })
                            .collect::<Vec<_>>(),
                        mesh.indices.clone(),
                    )
                };
                let vertex_start = u32::try_from(self.path_vertices.len())
                    .expect("path vertex count exceeds renderer limits");
                let index_start = u32::try_from(self.path_indices.len())
                    .expect("path index count exceeds renderer limits");
                self.path_vertices.extend_from_slice(&packed_vertices);
                self.path_indices.extend(local_indices.iter().map(|index| {
                    index
                        .checked_add(vertex_start)
                        .expect("path index exceeds renderer limits")
                }));
                let vertex_end = u32::try_from(self.path_vertices.len())
                    .expect("path vertex count exceeds renderer limits");
                let index_end = u32::try_from(self.path_indices.len())
                    .expect("path index count exceeds renderer limits");
                let vertex_range = vertex_start..vertex_end;
                let index_range = index_start..index_end;
                self.path_vertex_dirty_ranges
                    .push(range_usize_u32(&vertex_range));
                self.path_index_dirty_ranges
                    .push(range_usize_u32(&index_range));
                self.path_geometry_dirty = true;
                (
                    vertex_range,
                    index_range,
                    packed_vertices.len(),
                    local_indices.len(),
                    true,
                )
            };

        let batch = self.path_batches.len();
        let instance_start = u32::try_from(path_index).expect("path instance count exceeds limits");
        self.path_batch_vertex_ranges.push(vertex_range);
        self.path_batches.push(PathBatch {
            index_range,
            instance_range: instance_start..instance_start + 1,
        });
        self.path_batch_cache_indices.push(cache_index);
        if !self.individual_path_draws {
            self.mega_path_segments.push(None);
            self.mega_path_detached.push(false);
        }
        let slot = PreparedSlot::Path {
            index: path_index,
            batch,
            analytic_reveal,
            partial_reveal_bits,
            reveal_head,
        };
        self.slots.push(slot);

        let appended_to_mega = mega_eligible && self.append_mega_path_draw(batch, packed);
        if appended_to_mega {
            if let Some(line_index) = reveal_head {
                self.append_ordered_reveal_head(line_index);
            }
        } else {
            self.append_ordered_render_slot(slot);
        }

        StructuralAppendStats {
            instances_repacked: 1 + usize::from(reveal_head.is_some()),
            geometry_cache_misses: usize::from(cache_miss),
            path_vertices_repacked: vertices_repacked,
            path_indices_repacked: indices_repacked,
        }
    }

    fn retire_structural_slot(&mut self, object_index: usize) -> usize {
        match self.slots[object_index] {
            PreparedSlot::Absent => 0,
            PreparedSlot::Circle(index) => {
                if self.circles[index].style.opacity != 0.0 {
                    self.circles[index].style.opacity = 0.0;
                    push_dirty_range(&mut self.circle_dirty_ranges, index);
                }
                1
            }
            PreparedSlot::Rectangle(index) => {
                if self.rectangles[index].style.opacity != 0.0 {
                    self.rectangles[index].style.opacity = 0.0;
                    push_dirty_range(&mut self.rectangle_dirty_ranges, index);
                }
                1
            }
            PreparedSlot::Line(index) => {
                if self.lines[index].style.opacity != 0.0 {
                    self.lines[index].style.opacity = 0.0;
                    push_dirty_range(&mut self.line_dirty_ranges, index);
                }
                1
            }
            PreparedSlot::Path {
                index,
                batch,
                reveal_head,
                ..
            } => {
                let mut packed = self.paths[index];
                packed.style.opacity = 0.0;
                if self.paths[index] != packed {
                    self.paths[index] = packed;
                    push_dirty_range(&mut self.path_dirty_ranges, index);
                    self.update_mega_path_instance(batch, packed);
                }
                let mut repacked = 1;
                if let Some(head_index) = reveal_head {
                    repacked += 1;
                    if self.lines[head_index].style.opacity != 0.0 {
                        self.lines[head_index].style.opacity = 0.0;
                        push_dirty_range(&mut self.line_dirty_ranges, head_index);
                    }
                }
                repacked
            }
            PreparedSlot::Unsupported(_) => 0,
        }
    }

    fn can_replace_unique_path_geometry(&self, frame: &FrameState, object_index: usize) -> bool {
        let Some(object) = frame.objects.get(object_index) else {
            return false;
        };
        if !frame.is_present(object_index) {
            return false;
        }
        let Some(PreparedSlot::Path { index, batch, .. }) = self.slots.get(object_index) else {
            return false;
        };
        let Some(path_batch) = self.path_batches.get(*batch) else {
            return false;
        };
        let Some(render_geometry) = frame.render_geometry(object_index) else {
            return false;
        };
        let reveal = frame.reveal(object_index);
        let has_replacement_path = temporary_reveal_path(render_geometry, reveal).is_some()
            || matches!(render_geometry, GeometryRef::VectorPath(_));
        path_batch.instance_range.end == path_batch.instance_range.start + 1
            && self.path_ids.get(*index) == Some(&object.id)
            && has_replacement_path
    }

    fn replace_unique_path_geometry(
        &mut self,
        frame: &FrameState,
        object_index: usize,
    ) -> Result<PathReplacementStats, noon_geometry::GeometryError> {
        let object = &frame.objects[object_index];
        let render_geometry = frame
            .render_geometry(object_index)
            .expect("unique path replacement preflight requires renderable geometry");
        let reveal = frame.reveal(object_index);
        let temporary_reveal = temporary_reveal_path(render_geometry, reveal);
        let path = temporary_reveal
            .as_ref()
            .map(|(_, path)| path)
            .or(match render_geometry {
                GeometryRef::VectorPath(path) => Some(path),
                _ => None,
            })
            .expect("unique path replacement preflight requires renderable path geometry");
        let PreparedSlot::Path { batch, .. } = self.slots[object_index] else {
            unreachable!("unique path replacement preflight requires a path slot");
        };
        let (cache_index, cache_miss) =
            self.cache_path_mesh(path, object.style, frame.render_transform(object_index))?;
        let resident = self.path_mesh_cache[cache_index].resident.clone();
        let (vertices_repacked, indices_repacked) = if let Some(ranges) = resident {
            let old_vertices = self.path_batch_vertex_ranges[batch].clone();
            let old_indices = self.path_batches[batch].index_range.clone();
            if old_vertices.start as usize >= self.resident_vertex_count {
                insert_free_range(&mut self.path_vertex_free_ranges, old_vertices);
            }
            if old_indices.start as usize >= self.resident_index_count {
                insert_free_range(&mut self.path_index_free_ranges, old_indices);
            }
            self.path_batch_vertex_ranges[batch] = ranges.vertices;
            self.path_batches[batch].index_range = ranges.indices;
            (0, 0)
        } else {
            let mesh = &self.path_mesh_cache[cache_index].mesh;
            let packed_vertices = mesh
                .vertices
                .iter()
                .map(|vertex| PathVertex {
                    position: [vertex.position.x, vertex.position.y],
                    target_position: [vertex.target_position.x, vertex.target_position.y],
                    surface: pack_path_surface(vertex.surface, vertex.path_progress),
                })
                .collect::<Vec<_>>();
            let local_indices = mesh.indices.clone();

            let old_vertex_range = self.path_batch_vertex_ranges[batch].clone();
            let old_vertex_range = if old_vertex_range.start < self.resident_vertex_count as u32 {
                0..0
            } else {
                old_vertex_range
            };
            let old_index_range = self.path_batches[batch].index_range.clone();
            let old_index_range = if old_index_range.start < self.resident_index_count as u32 {
                0..0
            } else {
                old_index_range
            };
            let vertex_range = allocate_replacement_range(
                old_vertex_range,
                packed_vertices.len(),
                &mut self.path_vertex_free_ranges,
                self.path_vertices.len(),
            );
            let index_range = allocate_replacement_range(
                old_index_range,
                local_indices.len(),
                &mut self.path_index_free_ranges,
                self.path_indices.len(),
            );

            let vertex_range_usize = range_usize_u32(&vertex_range);
            if self.path_vertices.len() < vertex_range_usize.end {
                self.path_vertices
                    .resize(vertex_range_usize.end, PathVertex::zeroed());
            }
            self.path_vertices[vertex_range_usize.clone()].copy_from_slice(&packed_vertices);
            self.path_vertex_dirty_ranges.push(vertex_range_usize);

            let index_range_usize = range_usize_u32(&index_range);
            if self.path_indices.len() < index_range_usize.end {
                self.path_indices.resize(index_range_usize.end, 0);
            }
            let vertex_start = vertex_range.start;
            for (target, local) in self.path_indices[index_range_usize.clone()]
                .iter_mut()
                .zip(local_indices.iter().copied())
            {
                *target = local
                    .checked_add(vertex_start)
                    .expect("path index exceeds renderer limits");
            }
            self.path_index_dirty_ranges.push(index_range_usize);

            self.path_batch_vertex_ranges[batch] = vertex_range;
            self.path_batches[batch].index_range = index_range;
            self.path_geometry_dirty = true;
            (packed_vertices.len(), local_indices.len())
        };
        self.path_batch_cache_indices[batch] = cache_index;
        if let PreparedSlot::Path {
            analytic_reveal,
            partial_reveal_bits,
            reveal_head,
            ..
        } = &mut self.slots[object_index]
        {
            *analytic_reveal = temporary_reveal.as_ref().map(|(key, _)| *key);
            *partial_reveal_bits = temporary_reveal
                .as_ref()
                .map(|_| reveal.clamp(0.0, 1.0).to_bits());
            *reveal_head = None;
        }
        self.detach_mega_path(batch);
        Ok(PathReplacementStats {
            cache_miss,
            vertices_repacked,
            indices_repacked,
        })
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
        self.path_batch_vertex_ranges.clear();
        self.path_vertex_free_ranges.clear();
        self.path_index_free_ranges.clear();
        self.render_batches.clear();
        let previous_path_batch_cache_indices = std::mem::take(&mut self.path_batch_cache_indices);
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
            let Some(render_geometry) = frame.render_geometry(object_index) else {
                self.slots.push(PreparedSlot::Absent);
                continue;
            };
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
                let cache_index = match self.cache_path_mesh(
                    path,
                    object.style,
                    frame.render_transform(object_index),
                ) {
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
                let partial_reveal_bits = temporary_reveal
                    .as_ref()
                    .map(|_| reveal.clamp(0.0, 1.0).to_bits());
                path_groups[batch].ids.push(object.id);
                path_groups[batch]
                    .instances
                    .push(if partial_reveal_bits.is_some() {
                        pack_path(object, frame.render_transform(object_index), 1.0, 0.0)
                    } else {
                        pack_path(
                            object,
                            frame.render_transform(object_index),
                            reveal,
                            frame.morph(object_index),
                        )
                    });
                let reveal_head = if partial_reveal_bits.is_none()
                    && should_create_path_reveal_head(object, reveal)
                {
                    let head_index = self.lines.len();
                    self.line_ids.push(object.id);
                    self.lines.push(pack_path_reveal_head(
                        object,
                        frame.render_transform(object_index),
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
                    partial_reveal_bits,
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

        let (group_offsets, path_vertices_repacked, path_indices_repacked) =
            if self.resident_vertex_count != 0 || self.resident_index_count != 0 {
                self.pack_resident_path_groups(path_groups)
            } else {
                self.pack_transient_path_groups(path_groups, &previous_path_batch_cache_indices)
            };
        for slot in &mut self.slots {
            if let PreparedSlot::Path { index, batch, .. } = slot {
                *index += group_offsets[*batch];
            }
        }
        self.rebuild_ordered_render_batches();
        self.rebuild_mega_path_draws();

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
            path_vertices_repacked,
            path_indices_repacked,
            0,
            0,
            1,
        )
    }

    fn pack_transient_path_groups(
        &mut self,
        path_groups: Vec<PathGroup>,
        previous_path_batch_cache_indices: &[usize],
    ) -> (Vec<usize>, usize, usize) {
        let reuse_packed_path_geometry = self.packed_path_mesh_cache_generation
            == self.path_mesh_cache_generation
            && previous_path_batch_cache_indices.len() == path_groups.len()
            && previous_path_batch_cache_indices
                .iter()
                .zip(&path_groups)
                .all(|(&cache_index, group)| cache_index == group.cache_index);
        let mut next_vertices = (!reuse_packed_path_geometry).then(Vec::new);
        let mut next_indices = (!reuse_packed_path_geometry).then(Vec::new);
        let mut vertex_count = 0usize;
        let mut index_count = 0usize;
        let mut group_offsets = Vec::with_capacity(path_groups.len());
        for group in path_groups {
            let instance_start = self.paths.len();
            group_offsets.push(instance_start);
            self.path_ids.extend(group.ids);
            self.paths.extend(group.instances);

            let mesh = &self.path_mesh_cache[group.cache_index].mesh;
            let vertex_start =
                u32::try_from(vertex_count).expect("path vertex count exceeds renderer limits");
            let index_start =
                u32::try_from(index_count).expect("path index count exceeds renderer limits");
            if let (Some(vertices), Some(indices)) = (next_vertices.as_mut(), next_indices.as_mut())
            {
                vertices.extend(mesh.vertices.iter().map(|vertex| PathVertex {
                    position: [vertex.position.x, vertex.position.y],
                    target_position: [vertex.target_position.x, vertex.target_position.y],
                    surface: pack_path_surface(vertex.surface, vertex.path_progress),
                }));
                indices.extend(mesh.indices.iter().map(|index| {
                    index
                        .checked_add(vertex_start)
                        .expect("path index exceeds renderer limits")
                }));
            }
            vertex_count += mesh.vertices.len();
            index_count += mesh.indices.len();
            let vertex_end =
                u32::try_from(vertex_count).expect("path vertex count exceeds renderer limits");
            let index_end =
                u32::try_from(index_count).expect("path index count exceeds renderer limits");
            self.path_batch_vertex_ranges.push(vertex_start..vertex_end);
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
        let (path_vertices_repacked, path_indices_repacked) = if reuse_packed_path_geometry {
            self.path_geometry_dirty = false;
            (0, 0)
        } else {
            let next_vertices = next_vertices.expect("path vertices must be repacked");
            let next_indices = next_indices.expect("path indices must be repacked");
            let repacked = (next_vertices.len(), next_indices.len());
            self.path_geometry_dirty =
                self.path_vertices != next_vertices || self.path_indices != next_indices;
            self.path_vertices = next_vertices;
            self.path_indices = next_indices;
            if !self.path_vertices.is_empty() {
                self.path_vertex_dirty_ranges
                    .push(0..self.path_vertices.len());
            }
            if !self.path_indices.is_empty() {
                self.path_index_dirty_ranges
                    .push(0..self.path_indices.len());
            }
            self.packed_path_mesh_cache_generation = self.path_mesh_cache_generation;
            repacked
        };
        (group_offsets, path_vertices_repacked, path_indices_repacked)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepared_frame(
        &self,
        time: f64,
        capacity_growths: usize,
        instances_repacked: usize,
        geometry_cache_misses: usize,
        path_vertices_repacked: usize,
        path_indices_repacked: usize,
        structural_slots_added: usize,
        structural_slots_retired: usize,
        full_rebuilds: usize,
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
            mega_path_indices: &self.mega_path_indices,
            mega_path_vertex_instances: &self.mega_path_vertex_instances,
            mega_path_batches: &self.mega_path_batches,
            render_batches: &self.render_batches,
            unsupported: &self.unsupported,
            circle_dirty_ranges: &self.circle_dirty_ranges,
            rectangle_dirty_ranges: &self.rectangle_dirty_ranges,
            line_dirty_ranges: &self.line_dirty_ranges,
            path_dirty_ranges: &self.path_dirty_ranges,
            path_vertex_dirty_ranges: &self.path_vertex_dirty_ranges,
            path_index_dirty_ranges: &self.path_index_dirty_ranges,
            mega_path_instance_dirty_ranges: &self.mega_path_instance_dirty_ranges,
            mega_path_index_dirty_ranges: &self.mega_path_index_dirty_ranges,
            mega_path_index_dirty: self.mega_path_index_dirty,
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
                path_vertices_repacked,
                path_indices_repacked,
                mega_path_count: self
                    .mega_path_batches
                    .iter()
                    .map(|batch| batch.path_count)
                    .sum(),
                mega_path_batch_count: self.mega_path_batches.len(),
                mega_path_index_count: self.mega_path_indices.len(),
                mega_path_indices_repacked: dirty_len(&self.mega_path_index_dirty_ranges),
                mega_path_instance_vertices_repacked: dirty_len(
                    &self.mega_path_instance_dirty_ranges,
                ),
                path_vertex_free_range_count: self.path_vertex_free_ranges.len(),
                path_vertex_free_element_count: self
                    .path_vertex_free_ranges
                    .iter()
                    .map(|range| (range.end - range.start) as usize)
                    .sum(),
                path_index_free_range_count: self.path_index_free_ranges.len(),
                path_index_free_element_count: self
                    .path_index_free_ranges
                    .iter()
                    .map(|range| (range.end - range.start) as usize)
                    .sum(),
                mega_path_detached_count: self
                    .mega_path_detached
                    .iter()
                    .filter(|&&detached| detached)
                    .count(),
                structural_slots_added,
                structural_slots_retired,
                full_rebuilds,
            },
        }
    }

    fn clear_dirty_ranges(&mut self) {
        self.circle_dirty_ranges.clear();
        self.rectangle_dirty_ranges.clear();
        self.line_dirty_ranges.clear();
        self.path_dirty_ranges.clear();
        self.path_vertex_dirty_ranges.clear();
        self.path_index_dirty_ranges.clear();
        self.mega_path_instance_dirty_ranges.clear();
        self.mega_path_index_dirty_ranges.clear();
        self.mega_path_index_dirty = false;
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
        let Some(render_geometry) = frame.render_geometry(object_index) else {
            return false;
        };
        match slot {
            PreparedSlot::Absent => false,
            PreparedSlot::Circle(index) => {
                matches!(render_geometry, GeometryRef::Circle { .. })
                    && self.circle_ids.get(*index) == Some(&object.id)
            }
            PreparedSlot::Rectangle(index) => {
                matches!(render_geometry, GeometryRef::Rectangle { .. })
                    && frame.reveal(object_index) >= 1.0
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
                partial_reveal_bits,
                reveal_head,
            } => {
                let Some(cache_index) = self.path_batch_cache_indices.get(*batch) else {
                    return false;
                };
                let cache = &self.path_mesh_cache[*cache_index];
                let geometry_matches = match analytic_reveal {
                    Some(expected) => {
                        let reveal = frame.reveal(object_index).clamp(0.0, 1.0);
                        reveal < 1.0
                            && analytic_reveal_key(render_geometry) == Some(*expected)
                            && *partial_reveal_bits == Some(reveal.to_bits())
                            && temporary_reveal_path(render_geometry, reveal)
                                .is_some_and(|(_, path)| cache.path == path)
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
                    && cache.stroke_transform
                        == path_stroke_transform_key(
                            object.style,
                            frame.render_transform(object_index),
                        )
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

    fn capacities(&self) -> [usize; 32] {
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
            self.path_batch_vertex_ranges.capacity(),
            self.mega_path_indices.capacity(),
            self.mega_path_vertex_instances.capacity(),
            self.mega_path_batches.capacity(),
            self.mega_path_segments.capacity(),
            self.mega_path_detached.capacity(),
            self.mega_path_instance_dirty_ranges.capacity(),
            self.mega_path_index_dirty_ranges.capacity(),
            self.path_batch_cache_indices.capacity(),
            self.path_mesh_cache.capacity(),
            self.path_mesh_lookup.capacity(),
            self.unsupported.capacity(),
            self.slots.capacity(),
            self.circle_dirty_ranges.capacity(),
            self.rectangle_dirty_ranges.capacity(),
            self.line_dirty_ranges.capacity(),
            self.path_dirty_ranges.capacity(),
            self.path_vertex_dirty_ranges.capacity(),
            self.path_index_dirty_ranges.capacity(),
            self.path_vertex_free_ranges.capacity(),
            self.path_index_free_ranges.capacity(),
        ]
    }

    fn cache_path_mesh(
        &mut self,
        path: &VectorPath,
        style: Style,
        transform: Transform2D,
    ) -> Result<(usize, bool), noon_geometry::GeometryError> {
        let stroke_transform = path_stroke_transform_key(style, transform);
        let stroke_width_bits = style.stroke_width.to_bits();
        let fill_enabled = style.fill.is_some();
        let key = path_mesh_key(
            path,
            stroke_transform,
            stroke_width_bits,
            style.stroke_join,
            style.stroke_cap,
            fill_enabled,
        );
        let existing = self.path_mesh_lookup.get(&key).and_then(|candidates| {
            candidates.iter().copied().find(|&index| {
                let entry = &self.path_mesh_cache[index];
                entry.path == *path
                    && entry.stroke_transform == stroke_transform
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

        let transformed_path;
        let tessellation_path = if style.stroke_width_mode == StrokeWidthMode::ScreenSpace {
            transformed_path = transform_path_without_translation(path, transform);
            &transformed_path
        } else {
            path
        };
        let mesh = if style.stroke_width_mode == StrokeWidthMode::ScreenSpace
            && tessellation_path.morph_target().is_some()
        {
            noon_geometry::tessellate_styled_with_fill_preserving_morph_order(
                tessellation_path,
                style.stroke_width,
                style.stroke_join,
                style.stroke_cap,
                fill_enabled,
            )?
        } else {
            noon_geometry::tessellate_styled_with_fill(
                tessellation_path,
                style.stroke_width,
                style.stroke_join,
                style.stroke_cap,
                fill_enabled,
            )?
        };
        let index = self.path_mesh_cache.len();
        let last_used = self.next_path_mesh_use();
        self.path_mesh_cache.push(CachedPathMesh {
            path: path.clone(),
            stroke_transform,
            stroke_width_bits,
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            fill_enabled,
            mesh,
            resident: None,
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
                && frame
                    .render_geometry(object_index)
                    .and_then(analytic_reveal_key)
                    .is_some()
        }) {
            return;
        }

        let mut keep: Vec<_> = self
            .path_mesh_cache
            .iter()
            .map(|entry| entry.resident.is_some())
            .collect();
        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                continue;
            }
            let Some(GeometryRef::VectorPath(path)) = frame.render_geometry(object_index) else {
                continue;
            };
            let stroke_transform =
                path_stroke_transform_key(object.style, frame.render_transform(object_index));
            let stroke_width_bits = object.style.stroke_width.to_bits();
            let fill_enabled = object.style.fill.is_some();
            let key = path_mesh_key(
                path,
                stroke_transform,
                stroke_width_bits,
                object.style.stroke_join,
                object.style.stroke_cap,
                fill_enabled,
            );
            if let Some(candidates) = self.path_mesh_lookup.get(&key) {
                if let Some(index) = candidates.iter().copied().find(|&index| {
                    let entry = &self.path_mesh_cache[index];
                    entry.path == *path
                        && entry.stroke_transform == stroke_transform
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

        let packed_generation_current =
            self.packed_path_mesh_cache_generation == self.path_mesh_cache_generation;
        self.path_mesh_cache_generation = self.path_mesh_cache_generation.saturating_add(1);
        let old_cache = std::mem::take(&mut self.path_mesh_cache);
        let mut remapped_indices = vec![None; old_cache.len()];
        self.path_mesh_lookup.clear();
        for (old_index, entry) in old_cache.into_iter().enumerate() {
            if !keep[old_index] {
                continue;
            }
            let key = path_mesh_key(
                &entry.path,
                entry.stroke_transform,
                entry.stroke_width_bits,
                entry.stroke_join,
                entry.stroke_cap,
                entry.fill_enabled,
            );
            let new_index = self.path_mesh_cache.len();
            remapped_indices[old_index] = Some(new_index);
            self.path_mesh_cache.push(entry);
            self.path_mesh_lookup
                .entry(key)
                .or_default()
                .push(new_index);
        }
        // Evicting stale cache entries changes their indices, not the packed
        // geometry of surviving batches. Preserve that correspondence so the
        // next frame does not repack and upload the same meshes a second time.
        if packed_generation_current
            && self
                .path_batch_cache_indices
                .iter()
                .all(|&index| remapped_indices[index].is_some())
        {
            for index in &mut self.path_batch_cache_indices {
                *index = remapped_indices[*index].expect("packed mesh survived cache pruning");
            }
            self.packed_path_mesh_cache_generation = self.path_mesh_cache_generation;
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

fn range_usize_u32(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

fn allocate_replacement_range(
    old: Range<u32>,
    required_len: usize,
    free_ranges: &mut Vec<Range<u32>>,
    arena_len: usize,
) -> Range<u32> {
    let required = u32::try_from(required_len).expect("path arena range exceeds renderer limits");
    let old_len = old.end.saturating_sub(old.start);
    if required == 0 {
        insert_free_range(free_ranges, old);
        return 0..0;
    }
    if required <= old_len {
        let used = old.start..old.start + required;
        insert_free_range(free_ranges, used.end..old.end);
        return used;
    }

    insert_free_range(free_ranges, old);
    if let Some(index) = free_ranges
        .iter()
        .position(|range| range.end.saturating_sub(range.start) >= required)
    {
        let start = free_ranges[index].start;
        let used = start..start + required;
        free_ranges[index].start += required;
        if free_ranges[index].is_empty() {
            free_ranges.remove(index);
        }
        return used;
    }

    let start = u32::try_from(arena_len).expect("path arena exceeds renderer limits");
    start..start + required
}

fn insert_free_range(free_ranges: &mut Vec<Range<u32>>, range: Range<u32>) {
    if range.is_empty() {
        return;
    }
    free_ranges.push(range);
    free_ranges.sort_unstable_by_key(|range| range.start);
    let mut write = 0usize;
    for read in 0..free_ranges.len() {
        if write > 0 && free_ranges[write - 1].end >= free_ranges[read].start {
            free_ranges[write - 1].end = free_ranges[write - 1].end.max(free_ranges[read].end);
        } else {
            free_ranges[write] = free_ranges[read].clone();
            write += 1;
        }
    }
    free_ranges.truncate(write);
}

fn path_stroke_transform_key(style: Style, transform: Transform2D) -> PathStrokeTransformKey {
    if style.stroke_width_mode == StrokeWidthMode::ScreenSpace {
        PathStrokeTransformKey {
            scale_x_bits: transform.scale.x.to_bits(),
            scale_y_bits: transform.scale.y.to_bits(),
            rotation_bits: transform.rotation.to_bits(),
        }
    } else {
        PathStrokeTransformKey::default()
    }
}

fn transform_path_without_translation(path: &VectorPath, transform: Transform2D) -> VectorPath {
    path.transformed(Transform2D {
        translation: Vec2::ZERO,
        ..transform
    })
}

fn packed_path_transform(style: Style, transform: Transform2D) -> PackedTransform {
    if style.stroke_width_mode == StrokeWidthMode::ScreenSpace {
        PackedTransform {
            translation: [transform.translation.x, transform.translation.y],
            scale: [1.0, 1.0],
            rotation: 0.0,
            padding: 0.0,
        }
    } else {
        transform.into()
    }
}

fn path_mesh_key(
    path: &VectorPath,
    stroke_transform: PathStrokeTransformKey,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
) -> PathMeshKey {
    let mut hasher = DefaultHasher::new();
    hash_vector_path(path, &mut hasher);
    PathMeshKey {
        path_hash: hasher.finish(),
        stroke_transform,
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
    let Some(GeometryRef::Circle { radius }) = object.geometry() else {
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
    let Some(GeometryRef::Rectangle { size }) = object.geometry() else {
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
    let Some(GeometryRef::Line { start, end }) = object.geometry() else {
        unreachable!("line slot must retain line geometry")
    };
    let mut transform: PackedTransform = object.transform.into();
    transform.padding = reveal.clamp(0.0, 1.0);
    let mut style = pack_style(object);
    // Cap mode is line geometry, not a global style-packing concern. Keeping
    // these bits off circles/rectangles/paths preserves their exact packed bytes
    // and prevents line-cap semantics from perturbing unrelated raster paths.
    style.stroke_enabled |= match object.style.stroke_cap {
        StrokeCap::Round => 0,
        StrokeCap::Butt => 1 << 2,
        StrokeCap::Square => 2 << 2,
    };
    LineInstance {
        transform,
        style,
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
    render_transform: Transform2D,
    mesh: &TessellatedPath,
    reveal: f32,
) -> LineInstance {
    let reveal = reveal.clamp(0.0, 1.0);
    let point = mesh.reveal_head_position(reveal).unwrap_or(Vec2::ZERO);
    let mut transform = packed_path_transform(object.style, render_transform);
    transform.padding = 1.0;
    let mut style = pack_style(object);
    style.fill = [0.0; 4];
    style.fill_enabled = 0;
    if let Some(color) = object.style.stroke.or(object.style.fill) {
        style.stroke = [color.red, color.green, color.blue, color.alpha];
        style.stroke_enabled = (style.stroke_enabled & 2) | 1;
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

fn pack_path(
    object: &FrameObjectState,
    render_transform: Transform2D,
    reveal: f32,
    morph: f32,
) -> PathInstance {
    PathInstance {
        transform: packed_path_transform(object.style, render_transform),
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
    mod path_residency;

    use noon_core::{Color, GeometryId, Vec2, VectorPath};
    use noon_runtime::FrameObjectState;

    use super::*;

    fn object(id: u64, geometry: GeometryRef) -> FrameObjectState {
        FrameObjectState {
            id: ObjectId::new(id),
            content: noon_core::ObjectContentRef::Geometry(geometry),
            text_bounds: None,
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
            render_transforms: vec![None; render_geometries.len()],
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
        initial.render_geometries[0] = Some(geometry.clone().into());
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
    fn full_rebuild_reuses_unchanged_packed_path_geometry() {
        let geometry = GeometryRef::path(curved_path());
        let mut path = object(23, geometry);
        path.style.fill = Some(Color::WHITE);
        path.style.stroke = Some(Color::BLACK);
        path.style.stroke_width = 0.08;
        let initial = frame(vec![path]);
        let mut preparer = FramePreparer::new();

        let (vertices, indices) = {
            let cold = preparer.prepare(&initial);
            assert!(cold.stats.path_vertices_repacked > 0);
            assert!(cold.stats.path_indices_repacked > 0);
            (cold.path_vertices.to_vec(), cold.path_indices.to_vec())
        };
        let rebuilt = preparer.prepare(&initial);
        assert_eq!(rebuilt.stats.geometry_cache_misses, 0);
        assert_eq!(rebuilt.stats.path_vertices_repacked, 0);
        assert_eq!(rebuilt.stats.path_indices_repacked, 0);
        assert!(!rebuilt.path_geometry_dirty);
        assert_eq!(rebuilt.path_vertices, vertices);
        assert_eq!(rebuilt.path_indices, indices);
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
    fn circle_create_uses_partial_geometry_then_returns_to_analytic_circle() {
        let mut state = object(7, GeometryRef::circle(1.25));
        state.style.fill = Some(Color::rgba(1.0, 0.0, 0.5, 0.5));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.08;
        let mut frame = frame(vec![state]);
        frame.reveals[0] = 0.25;
        let mut preparer = FramePreparer::new();

        let (cold_vertices, cold_indices) = {
            let cold = preparer.prepare(&frame);
            assert!(cold.circles.is_empty());
            assert_eq!(cold.paths.len(), 1);
            assert!(cold.lines.is_empty());
            assert_eq!(cold.paths[0].path_params, [1.0, 0.0]);
            assert_eq!(cold.stats.geometry_cache_misses, 1);
            assert!(cold.path_geometry_dirty);
            (cold.path_vertices.to_vec(), cold.path_indices.to_vec())
        };

        frame.reveals[0] = 0.6;
        let steady = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert!(steady.circles.is_empty());
        assert_eq!(steady.paths.len(), 1);
        assert_eq!(steady.paths[0].path_params, [1.0, 0.0]);
        assert!(steady.lines.is_empty());
        assert_eq!(steady.stats.geometry_cache_misses, 1);
        assert!(steady.path_geometry_dirty);
        assert!(steady.stats.path_vertices_repacked > 0);
        assert!(steady.stats.path_indices_repacked > 0);
        assert!(steady.path_vertices != cold_vertices || steady.path_indices != cold_indices);

        frame.reveals[0] = 1.0;
        let complete = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(complete.circles.len(), 1);
        assert!(complete.paths.is_empty());
        assert_eq!(complete.circles[0].padding[0], 1.0);
        assert_eq!(complete.stats.instance_count, 1);
    }

    #[test]
    fn closed_analytic_create_uses_partial_paths_while_line_stays_analytic() {
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
        assert!(prepared.circles.is_empty());
        assert!(prepared.rectangles.is_empty());
        assert_eq!(prepared.paths.len(), 2);
        assert!(prepared
            .paths
            .iter()
            .all(|path| path.path_params == [1.0, 0.0]));
        assert_eq!(prepared.lines.len(), 1);
        assert_eq!(prepared.lines[0].transform.padding, 0.5);
        assert_eq!(prepared.stats.instance_count, 3);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 2);

        frame.reveals[2] = 0.8;
        let advanced = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2]));
        assert_eq!(advanced.lines.len(), 1);
        assert_eq!(advanced.lines[0].transform.padding, 0.8);
        assert_eq!(advanced.stats.geometry_cache_misses, 0);
        assert_eq!(advanced.stats.instances_repacked, 1);
        assert_eq!(advanced.line_dirty_ranges.len(), 1);
        assert_eq!(advanced.line_dirty_ranges[0], 0..1);
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
    fn screen_space_stroke_mode_uses_existing_style_flag_bits() {
        let mut state = object(42, GeometryRef::circle(1.0));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.04;
        state.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.circles[0].style.stroke_enabled, 3);
        assert_eq!(prepared.circles[0].style.stroke_width, 0.04);
        assert_eq!(std::mem::size_of::<PackedStyle>(), 48);
    }

    #[test]
    fn screen_space_path_bakes_scale_and_rotation_before_tessellation() {
        let geometry = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(0.0, 0.0))
                .line_to(Vec2::new(1.0, 0.0)),
        );
        let mut state = object(43, geometry);
        state.transform = Transform2D {
            translation: Vec2::new(5.0, -2.0),
            rotation: std::f32::consts::FRAC_PI_2,
            scale: Vec2::new(2.0, 3.0),
        };
        state.style.fill = None;
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.04;
        state.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
        let frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.paths[0].transform.translation, [5.0, -2.0]);
        assert_eq!(prepared.paths[0].transform.scale, [1.0, 1.0]);
        assert_eq!(prepared.paths[0].transform.rotation, 0.0);
        let max_y = prepared
            .path_vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(max_y > 1.9, "scaled path must be tessellated near y=2");
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
            stroke_width_mode: Default::default(),
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
            stroke_width_mode: Default::default(),
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
        frame.objects[0].content =
            noon_core::ObjectContentRef::Geometry(GeometryRef::rectangle(2.0, 3.0));

        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(prepared.circles.is_empty());
        assert_eq!(prepared.rectangle_dirty_ranges.len(), 1);
        assert_eq!(prepared.rectangle_dirty_ranges[0], 0..1);
    }

    #[test]
    fn ten_thousand_unique_paths_collapse_into_one_mega_draw_batch() {
        const OBJECT_COUNT: usize = 10_000;
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let y = index as f32 * 0.0001;
                let geometry = GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.5, y))
                        .line_to(Vec2::new(0.5, y)),
                );
                let mut state = object(index as u64, geometry);
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.01;
                state
            })
            .collect();
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.path_batches.len(), OBJECT_COUNT);
        assert_eq!(prepared.mega_path_batches.len(), 1);
        assert_eq!(prepared.stats.mega_path_count, OBJECT_COUNT);
        assert_eq!(prepared.stats.batch_count, 1);
        assert_eq!(prepared.render_batches.len(), 1);
        assert!(matches!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::MegaPath { .. }
        ));
    }

    #[test]
    fn structural_unique_path_append_touches_only_new_mesh_and_mega_suffix() {
        const OBJECT_COUNT: usize = 10_000;
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let y = index as f32 * 0.0001;
                let mut state = object(
                    index as u64,
                    GeometryRef::path(
                        VectorPath::new()
                            .move_to(Vec2::new(-0.5, y))
                            .line_to(Vec2::new(0.5, y)),
                    ),
                );
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.01;
                state
            })
            .collect();
        let mut frame = frame(objects);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        let first_vertex_range = preparer.path_batch_vertex_ranges[0].clone();
        let middle_vertex_range = preparer.path_batch_vertex_ranges[OBJECT_COUNT / 2].clone();
        let last_vertex_range = preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1].clone();
        let first_index_range = preparer.path_batches[0].index_range.clone();
        let last_index_range = preparer.path_batches[OBJECT_COUNT - 1].index_range.clone();
        let old_vertex_count = preparer.path_vertices.len();
        let old_index_count = preparer.path_indices.len();
        let old_mega_index_count = preparer.mega_path_indices.len();

        let mut appended = object(
            OBJECT_COUNT as u64,
            GeometryRef::path(VectorPath::new().move_to(Vec2::new(-0.75, 1.5)).cubic_to(
                Vec2::new(-0.25, 2.0),
                Vec2::new(0.25, 1.0),
                Vec2::new(0.75, 1.5),
            )),
        );
        appended.style.fill = None;
        appended.style.stroke = Some(Color::WHITE);
        appended.style.stroke_width = 0.015;
        frame.objects.push(appended);
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);

        let prepared = preparer.prepare_incremental(
            &frame,
            &FrameChanges::structural(vec![OBJECT_COUNT], Vec::new()),
        );

        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.structural_slots_added, 1);
        assert_eq!(prepared.stats.geometry_cache_misses, 1);
        assert_eq!(prepared.path_batches.len(), OBJECT_COUNT + 1);
        assert_eq!(prepared.stats.mega_path_count, OBJECT_COUNT + 1);
        assert_eq!(prepared.stats.mega_path_batch_count, 1);
        assert!(prepared.stats.path_vertices_repacked > 0);
        assert!(prepared.stats.path_vertices_repacked < old_vertex_count);
        assert!(prepared.stats.path_indices_repacked > 0);
        assert!(prepared.stats.path_indices_repacked < old_index_count);
        assert_eq!(prepared.path_vertex_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_vertex_dirty_ranges[0].start, old_vertex_count);
        assert_eq!(prepared.path_index_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_index_dirty_ranges[0].start, old_index_count);
        assert_eq!(prepared.mega_path_index_dirty_ranges.len(), 1);
        assert_eq!(
            prepared.mega_path_index_dirty_ranges[0].start,
            old_mega_index_count
        );
        assert_eq!(
            prepared.stats.mega_path_indices_repacked,
            prepared.mega_path_index_dirty_ranges[0].len()
        );
        assert!(prepared.stats.mega_path_indices_repacked < old_mega_index_count);
        assert_eq!(preparer.path_batch_vertex_ranges[0], first_vertex_range);
        assert_eq!(
            preparer.path_batch_vertex_ranges[OBJECT_COUNT / 2],
            middle_vertex_range
        );
        assert_eq!(
            preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1],
            last_vertex_range
        );
        assert_eq!(preparer.path_batches[0].index_range, first_index_range);
        assert_eq!(
            preparer.path_batches[OBJECT_COUNT - 1].index_range,
            last_index_range
        );
    }

    #[test]
    fn structural_repeated_path_append_reuses_existing_geometry_without_repacking() {
        let mut original = object(
            1,
            GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::new(-1.0, 0.0))
                    .line_to(Vec2::new(1.0, 0.0)),
            ),
        );
        original.style.fill = None;
        original.style.stroke = Some(Color::WHITE);
        original.style.stroke_width = 0.02;
        let mut frame = frame(vec![original.clone()]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        let old_vertex_count = preparer.path_vertices.len();
        let old_index_count = preparer.path_indices.len();

        original.id = ObjectId::new(2);
        original.transform.translation = Vec2::new(0.0, 1.0);
        frame.objects.push(original);
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);
        let prepared =
            preparer.prepare_incremental(&frame, &FrameChanges::structural(vec![1], Vec::new()));

        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.structural_slots_added, 1);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert_eq!(prepared.path_vertices.len(), old_vertex_count);
        assert_eq!(prepared.path_indices.len(), old_index_count);
        assert!(prepared.path_vertex_dirty_ranges.is_empty());
        assert!(prepared.path_index_dirty_ranges.is_empty());
        assert!(prepared.mega_path_index_dirty_ranges.is_empty());
        assert_eq!(prepared.path_batches.len(), 2);
        assert_eq!(
            prepared.path_batches[0].index_range,
            prepared.path_batches[1].index_range
        );
        assert!(matches!(
            prepared.render_batches.last().unwrap().primitive,
            RenderPrimitive::Path { batch: 1 }
        ));
    }

    #[test]
    fn repeated_paths_keep_true_instancing_instead_of_entering_mega_mesh() {
        const OBJECT_COUNT: usize = 2_000;
        let geometry = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.5, 0.0))
                .line_to(Vec2::new(0.5, 0.0)),
        );
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let mut state = object(index as u64, geometry.clone());
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.01;
                state
            })
            .collect();
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.path_batches.len(), 1);
        assert!(prepared.mega_path_batches.is_empty());
        assert_eq!(prepared.stats.batch_count, 1);
        assert!(matches!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::Path { batch: 0 }
        ));
    }

    #[test]
    fn unique_path_transform_update_rewrites_attributes_not_geometry() {
        let make_path = |id: u64, y: f32| {
            let mut state = object(
                id,
                GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.5, y))
                        .line_to(Vec2::new(0.5, y)),
                ),
            );
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.01;
            state
        };
        let mut frame = frame(vec![make_path(1, 0.0), make_path(2, 0.2)]);
        let mut preparer = FramePreparer::new();
        let total_vertex_instances = preparer.prepare(&frame).mega_path_vertex_instances.len();

        frame.objects[0].transform.translation = Vec2::new(1.25, -0.75);
        frame.objects[0].style.stroke = Some(Color::rgb(0.2, 0.8, 0.4));
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.mega_path_instance_dirty_ranges.len(), 1);
        let dirty = &prepared.mega_path_instance_dirty_ranges[0];
        assert!(!dirty.is_empty());
        assert!(dirty.len() < total_vertex_instances);
    }

    #[test]
    fn unique_path_replacement_dirties_only_its_geometry_chunks() {
        const OBJECT_COUNT: usize = 1_000;
        const REPLACED: usize = 500;
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let y = index as f32 * 0.002;
                let mut state = object(
                    index as u64,
                    GeometryRef::path(
                        VectorPath::new()
                            .move_to(Vec2::new(-0.5, y))
                            .line_to(Vec2::new(0.5, y)),
                    ),
                );
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.01;
                state
            })
            .collect();
        let mut frame = frame(objects);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        let first_vertex_range = preparer.path_batch_vertex_ranges[0].clone();
        let last_vertex_range = preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1].clone();
        let first_index_range = preparer.path_batches[0].index_range.clone();
        let last_index_range = preparer.path_batches[OBJECT_COUNT - 1].index_range.clone();
        let original_vertex_count = preparer.path_vertices.len();
        let original_index_count = preparer.path_indices.len();

        frame.objects[REPLACED].content = noon_core::ObjectContentRef::Geometry(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.5, 1.0))
                .cubic_to(
                    Vec2::new(-0.5, 2.0),
                    Vec2::new(0.5, 0.0),
                    Vec2::new(0.5, 1.0),
                )
                .cubic_to(
                    Vec2::new(0.5, 2.0),
                    Vec2::new(-0.5, 0.0),
                    Vec2::new(-0.5, 1.0),
                ),
        ));
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![REPLACED]));

        assert_eq!(prepared.path_vertex_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_index_dirty_ranges.len(), 1);
        assert!(prepared.stats.path_vertices_repacked > 0);
        assert!(prepared.stats.path_indices_repacked > 0);
        assert!(prepared.stats.path_vertices_repacked < original_vertex_count);
        assert!(prepared.stats.path_indices_repacked < original_index_count);
        assert!(prepared.path_geometry_dirty);
        assert_eq!(prepared.stats.geometry_cache_misses, 1);
        assert_eq!(prepared.stats.mega_path_count, OBJECT_COUNT - 1);
        assert_eq!(prepared.stats.mega_path_batch_count, 2);
        assert_eq!(prepared.stats.mega_path_detached_count, 1);
        assert_eq!(prepared.render_batches.len(), 3);
        assert!(matches!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::MegaPath { .. }
        ));
        assert!(matches!(
            prepared.render_batches[1].primitive,
            RenderPrimitive::Path { batch: REPLACED }
        ));
        assert!(matches!(
            prepared.render_batches[2].primitive,
            RenderPrimitive::MegaPath { .. }
        ));
        assert!(!prepared.mega_path_index_dirty);
        assert!(prepared.mega_path_instance_dirty_ranges.is_empty());
        assert!(prepared.stats.path_vertex_free_range_count > 0);
        assert!(prepared.stats.path_vertex_free_element_count > 0);
        assert!(prepared.stats.path_index_free_range_count > 0);
        assert!(prepared.stats.path_index_free_element_count > 0);
        assert_eq!(preparer.path_batch_vertex_ranges[0], first_vertex_range);
        assert_eq!(
            preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1],
            last_vertex_range
        );
        assert_eq!(preparer.path_batches[0].index_range, first_index_range);
        assert_eq!(
            preparer.path_batches[OBJECT_COUNT - 1].index_range,
            last_index_range
        );

        let compacted = preparer.prepare(&frame);
        assert_eq!(compacted.stats.path_vertex_free_range_count, 0);
        assert_eq!(compacted.stats.path_vertex_free_element_count, 0);
        assert_eq!(compacted.stats.path_index_free_range_count, 0);
        assert_eq!(compacted.stats.path_index_free_element_count, 0);
        assert_eq!(compacted.stats.mega_path_detached_count, 0);
        assert_eq!(compacted.stats.mega_path_count, OBJECT_COUNT);
        assert_eq!(compacted.stats.mega_path_batch_count, 1);
    }

    #[test]
    fn path_arena_reuses_released_chunks_without_shifting_other_batches() {
        let make_path = |id: u64, y: f32| {
            let mut state = object(
                id,
                GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.5, y))
                        .line_to(Vec2::new(0.5, y)),
                ),
            );
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.02;
            state
        };
        let mut frame = frame(vec![
            make_path(1, 0.0),
            make_path(2, 0.5),
            make_path(3, 1.0),
        ]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);

        frame.objects[1].content = noon_core::ObjectContentRef::Geometry(GeometryRef::path(
            VectorPath::new().move_to(Vec2::new(-1.0, 0.5)).cubic_to(
                Vec2::new(-0.5, 1.5),
                Vec2::new(0.5, -0.5),
                Vec2::new(1.0, 0.5),
            ),
        ));
        preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![1]));
        assert!(!preparer.path_vertex_free_ranges.is_empty());
        assert!(!preparer.path_index_free_ranges.is_empty());
        let arena_vertices_after_growth = preparer.path_vertices.len();
        let arena_indices_after_growth = preparer.path_indices.len();

        frame.objects[0].content = noon_core::ObjectContentRef::Geometry(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.25, 0.0))
                .line_to(Vec2::new(0.25, 0.0)),
        ));
        preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(preparer.path_vertices.len(), arena_vertices_after_growth);
        assert_eq!(preparer.path_indices.len(), arena_indices_after_growth);
    }

    #[test]
    fn stale_cache_pruning_preserves_packed_survivors_and_gpu_uploads() {
        const CACHE_LIMIT: usize = 256;
        const PINNED: usize = 300;
        let make_frame = |seed: usize, count: usize| {
            frame(
                (0..count)
                    .map(|index| {
                        let x = (seed + index) as f32;
                        let mut state = object(
                            index as u64,
                            GeometryRef::path(
                                VectorPath::new()
                                    .move_to(Vec2::new(x, 0.0))
                                    .line_to(Vec2::new(x + 0.5, 0.5)),
                            ),
                        );
                        state.style.fill = None;
                        state.style.stroke = Some(Color::WHITE);
                        state.style.stroke_width = 0.02;
                        state
                    })
                    .collect(),
            )
        };
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut preparer = FramePreparer::for_individual_path_draws();
        preparer.set_path_mesh_cache_limit(CACHE_LIMIT);
        let stale = make_frame(0, CACHE_LIMIT);
        renderer.upload(&device, &queue, &preparer.prepare(&stale));
        let mut active = make_frame(1000, PINNED);
        let cold = preparer.prepare(&active);
        assert_eq!(cold.stats.geometry_cache_misses, PINNED);
        renderer.upload(&device, &queue, &cold);
        assert_eq!(preparer.cached_path_mesh_count(), CACHE_LIMIT + PINNED);

        active.morphs.fill(0.3);
        let warm = preparer.prepare(&active);
        assert_eq!(warm.stats.geometry_cache_misses, 0);
        assert_eq!(warm.stats.path_vertices_repacked, 0);
        assert_eq!(warm.stats.path_indices_repacked, 0);
        assert!(warm.path_vertex_dirty_ranges.is_empty());
        assert!(warm.path_index_dirty_ranges.is_empty());
        let upload = renderer.upload(&device, &queue, &warm);
        assert_eq!(upload.bytes_uploaded, std::mem::size_of_val(warm.paths));
        assert_eq!(upload.buffer_reallocations, 0);
        assert_eq!(preparer.cached_path_mesh_count(), PINNED);

        // If a packed mesh actually disappears, cache compaction must invalidate
        // packed reuse even when all remaining geometry is already cached.
        active.objects[0].content = active.objects[1].content.clone();
        let changed = preparer.prepare(&active);
        assert_eq!(changed.stats.geometry_cache_misses, 0);
        assert!(changed.stats.path_vertices_repacked > 0);
        assert!(changed.stats.path_indices_repacked > 0);
        let actual_vertices = changed.path_vertices.to_vec();
        let actual_indices = changed.path_indices.to_vec();
        let mut fresh = FramePreparer::for_individual_path_draws();
        let expected = fresh.prepare(&active);
        assert_eq!(actual_vertices, expected.path_vertices);
        assert_eq!(actual_indices, expected.path_indices);
        assert_eq!(preparer.cached_path_mesh_count(), PINNED - 1);
    }

    #[test]
    fn mega_paths_never_coalesce_across_an_analytic_painter_boundary() {
        let mut first = object(
            1,
            GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::new(-1.0, 0.0))
                    .line_to(Vec2::new(-0.25, 0.0)),
            ),
        );
        first.style.fill = None;
        first.style.stroke = Some(Color::WHITE);
        first.style.stroke_width = 0.02;
        let middle = object(2, GeometryRef::circle(0.5));
        let mut last = object(
            3,
            GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::new(0.25, 0.0))
                    .line_to(Vec2::new(1.0, 0.0)),
            ),
        );
        last.style.fill = None;
        last.style.stroke = Some(Color::WHITE);
        last.style.stroke_width = 0.02;
        let frame = frame(vec![first, middle, last]);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);

        assert_eq!(prepared.render_batches.len(), 3);
        assert!(matches!(
            prepared.render_batches[0].primitive,
            RenderPrimitive::MegaPath { .. }
        ));
        assert_eq!(
            prepared.render_batches[1].primitive,
            RenderPrimitive::Circle
        );
        assert!(matches!(
            prepared.render_batches[2].primitive,
            RenderPrimitive::MegaPath { .. }
        ));
    }
}

#[cfg(test)]
mod structural_execution_delta_tests {
    use super::*;
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D};

    fn circle(id: u64) -> FrameObjectState {
        FrameObjectState {
            id: ObjectId::new(id),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.1)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
        }
    }

    #[test]
    fn removing_one_of_100k_objects_retires_one_packed_slot_without_rebuild() {
        let count = 100_000usize;
        let mut frame = FrameState {
            time: 0.0,
            objects: (0..count).map(|id| circle(id as u64)).collect(),
            presences: vec![true; count],
            reveals: vec![1.0; count],
            morphs: vec![0.0; count],
            render_geometries: vec![None; count],
            render_transforms: vec![None; count],
        };
        let mut preparer = FramePreparer::new();
        let initial = preparer.prepare(&frame);
        assert_eq!(initial.stats.full_rebuilds, 1);
        frame.presences[10] = false;
        let changes = FrameChanges::structural(Vec::new(), vec![10]);
        let prepared = preparer.prepare_incremental(&frame, &changes);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.structural_slots_retired, 1);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert_eq!(prepared.circles.len(), count);
        assert_eq!(prepared.circle_dirty_ranges.len(), 1);
        assert_eq!(prepared.circle_dirty_ranges[0], 10..11);
        assert_eq!(prepared.circles[10].style.opacity, 0.0);
    }

    #[test]
    fn appended_analytic_object_packs_only_the_new_slot() {
        let count = 10_000usize;
        let mut frame = FrameState {
            time: 0.0,
            objects: (0..count).map(|id| circle(id as u64)).collect(),
            presences: vec![true; count],
            reveals: vec![1.0; count],
            morphs: vec![0.0; count],
            render_geometries: vec![None; count],
            render_transforms: vec![None; count],
        };
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        frame.objects.push(circle(count as u64));
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);
        let changes = FrameChanges::structural(vec![count], Vec::new());
        let prepared = preparer.prepare_incremental(&frame, &changes);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.structural_slots_added, 1);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert_eq!(prepared.circle_dirty_ranges.len(), 1);
        assert_eq!(prepared.circle_dirty_ranges[0], count..count + 1);
    }
}
