use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use noon_core::{Inset2DViewState, Vec2};
use noon_runtime::{FrameChanges, FrameState};

mod derived_display;
mod overlay;
mod path_batching;
mod presentation;
pub use overlay::{AnalyticOverlay, OverlayGpuState, OverlayPrepareError};
mod inset_capture;
mod mesh;
mod spatial_path;
pub use mesh::{SpatialPrepareError, SpatialUploadStats};
pub use spatial_path::SpatialPathError;
mod raster_image_gpu;
mod raster_image_prepare;
use derived_display::DerivedDisplayGpu;
pub use presentation::OutputTransfer;
use presentation::PresentationBridge;
pub use raster_image_prepare::RasterImagePrepareError;

use crate::{
    CircleInstance, LineInstance, OrderedRenderBatch, PathBatch, PathInstance, PathVertex,
    PreparedDerivedDisplay, PreparedFrame, RectangleInstance, RenderPrimitive,
};

const QUAD_VERTICES: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
const ANALYTIC_BLEND_STATE: wgpu::BlendState = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
const PATH_SAMPLE_COUNT: u32 = 4;

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 16,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 72,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 24,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 40,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 56,
        shader_location: 7,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32x2,
        offset: 64,
        shader_location: 8,
    },
];

const LINE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 16,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 72,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 24,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 40,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 56,
        shader_location: 7,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32x2,
        offset: 64,
        shader_location: 8,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 80,
        shader_location: 9,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 20,
        shader_location: 10,
    },
];

const PATH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32];
const POLYGON_PATH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Uint32,
    11 => Float32x4,
    12 => Float32x4,
    13 => Float32x4
];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CompactPathVertex {
    position: [f32; 2],
    target_position: [f32; 2],
    surface: u32,
}

impl From<PathVertex> for CompactPathVertex {
    fn from(vertex: PathVertex) -> Self {
        Self {
            position: vertex.position,
            target_position: vertex.target_position,
            surface: vertex.surface,
        }
    }
}

fn update_compact_path_vertices(
    compact: &mut Vec<CompactPathVertex>,
    source: &[PathVertex],
    dirty_ranges: &[std::ops::Range<usize>],
) {
    compact.resize(source.len(), CompactPathVertex::zeroed());
    for range in dirty_ranges {
        let end = range.end.min(source.len());
        let start = range.start.min(end);
        for (target, source) in compact[start..end].iter_mut().zip(&source[start..end]) {
            *target = (*source).into();
        }
    }
}
const PATH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 16,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 24,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 40,
        shader_location: 7,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 56,
        shader_location: 8,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32x2,
        offset: 64,
        shader_location: 9,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 72,
        shader_location: 10,
    },
];

struct AnalyticPipelineDescriptor {
    vertex_entry: &'static str,
    fragment_entry: &'static str,
    label: &'static str,
    instance_layout: wgpu::VertexBufferLayout<'static>,
}

struct PathPipelineDescriptor {
    instance_layout: wgpu::VertexBufferLayout<'static>,
    vertex_layout: wgpu::VertexBufferLayout<'static>,
    vertex_entry: &'static str,
    fragment_entry: &'static str,
    label: &'static str,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct CameraUniform {
    center: [f32; 2],
    clip_scale: [f32; 2],
    viewport_size: [f32; 2],
    padding: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera2D {
    pub center: Vec2,
    pub world_size: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Inset2DGpuView {
    state: Inset2DViewState,
    capture_size: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    InvalidWorldSize,
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWorldSize => {
                formatter.write_str("camera world size must be finite and positive")
            }
        }
    }
}

impl std::error::Error for CameraError {}

impl Camera2D {
    pub const DEFAULT: Self = Self {
        center: Vec2::ZERO,
        world_size: Vec2::new(2.0, 2.0),
    };

    pub fn new(center: Vec2, world_size: Vec2) -> Result<Self, CameraError> {
        if !center.x.is_finite()
            || !center.y.is_finite()
            || !world_size.x.is_finite()
            || !world_size.y.is_finite()
            || world_size.x <= 0.0
            || world_size.y <= 0.0
        {
            return Err(CameraError::InvalidWorldSize);
        }
        Ok(Self { center, world_size })
    }

    fn uniform(self, viewport_size: [u32; 2]) -> CameraUniform {
        CameraUniform {
            center: [self.center.x, self.center.y],
            clip_scale: [2.0 / self.world_size.x, 2.0 / self.world_size.y],
            viewport_size: [viewport_size[0] as f32, viewport_size[1] as f32],
            padding: [0.0; 2],
        }
    }
}

impl Default for Camera2D {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One validated secondary viewport over the renderer's existing retained frame.
///
/// This descriptor owns no scene, execution, resources, or camera buffer. It is a
/// pure composition request that can be encoded against the same prepared frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondaryViewport {
    pub camera: Camera2D,
    /// Destination rectangle in physical output pixels: [x, y, width, height].
    pub destination: [u32; 4],
}

/// One complete retained-frame composition request.
///
/// Grouping the frame inputs makes composition ownership explicit without
/// proliferating renderer entry-point arguments. The request only borrows
/// prepared CPU-side state; GPU resources remain owned by the renderer.
pub struct FrameComposition<'a, 'frame> {
    pub prepared: &'a PreparedFrame<'frame>,
    pub presentations: Option<&'a PreparedDerivedDisplay>,
    pub secondary_viewports: &'a [SecondaryViewport],
    pub overlay: Option<&'a OverlayGpuState>,
    pub clear_color: wgpu::Color,
    pub query_set: Option<&'a wgpu::QuerySet>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryViewportError {
    EmptyDestination,
    DestinationOutOfBounds,
    MultisampledContentUnsupported,
}

impl std::fmt::Display for SecondaryViewportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EmptyDestination => "secondary viewport destination must be non-empty",
            Self::DestinationOutOfBounds => {
                "secondary viewport destination must fit inside the output viewport"
            }
            Self::MultisampledContentUnsupported => {
                "secondary viewport currently supports single-sample analytic content only"
            }
        })
    }
}

impl std::error::Error for SecondaryViewportError {}

impl SecondaryViewport {
    pub fn new(
        camera: Camera2D,
        destination: [u32; 4],
        output_size: [u32; 2],
    ) -> Result<Self, SecondaryViewportError> {
        let [x, y, width, height] = destination;
        if width == 0 || height == 0 {
            return Err(SecondaryViewportError::EmptyDestination);
        }
        let Some(end_x) = x.checked_add(width) else {
            return Err(SecondaryViewportError::DestinationOutOfBounds);
        };
        let Some(end_y) = y.checked_add(height) else {
            return Err(SecondaryViewportError::DestinationOutOfBounds);
        };
        if end_x > output_size[0] || end_y > output_size[1] {
            return Err(SecondaryViewportError::DestinationOutOfBounds);
        }
        Ok(Self {
            camera,
            destination,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadStats {
    pub bytes_uploaded: usize,
    pub buffer_reallocations: usize,
}

/// One actual `Queue::write_buffer` operation performed by an instrumented upload.
///
/// This is opt-in diagnostic data. Normal rendering keeps the existing compact
/// `UploadStats` path and does not allocate a write trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadWrite {
    pub buffer: &'static str,
    pub instance_range: std::ops::Range<usize>,
    pub byte_offset: u64,
    pub byte_length: usize,
    pub payload_hash: u64,
}

pub(crate) type UploadWriteTrace<'a> =
    dyn FnMut(&'static str, std::ops::Range<usize>, wgpu::BufferAddress, &[u8]) + 'a;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    pub draw_calls: usize,
    pub instances_drawn: usize,
}

// Valid only within one pass/camera/sample-count run; foreign draws invalidate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeometryBinding {
    Circle,
    Rectangle,
    Line,
    Path,
    Polygon,
    MegaPath,
}

#[derive(Clone, Debug)]
struct ResolvedOrderedBatch {
    batch: crate::OrderedRenderBatch,
    mega: Option<crate::MegaPathBatch>,
}

impl ResolvedOrderedBatch {
    fn merge(&mut self, next: &Self) -> bool {
        match (&mut self.mega, &next.mega) {
            (Some(current), Some(next)) if current.index_range.end == next.index_range.start => {
                current.index_range.end = next.index_range.end;
                current.path_count += next.path_count;
                true
            }
            (None, None)
                if self.batch.primitive == next.batch.primitive
                    && self.batch.instance_range.end == next.batch.instance_range.start =>
            {
                self.batch.instance_range.end = next.batch.instance_range.end;
                true
            }
            _ => false,
        }
    }
}

/// Failure before a resident geometry preload touches GPU buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPreloadUploadError {
    NonEmptyDrawState,
    BufferLimit {
        buffer: &'static str,
        requested: usize,
        limit: u64,
    },
}

impl std::fmt::Display for PathPreloadUploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonEmptyDrawState => f.write_str("path preload requires empty draw state"),
            Self::BufferLimit {
                buffer,
                requested,
                limit,
            } => write!(
                f,
                "{buffer} preload allocation {requested} exceeds device buffer limit {limit}"
            ),
        }
    }
}
impl std::error::Error for PathPreloadUploadError {}

