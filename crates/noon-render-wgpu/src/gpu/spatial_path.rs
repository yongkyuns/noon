//! Local tessellation for the retained spatial planar-path lane.
//!
//! Geometry is tessellated only when its version/style specialization changes.
//! The spatial renderer owns the resulting buffers and updates only compact
//! per-object pose data for ordinary world motion.

mod cairo;

use super::create_buffer_with_data;
use super::retained_text::{
    append_transformed_path, resolved_text_vector_style, transform_path, variation_fingerprint,
    GlyphOutlineCache,
};
use bytemuck::{Pod, Zeroable};
use noon_core::{
    Color, FontResourceHandle, FontResourceLookup, GeometryRef, GeometryResource,
    GeometryResourceHandle, GeometryResourceLookup, SemanticSpatialCompositionDomain as Domain,
    SemanticSpatialMaterial, SemanticVec3, SemanticWorldTransform3D, StrokeWidthMode, Style,
    TextRenderItem, TextResource, TextResourceHandle, TextResourceLookup, Vec2,
};
use noon_runtime::{FrameChanges, FrameState};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct SpatialPathVertex {
    pub position: [f32; 2],
    /// 0 is fill, 1 is stroke, matching `noon_geometry::PathSurface`.
    pub surface: u32,
    /// Retained straight centerline tangent and cap/body offsets. Both are zero
    /// for ordinary local tessellation; screen strokes expand after projection.
    pub tangent: [f32; 2],
    pub extrusion: [f32; 2],
    pub stroke_metadata: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SourceKey {
    Resource(GeometryResourceHandle),
    TextGlyphRun(TextResourceHandle, u32, u8, FontResourceHandle, u32, u64),
    TextVector(TextResourceHandle, u32, GeometryResourceHandle),
    Inline(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct DrawId {
    row: usize,
    item: u32,
}

type FixedOrientationOrderKey = (u32, u32, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Specialization {
    pub stroke_width: u32,
    pub screen_stroke: bool,
    pub stroke_join: noon_core::StrokeJoin,
    pub stroke_cap: noon_core::StrokeCap,
    pub fill: bool,
    pub stroke: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PathKey {
    pub source: SourceKey,
    pub specialization: Specialization,
}

#[derive(Debug)]
pub(super) struct TessellatedSpatialPath {
    pub vertices: Vec<SpatialPathVertex>,
    pub indices: Vec<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct PathInstance {
    world: [f32; 16],
    fill: [f32; 4],
    stroke: [f32; 4],
    opacity: f32,
    fixed_orientation: u32,
    fixed_anchor: [f32; 3],
    screen_stroke_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PathDraw {
    key: PathKey,
    instance: usize,
    domain: Domain,
    painter_rank: u32,
}

#[derive(Debug)]
struct ResidentPath {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    users: usize,
}

#[derive(Debug, Default)]
pub(super) struct SpatialPathGpuState {
    draws: BTreeMap<DrawId, PathDraw>,
    /// Retained painter ordering for fixed-orientation paths. The map remains
    /// the draw authority; this set is only an ordered index into it.
    fixed_orientation_order: BTreeSet<FixedOrientationOrderKey>,
    /// Painter ranks are a derived lookup retained across ordinary local draws.
    /// Rebuild only when the publication reports an order change.
    painter_ranks: HashMap<usize, u32>,
    painter_ranks_initialized: bool,
    clip_scale_users: usize,
    paths: HashMap<PathKey, ResidentPath>,
    instances: Vec<PathInstance>,
    free_instances: Vec<usize>,
    gpu: Option<PathGpuState>,
    outlines: GlyphOutlineCache,
    cairo: Option<Box<cairo::State>>,
}

#[derive(Debug)]
struct PathGpuState {
    camera_layout: wgpu::BindGroupLayout,
    fixed_camera_layout: wgpu::BindGroupLayout,
    camera_group: wgpu::BindGroup,
    fixed_camera_group: wgpu::BindGroup,
    fixed_camera: wgpu::Buffer,
    fixed_camera_scale: [f32; 2],
    world_pipeline: wgpu::RenderPipeline,
    world_pipeline_msaa: wgpu::RenderPipeline,
    fixed_pipeline: wgpu::RenderPipeline,
    fixed_pipeline_msaa: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    capacity: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct FixedCameraUniform {
    clip_scale: [f32; 2],
    _padding: [f32; 2],
}

const PATH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Uint32, 11 => Float32x2, 12 => Float32x2, 14 => Float32x3
];
const PATH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
    2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
    6 => Float32x4, 7 => Float32x4, 8 => Float32, 9 => Uint32, 10 => Float32x3, 13 => Float32
];

impl PathGpuState {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_layout: &wgpu::BindGroupLayout,
        camera_group: &wgpu::BindGroup,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let layout = camera_layout.clone();
        let camera_group = camera_group.clone();
        let fixed_camera_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Noon spatial path fixed-orientation camera layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            FixedCameraUniform,
                        >() as u64),
                    },
                    count: None,
                }],
            });
        let fixed_camera = create_buffer_with_data(
            device,
            queue,
            Some("Noon spatial path 2D clip scale"),
            bytemuck::bytes_of(&FixedCameraUniform::zeroed()),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let fixed_camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon spatial path fixed-orientation camera"),
            layout: &fixed_camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: fixed_camera.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon spatial path pipeline layout"),
            bind_group_layouts: &[Some(&layout), Some(&fixed_camera_layout)],
            immediate_size: 0,
        });
        let source = format!(
            "{}\n{}",
            include_str!("spatial_path.wgsl"),
            include_str!("../cairo_color.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon retained spatial path shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let world_pipeline = spatial_path_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            1,
            true,
            &PATH_VERTEX_ATTRIBUTES,
            &PATH_INSTANCE_ATTRIBUTES,
            false,
        );
        let world_pipeline_msaa = spatial_path_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            sample_count,
            true,
            &PATH_VERTEX_ATTRIBUTES,
            &PATH_INSTANCE_ATTRIBUTES,
            false,
        );
        let fixed_pipeline = spatial_path_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            1,
            false,
            &PATH_VERTEX_ATTRIBUTES,
            &PATH_INSTANCE_ATTRIBUTES,
            false,
        );
        let fixed_pipeline_msaa = spatial_path_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            sample_count,
            false,
            &PATH_VERTEX_ATTRIBUTES,
            &PATH_INSTANCE_ATTRIBUTES,
            false,
        );
        let instances = create_buffer_with_data(
            device,
            queue,
            Some("Noon empty spatial path instances"),
            bytemuck::bytes_of(&PathInstance::zeroed()),
            wgpu::BufferUsages::VERTEX,
        );
        Self {
            camera_layout: layout,
            fixed_camera_layout,
            camera_group,
            fixed_camera_group,
            fixed_camera,
            fixed_camera_scale: [f32::NAN; 2],
            world_pipeline,
            world_pipeline_msaa,
            fixed_pipeline,
            fixed_pipeline_msaa,
            instances,
            capacity: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spatial_path_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    samples: u32,
    depth: bool,
    vertex_attributes: &'static [wgpu::VertexAttribute],
    instance_attributes: &'static [wgpu::VertexAttribute],
    cairo: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if depth {
            "Noon world spatial path pipeline"
        } else {
            "Noon fixed-orientation spatial path pipeline"
        }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if cairo { "vs_cairo" } else { "vs_main" }),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpatialPathVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: vertex_attributes,
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PathInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: instance_attributes,
                }),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if cairo { "fs_cairo" } else { "fs_main" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        // Both domains execute in the shared spatial render pass, which owns a
        // depth attachment. Fixed-orientation content uses painter order and
        // ignores that depth without declaring an incompatible pipeline.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(depth),
            depth_compare: Some(if depth {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

#[derive(Debug)]
pub(super) struct PathPlan {
    staged: Vec<(DrawId, Option<StagedPath>)>,
    new_paths: HashMap<PathKey, TessellatedSpatialPath>,
    new_cairo_geometry: HashMap<PathKey, noon_geometry::CairoPathGeometry>,
    needed: usize,
    capacity: usize,
    camera_clip_scale: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PathUploadStats {
    pub geometry_bytes: usize,
    pub instance_bytes: usize,
    pub camera_bytes: usize,
}

#[derive(Debug)]
struct StagedPath {
    key: PathKey,
    instance: PathInstance,
    domain: Domain,
    painter_rank: u32,
    cairo: Option<Box<cairo::Uniform>>,
}

#[derive(Debug)]
struct PathCandidate<'a> {
    id: DrawId,
    source: Option<SourceKey>,
    geometry: Cow<'a, GeometryRef>,
    style: Style,
}

impl SpatialPathGpuState {
    pub(super) fn is_active(&self) -> bool {
        !self.draws.is_empty()
    }

    pub(super) fn draw_count(&self) -> usize {
        self.draws.len()
    }

    pub(super) fn draw_indices(&self) -> impl Iterator<Item = usize> {
        self.draws
            .keys()
            .map(|draw_id| draw_id.row)
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    fn fixed_order_key(id: DrawId, draw: &PathDraw) -> Option<FixedOrientationOrderKey> {
        (draw.domain == Domain::FixedOrientation).then_some((draw.painter_rank, id.item, id.row))
    }

    fn update_fixed_order(
        &mut self,
        id: DrawId,
        previous: Option<&PathDraw>,
        next: Option<&PathDraw>,
    ) {
        let previous_key = previous.and_then(|draw| Self::fixed_order_key(id, draw));
        let next_key = next.and_then(|draw| Self::fixed_order_key(id, draw));
        if previous_key == next_key {
            return;
        }
        if let Some(key) = previous_key {
            self.fixed_orientation_order.remove(&key);
        }
        if let Some(key) = next_key {
            self.fixed_orientation_order.insert(key);
        }
    }

    fn publish_draw(&mut self, id: DrawId, draw: PathDraw) {
        let previous = self.draws.insert(id, draw);
        self.clip_scale_users -= usize::from(previous.is_some_and(Self::uses_clip_scale));
        self.clip_scale_users += usize::from(Self::uses_clip_scale(draw));
        self.update_fixed_order(id, previous.as_ref(), Some(&draw));
    }

    fn remove_draw(&mut self, id: DrawId) -> Option<PathDraw> {
        let previous = self.draws.remove(&id)?;
        self.clip_scale_users -= usize::from(Self::uses_clip_scale(previous));
        self.update_fixed_order(id, Some(&previous), None);
        Some(previous)
    }

    fn uses_clip_scale(draw: PathDraw) -> bool {
        draw.domain == Domain::FixedOrientation || draw.key.specialization.screen_stroke
    }

    fn row_draw_ids(&self, row: usize) -> impl Iterator<Item = DrawId> + '_ {
        let lower = std::ops::Bound::Included(DrawId { row, item: 0 });
        let upper = row
            .checked_add(1)
            .map_or(std::ops::Bound::Unbounded, |next_row| {
                std::ops::Bound::Excluded(DrawId {
                    row: next_row,
                    item: 0,
                })
            });
        self.draws
            .range((lower, upper))
            .map(|(&draw_id, _)| draw_id)
    }

    pub(super) fn painter_rank(&self, index: usize) -> Option<u32> {
        self.painter_ranks.get(&index).copied()
    }

    fn refresh_painter_ranks(&mut self, painter_order: &[u32]) {
        self.painter_ranks.clear();
        self.painter_ranks.extend(
            painter_order.iter().enumerate().filter_map(|(rank, &row)| {
                u32::try_from(rank).ok().map(|rank| (row as usize, rank))
            }),
        );
        self.painter_ranks_initialized = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan(
        &mut self,
        device: &wgpu::Device,
        frame: &FrameState,
        indices: &BTreeSet<usize>,
        changes: &FrameChanges,
        camera: super::Camera2D,
        painter_order: &[u32],
        resources: &dyn GeometryResourceLookup,
        texts: &dyn TextResourceLookup,
        fonts: &dyn FontResourceLookup,
    ) -> Result<PathPlan, SpatialPathError> {
        let mut staged = Vec::with_capacity(indices.len());
        let mut new_paths = HashMap::new();
        let mut new_cairo_geometry = HashMap::new();
        let camera_clip_scale = [2.0 / camera.world_size.x, 2.0 / camera.world_size.y];
        if !camera_clip_scale
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            return Err(SpatialPathError::InvalidStyle);
        }
        if !self.painter_ranks_initialized || changes.is_all() || changes.has_painter_order_change()
        {
            self.refresh_painter_ranks(painter_order);
        }
        for &index in indices {
            staged.extend(self.row_draw_ids(index).map(|draw_id| (draw_id, None)));
            let Some(object) = frame.objects.get(index).filter(|_| frame.is_present(index)) else {
                continue;
            };
            let Some(spatial) = object.spatial.as_deref().filter(|spatial| {
                !spatial.point_light
                    && spatial.camera_projection.is_none()
                    && spatial.draw_kind == noon_compile::CompiledSpatialDrawKind::Planar
                    && spatial.composition_domain != Domain::FixedFrame
            }) else {
                continue;
            };
            if !matches!(
                spatial.material,
                SemanticSpatialMaterial::Unlit | SemanticSpatialMaterial::CairoPath
            ) {
                return Err(SpatialPathError::UnsupportedMaterial);
            }
            if frame.reveal(index) != 1.0 || frame.morph(index) != 0.0 {
                return Err(SpatialPathError::UnsupportedAnimation);
            }
            let world = lower_world(spatial.world).ok_or(SpatialPathError::InvalidStyle)?;
            let fixed = spatial.composition_domain == Domain::FixedOrientation;
            if !fixed && spatial.fixed_orientation_center.is_some() {
                return Err(SpatialPathError::InvalidStyle);
            }
            let anchor = if fixed {
                spatial
                    .fixed_orientation_center
                    .ok_or(SpatialPathError::MissingAnchor)?
            } else {
                SemanticVec3::ZERO
            };
            let painter_rank = if fixed {
                *self
                    .painter_ranks
                    .get(&index)
                    .ok_or(SpatialPathError::MissingPainterRank)?
            } else {
                0
            };
            let object_alpha = object.style.opacity * object.appearance;
            let mut candidates = Vec::new();
            if let Some(geometry) = object.content.geometry() {
                candidates.push(PathCandidate {
                    id: DrawId {
                        row: index,
                        item: 0,
                    },
                    source: None,
                    geometry: Cow::Borrowed(geometry),
                    style: object.style,
                });
            } else if let Some(handle) = object.content.text() {
                let resource = texts
                    .get(handle)
                    .ok_or(SpatialPathError::MissingTextResource)?;
                candidates.extend(collect_text_paths(
                    &mut self.outlines,
                    handle,
                    resource,
                    fonts,
                    resources,
                    object.style,
                    index,
                    |source, style| {
                        let key = PathKey {
                            source: *source,
                            specialization: specialization(style),
                        };
                        self.paths.contains_key(&key) || new_paths.contains_key(&key)
                    },
                )?);
            } else {
                return Err(if object.content.image().is_some() {
                    SpatialPathError::UnsupportedImage
                } else {
                    SpatialPathError::MissingGeometry
                });
            }
            for candidate in candidates {
                let PathCandidate {
                    id: draw_id,
                    source: text_key,
                    geometry,
                    style,
                } = candidate;
                validate_style(style, object_alpha, spatial.composition_domain)?;
                let style = visible_path_style(style);
                let keyed = match text_key {
                    Some(source) => PathKey {
                        source,
                        specialization: specialization(style),
                    },
                    None => path_key(geometry.as_ref(), resources, style, index as u64)?,
                };
                let cairo = if spatial.material == SemanticSpatialMaterial::CairoPath {
                    let appearance = spatial
                        .cairo_path_appearance
                        .as_deref()
                        .ok_or(SpatialPathError::UnsupportedMaterial)?;
                    let geometry_metadata = if let Some(corners) = self
                        .cairo
                        .as_deref()
                        .and_then(|state| state.geometry.get(&keyed))
                        .or_else(|| new_cairo_geometry.get(&keyed))
                    {
                        *corners
                    } else {
                        let path = local_path(geometry.as_ref(), resources)?;
                        let corners = noon_geometry::cairo_path_geometry(&path)
                            .ok_or(SpatialPathError::UnsupportedGeometry)?;
                        new_cairo_geometry.insert(keyed, corners);
                        corners
                    };
                    Some(Box::new(cairo::Uniform::lower(
                        geometry_metadata,
                        appearance,
                    )?))
                } else {
                    None
                };
                if !self.paths.contains_key(&keyed) && !new_paths.contains_key(&keyed) {
                    let path = tessellate(
                        geometry.as_ref(),
                        resources,
                        style,
                        object_alpha,
                        spatial.composition_domain,
                    )?;
                    if path.indices.is_empty() {
                        continue;
                    }
                    new_paths.insert(keyed, path);
                }
                let color = |value: Option<Color>| {
                    value.map_or([0.0; 4], |c| [c.red, c.green, c.blue, c.alpha])
                };
                let instance = PathInstance {
                    world,
                    fill: color(style.fill),
                    stroke: color(style.stroke),
                    opacity: object_alpha,
                    fixed_orientation: u32::from(fixed),
                    fixed_anchor: [anchor.x as f32, anchor.y as f32, anchor.z as f32],
                    screen_stroke_width: if screen_stroke(style) {
                        style.stroke_width
                    } else {
                        0.0
                    },
                };
                if instance
                    .fill
                    .iter()
                    .chain(instance.stroke.iter())
                    .chain(instance.fixed_anchor.iter())
                    .chain([instance.opacity].iter())
                    .any(|value| !value.is_finite())
                {
                    return Err(SpatialPathError::InvalidStyle);
                }
                staged.push((
                    draw_id,
                    Some(StagedPath {
                        key: keyed,
                        instance,
                        domain: spatial.composition_domain,
                        painter_rank,
                        cairo,
                    }),
                ));
            }
        }
        for path in new_paths.values() {
            let vertex_bytes = path
                .vertices
                .len()
                .checked_mul(std::mem::size_of::<SpatialPathVertex>())
                .ok_or(SpatialPathError::BufferLimit)?;
            let index_bytes = path
                .indices
                .len()
                .checked_mul(std::mem::size_of::<u32>())
                .ok_or(SpatialPathError::BufferLimit)?;
            if vertex_bytes > device.limits().max_buffer_size as usize
                || index_bytes > device.limits().max_buffer_size as usize
                || u32::try_from(path.indices.len()).is_err()
            {
                return Err(SpatialPathError::BufferLimit);
            }
        }
        let mut staged_presence = HashMap::<DrawId, bool>::new();
        let mut staged_live = self.draws.len();
        let mut staged_free = self.free_instances.len();
        let mut needed = self.instances.len();
        for (draw_id, draw) in &staged {
            let previous = staged_presence
                .get(draw_id)
                .copied()
                .unwrap_or_else(|| self.draws.contains_key(draw_id));
            let next = draw.is_some();
            match (previous, next) {
                (false, true) => {
                    staged_live += 1;
                    if staged_free == 0 {
                        needed += 1;
                    } else {
                        staged_free -= 1;
                    }
                }
                (true, false) => {
                    staged_live -= 1;
                    staged_free += 1;
                }
                _ => {}
            }
            staged_presence.insert(*draw_id, next);
        }
        debug_assert_eq!(staged_live + staged_free, needed);
        let capacity = needed
            .max(1)
            .checked_next_power_of_two()
            .ok_or(SpatialPathError::BufferLimit)?;
        if capacity
            .checked_mul(std::mem::size_of::<PathInstance>())
            .ok_or(SpatialPathError::BufferLimit)?
            > device.limits().max_buffer_size as usize
        {
            return Err(SpatialPathError::BufferLimit);
        }
        Ok(PathPlan {
            staged,
            new_paths,
            new_cairo_geometry,
            needed,
            capacity,
            camera_clip_scale,
        })
    }

    pub(super) fn remaining_after(&self, plan: &PathPlan) -> usize {
        // Track only touched ids. Plans can contain repeated updates for one
        // row/item, so compare each staged value with the prior staged value
        // before adjusting the retained count.
        let mut staged_presence = HashMap::<DrawId, bool>::new();
        let mut remaining = self.draws.len();
        for (draw_id, draw) in &plan.staged {
            let previous = staged_presence
                .get(draw_id)
                .copied()
                .unwrap_or_else(|| self.draws.contains_key(draw_id));
            let next = draw.is_some();
            match (previous, next) {
                (false, true) => remaining += 1,
                (true, false) => remaining -= 1,
                _ => {}
            }
            staged_presence.insert(*draw_id, next);
        }
        remaining
    }

    pub(super) fn commit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_layout: &wgpu::BindGroupLayout,
        camera_group: &wgpu::BindGroup,
        format: wgpu::TextureFormat,
        sample_count: u32,
        plan: PathPlan,
    ) -> PathUploadStats {
        let mut stats = PathUploadStats::default();
        for (key, path) in plan.new_paths {
            if self.paths.contains_key(&key) {
                continue;
            }
            let vertex_bytes = bytemuck::cast_slice(&path.vertices);
            let index_bytes = bytemuck::cast_slice(&path.indices);
            self.paths.insert(
                key,
                ResidentPath {
                    vertices: create_buffer_with_data(
                        device,
                        queue,
                        Some("Noon retained spatial path vertices"),
                        vertex_bytes,
                        wgpu::BufferUsages::VERTEX,
                    ),
                    indices: create_buffer_with_data(
                        device,
                        queue,
                        Some("Noon retained spatial path indices"),
                        index_bytes,
                        wgpu::BufferUsages::INDEX,
                    ),
                    index_count: path.indices.len() as u32,
                    users: 0,
                },
            );
            stats.geometry_bytes += vertex_bytes.len() + index_bytes.len();
        }
        if self.gpu.is_none() {
            self.gpu = Some(PathGpuState::new(
                device,
                queue,
                camera_layout,
                camera_group,
                format,
                sample_count,
            ));
        }
        if !plan.new_cairo_geometry.is_empty()
            || plan
                .staged
                .iter()
                .any(|(_, draw)| draw.as_ref().is_some_and(|draw| draw.cairo.is_some()))
        {
            let state = self.cairo.get_or_insert_with(Default::default);
            state.geometry.extend(plan.new_cairo_geometry);
            if state.pipelines.is_none() {
                state.pipelines = Some(cairo::Pipelines::new(
                    device,
                    self.gpu.as_ref().expect("spatial path GPU state"),
                    format,
                    sample_count,
                ));
            }
        }
        let mut dirty = BTreeSet::new();
        let mut released = HashSet::new();
        for (draw_id, staged) in plan.staged {
            if let Some(state) = self.cairo.as_deref_mut() {
                if let Some(value) = staged.as_ref().and_then(|draw| draw.cairo.as_deref()) {
                    if let Some(draw) = state.draws.get_mut(&draw_id) {
                        if draw.value != *value {
                            queue.write_buffer(&draw.buffer, 0, bytemuck::bytes_of(value));
                            draw.value = *value;
                            stats.instance_bytes += std::mem::size_of::<cairo::Uniform>();
                        }
                    } else {
                        let draw = state
                            .pipelines
                            .as_ref()
                            .expect("Cairo path pipelines")
                            .retain(device, queue, *value);
                        state.draws.insert(draw_id, draw);
                        stats.instance_bytes += std::mem::size_of::<cairo::Uniform>();
                    }
                } else {
                    state.draws.remove(&draw_id);
                }
            }
            let previous = self.draws.get(&draw_id).copied();
            let same_path = previous
                .as_ref()
                .zip(staged.as_ref())
                .is_some_and(|(old, new)| old.key == new.key);
            if let Some(old) = previous.as_ref().filter(|_| !same_path) {
                self.paths.get_mut(&old.key).expect("resident path").users -= 1;
                released.insert(old.key);
            }
            if let Some(staged) = staged {
                let slot = previous.map(|old| old.instance).unwrap_or_else(|| {
                    self.free_instances.pop().unwrap_or_else(|| {
                        self.instances.push(PathInstance::zeroed());
                        self.instances.len() - 1
                    })
                });
                if self.instances[slot] != staged.instance {
                    self.instances[slot] = staged.instance;
                    dirty.insert(slot);
                }
                if !same_path {
                    self.paths.get_mut(&staged.key).expect("staged path").users += 1;
                }
                self.publish_draw(
                    draw_id,
                    PathDraw {
                        key: staged.key,
                        instance: slot,
                        domain: staged.domain,
                        painter_rank: staged.painter_rank,
                    },
                );
            } else if let Some(old) = previous {
                self.remove_draw(draw_id);
                self.free_instances.push(old.instance);
            }
        }
        for key in released {
            if self.paths.get(&key).is_some_and(|path| path.users == 0) {
                self.paths.remove(&key);
                if let Some(state) = self.cairo.as_deref_mut() {
                    state.geometry.remove(&key);
                }
            }
        }
        let gpu = self.gpu.as_mut().expect("spatial path GPU state");
        if plan.needed > gpu.capacity {
            gpu.instances = create_buffer_with_data(
                device,
                queue,
                Some("Noon retained spatial path instances"),
                bytemuck::cast_slice(&self.instances),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            gpu.capacity = plan.capacity;
            stats.instance_bytes += std::mem::size_of_val(self.instances.as_slice());
        } else {
            for slot in dirty {
                queue.write_buffer(
                    &gpu.instances,
                    (slot * std::mem::size_of::<PathInstance>()) as u64,
                    bytemuck::bytes_of(&self.instances[slot]),
                );
                stats.instance_bytes += std::mem::size_of::<PathInstance>();
            }
        }
        if self.clip_scale_users > 0 && gpu.fixed_camera_scale != plan.camera_clip_scale {
            queue.write_buffer(
                &gpu.fixed_camera,
                0,
                bytemuck::bytes_of(&FixedCameraUniform {
                    clip_scale: plan.camera_clip_scale,
                    _padding: [0.0; 2],
                }),
            );
            gpu.fixed_camera_scale = plan.camera_clip_scale;
            stats.camera_bytes = std::mem::size_of::<FixedCameraUniform>();
        }
        stats
    }

    pub(super) fn encode<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        sample_count: u32,
    ) -> usize {
        let Some(gpu) = self.gpu.as_ref() else {
            return 0;
        };
        if self.draws.is_empty() {
            return 0;
        }
        pass.set_bind_group(0, &gpu.camera_group, &[]);
        pass.set_bind_group(1, &gpu.fixed_camera_group, &[]);
        pass.set_vertex_buffer(1, gpu.instances.slice(..));
        let mut draw_calls = 0;
        for domain in [Domain::World, Domain::FixedOrientation] {
            pass.set_pipeline(match (domain, sample_count) {
                (Domain::World, 1) => &gpu.world_pipeline,
                (Domain::World, _) => &gpu.world_pipeline_msaa,
                (_, 1) => &gpu.fixed_pipeline,
                (_, _) => &gpu.fixed_pipeline_msaa,
            });
            match domain {
                Domain::World => {
                    let mut using_cairo = false;
                    for (id, draw) in self
                        .draws
                        .iter()
                        .filter(|(_, draw)| draw.domain == Domain::World)
                    {
                        let cairo_draw =
                            self.cairo.as_deref().and_then(|state| state.draws.get(id));
                        if using_cairo != cairo_draw.is_some() {
                            if cairo_draw.is_some() {
                                let pipelines = self
                                    .cairo
                                    .as_deref()
                                    .and_then(|state| state.pipelines.as_ref())
                                    .expect("Cairo path pipelines");
                                pass.set_pipeline(&pipelines.world[usize::from(sample_count != 1)]);
                            } else {
                                pass.set_pipeline(if sample_count == 1 {
                                    &gpu.world_pipeline
                                } else {
                                    &gpu.world_pipeline_msaa
                                });
                            }
                            using_cairo = cairo_draw.is_some();
                        }
                        if let Some(appearance) = cairo_draw {
                            pass.set_bind_group(2, &appearance.binding, &[]);
                        }
                        draw_calls += encode_path_draw(pass, &self.paths, draw);
                    }
                }
                Domain::FixedOrientation => {
                    for &(_, item, row) in &self.fixed_orientation_order {
                        let id = DrawId { row, item };
                        if let Some(draw) = self.draws.get(&id) {
                            draw_calls += encode_path_draw(pass, &self.paths, draw);
                        }
                    }
                }
                Domain::FixedFrame => {}
            }
        }
        draw_calls
    }
}

fn local_path<'a>(
    geometry: &'a GeometryRef,
    resources: &'a dyn GeometryResourceLookup,
) -> Result<Cow<'a, noon_core::VectorPath>, SpatialPathError> {
    match geometry {
        GeometryRef::External(id) => {
            let handle = resources
                .current_handle(*id)
                .ok_or(SpatialPathError::MissingPathResource)?;
            let Some(GeometryResource::VectorPath(path)) = resources.get(handle) else {
                return Err(SpatialPathError::UnsupportedGeometry);
            };
            Ok(Cow::Borrowed(path.as_ref()))
        }
        GeometryRef::VectorPath(path) => Ok(Cow::Borrowed(path)),
        _ => noon_geometry::canonical_outline_path(geometry)
            .map(Cow::Owned)
            .ok_or(SpatialPathError::MissingGeometry),
    }
}