#[derive(Debug)]
pub struct GpuRenderer {
    spatial: mesh::SpatialGpuState,
    circle_pipeline: wgpu::RenderPipeline,
    rectangle_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    circle_pipeline_single_sample: wgpu::RenderPipeline,
    rectangle_pipeline_single_sample: wgpu::RenderPipeline,
    line_pipeline_single_sample: wgpu::RenderPipeline,
    path_pipeline: wgpu::RenderPipeline,
    /// Full path vertices carry the renderer-local exact-triangle coordinates.
    /// Transient presentation paths also retain this full format.
    full_path_pipeline: wgpu::RenderPipeline,
    mega_path_pipeline: wgpu::RenderPipeline,
    quad_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    inset_camera_buffers: Vec<wgpu::Buffer>,
    inset_camera_bind_groups: Vec<wgpu::BindGroup>,
    inset_views: Vec<Inset2DGpuView>,
    inset_targets: Vec<inset_capture::InsetCaptureTarget>,
    inset_camera_by_display: std::collections::HashMap<noon_core::ObjectId, usize>,
    camera: Camera2D,
    viewport_size: [u32; 2],
    target_format: wgpu::TextureFormat,
    presentation: PresentationBridge,
    path_msaa_texture: wgpu::Texture,
    path_msaa_view: wgpu::TextureView,
    circle_buffer: wgpu::Buffer,
    rectangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    path_vertex_buffer: wgpu::Buffer,
    compact_path_vertex_buffer: wgpu::Buffer,
    compact_path_vertices: Vec<CompactPathVertex>,
    path_index_buffer: wgpu::Buffer,
    path_instance_buffer: wgpu::Buffer,
    mega_path_index_buffer: wgpu::Buffer,
    mega_path_vertex_instance_buffer: wgpu::Buffer,
    derived_display: DerivedDisplayGpu,
    images: Option<raster_image_gpu::RasterImageGpuRenderer>,
    circle_capacity_bytes: usize,
    rectangle_capacity_bytes: usize,
    line_capacity_bytes: usize,
    path_vertex_capacity_bytes: usize,
    compact_path_vertex_capacity_bytes: usize,
    path_index_capacity_bytes: usize,
    path_instance_capacity_bytes: usize,
    mega_path_index_capacity_bytes: usize,
    mega_path_vertex_instance_capacity_bytes: usize,
}

#[derive(Clone, Copy)]
struct FramePassOptions<'a> {
    clear_color: wgpu::Color,
    query_set: Option<&'a wgpu::QuerySet>,
    overlay: Option<&'a OverlayGpuState>,
    finalize: bool,
}
impl<'a> FramePassOptions<'a> {
    fn new(clear_color: wgpu::Color, query_set: Option<&'a wgpu::QuerySet>) -> Self {
        Self {
            clear_color,
            query_set,
            overlay: None,
            finalize: true,
        }
    }

    fn deferred(clear_color: wgpu::Color, query_set: Option<&'a wgpu::QuerySet>) -> Self {
        Self {
            clear_color,
            query_set,
            overlay: None,
            finalize: false,
        }
    }
}

impl GpuRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_output_transfer(device, queue, target_format, OutputTransfer::Direct)
    }

    pub fn new_with_output_transfer(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        output_transfer: OutputTransfer,
    ) -> Self {
        assert_eq!(
            size_of::<CircleInstance>(),
            size_of::<RectangleInstance>(),
            "analytic instance layouts must stay identical"
        );
        assert_eq!(
            size_of::<CircleInstance>(),
            size_of::<LineInstance>(),
            "analytic instance layouts must stay identical"
        );

        let camera = Camera2D::DEFAULT;
        let viewport_size = [1, 1];
        let presentation =
            PresentationBridge::new(device, surface_format, output_transfer, viewport_size);
        let target_format = presentation.scene_format();
        let camera_uniform = camera.uniform(viewport_size);
        let camera_buffer = create_buffer_with_data(
            device,
            queue,
            Some("Noon camera uniform"),
            bytemuck::bytes_of(&camera_uniform),
            wgpu::BufferUsages::UNIFORM,
        );
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<CameraUniform>() as _),
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let polygon_coverage = include_str!("../polygon_coverage.wgsl");
        let cairo_color = include_str!("../cairo_color.wgsl");
        let analytic_source = format!(
            "{}\n{}\n{}",
            include_str!("../analytic.wgsl"),
            cairo_color,
            polygon_coverage
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon analytic shader"),
            source: wgpu::ShaderSource::Wgsl(analytic_source.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon analytic pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let circle_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            PATH_SAMPLE_COUNT,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_circle",
                fragment_entry: "fs_circle",
                label: "Noon circle pipeline",
                instance_layout: analytic_instance_layout(),
            },
        );
        let rectangle_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            PATH_SAMPLE_COUNT,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_rectangle",
                fragment_entry: "fs_rectangle",
                label: "Noon rectangle pipeline",
                instance_layout: analytic_instance_layout(),
            },
        );
        let line_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            PATH_SAMPLE_COUNT,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_line",
                fragment_entry: "fs_line",
                label: "Noon line pipeline",
                instance_layout: line_instance_layout(),
            },
        );
        let circle_pipeline_single_sample = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            1,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_circle",
                fragment_entry: "fs_circle",
                label: "Noon circle single-sample pipeline",
                instance_layout: analytic_instance_layout(),
            },
        );
        let rectangle_pipeline_single_sample = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            1,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_rectangle",
                fragment_entry: "fs_rectangle",
                label: "Noon rectangle single-sample pipeline",
                instance_layout: analytic_instance_layout(),
            },
        );
        let line_pipeline_single_sample = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            1,
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_line",
                fragment_entry: "fs_line",
                label: "Noon line single-sample pipeline",
                instance_layout: line_instance_layout(),
            },
        );
        let path_source = format!(
            "{}\n{}\n{}",
            include_str!("../path.wgsl"),
            include_str!("../cairo_color.wgsl"),
            polygon_coverage
        );
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon path shader"),
            source: wgpu::ShaderSource::Wgsl(path_source.into()),
        });
        let path_pipeline =
            create_path_pipeline(device, &pipeline_layout, &path_shader, target_format);
        let full_path_pipeline =
            create_full_path_pipeline(device, &pipeline_layout, &path_shader, target_format);
        let mega_path_pipeline =
            create_mega_path_pipeline(device, &pipeline_layout, &path_shader, target_format);
        let quad_buffer = create_buffer_with_data(
            device,
            queue,
            Some("Noon unit quad"),
            bytemuck::cast_slice(&QUAD_VERTICES),
            wgpu::BufferUsages::VERTEX,
        );
        let circle_buffer = empty_instance_buffer(device, "Noon circle instances");
        let rectangle_buffer = empty_instance_buffer(device, "Noon rectangle instances");
        let line_buffer = empty_instance_buffer(device, "Noon line instances");
        let path_vertex_buffer = empty_buffer(
            device,
            "Noon path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let compact_path_vertex_buffer = empty_buffer(
            device,
            "Noon compact path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let path_index_buffer = empty_buffer(
            device,
            "Noon path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let path_instance_buffer = empty_instance_buffer(device, "Noon path instances");
        let mega_path_index_buffer = empty_buffer(
            device,
            "Noon packed path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let mega_path_vertex_instance_buffer =
            empty_instance_buffer(device, "Noon packed path vertex attributes");
        let derived_display = DerivedDisplayGpu::new(device);
        let (path_msaa_texture, path_msaa_view) =
            create_path_msaa_target(device, target_format, viewport_size);

        Self {
            spatial: mesh::SpatialGpuState::default(),
            circle_pipeline,
            rectangle_pipeline,
            line_pipeline,
            circle_pipeline_single_sample,
            rectangle_pipeline_single_sample,
            line_pipeline_single_sample,
            path_pipeline,
            full_path_pipeline,
            mega_path_pipeline,
            quad_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout: camera_layout,
            inset_camera_buffers: Vec::new(),
            inset_camera_bind_groups: Vec::new(),
            inset_views: Vec::new(),
            inset_targets: Vec::new(),
            inset_camera_by_display: std::collections::HashMap::new(),
            camera,
            viewport_size,
            target_format,
            presentation,
            path_msaa_texture,
            path_msaa_view,
            circle_buffer,
            rectangle_buffer,
            line_buffer,
            path_vertex_buffer,
            compact_path_vertex_buffer,
            compact_path_vertices: Vec::new(),
            path_index_buffer,
            path_instance_buffer,
            mega_path_index_buffer,
            mega_path_vertex_instance_buffer,
            derived_display,
            images: None,
            circle_capacity_bytes: 0,
            rectangle_capacity_bytes: 0,
            line_capacity_bytes: 0,
            path_vertex_capacity_bytes: 0,
            compact_path_vertex_capacity_bytes: 0,
            path_index_capacity_bytes: 0,
            path_instance_capacity_bytes: 0,
            mega_path_index_capacity_bytes: 0,
            mega_path_vertex_instance_capacity_bytes: 0,
        }
    }

    pub fn set_camera(&mut self, queue: &wgpu::Queue, camera: Camera2D) {
        self.camera = camera;
        self.write_camera_uniform(queue);
    }

    /// Derive mesh residency and effective camera uniforms from the same runtime
    /// publication consumed by retained 2D/text preparation. No authored store is
    /// consulted here and no frame motion rebuilds immutable vertices.
    pub fn prepare_spatial(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: &noon_runtime::RendererPublication<'_>,
    ) -> Result<SpatialUploadStats, SpatialPrepareError> {
        self.spatial.prepare(
            device,
            queue,
            self.target_format,
            self.viewport_size,
            publication,
            self.camera,
        )
    }

    /// Prepare spatial draws from an already-installed worker frame and its
    /// immutable geometry arena. The supplied context is the worker's exact
    /// installed publication context, so spatial uploads share the same
    /// stale/retry semantics as the runtime-owned publication path.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_spatial_with_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        context: noon_core::PublicationContext,
        frame: &FrameState,
        changes: &FrameChanges,
        geometry_resources: &dyn noon_core::GeometryResourceLookup,
        text_resources: &dyn noon_core::TextResourceLookup,
        font_resources: &dyn noon_core::FontResourceLookup,
        painter_order: &[u32],
    ) -> Result<SpatialUploadStats, SpatialPrepareError> {
        self.spatial.prepare_with_resources(
            device,
            queue,
            self.target_format,
            self.viewport_size,
            context,
            frame,
            changes,
            geometry_resources,
            text_resources,
            font_resources,
            painter_order,
            self.camera,
        )
    }

    /// Start a new installed retained session whose source publication context
    /// may begin at an earlier frame epoch than the session just replaced.
    pub fn reset_spatial_publication_context(&mut self) {
        self.spatial.reset_publication_context();
    }

    pub fn set_viewport(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) {
        let viewport_size = [width.max(1), height.max(1)];
        if self.viewport_size != viewport_size {
            self.viewport_size = viewport_size;
            (self.path_msaa_texture, self.path_msaa_view) =
                create_path_msaa_target(device, self.target_format, self.viewport_size);
            self.presentation.resize(device, self.viewport_size);
        }
        self.write_camera_uniform(queue);
    }

    fn write_camera_uniform(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera.uniform(self.viewport_size)),
        );
    }

    pub const fn camera(&self) -> Camera2D {
        self.camera
    }

    pub const fn viewport_size(&self) -> [u32; 2] {
        self.viewport_size
    }

    /// Check device allocation limits and upload an empty-draw resident prefix.
    /// This queues writes only; the platform host owns submission and readiness.
    pub fn upload_preloaded_paths(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
    ) -> Result<UploadStats, PathPreloadUploadError> {
        validate_empty_preload_frame(prepared)?;
        let limit = device.limits().max_buffer_size;
        validate_preload_allocation(
            "path vertices",
            std::mem::size_of_val(prepared.path_vertices),
            self.path_vertex_capacity_bytes,
            limit,
        )?;
        validate_preload_allocation(
            "path indices",
            std::mem::size_of_val(prepared.path_indices),
            self.path_index_capacity_bytes,
            limit,
        )?;
        Ok(self.upload(device, queue, prepared))
    }

    /// Replace the resident path buffers with exact-size buffers for a complete,
    /// empty-draw preload frame. The replacements are staged before any active
    /// renderer state changes; old submitted command buffers retain their old
    /// buffer handles through wgpu's normal resource lifetime tracking.
    pub(crate) fn replace_preloaded_path_buffers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
    ) -> Result<UploadStats, PathPreloadUploadError> {
        validate_empty_preload_frame(prepared)?;

        let vertices = bytemuck::cast_slice(prepared.path_vertices);
        let indices = bytemuck::cast_slice(prepared.path_indices);
        let mut compact_vertices = Vec::new();
        if !prepared.path_vertices.is_empty() {
            let full_vertex_range = 0..prepared.path_vertices.len();
            update_compact_path_vertices(
                &mut compact_vertices,
                prepared.path_vertices,
                std::slice::from_ref(&full_vertex_range),
            );
        }
        let compact_vertex_bytes = bytemuck::cast_slice(&compact_vertices);
        let vertex_bytes_len = vertices.len();
        let compact_vertex_bytes_len = compact_vertex_bytes.len();
        let index_bytes_len = indices.len();
        for (buffer, size) in [
            ("path vertices", vertices.len()),
            ("compact path vertices", compact_vertex_bytes.len()),
            ("path indices", indices.len()),
        ] {
            if size > device.limits().max_buffer_size as usize {
                return Err(PathPreloadUploadError::BufferLimit {
                    buffer,
                    requested: size,
                    limit: device.limits().max_buffer_size,
                });
            }
        }

        let path_vertex_buffer = replacement_path_buffer(
            device,
            queue,
            "Noon path vertices",
            vertices,
            wgpu::BufferUsages::VERTEX,
        );
        let compact_path_vertex_buffer = replacement_path_buffer(
            device,
            queue,
            "Noon compact path vertices",
            compact_vertex_bytes,
            wgpu::BufferUsages::VERTEX,
        );
        let path_index_buffer = replacement_path_buffer(
            device,
            queue,
            "Noon path indices",
            indices,
            wgpu::BufferUsages::INDEX,
        );
        let bytes_uploaded = vertex_bytes_len + compact_vertex_bytes_len + index_bytes_len;

        self.path_vertex_buffer = path_vertex_buffer;
        self.compact_path_vertex_buffer = compact_path_vertex_buffer;
        self.path_index_buffer = path_index_buffer;
        self.compact_path_vertices = compact_vertices;
        self.path_vertex_capacity_bytes = vertex_bytes_len;
        self.compact_path_vertex_capacity_bytes = compact_vertex_bytes_len;
        self.path_index_capacity_bytes = index_bytes_len;

        Ok(UploadStats {
            bytes_uploaded,
            buffer_reallocations: 3,
        })
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
    ) -> UploadStats {
        self.upload_inner(device, queue, prepared, None)
    }

    /// Upload a frame while recording the exact CPU-to-GPU write operations.
    ///
    /// The trace is intentionally opt-in for diagnostics such as the WebGL
    /// host-updater reproducer; production callers continue to use `upload`.
    pub fn upload_with_trace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
        writes: &mut Vec<UploadWrite>,
    ) -> UploadStats {
        let mut trace = |buffer, instance_range, byte_offset, bytes: &[u8]| {
            push_upload_write(writes, buffer, instance_range, byte_offset, bytes);
        };
        self.upload_inner(device, queue, prepared, Some(&mut trace))
    }

    pub(crate) fn upload_with_write_trace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
        trace: &mut UploadWriteTrace<'_>,
    ) -> UploadStats {
        self.upload_inner(device, queue, prepared, Some(trace))
    }

    fn upload_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
        mut trace: Option<&mut UploadWriteTrace<'_>>,
    ) -> UploadStats {
        let circle_bytes = std::mem::size_of_val(prepared.circles);
        let rectangle_bytes = std::mem::size_of_val(prepared.rectangles);
        let line_bytes = std::mem::size_of_val(prepared.lines);
        let path_vertex_bytes = std::mem::size_of_val(prepared.path_vertices);
        let path_index_bytes = std::mem::size_of_val(prepared.path_indices);
        let path_instance_bytes = std::mem::size_of_val(prepared.paths);
        let mega_path_index_bytes = std::mem::size_of_val(prepared.mega_path_indices);
        let mega_path_vertex_instance_bytes =
            std::mem::size_of_val(prepared.mega_path_vertex_instances);
        let mut buffer_reallocations = 0;

        let circle_reallocated = ensure_capacity(
            device,
            &mut self.circle_buffer,
            &mut self.circle_capacity_bytes,
            circle_bytes,
            "Noon circle instances",
        );
        if circle_reallocated {
            buffer_reallocations += 1;
        }
        let rectangle_reallocated = ensure_capacity(
            device,
            &mut self.rectangle_buffer,
            &mut self.rectangle_capacity_bytes,
            rectangle_bytes,
            "Noon rectangle instances",
        );
        if rectangle_reallocated {
            buffer_reallocations += 1;
        }
        let line_reallocated = ensure_capacity(
            device,
            &mut self.line_buffer,
            &mut self.line_capacity_bytes,
            line_bytes,
            "Noon line instances",
        );
        if line_reallocated {
            buffer_reallocations += 1;
        }
        let path_vertex_reallocated = ensure_capacity_with_usage(
            device,
            &mut self.path_vertex_buffer,
            &mut self.path_vertex_capacity_bytes,
            path_vertex_bytes,
            "Noon path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        update_compact_path_vertices(
            &mut self.compact_path_vertices,
            prepared.path_vertices,
            prepared.path_vertex_dirty_ranges,
        );
        let compact_path_vertex_reallocated = ensure_capacity_with_usage(
            device,
            &mut self.compact_path_vertex_buffer,
            &mut self.compact_path_vertex_capacity_bytes,
            self.compact_path_vertices.len() * size_of::<CompactPathVertex>(),
            "Noon compact path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        buffer_reallocations +=
            usize::from(path_vertex_reallocated) + usize::from(compact_path_vertex_reallocated);
        let path_index_reallocated = ensure_capacity_with_usage(
            device,
            &mut self.path_index_buffer,
            &mut self.path_index_capacity_bytes,
            path_index_bytes,
            "Noon path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        buffer_reallocations += usize::from(path_index_reallocated);
        let path_instance_reallocated = ensure_capacity(
            device,
            &mut self.path_instance_buffer,
            &mut self.path_instance_capacity_bytes,
            path_instance_bytes,
            "Noon path instances",
        );
        buffer_reallocations += usize::from(path_instance_reallocated);
        let mega_path_index_reallocated = ensure_capacity_with_usage(
            device,
            &mut self.mega_path_index_buffer,
            &mut self.mega_path_index_capacity_bytes,
            mega_path_index_bytes,
            "Noon packed path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        buffer_reallocations += usize::from(mega_path_index_reallocated);
        let mega_path_vertex_instance_reallocated = ensure_capacity(
            device,
            &mut self.mega_path_vertex_instance_buffer,
            &mut self.mega_path_vertex_instance_capacity_bytes,
            mega_path_vertex_instance_bytes,
            "Noon packed path vertex attributes",
        );
        buffer_reallocations += usize::from(mega_path_vertex_instance_reallocated);

        let bytes_uploaded = upload_dirty(
            queue,
            &self.circle_buffer,
            prepared.circles,
            prepared.circle_dirty_ranges,
            circle_reallocated,
            "circle",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.compact_path_vertex_buffer,
            &self.compact_path_vertices,
            prepared.path_vertex_dirty_ranges,
            compact_path_vertex_reallocated,
            "compact_path_vertex",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.rectangle_buffer,
            prepared.rectangles,
            prepared.rectangle_dirty_ranges,
            rectangle_reallocated,
            "rectangle",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.line_buffer,
            prepared.lines,
            prepared.line_dirty_ranges,
            line_reallocated,
            "line",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.path_vertex_buffer,
            prepared.path_vertices,
            prepared.path_vertex_dirty_ranges,
            path_vertex_reallocated,
            "path_vertex",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.path_index_buffer,
            prepared.path_indices,
            prepared.path_index_dirty_ranges,
            path_index_reallocated,
            "path_index",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.path_instance_buffer,
            prepared.paths,
            prepared.path_dirty_ranges,
            path_instance_reallocated,
            "path_instance",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.mega_path_index_buffer,
            prepared.mega_path_indices,
            prepared.mega_path_index_dirty_ranges,
            mega_path_index_reallocated,
            "mega_path_index",
            &mut trace,
        ) + upload_dirty(
            queue,
            &self.mega_path_vertex_instance_buffer,
            prepared.mega_path_vertex_instances,
            prepared.mega_path_instance_dirty_ranges,
            mega_path_vertex_instance_reallocated,
            "mega_path_vertex_instance",
            &mut trace,
        );

        UploadStats {
            bytes_uploaded,
            buffer_reallocations,
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            None,
            FramePassOptions::new(clear_color, None),
        )
    }

    /// Encode one complete retained composition in presentation order.
    ///
    /// The primary scene is encoded first without finalization, then every
    /// secondary analytic viewport is appended, then the optional session overlay
    /// is drawn once and the output is presented once. All secondary requests are
    /// validated before the primary pass begins, so a rejected composition leaves
    /// the target untouched.
    ///
    /// GPU timestamps, when supplied, cover the primary scene pass only.
    pub fn encode_composed_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        composition: FrameComposition<'_, '_>,
    ) -> Result<DrawStats, SecondaryViewportError> {
        let FrameComposition {
            prepared,
            presentations,
            secondary_viewports,
            overlay,
            clear_color,
            query_set,
        } = composition;

        for secondary in secondary_viewports {
            SecondaryViewport::new(secondary.camera, secondary.destination, self.viewport_size)?;
        }
        if !secondary_viewports.is_empty()
            && self.scene_render_sample_count(prepared, presentations) != 1
        {
            return Err(SecondaryViewportError::MultisampledContentUnsupported);
        }

        let mut stats = self.encode_inner(
            encoder,
            view,
            prepared,
            presentations,
            FramePassOptions::deferred(clear_color, query_set),
        );
        for secondary in secondary_viewports {
            stats += self.encode_secondary_viewport_validated(
                device,
                queue,
                encoder,
                view,
                &composition,
                *secondary,
            );
        }
        stats += self.finalize_frame(encoder, view, overlay);
        Ok(stats)
    }

    fn scene_render_sample_count(
        &self,
        prepared: &PreparedFrame<'_>,
        presentations: Option<&PreparedDerivedDisplay>,
    ) -> u32 {
        let visible_derived_paths = presentations.is_some_and(|presentations| {
            presentations
                .path_batches
                .iter()
                .any(|batch| !batch.index_range.is_empty())
        });
        ordered_render_sample_count(
            prepared.path_batches,
            self.spatial.has_active_paths() || visible_derived_paths,
        )
    }

    fn encode_secondary_viewport_validated(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        composition: &FrameComposition<'_, '_>,
        secondary: SecondaryViewport,
    ) -> DrawStats {
        let prepared = composition.prepared;
        let presentations = composition.presentations;
        let [x, y, width, height] = secondary.destination;
        let camera_uniform = secondary.camera.uniform([width, height]);
        let secondary_camera_buffer = create_buffer_with_data(
            device,
            queue,
            Some("Noon secondary viewport camera uniform"),
            bytemuck::bytes_of(&camera_uniform),
            wgpu::BufferUsages::UNIFORM,
        );
        let secondary_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon secondary viewport camera bind group"),
            layout: &self.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: secondary_camera_buffer.as_entire_binding(),
            }],
        });

        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: self.presentation.scene_view(view),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon secondary viewport render pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, width, height);
        match presentations {
            Some(presentations) => self.draw_with_derived_camera(
                &mut pass,
                prepared,
                presentations,
                true,
                &secondary_camera_bind_group,
            ),
            None => self.draw_ordered_with_camera(
                &mut pass,
                prepared,
                true,
                &secondary_camera_bind_group,
            ),
        }
    }

    /// Encodes a render pass with beginning/end GPU timestamp writes.
    pub fn encode_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            None,
            FramePassOptions::new(clear_color, Some(query_set)),
        )
    }

    fn encode_inner(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        derived: Option<&PreparedDerivedDisplay>,
        options: FramePassOptions<'_>,
    ) -> DrawStats {
        let FramePassOptions {
            clear_color,
            query_set,
            overlay,
            finalize,
        } = options;
        let scene_view = self.presentation.scene_view(view);
        let sample_count = self.scene_render_sample_count(prepared, derived);
        let spatial_stats = self.encode_spatial(encoder, scene_view, clear_color, sample_count);
        let load = if self.spatial.is_active() {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(clear_color)
        };
        let stats = if sample_count == 1 {
            // Analytic SDF primitives already use derivative-based edge coverage. When
            // no visible vector path participates in painter order, avoid 4x sample
            // shading and the multisample resolve without changing alpha ordering.
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })];
            let timestamp_writes = query_set.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon ordered single-sample analytic render pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match derived {
                Some(derived) => self.draw_with_derived(&mut pass, prepared, derived, true),
                None => self.draw_ordered(&mut pass, prepared, true),
            }
        } else {
            // Mixed vector/analytic content still shares one 4x multisampled target so
            // pipeline switches follow semantic painter order. Splitting these primitives
            // into separate passes would not be alpha-order safe.
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &self.path_msaa_view,
                depth_slice: None,
                resolve_target: Some(scene_view),
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Discard,
                },
            })];
            let timestamp_writes = query_set.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon ordered multisampled render pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match derived {
                Some(derived) => self.draw_with_derived(&mut pass, prepared, derived, false),
                None => self.draw_ordered(&mut pass, prepared, false),
            }
        };
        let mut stats = stats;
        stats += spatial_stats;
        if finalize {
            stats += self.finalize_frame(encoder, view, overlay);
        }
        stats
    }

    fn encode_spatial(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        clear: wgpu::Color,
        samples: u32,
    ) -> DrawStats {
        if samples == 1 {
            self.spatial.encode(encoder, scene, None, clear, samples)
        } else {
            self.spatial
                .encode(encoder, &self.path_msaa_view, Some(scene), clear, samples)
        }
    }

    pub(crate) fn finalize_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        overlay: Option<&OverlayGpuState>,
    ) -> DrawStats {
        let stats = self.encode_overlay(encoder, view, overlay);
        self.presentation.encode_present(encoder, view);
        stats
    }

    fn draw_ordered<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        single_sample_analytics: bool,
    ) -> DrawStats {
        self.draw_ordered_with_camera(
            pass,
            prepared,
            single_sample_analytics,
            &self.camera_bind_group,
        )
    }

    fn draw_ordered_with_camera<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        single_sample_analytics: bool,
        camera_bind_group: &'a wgpu::BindGroup,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, camera_bind_group, &[]);
        let mut pending = None::<ResolvedOrderedBatch>;
        let mut binding = None;
        for resolved in prepared.ordered_render_batches() {
            let next = ResolvedOrderedBatch {
                batch: resolved.batch.clone(),
                mega: resolved.mega_path_batch.cloned(),
            };
            if pending.as_mut().is_some_and(|current| current.merge(&next)) {
                continue;
            }
            if let Some(current) = pending.replace(next) {
                let drawn = self.draw_resolved_ordered_batch(
                    pass,
                    prepared,
                    &current,
                    single_sample_analytics,
                    &mut binding,
                );
                stats.draw_calls += drawn.draw_calls;
                stats.instances_drawn += drawn.instances_drawn;
            }
        }
        if let Some(current) = pending {
            let drawn = self.draw_resolved_ordered_batch(
                pass,
                prepared,
                &current,
                single_sample_analytics,
                &mut binding,
            );
            stats.draw_calls += drawn.draw_calls;
            stats.instances_drawn += drawn.instances_drawn;
        }
        stats
    }

    fn draw_resolved_ordered_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        resolved: &ResolvedOrderedBatch,
        single_sample_analytics: bool,
        binding: &mut Option<GeometryBinding>,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        for instance_range in prepared.contributing_instance_ranges(&resolved.batch) {
            let segment = ResolvedOrderedBatch {
                batch: OrderedRenderBatch {
                    primitive: resolved.batch.primitive,
                    instance_range,
                },
                mega: resolved.mega.clone(),
            };
            stats += self.draw_contributing_ordered_batch(
                pass,
                prepared,
                &segment,
                single_sample_analytics,
                binding,
            );
        }
        stats
    }

    fn draw_contributing_ordered_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        resolved: &ResolvedOrderedBatch,
        single_sample_analytics: bool,
        binding: &mut Option<GeometryBinding>,
    ) -> DrawStats {
        let batch = &resolved.batch;
        let (next, pipeline, vertices, instances, indices) = match batch.primitive {
            RenderPrimitive::Circle => (
                GeometryBinding::Circle,
                if single_sample_analytics {
                    &self.circle_pipeline_single_sample
                } else {
                    &self.circle_pipeline
                },
                &self.quad_buffer,
                &self.circle_buffer,
                None,
            ),
            RenderPrimitive::Rectangle => (
                GeometryBinding::Rectangle,
                if single_sample_analytics {
                    &self.rectangle_pipeline_single_sample
                } else {
                    &self.rectangle_pipeline
                },
                &self.quad_buffer,
                &self.rectangle_buffer,
                None,
            ),
            RenderPrimitive::Line => (
                GeometryBinding::Line,
                if single_sample_analytics {
                    &self.line_pipeline_single_sample
                } else {
                    &self.line_pipeline
                },
                &self.quad_buffer,
                &self.line_buffer,
                None,
            ),
            RenderPrimitive::Path { batch } => {
                let path = &prepared.path_batches[batch];
                if path.index_range.is_empty() {
                    return DrawStats::default();
                }
                if path_batch_uses_polygon_coverage(prepared, path) {
                    (
                        GeometryBinding::Polygon,
                        &self.full_path_pipeline,
                        &self.path_vertex_buffer,
                        &self.path_instance_buffer,
                        Some(&self.path_index_buffer),
                    )
                } else {
                    (
                        GeometryBinding::Path,
                        &self.path_pipeline,
                        &self.compact_path_vertex_buffer,
                        &self.path_instance_buffer,
                        Some(&self.path_index_buffer),
                    )
                }
            }
            RenderPrimitive::MegaPath { .. } => {
                if resolved
                    .mega
                    .as_ref()
                    .expect("resolved mega metadata")
                    .index_range
                    .is_empty()
                {
                    return DrawStats::default();
                }
                (
                    GeometryBinding::MegaPath,
                    &self.mega_path_pipeline,
                    &self.compact_path_vertex_buffer,
                    &self.mega_path_vertex_instance_buffer,
                    Some(&self.mega_path_index_buffer),
                )
            }
        };
        if *binding != Some(next) {
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(..));
            if let Some(indices) = indices {
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            }
            *binding = Some(next);
        }
        match batch.primitive {
            RenderPrimitive::Path { batch: path } => {
                pass.draw_indexed(
                    prepared.path_batches[path].index_range.clone(),
                    0,
                    batch.instance_range.clone(),
                );
            }
            RenderPrimitive::MegaPath { .. } => {
                let mega = resolved.mega.as_ref().expect("resolved mega metadata");
                let mut stats = DrawStats::default();
                for (index_range, path_count) in
                    prepared.contributing_mega_index_ranges(mega.index_range.clone())
                {
                    pass.draw_indexed(index_range, 0, 0..1);
                    stats.draw_calls += 1;
                    stats.instances_drawn += path_count;
                }
                return stats;
            }
            _ => pass.draw(0..6, batch.instance_range.clone()),
        }
        DrawStats {
            draw_calls: 1,
            instances_drawn: batch.instance_range.len(),
        }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
    ) -> DrawStats {
        self.draw_ordered(pass, prepared, false)
    }

    pub const fn circle_capacity_bytes(&self) -> usize {
        self.circle_capacity_bytes
    }

    pub const fn rectangle_capacity_bytes(&self) -> usize {
        self.rectangle_capacity_bytes
    }

    pub const fn line_capacity_bytes(&self) -> usize {
        self.line_capacity_bytes
    }

    pub const fn path_vertex_capacity_bytes(&self) -> usize {
        self.path_vertex_capacity_bytes
    }

    pub const fn path_index_capacity_bytes(&self) -> usize {
        self.path_index_capacity_bytes
    }

    pub const fn path_instance_capacity_bytes(&self) -> usize {
        self.path_instance_capacity_bytes
    }

    pub const fn mega_path_index_capacity_bytes(&self) -> usize {
        self.mega_path_index_capacity_bytes
    }

    pub const fn mega_path_vertex_instance_capacity_bytes(&self) -> usize {
        self.mega_path_vertex_instance_capacity_bytes
    }
}