fn encode_path_draw<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    paths: &'a HashMap<PathKey, ResidentPath>,
    draw: &'a PathDraw,
) -> usize {
    let path = &paths[&draw.key];
    pass.set_vertex_buffer(0, path.vertices.slice(..));
    pass.set_index_buffer(path.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(
        0..path.index_count,
        0,
        draw.instance as u32..draw.instance as u32 + 1,
    );
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialPathError {
    MissingGeometry,
    MissingAnchor,
    MissingPathResource,
    UnsupportedGeometry,
    UnsupportedImage,
    UnsupportedMorph,
    UnsupportedAnimation,
    UnsupportedMaterial,
    MissingPainterRank,
    MissingTextResource,
    InvalidText,
    UnrepresentableVertex,
    NonOpaqueStyle,
    InvalidStyle,
    Tessellation,
    BufferLimit,
}

fn path_key(
    geometry: &GeometryRef,
    resources: &dyn GeometryResourceLookup,
    style: Style,
    identity: u64,
) -> Result<PathKey, SpatialPathError> {
    let style = visible_path_style(style);
    if !style.stroke_width.is_finite() || style.stroke_width < 0.0 {
        return Err(SpatialPathError::InvalidStyle);
    }
    let source = match geometry {
        GeometryRef::External(id) => {
            let handle = resources
                .current_handle(*id)
                .ok_or(SpatialPathError::MissingPathResource)?;
            if !matches!(resources.get(handle), Some(GeometryResource::VectorPath(_))) {
                return Err(SpatialPathError::UnsupportedGeometry);
            }
            SourceKey::Resource(handle)
        }
        GeometryRef::VectorPath(path) => SourceKey::Inline(inline_path_fingerprint(identity, path)),
        GeometryRef::Circle { radius } => {
            SourceKey::Inline(inline_fingerprint(identity, &[radius.to_bits()]))
        }
        GeometryRef::Rectangle { size } => SourceKey::Inline(inline_fingerprint(
            identity,
            &[size.x.to_bits(), size.y.to_bits()],
        )),
        GeometryRef::Line { start, end } => SourceKey::Inline(inline_fingerprint(
            identity,
            &[
                start.x.to_bits(),
                start.y.to_bits(),
                end.x.to_bits(),
                end.y.to_bits(),
            ],
        )),
    };
    Ok(PathKey {
        source,
        specialization: specialization(style),
    })
}

fn specialization(style: Style) -> Specialization {
    Specialization {
        screen_stroke: screen_stroke(style),
        stroke_width: if style.stroke.is_some() {
            if screen_stroke(style) {
                1.0_f32.to_bits()
            } else {
                style.stroke_width.to_bits()
            }
        } else {
            0.0_f32.to_bits()
        },
        stroke_join: style.stroke_join,
        stroke_cap: style.stroke_cap,
        fill: style.fill.is_some(),
        stroke: style.stroke.is_some() && style.stroke_width > 0.0,
    }
}

fn screen_stroke(style: Style) -> bool {
    style.stroke.is_some()
        && style.stroke_width > 0.0
        && style.stroke_width_mode == StrokeWidthMode::ScreenSpace
}

#[allow(clippy::too_many_arguments)]
fn collect_text_paths<'a>(
    outlines: &mut GlyphOutlineCache,
    handle: TextResourceHandle,
    resource: &'a TextResource,
    fonts: &dyn FontResourceLookup,
    geometries: &dyn GeometryResourceLookup,
    object_style: Style,
    row: usize,
    mut path_is_resident: impl FnMut(&SourceKey, Style) -> bool,
) -> Result<Vec<PathCandidate<'a>>, SpatialPathError> {
    let mut output = Vec::new();
    for (item_index, item) in resource.render_items.iter().copied().enumerate() {
        let item_index = u32::try_from(item_index).map_err(|_| SpatialPathError::BufferLimit)?;
        match item {
            TextRenderItem::GlyphRun(run_index) => {
                let run = resource
                    .runs
                    .get(run_index as usize)
                    .ok_or(SpatialPathError::InvalidText)?;
                let font_handle = fonts
                    .handle_for_face(&run.font)
                    .ok_or(SpatialPathError::InvalidText)?;
                let item_base = item_index
                    .checked_mul(2)
                    .ok_or(SpatialPathError::BufferLimit)?;
                let base_style = Style {
                    fill: Some(run.fill.or(object_style.fill).unwrap_or(Color::WHITE)),
                    stroke: None,
                    stroke_width: 0.0,
                    stroke_width_mode: StrokeWidthMode::ScaleWithObject,
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    opacity: 1.0,
                };
                let fill_source = SourceKey::TextGlyphRun(
                    handle,
                    run_index,
                    0,
                    font_handle,
                    run.font_size.to_bits(),
                    variation_fingerprint(run),
                );
                let needs_fill =
                    base_style.fill.is_some() && !path_is_resident(&fill_source, base_style);
                let stroke_style = run.stroke.as_ref().map(|stroke| Style {
                    fill: Some(stroke.paint.or(object_style.fill).unwrap_or(Color::WHITE)),
                    ..base_style
                });
                let stroke_source = SourceKey::TextGlyphRun(
                    handle,
                    run_index,
                    1,
                    font_handle,
                    run.font_size.to_bits(),
                    variation_fingerprint(run),
                );
                let needs_stroke =
                    stroke_style.is_some_and(|style| !path_is_resident(&stroke_source, style));
                let (mut fill_path, mut stroke_path) =
                    (noon_core::VectorPath::new(), noon_core::VectorPath::new());
                if needs_fill || needs_stroke {
                    for glyph in run.glyphs.iter() {
                        let (outline_key, outline) = outlines
                            .outline(fonts, run, glyph.glyph_id)
                            .map_err(|_| SpatialPathError::InvalidText)?;
                        if needs_fill {
                            fill_path = append_transformed_path(
                                fill_path,
                                outline.as_ref(),
                                run.transform,
                                glyph.origin,
                            );
                        }
                        if needs_stroke {
                            if let Some(stroke) = run.stroke.as_ref() {
                                let expanded =
                                    outlines.stroked_outline(outline_key, outline.as_ref(), stroke);
                                stroke_path = append_transformed_path(
                                    stroke_path,
                                    expanded.as_ref(),
                                    run.transform,
                                    glyph.origin,
                                );
                            }
                        }
                    }
                }
                if base_style.fill.is_some() {
                    output.push(PathCandidate {
                        id: DrawId {
                            row,
                            item: item_base,
                        },
                        source: Some(fill_source),
                        geometry: Cow::Owned(GeometryRef::VectorPath(if needs_fill {
                            fill_path
                        } else {
                            noon_core::VectorPath::new()
                        })),
                        style: base_style,
                    });
                }
                if let Some(stroke_style) = stroke_style {
                    output.push(PathCandidate {
                        id: DrawId {
                            row,
                            item: item_base + 1,
                        },
                        source: Some(stroke_source),
                        geometry: Cow::Owned(GeometryRef::VectorPath(if needs_stroke {
                            stroke_path
                        } else {
                            noon_core::VectorPath::new()
                        })),
                        style: stroke_style,
                    });
                }
            }
            TextRenderItem::Vector(vector_index) => {
                let vector = resource
                    .vector_items
                    .get(vector_index as usize)
                    .ok_or(SpatialPathError::InvalidText)?;
                let Some(GeometryResource::VectorPath(path)) = geometries.get(vector.geometry)
                else {
                    return Err(SpatialPathError::InvalidText);
                };
                let item_base = item_index
                    .checked_mul(2)
                    .ok_or(SpatialPathError::BufferLimit)?;
                let style = resolved_text_vector_style(object_style, vector);
                let source = SourceKey::TextVector(handle, vector_index, vector.geometry);
                let path = if path_is_resident(&source, style) {
                    noon_core::VectorPath::new()
                } else {
                    transform_path(path, vector.transform, Vec2::ZERO)
                };
                output.push(PathCandidate {
                    id: DrawId {
                        row,
                        item: item_base,
                    },
                    source: Some(source),
                    geometry: Cow::Owned(GeometryRef::VectorPath(path)),
                    style,
                });
            }
        }
    }
    Ok(output)
}