pub fn quad_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<[f32; 2]>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &QUAD_ATTRIBUTES,
    }
}

pub fn analytic_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<CircleInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRIBUTES,
    }
}

pub fn line_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<LineInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &LINE_INSTANCE_ATTRIBUTES,
    }
}

pub fn path_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<CompactPathVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &PATH_VERTEX_ATTRIBUTES,
    }
}

pub fn polygon_path_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<PathVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &POLYGON_PATH_VERTEX_ATTRIBUTES,
    }
}

pub fn path_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<PathInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &PATH_INSTANCE_ATTRIBUTES,
    }
}

pub fn mega_path_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<PathInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &PATH_INSTANCE_ATTRIBUTES,
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    descriptor: AnalyticPipelineDescriptor,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(descriptor.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(descriptor.vertex_entry),
            compilation_options: Default::default(),
            buffers: &[Some(quad_vertex_layout()), Some(descriptor.instance_layout)],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(descriptor.fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(ANALYTIC_BLEND_STATE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn ordered_render_sample_count(path_batches: &[PathBatch], other_visible_paths: bool) -> u32 {
    if other_visible_paths
        || path_batches
            .iter()
            .any(|batch| !batch.index_range.is_empty())
    {
        PATH_SAMPLE_COUNT
    } else {
        1
    }
}

fn path_batch_uses_polygon_coverage(prepared: &PreparedFrame<'_>, batch: &PathBatch) -> bool {
    let _ = prepared;
    batch.polygon_coverage
}

fn create_path_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_path_pipeline_with_instance_layout(
        device,
        layout,
        shader,
        target_format,
        PathPipelineDescriptor {
            instance_layout: path_instance_layout(),
            label: "Noon vector path pipeline",
            vertex_layout: path_vertex_layout(),
            vertex_entry: "vs_path_compact",
            fragment_entry: "fs_path_compact",
        },
    )
}

fn create_mega_path_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_path_pipeline_with_instance_layout(
        device,
        layout,
        shader,
        target_format,
        PathPipelineDescriptor {
            instance_layout: mega_path_instance_layout(),
            label: "Noon packed mega-path pipeline",
            vertex_layout: path_vertex_layout(),
            vertex_entry: "vs_path_compact",
            fragment_entry: "fs_path_compact",
        },
    )
}

fn create_full_path_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_path_pipeline_with_instance_layout(
        device,
        layout,
        shader,
        target_format,
        PathPipelineDescriptor {
            instance_layout: path_instance_layout(),
            label: "Noon full path pipeline",
            vertex_layout: polygon_path_vertex_layout(),
            vertex_entry: "vs_path",
            fragment_entry: "fs_path",
        },
    )
}

fn create_path_pipeline_with_instance_layout(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    descriptor: PathPipelineDescriptor,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(descriptor.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(descriptor.vertex_entry),
            compilation_options: Default::default(),
            buffers: &[
                Some(descriptor.vertex_layout),
                Some(descriptor.instance_layout),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(descriptor.fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(ANALYTIC_BLEND_STATE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: PATH_SAMPLE_COUNT,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

/// Initialize an aligned typed buffer through the queue instead of a
/// mapped-at-creation range. This avoids the mapped-range exception observed
/// in browser WebGPU while preserving the buffer's exact upload contract.
pub(super) fn create_buffer_with_data(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: wgpu::Label<'_>,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    assert!(
        !contents.is_empty()
            && contents
                .len()
                .is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT as usize),
        "typed buffer initialization must have a non-zero copy-aligned length"
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label,
        size: contents.len() as wgpu::BufferAddress,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, contents);
    buffer
}

fn empty_instance_buffer(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    empty_buffer(
        device,
        label,
        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    )
}

fn empty_buffer(device: &wgpu::Device, label: &str, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: 4,
        usage,
        mapped_at_creation: false,
    })
}

fn replacement_path_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    if contents.is_empty() {
        empty_buffer(device, label, usage | wgpu::BufferUsages::COPY_DST)
    } else {
        create_buffer_with_data(device, queue, Some(label), contents, usage)
    }
}