fn inline_fingerprint(identity: u64, values: &[u32]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hasher);
    values.hash(&mut hasher);
    hasher.finish()
}

fn inline_path_fingerprint(identity: u64, path: &noon_core::VectorPath) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hasher);
    path.commands().len().hash(&mut hasher);
    for command in path.commands() {
        match command {
            noon_core::PathCommand::MoveTo { to } => {
                0_u8.hash(&mut hasher);
                to.x.to_bits().hash(&mut hasher);
                to.y.to_bits().hash(&mut hasher);
            }
            noon_core::PathCommand::LineTo { to } => {
                1_u8.hash(&mut hasher);
                to.x.to_bits().hash(&mut hasher);
                to.y.to_bits().hash(&mut hasher);
            }
            noon_core::PathCommand::QuadraticTo { control, to } => {
                2_u8.hash(&mut hasher);
                control.x.to_bits().hash(&mut hasher);
                control.y.to_bits().hash(&mut hasher);
                to.x.to_bits().hash(&mut hasher);
                to.y.to_bits().hash(&mut hasher);
            }
            noon_core::PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                3_u8.hash(&mut hasher);
                control1.x.to_bits().hash(&mut hasher);
                control1.y.to_bits().hash(&mut hasher);
                control2.x.to_bits().hash(&mut hasher);
                control2.y.to_bits().hash(&mut hasher);
                to.x.to_bits().hash(&mut hasher);
                to.y.to_bits().hash(&mut hasher);
            }
            noon_core::PathCommand::Close => 4_u8.hash(&mut hasher),
        }
    }
    hasher.finish()
}