fn validate_empty_preload_frame(
    prepared: &PreparedFrame<'_>,
) -> Result<(), PathPreloadUploadError> {
    if !prepared.circles.is_empty()
        || !prepared.rectangles.is_empty()
        || !prepared.lines.is_empty()
        || !prepared.paths.is_empty()
        || !prepared.path_batches.is_empty()
        || !prepared.render_batches.is_empty()
        || prepared.ordered_render_chunks().next().is_some()
        || !prepared.mega_path_indices.is_empty()
        || !prepared.mega_path_vertex_instances.is_empty()
        || !prepared.mega_path_batches.is_empty()
    {
        return Err(PathPreloadUploadError::NonEmptyDrawState);
    }
    Ok(())
}

fn create_path_msaa_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Noon multisampled path target"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: PATH_SAMPLE_COUNT,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn ensure_capacity(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    capacity_bytes: &mut usize,
    required_bytes: usize,
    label: &str,
) -> bool {
    ensure_capacity_with_usage(
        device,
        buffer,
        capacity_bytes,
        required_bytes,
        label,
        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    )
}

fn buffer_growth_capacity(required_bytes: usize) -> usize {
    required_bytes
        .checked_next_power_of_two()
        .unwrap_or(required_bytes)
        .max(256)
}

fn validate_preload_allocation(
    buffer: &'static str,
    required: usize,
    current: usize,
    limit: u64,
) -> Result<(), PathPreloadUploadError> {
    if required > current && buffer_growth_capacity(required) as u64 > limit {
        return Err(PathPreloadUploadError::BufferLimit {
            buffer,
            requested: buffer_growth_capacity(required),
            limit,
        });
    }
    Ok(())
}

fn ensure_capacity_with_usage(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    capacity_bytes: &mut usize,
    required_bytes: usize,
    label: &str,
    usage: wgpu::BufferUsages,
) -> bool {
    if required_bytes == 0 || required_bytes <= *capacity_bytes {
        return false;
    }

    let new_capacity = buffer_growth_capacity(required_bytes);
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: new_capacity as wgpu::BufferAddress,
        usage,
        mapped_at_creation: false,
    });
    *capacity_bytes = new_capacity;
    true
}

fn upload_dirty<T: Pod>(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    instances: &[T],
    dirty_ranges: &[std::ops::Range<usize>],
    force_full_upload: bool,
    buffer_name: &'static str,
    trace: &mut Option<&mut UploadWriteTrace<'_>>,
) -> usize {
    if instances.is_empty() {
        return 0;
    }
    if force_full_upload {
        let bytes = bytemuck::cast_slice(instances);
        record_upload_write(trace, buffer_name, 0..instances.len(), 0, bytes);
        queue.write_buffer(buffer, 0, bytes);
        return bytes.len();
    }

    let stride = size_of::<T>();
    let mut bytes_uploaded = 0;
    for range in dirty_ranges {
        let bytes = bytemuck::cast_slice(&instances[range.clone()]);
        let byte_offset = (range.start * stride) as wgpu::BufferAddress;
        record_upload_write(trace, buffer_name, range.clone(), byte_offset, bytes);
        queue.write_buffer(buffer, byte_offset, bytes);
        bytes_uploaded += bytes.len();
    }
    bytes_uploaded
}

fn record_upload_write(
    trace: &mut Option<&mut UploadWriteTrace<'_>>,
    buffer: &'static str,
    instance_range: std::ops::Range<usize>,
    byte_offset: wgpu::BufferAddress,
    bytes: &[u8],
) {
    if let Some(trace) = trace.as_deref_mut() {
        trace(buffer, instance_range, byte_offset, bytes);
    }
}

pub(crate) fn push_upload_write(
    writes: &mut Vec<UploadWrite>,
    buffer: &'static str,
    instance_range: std::ops::Range<usize>,
    byte_offset: wgpu::BufferAddress,
    bytes: &[u8],
) {
    writes.push(UploadWrite {
        buffer,
        instance_range,
        byte_offset,
        byte_length: bytes.len(),
        payload_hash: payload_hash(bytes),
    });
}