fn lower_world(world: SemanticWorldTransform3D) -> Option<[f32; 16]> {
    let matrix = world.world_matrix()?.map(|value| value as f32);
    matrix
        .iter()
        .all(|value| value.is_finite())
        .then_some(matrix)
}

fn validate_style(style: Style, object_alpha: f32, domain: Domain) -> Result<(), SpatialPathError> {
    if !object_alpha.is_finite()
        || !(0.0..=1.0).contains(&object_alpha)
        || !style.opacity.is_finite()
        || !(0.0..=1.0).contains(&style.opacity)
        || !style.stroke_width.is_finite()
        || style.stroke_width < 0.0
    {
        return Err(SpatialPathError::InvalidStyle);
    }
    if [style.fill, style.stroke]
        .into_iter()
        .flatten()
        .any(|color| {
            [color.red, color.green, color.blue, color.alpha]
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
                || (domain == Domain::World
                    && color.alpha != 0.0
                    && color.alpha * object_alpha != 1.0)
        })
    {
        return Err(SpatialPathError::NonOpaqueStyle);
    }
    Ok(())
}

// Zero-alpha paint has no surface and must not populate the depth buffer.
// Partially transparent World paths retain their explicit rejection.
fn visible_path_style(mut style: Style) -> Style {
    style.fill = style.fill.filter(|color| color.alpha != 0.0);
    style.stroke = style.stroke.filter(|color| color.alpha != 0.0);
    style
}