fn payload_hash(bytes: &[u8]) -> u64 {
    // FNV-1a is sufficient here: this is a compact diagnostic fingerprint, not
    // a security primitive or a content-addressing identity.
    let mut hash = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    #[test]
    fn preload_allocation_checks_actual_rounded_device_capacity() {
        assert!(super::validate_preload_allocation("vertices", 513, 0, 1000).is_err());
        assert!(super::validate_preload_allocation("vertices", 512, 0, 1000).is_ok());
        assert!(super::validate_preload_allocation("indices", 1, 0, 255).is_err());
        assert!(super::validate_preload_allocation("indices", 0, 0, 0).is_ok());
    }

    use noon_core::{GeometryRef, ObjectId, Style, Transform2D, VectorPath};
    use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

    use crate::FramePreparer;

    use super::*;

    #[test]
    fn compact_path_uploads_follow_only_dirty_vertex_ranges() {
        let mut compact = vec![
            CompactPathVertex {
                position: [9.0, 9.0],
                target_position: [9.0, 9.0],
                surface: 9,
            };
            3
        ];
        let source = (0..3)
            .map(|value| PathVertex {
                position: [value as f32, 0.0],
                target_position: [value as f32, 1.0],
                surface: value,
                polygon: [[[0.0; 2]; 2]; 3],
            })
            .collect::<Vec<_>>();
        let dirty_range = 1..2;
        update_compact_path_vertices(&mut compact, &source, std::slice::from_ref(&dirty_range));
        assert_eq!(compact[0].surface, 9);
        assert_eq!(compact[1].surface, 1);
        assert_eq!(compact[2].surface, 9);
    }

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    fn test_frame() -> FrameState {
        FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(1),
                    content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.25)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(3),
                    content: noon_core::ObjectContentRef::Geometry(GeometryRef::line(
                        Vec2::new(-0.5, 0.0),
                        Vec2::new(0.5, 0.0),
                    )),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(2),
                    content: noon_core::ObjectContentRef::Geometry(GeometryRef::rectangle(
                        0.5, 0.25,
                    )),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true; 3],
            reveals: vec![1.0; 3],
            morphs: vec![0.0; 3],
            render_geometries: vec![None; 3],
            render_transforms: vec![None; 3],
        }
    }

    fn test_frame_with_path() -> FrameState {
        let path = VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.5))
            .quadratic_to(Vec2::new(0.0, 0.75), Vec2::new(0.5, -0.5))
            .close();
        FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    z_index: 0.0,
                    id: ObjectId::new(1),
                    content: noon_core::ObjectContentRef::Geometry(GeometryRef::path(path)),
                    spatial: None,
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style {
                        stroke: Some(noon_core::Color::WHITE),
                        stroke_width: 0.1,
                        stroke_width_mode: Default::default(),
                        stroke_join: noon_core::StrokeJoin::Round,
                        stroke_cap: noon_core::StrokeCap::Round,
                        ..Style::default()
                    },
                    appearance: 1.0,
                },
                FrameObjectState {
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(2),
                    content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.2)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true; 2],
            reveals: vec![1.0; 2],
            morphs: vec![0.0; 2],
            render_geometries: vec![None; 2],
            render_transforms: vec![None; 2],
        }
    }

    #[test]
    fn analytic_only_rendering_avoids_multisampling_but_visible_paths_keep_it() {
        assert_eq!(ordered_render_sample_count(&[], false), 1);
        assert_eq!(ordered_render_sample_count(&[], true), PATH_SAMPLE_COUNT);
        assert_eq!(
            ordered_render_sample_count(
                &[PathBatch {
                    index_range: 0..0,
                    instance_range: 0..1,
                    polygon_coverage: false,
                }],
                false
            ),
            1
        );
        assert_eq!(
            ordered_render_sample_count(
                &[PathBatch {
                    index_range: 0..3,
                    instance_range: 0..1,
                    polygon_coverage: false,
                }],
                false
            ),
            PATH_SAMPLE_COUNT
        );
    }

    #[test]
    fn camera_rejects_invalid_world_size() {
        assert_eq!(
            Camera2D::new(Vec2::ZERO, Vec2::new(0.0, 2.0)),
            Err(CameraError::InvalidWorldSize)
        );
        assert_eq!(
            Camera2D::new(Vec2::ZERO, Vec2::new(f32::NAN, 2.0)),
            Err(CameraError::InvalidWorldSize)
        );
    }

    #[test]
    fn camera_maps_world_size_to_clip_scale() {
        assert_eq!(size_of::<CameraUniform>(), 32);
        let camera =
            Camera2D::new(Vec2::new(3.0, -2.0), Vec2::new(16.0, 9.0)).expect("valid camera");
        let uniform = camera.uniform([1_920, 1_080]);
        assert_eq!(uniform.center, [3.0, -2.0]);
        assert_eq!(uniform.clip_scale, [0.125, 2.0 / 9.0]);
        assert_eq!(uniform.viewport_size, [1_920.0, 1_080.0]);
        assert_eq!(uniform.padding, [0.0; 2]);
    }

    #[test]
    fn instance_vertex_layout_matches_packed_struct() {
        let layout = analytic_instance_layout();
        assert_eq!(layout.array_stride, 88);
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        assert_eq!(layout.attributes.len(), 8);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[3].offset, 72);
        assert_eq!(layout.attributes[7].offset, 64);

        let line_layout = line_instance_layout();
        assert_eq!(line_layout.array_stride, 88);
        assert_eq!(line_layout.attributes.len(), 10);
        assert_eq!(line_layout.attributes[3].offset, 72);
        assert_eq!(line_layout.attributes[8].offset, 80);
        assert_eq!(line_layout.attributes[8].shader_location, 9);
        assert_eq!(line_layout.attributes[9].offset, 20);
        assert_eq!(line_layout.attributes[9].shader_location, 10);

        let path_vertex_layout = path_vertex_layout();
        assert_eq!(path_vertex_layout.array_stride, 20);
        assert_eq!(path_vertex_layout.step_mode, wgpu::VertexStepMode::Vertex);
        assert_eq!(path_vertex_layout.attributes.len(), 3);
        assert_eq!(
            path_vertex_layout.attributes[1].format,
            wgpu::VertexFormat::Float32x2
        );
        assert_eq!(
            path_vertex_layout.attributes[2].format,
            wgpu::VertexFormat::Uint32
        );
        let triangle_layout = polygon_path_vertex_layout();
        assert_eq!(triangle_layout.array_stride, 68);
        assert_eq!(triangle_layout.attributes.len(), 6);
        assert_eq!(triangle_layout.attributes[3].offset, 20);
        assert_eq!(triangle_layout.attributes[5].offset, 52);
        assert_eq!(triangle_layout.attributes[3].shader_location, 11);
        assert_eq!(triangle_layout.attributes[5].shader_location, 13);

        let path_instance_layout = path_instance_layout();
        assert_eq!(path_instance_layout.array_stride, 80);
        assert_eq!(
            path_instance_layout.step_mode,
            wgpu::VertexStepMode::Instance
        );
        assert_eq!(path_instance_layout.attributes.len(), 8);
        assert_eq!(path_instance_layout.attributes[0].shader_location, 3);
        assert_eq!(path_instance_layout.attributes[6].shader_location, 9);
        assert_eq!(path_instance_layout.attributes[7].shader_location, 10);
    }

    #[test]
    fn analytic_shader_uses_derivative_based_edge_coverage() {
        let shader = include_str!("../analytic.wgsl");
        assert!(shader.contains("fwidth(signed_distance)"));
        assert!(shader.contains("smoothstep(-half_width, half_width, signed_distance)"));
        assert!(shader.contains("local_units_per_pixel"));
        assert!(shader.contains("rectangle_signed_distance"));
        assert!(shader.contains("capsule_signed_distance"));
        assert!(shader.contains("source_over(stroke_layer, fill_layer)"));
        assert!(
            shader
                .find("let inner_stroke_coverage =")
                .expect("stroke coverage")
                < shader.find("if has_stroke").expect("stroke branch"),
            "fragment derivatives must run before non-uniform stroke control flow"
        );
        assert!(!shader.contains("if distance > 1.0"));
        assert!(!shader.contains("let stroke_region ="));
    }

    #[test]
    fn noop_device_validates_pipelines_camera_upload_and_draw_encoding() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        let camera =
            Camera2D::new(Vec2::new(1.0, -1.0), Vec2::new(16.0, 9.0)).expect("valid camera");
        renderer.set_viewport(&device, &queue, 64, 64);
        renderer.set_camera(&queue, camera);
        assert_eq!(renderer.camera(), camera);
        assert_eq!(renderer.viewport_size(), [64, 64]);

        let mut frame = test_frame();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);

        let first_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(first_upload.buffer_reallocations, 3);
        assert_eq!(
            first_upload.bytes_uploaded,
            size_of::<CircleInstance>()
                + size_of::<RectangleInstance>()
                + size_of::<LineInstance>()
        );
        assert!(renderer.circle_capacity_bytes() >= size_of::<CircleInstance>());
        assert!(renderer.rectangle_capacity_bytes() >= size_of::<RectangleInstance>());
        assert!(renderer.line_capacity_bytes() >= size_of::<LineInstance>());

        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::default());
        let second_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(second_upload.buffer_reallocations, 0);
        assert_eq!(second_upload.bytes_uploaded, 0);

        frame.objects[0].transform.translation = Vec2::new(0.25, -0.5);
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        let partial_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(partial_upload.buffer_reallocations, 0);
        assert_eq!(partial_upload.bytes_uploaded, size_of::<CircleInstance>());

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon noop render target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
        queue.submit(Some(encoder.finish()));

        assert_eq!(draw.draw_calls, 3);
        assert_eq!(draw.instances_drawn, 3);
    }

    #[test]
    fn viewport_updates_reuse_targets_until_clamped_dimensions_change() {
        const WEBGL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new_with_output_transfer(
            &device,
            &queue,
            WEBGL_FORMAT,
            OutputTransfer::BrowserWebGlSrgb,
        );
        let camera =
            Camera2D::new(Vec2::new(1.0, -1.0), Vec2::new(16.0, 9.0)).expect("valid camera");
        renderer.set_camera(&queue, camera);

        let initial_msaa_target = renderer.path_msaa_texture.clone();
        let initial_presentation_target = renderer
            .presentation
            .scene_target_for_tests()
            .expect("WebGL presentation target")
            .clone();
        renderer.set_viewport(&device, &queue, 0, 0);
        assert_eq!(renderer.viewport_size(), [1, 1]);
        assert_eq!(renderer.path_msaa_texture, initial_msaa_target);
        assert_eq!(
            renderer.presentation.scene_target_for_tests(),
            Some(&initial_presentation_target)
        );

        renderer.set_viewport(&device, &queue, 96, 54);
        let msaa_target = renderer.path_msaa_texture.clone();
        let presentation_target = renderer
            .presentation
            .scene_target_for_tests()
            .expect("WebGL presentation target")
            .clone();
        renderer.set_viewport(&device, &queue, 96, 54);
        assert_eq!(renderer.camera(), camera);
        assert_eq!(renderer.path_msaa_texture, msaa_target);
        assert_eq!(
            renderer.presentation.scene_target_for_tests(),
            Some(&presentation_target)
        );

        renderer.set_viewport(&device, &queue, 192, 108);
        assert_eq!(renderer.viewport_size(), [192, 108]);
        assert_ne!(renderer.path_msaa_texture, msaa_target);
        assert_ne!(
            renderer.presentation.scene_target_for_tests(),
            Some(&presentation_target)
        );
    }

    #[test]
    fn noop_device_validates_webgl_presentation_transfer() {
        const WEBGL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new_with_output_transfer(
            &device,
            &queue,
            WEBGL_FORMAT,
            OutputTransfer::BrowserWebGlSrgb,
        );
        renderer.set_viewport(&device, &queue, 64, 64);
        assert_eq!(renderer.target_format, WEBGL_FORMAT);

        let frame = test_frame();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        renderer.upload(&device, &queue, &prepared);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon WebGL presentation noop target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: WEBGL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
        queue.submit(Some(encoder.finish()));

        assert_eq!(draw.draw_calls, 3);
        assert_eq!(draw.instances_drawn, 3);
    }

    #[test]
    fn noop_device_validates_timestamp_profiled_draw_encoding() {
        let descriptor = wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        };
        let (device, queue) = wgpu::Device::noop(&descriptor);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, 64, 64);

        let frame = test_frame_with_path();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        renderer.upload(&device, &queue, &prepared);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon profiled noop render target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("Noon noop timestamp query set"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon noop timestamp resolve buffer"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode_profiled(
            &mut encoder,
            &view,
            &prepared,
            wgpu::Color::BLACK,
            &query_set,
        );
        encoder.resolve_query_set(&query_set, 0..2, &resolve_buffer, 0);
        queue.submit(Some(encoder.finish()));

        assert_eq!(draw.draw_calls, 2);
        assert_eq!(draw.instances_drawn, 2);
    }

    #[test]
    fn noop_device_validates_multisampled_path_and_analytic_passes() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, 64, 64);
        let frame = test_frame_with_path();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);

        let upload = renderer.upload(&device, &queue, &prepared);
        // The compact ordinary-path stream is a distinct retained GPU buffer.
        assert_eq!(upload.buffer_reallocations, 7);
        assert!(upload.bytes_uploaded > size_of::<CircleInstance>());

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon path noop render target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
        queue.submit(Some(encoder.finish()));

        assert_eq!(draw.draw_calls, 2);
        assert_eq!(draw.instances_drawn, 2);

        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::default());
        let unchanged_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(unchanged_upload.buffer_reallocations, 0);
        assert_eq!(unchanged_upload.bytes_uploaded, 0);
    }
}

mod retained_text;
pub use retained_text::*;

mod retained_family_reveal;
pub use retained_family_reveal::*;

mod retained_family_draw_border_then_fill;
pub use retained_family_draw_border_then_fill::*;

mod retained_family_reveal_scene;
pub use retained_family_reveal_scene::*;