/// Resolve one planar vector outline and tessellate it in its own local frame.
/// `identity` is used only for inline lightweight geometry, which has no arena
/// generation/version of its own.
pub(super) fn tessellate(
    geometry: &GeometryRef,
    resources: &dyn GeometryResourceLookup,
    style: Style,
    object_alpha: f32,
    domain: Domain,
) -> Result<TessellatedSpatialPath, SpatialPathError> {
    validate_style(style, object_alpha * style.opacity, domain)?;
    let style = visible_path_style(style);
    if screen_stroke(style)
        && matches!(geometry, GeometryRef::Circle { radius } if !radius.is_finite() || *radius <= 0.0)
    {
        return Err(SpatialPathError::UnrepresentableVertex);
    }
    let path = local_path(geometry, resources)?;
    if path.morph_target().is_some() {
        return Err(SpatialPathError::UnsupportedMorph);
    }
    let fill = style.fill.is_some();
    let stroke = style.stroke.is_some() && style.stroke_width > 0.0;
    let mesh = noon_geometry::tessellate_styled_with_fill(
        &path,
        if screen_stroke(style) && matches!(geometry, GeometryRef::Line { .. }) {
            1.0
        } else if screen_stroke(style) {
            0.0
        } else if stroke {
            style.stroke_width
        } else {
            0.0
        },
        style.stroke_join,
        style.stroke_cap,
        fill,
    )
    .map_err(|_| SpatialPathError::Tessellation)?;
    let path_stroke_frames = if screen_stroke(style) {
        match geometry {
            GeometryRef::Line { .. } => None,
            GeometryRef::Circle { .. } => Some(
                noon_geometry::tessellate_screen_stroke(&path, style.stroke_join, style.stroke_cap)
                    .map_err(|_| SpatialPathError::Tessellation)?,
            ),
            _ => Some(
                noon_geometry::tessellate_projected_screen_stroke(
                    &path,
                    style.stroke_join,
                    style.stroke_cap,
                )
                .map_err(|_| SpatialPathError::Tessellation)?,
            ),
        }
    } else {
        None
    };
    let line = if screen_stroke(style) && matches!(geometry, GeometryRef::Line { .. }) {
        let GeometryRef::Line { start, end } = geometry else {
            unreachable!("screen stroke was checked above")
        };
        let delta = *end - *start;
        let length = delta.length();
        if !length.is_finite() || length <= 0.0 {
            return Err(SpatialPathError::UnrepresentableVertex);
        }
        Some((*start, delta / length, length))
    } else {
        None
    };
    let mut vertices: Vec<SpatialPathVertex> = mesh
        .vertices
        .into_iter()
        .map(|vertex| {
            let mut output = SpatialPathVertex {
                position: [vertex.position.x, vertex.position.y],
                surface: crate::pack_path_surface(vertex.surface, 1.0),
                tangent: [0.0; 2],
                extrusion: [0.0; 2],
                stroke_metadata: [0.0; 3],
            };
            if let Some((start, tangent, length)) =
                line.filter(|_| vertex.surface == noon_geometry::PathSurface::Stroke)
            {
                let perpendicular = Vec2::new(-tangent.y, tangent.x);
                let delta = vertex.position - start;
                let along = (delta.x * tangent.x + delta.y * tangent.y).clamp(0.0, length);
                let center = start + tangent * along;
                let offset = vertex.position - center;
                output.position = [center.x, center.y];
                output.tangent = [tangent.x, tangent.y];
                output.extrusion = [
                    offset.x * tangent.x + offset.y * tangent.y,
                    offset.x * perpendicular.x + offset.y * perpendicular.y,
                ];
            }
            output
        })
        .collect();
    let mut indices = mesh.indices;
    if let Some(frames) = path_stroke_frames {
        let offset = u32::try_from(vertices.len()).map_err(|_| SpatialPathError::BufferLimit)?;
        vertices.extend(frames.vertices.into_iter().map(|vertex| SpatialPathVertex {
            position: [vertex.position.x, vertex.position.y],
            surface: crate::pack_path_surface(noon_geometry::PathSurface::Stroke, 1.0),
            tangent: [vertex.tangent.x, vertex.tangent.y],
            extrusion: [vertex.extrusion.x, vertex.extrusion.y],
            stroke_metadata: vertex.metadata,
        }));
        indices.extend(
            frames
                .indices
                .into_iter()
                .map(|index| {
                    index
                        .checked_add(offset)
                        .ok_or(SpatialPathError::BufferLimit)
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if vertices.iter().any(|vertex| {
        vertex
            .position
            .iter()
            .chain(vertex.tangent.iter())
            .chain(vertex.extrusion.iter())
            .chain(vertex.stroke_metadata.iter())
            .any(|value| !value.is_finite())
    }) {
        return Err(SpatialPathError::UnrepresentableVertex);
    }
    Ok(TessellatedSpatialPath { vertices, indices })
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        GeometryResourceArena, Rect, StrokeWidthMode, TextResourceArena, TextSourceKind,
        TextVectorStyle,
    };

    #[test]
    fn fixed_camera_uniform_matches_webgl_uniform_buffer_layout() {
        assert_eq!(std::mem::size_of::<FixedCameraUniform>(), 16);
        assert_eq!(
            bytemuck::bytes_of(&FixedCameraUniform {
                clip_scale: [1.0, 2.0],
                _padding: [0.0; 2],
            })
            .len(),
            16
        );
    }

    #[test]
    fn tessellation_stays_local_and_rejects_unsupported_styles() {
        let resources = GeometryResourceArena::default();
        let geometry = GeometryRef::rectangle(2.0, 1.0);
        let style = Style::default();
        let mesh = tessellate(&geometry, &resources, style, 1.0, Domain::World).unwrap();
        assert!(matches!(
            path_key(&geometry, &resources, style, 7).unwrap().source,
            SourceKey::Inline(_)
        ));
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert!(mesh
            .vertices
            .iter()
            .all(|vertex| { vertex.position[0].abs() <= 1.0 && vertex.position[1].abs() <= 0.5 }));

        let screen_stroke = Style {
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke: Some(noon_core::Color::BLACK),
            ..style
        };
        let screen_mesh =
            tessellate(&geometry, &resources, screen_stroke, 1.0, Domain::World).unwrap();
        assert!(screen_mesh.indices.len() > mesh.indices.len());
        assert!(screen_mesh.vertices.iter().any(|vertex| {
            vertex.surface == crate::pack_path_surface(noon_geometry::PathSurface::Stroke, 1.0)
                && vertex.tangent != [0.0; 2]
        }));
        // A filled Dot has no stroke to expand. Its unused constructor width
        // mode must not reject an otherwise supported World path.
        let filled = Style {
            stroke_width: 0.0,
            ..screen_stroke
        };
        let filled_mesh = tessellate(&geometry, &resources, filled, 1.0, Domain::World).unwrap();
        assert_eq!(filled_mesh.indices, mesh.indices);
        assert!(filled_mesh
            .vertices
            .iter()
            .zip(&mesh.vertices)
            .all(|(left, right)| left.position == right.position && left.surface == right.surface));
        assert!(path_key(&geometry, &resources, filled, 7).is_ok());
        assert_eq!(
            tessellate(&geometry, &resources, style, 0.5, Domain::World).unwrap_err(),
            SpatialPathError::NonOpaqueStyle
        );
    }

    #[test]
    fn straight_screen_strokes_retain_centerline_caps_and_distinct_resource_keys() {
        let resources = GeometryResourceArena::default();
        for cap in [
            noon_core::StrokeCap::Butt,
            noon_core::StrokeCap::Round,
            noon_core::StrokeCap::Square,
        ] {
            let style = Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.2,
                stroke_width_mode: StrokeWidthMode::ScreenSpace,
                stroke_cap: cap,
                ..Style::default()
            };
            for (start, end) in [
                (Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
                (Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0)),
                (Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0)),
            ] {
                let geometry = GeometryRef::line(start, end);
                let mesh = tessellate(&geometry, &resources, style, 1.0, Domain::World).unwrap();
                assert!(!mesh.indices.is_empty());
                let tangent = (end - start).normalized().unwrap();
                let normal = Vec2::new(-tangent.y, tangent.x);
                for vertex in &mesh.vertices {
                    assert_eq!(vertex.tangent, [tangent.x, tangent.y]);
                    let center = Vec2::new(vertex.position[0], vertex.position[1]);
                    let offset = tangent * vertex.extrusion[0] + normal * vertex.extrusion[1];
                    assert!(vertex.extrusion[1].abs() <= 0.50001);
                    assert!(vertex.extrusion[0].abs() <= 0.50001);
                    if cap == noon_core::StrokeCap::Butt {
                        assert!(vertex.extrusion[0].abs() < 1e-6);
                    }
                    assert!((center + offset).x.is_finite());
                }
                let screen_key = path_key(&geometry, &resources, style, 1).unwrap();
                let world_key = path_key(
                    &geometry,
                    &resources,
                    Style {
                        stroke_width_mode: StrokeWidthMode::ScaleWithObject,
                        ..style
                    },
                    1,
                )
                .unwrap();
                assert_ne!(
                    screen_key, world_key,
                    "different expansion policies cannot reuse the same vertices"
                );
                assert_eq!(
                    screen_key,
                    path_key(
                        &geometry,
                        &resources,
                        Style {
                            stroke_width: 0.6,
                            ..style
                        },
                        1
                    )
                    .unwrap(),
                    "screen width is compact instance state, not topology"
                );
            }
        }
    }

    #[test]
    fn circle_screen_strokes_keep_fill_roles_and_projective_extrusion_frames() {
        let resources = GeometryResourceArena::default();
        let geometry = GeometryRef::circle(1.25);
        let style = Style {
            fill: Some(Color::WHITE),
            stroke: Some(Color::BLACK),
            stroke_width: 0.08,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let mesh = tessellate(&geometry, &resources, style, 1.0, Domain::World).unwrap();
        assert!(!mesh.indices.is_empty());
        let fill_surface = crate::pack_path_surface(noon_geometry::PathSurface::Fill, 1.0);
        let stroke_surface = crate::pack_path_surface(noon_geometry::PathSurface::Stroke, 1.0);
        assert!(mesh
            .vertices
            .iter()
            .any(|vertex| vertex.surface == fill_surface));
        assert!(mesh
            .vertices
            .iter()
            .any(|vertex| vertex.surface == stroke_surface));
        let stroke_vertices: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.surface == stroke_surface)
            .collect();
        assert!(
            stroke_vertices.len() >= 4,
            "screen-space Circle stroke must retain its extrusion ring"
        );
        for vertex in stroke_vertices {
            let tangent = Vec2::new(vertex.tangent[0], vertex.tangent[1]);
            assert!((tangent.length() - 1.0).abs() < 1e-4);
            assert!(vertex.position.iter().all(|value| value.is_finite()));
            assert!(vertex.extrusion.iter().all(|value| value.is_finite()));
            assert!(vertex.extrusion[1].abs() <= 0.5001);
        }
        assert_eq!(
            path_key(&geometry, &resources, style, 9).unwrap(),
            path_key(
                &geometry,
                &resources,
                Style {
                    stroke_width: 0.3,
                    ..style
                },
                9
            )
            .unwrap(),
            "screen width is instance state, not tessellation topology"
        );
        assert_eq!(
            tessellate(
                &GeometryRef::circle(0.0),
                &resources,
                style,
                1.0,
                Domain::World
            )
            .unwrap_err(),
            SpatialPathError::UnrepresentableVertex
        );
    }

    #[test]
    fn projected_screen_paths_retain_curve_and_corner_join_metadata() {
        let mut resources = GeometryResourceArena::default();
        let curved_open = noon_core::VectorPath::new()
            .move_to(Vec2::new(-1.0, -0.5))
            .cubic_to(
                Vec2::new(-0.4, 1.0),
                Vec2::new(0.3, -1.0),
                Vec2::new(1.0, -0.5),
            );
        let handle = resources.insert_path(curved_open.clone());
        let style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.2,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke_join: noon_core::StrokeJoin::Bevel,
            stroke_cap: noon_core::StrokeCap::Square,
            ..Style::default()
        };
        let external = GeometryRef::External(handle.id);
        let inline = GeometryRef::VectorPath(curved_open);
        for geometry in [&inline, &external] {
            let mesh = tessellate(geometry, &resources, style, 1.0, Domain::World).unwrap();
            let stroke = crate::pack_path_surface(noon_geometry::PathSurface::Stroke, 1.0);
            let vertices: Vec<_> = mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.surface == stroke)
                .collect();
            assert!(!vertices.is_empty());
            assert!(vertices.iter().all(|vertex| {
                let tangent = Vec2::new(vertex.tangent[0], vertex.tangent[1]);
                (tangent.length() - 1.0).abs() < 1e-4
                    && vertex.extrusion.iter().all(|value| value.is_finite())
            }));
            assert!(
                vertices
                    .windows(2)
                    .any(|pair| pair[0].tangent != pair[1].tangent),
                "curve/join retains changing local tangent frames"
            );
            assert!(path_key(geometry, &resources, style, 12).is_ok());
        }

        let closed = GeometryRef::rectangle(2.0, 1.0);
        let closed_mesh = tessellate(&closed, &resources, style, 1.0, Domain::World).unwrap();
        assert!(closed_mesh
            .vertices
            .iter()
            .any(|vertex| vertex.stroke_metadata[0] == 3.0));
        let corner = GeometryRef::VectorPath(
            noon_core::VectorPath::new()
                .move_to(Vec2::new(-1.0, 0.0))
                .line_to(Vec2::ZERO)
                .line_to(Vec2::new(0.0, 1.0)),
        );
        let corner_mesh = tessellate(&corner, &resources, style, 1.0, Domain::World).unwrap();
        assert!(corner_mesh
            .vertices
            .iter()
            .any(|vertex| vertex.stroke_metadata[0] == 3.0));
        let rounded = tessellate(
            &inline,
            &resources,
            Style {
                stroke_join: noon_core::StrokeJoin::Round,
                stroke_cap: noon_core::StrokeCap::Round,
                ..style
            },
            1.0,
            Domain::World,
        )
        .unwrap();
        assert_ne!(
            rounded.indices.len(),
            tessellate(&inline, &resources, style, 1.0, Domain::World,)
                .unwrap()
                .indices
                .len(),
            "join and cap choices specialize retained topology"
        );
    }

    #[test]
    fn inline_path_key_tracks_exact_command_topology_and_row_identity() {
        let resources = GeometryResourceArena::default();
        let path = |end| {
            GeometryRef::VectorPath(
                noon_core::VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(end)
                    .close(),
            )
        };
        let style = Style::default();
        let first = path(Vec2::new(1.0, 0.0));
        let edited = path(Vec2::new(1.0, 0.25));
        let first_key = path_key(&first, &resources, style, 7).unwrap();
        let edited_key = path_key(&edited, &resources, style, 7).unwrap();
        let other_row_key = path_key(&first, &resources, style, 8).unwrap();

        assert_ne!(
            first_key, edited_key,
            "command edits get a new retained key"
        );
        assert_ne!(first_key, other_row_key, "inline paths are row-scoped");
        assert_eq!(
            tessellate(&first, &resources, style, 1.0, Domain::World)
                .unwrap()
                .indices,
            tessellate(&edited, &resources, style, 1.0, Domain::World)
                .unwrap()
                .indices,
            "same topology remains tessellatable after coordinate edits"
        );
    }

    #[test]
    fn sparse_row_lookup_and_staged_count_touch_only_affected_draws() {
        let mut state = SpatialPathGpuState::default();
        let key = PathKey {
            source: SourceKey::Inline(1),
            specialization: specialization(Style::default()),
        };
        let draw = |row| PathDraw {
            key,
            instance: row,
            domain: Domain::World,
            painter_rank: 0,
        };
        state.publish_draw(DrawId { row: 2, item: 0 }, draw(2));
        state.publish_draw(DrawId { row: 2, item: 3 }, draw(2));
        state.publish_draw(DrawId { row: 9, item: 0 }, draw(9));
        assert_eq!(
            state.row_draw_ids(2).collect::<Vec<_>>(),
            vec![DrawId { row: 2, item: 0 }, DrawId { row: 2, item: 3 }]
        );

        let staged = |id, draw: Option<()>| {
            (
                id,
                draw.map(|_| StagedPath {
                    key,
                    instance: PathInstance::zeroed(),
                    domain: Domain::World,
                    painter_rank: 0,
                    cairo: None,
                }),
            )
        };
        let plan = PathPlan {
            staged: vec![
                staged(DrawId { row: 2, item: 0 }, None),
                staged(DrawId { row: 2, item: 0 }, Some(())),
                staged(DrawId { row: 2, item: 3 }, None),
                staged(DrawId { row: 12, item: 0 }, Some(())),
                staged(DrawId { row: 12, item: 0 }, None),
            ],
            new_paths: HashMap::new(),
            new_cairo_geometry: HashMap::new(),
            needed: 0,
            capacity: 1,
            camera_clip_scale: [1.0, 1.0],
        };
        // Row 9 remains, row 2/item 0 remains after its replacement, and the
        // transient row 12 draw is absent: two final draws total.
        assert_eq!(state.remaining_after(&plan), 2);
    }

    #[test]
    fn resource_specialization_keeps_exact_arena_version() {
        let mut resources = GeometryResourceArena::default();
        let handle = resources.insert(GeometryResource::VectorPath(std::sync::Arc::new(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::new(-1.0, 0.0))
                .line_to(noon_core::Vec2::new(1.0, 0.0)),
        )));
        let id = handle.id;
        assert_eq!(
            path_key(&GeometryRef::External(id), &resources, Style::default(), 0)
                .unwrap()
                .source,
            SourceKey::Resource(handle)
        );
    }

    #[test]
    fn changed_inline_geometry_gets_a_new_residency_key() {
        let resources = GeometryResourceArena::default();
        let first = path_key(
            &GeometryRef::rectangle(2.0, 1.0),
            &resources,
            Style::default(),
            4,
        )
        .unwrap();
        let changed = path_key(
            &GeometryRef::rectangle(3.0, 1.0),
            &resources,
            Style::default(),
            4,
        )
        .unwrap();
        assert_ne!(first, changed);
    }

    #[test]
    fn fixed_orientation_paths_keep_premultiplied_transparency_but_world_paths_reject_it() {
        let resources = GeometryResourceArena::default();
        let geometry = GeometryRef::square(1.0);
        let style = Style {
            fill: Some(noon_core::Color::rgba(0.25, 0.5, 0.75, 0.5)),
            ..Style::default()
        };
        assert_eq!(
            tessellate(&geometry, &resources, style, 1.0, Domain::World).unwrap_err(),
            SpatialPathError::NonOpaqueStyle
        );
        assert!(tessellate(&geometry, &resources, style, 1.0, Domain::FixedOrientation).is_ok());
    }

    #[test]
    fn invisible_world_fill_matches_an_absent_fill_and_never_writes_depth() {
        let resources = GeometryResourceArena::default();
        let geometry = GeometryRef::circle(1.0);
        let style = Style {
            fill: Some(noon_core::Color::rgba(1.0, 1.0, 1.0, 0.0)),
            stroke: Some(noon_core::Color::WHITE),
            stroke_width: 0.04,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            ..Style::default()
        };
        let absent_fill = Style {
            fill: None,
            ..style
        };
        assert_eq!(
            path_key(&geometry, &resources, style, 0).unwrap(),
            path_key(&geometry, &resources, absent_fill, 0).unwrap()
        );
        let actual = tessellate(&geometry, &resources, style, 1.0, Domain::World).unwrap();
        let expected = tessellate(&geometry, &resources, absent_fill, 1.0, Domain::World).unwrap();
        assert_eq!(actual.indices, expected.indices);
        assert_eq!(actual.vertices.len(), expected.vertices.len());
        assert!(actual
            .vertices
            .iter()
            .zip(&expected.vertices)
            .all(|(a, b)| a.position == b.position && a.surface == b.surface));
        let invalid = Style {
            fill: Some(noon_core::Color::rgba(f32::NAN, 1.0, 1.0, 0.0)),
            ..style
        };
        assert!(validate_style(invalid, 1.0, Domain::World).is_err());
    }

    #[test]
    fn fixed_orientation_draws_use_effective_painter_rank_then_item_order() {
        let mut state = SpatialPathGpuState::default();
        let draw = |index: usize, painter_rank: u32| PathDraw {
            key: PathKey {
                source: SourceKey::Inline(index as u64),
                specialization: Specialization {
                    stroke_width: 0,
                    screen_stroke: false,
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    fill: true,
                    stroke: false,
                },
            },
            instance: index,
            domain: Domain::FixedOrientation,
            painter_rank,
        };
        state.publish_draw(DrawId { row: 8, item: 0 }, draw(8, 2));
        state.publish_draw(DrawId { row: 3, item: 0 }, draw(3, 1));
        state.publish_draw(DrawId { row: 1, item: 0 }, draw(1, 1));
        assert_eq!(
            state
                .fixed_orientation_order
                .iter()
                .map(|key| key.2)
                .collect::<Vec<_>>(),
            vec![1, 3, 8]
        );
    }

    #[test]
    fn fixed_orientation_order_index_tracks_rank_domain_and_removal_changes() {
        let mut state = SpatialPathGpuState::default();
        let make_draw = |row: usize, domain, painter_rank| PathDraw {
            key: PathKey {
                source: SourceKey::Inline(row as u64),
                specialization: Specialization {
                    stroke_width: 0,
                    screen_stroke: false,
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    fill: true,
                    stroke: false,
                },
            },
            instance: row,
            domain,
            painter_rank,
        };
        let first = DrawId { row: 4, item: 0 };
        let second = DrawId { row: 9, item: 0 };
        state.publish_draw(first, make_draw(first.row, Domain::FixedOrientation, 3));
        state.publish_draw(second, make_draw(second.row, Domain::FixedOrientation, 1));
        assert_eq!(
            state
                .fixed_orientation_order
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(1, 0, 9), (3, 0, 4)]
        );

        // Re-ranking one draw changes only its ordered-index key.
        state.publish_draw(first, make_draw(first.row, Domain::FixedOrientation, 0));
        assert_eq!(
            state
                .fixed_orientation_order
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(0, 0, 4), (1, 0, 9)]
        );

        // Moving out of this domain and removal both retire the old key.
        state.publish_draw(first, make_draw(first.row, Domain::World, 0));
        assert_eq!(
            state
                .fixed_orientation_order
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(1, 0, 9)]
        );
        state.remove_draw(second);
        assert!(state.fixed_orientation_order.is_empty());
        assert!(!state.draws.contains_key(&second));
    }

    #[test]
    fn vector_text_materializes_in_text_painter_order_with_its_paint() {
        let mut geometries = GeometryResourceArena::default();
        let geometry_handle = geometries.insert(GeometryResource::VectorPath(std::sync::Arc::new(
            noon_core::VectorPath::new()
                .move_to(Vec2::new(0.0, 0.0))
                .line_to(Vec2::new(1.0, 0.0))
                .line_to(Vec2::new(0.0, 1.0))
                .close(),
        )));
        let vector = noon_core::TextVectorItem {
            geometry: geometry_handle,
            transform: noon_core::TextAffineTransform::translation(2.0, 3.0),
            style: TextVectorStyle {
                fill: Some(Color::BLUE),
                ..Default::default()
            },
            source_span: None,
            semantic_key: None,
        };
        let text = TextResource {
            source: std::sync::Arc::from("label"),
            kind: TextSourceKind::Plain,
            runs: std::sync::Arc::from([]),
            vector_items: std::sync::Arc::from([vector]),
            render_items: std::sync::Arc::from([TextRenderItem::Vector(0)]),
            parts: std::sync::Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
            baseline: 0.0,
            layout_artifact: None,
        };
        let handle = TextResourceArena::new().insert(text.clone()).unwrap();
        let candidates = collect_text_paths(
            &mut GlyphOutlineCache::default(),
            handle,
            &text,
            &noon_core::FontResourceArena::new(),
            &geometries,
            Style::default(),
            7,
            |_, _| false,
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert_eq!(candidate.id.row, 7);
        assert_eq!(candidate.id.item, 0);
        assert_eq!(
            candidate.source,
            Some(SourceKey::TextVector(handle, 0, geometry_handle))
        );
        assert_eq!(candidate.style.fill, Some(Color::BLUE));
        let GeometryRef::VectorPath(path) = candidate.geometry.as_ref() else {
            panic!("text vector is materialized as a transformed path")
        };
        assert!(path.commands().iter().any(|command| matches!(
            command,
            noon_core::PathCommand::MoveTo { to } if *to == Vec2::new(2.0, 3.0)
        )));
    }
}
