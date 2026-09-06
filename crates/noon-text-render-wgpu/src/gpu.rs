use std::{mem::size_of, ops::Range};

use bytemuck::{Pod, Zeroable};
use noon_core::Vec2;
use noon_text_atlas::{GlyphAtlasPlane, GpuGlyphAtlas};

use crate::{GlyphQuadInstance, PreparedRetainedTextFrame, PreparedTextItem};

const GLYPH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 16,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 24,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 32,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 40,
        shader_location: 5,
    },
];

const TEXT_BLEND_STATE: wgpu::BlendState = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
const TEXT_MSAA_SAMPLE_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct TextCameraUniform {
    center: [f32; 2],
    clip_scale: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextCamera2D {
    pub center: Vec2,
    pub world_size: Vec2,
}

impl TextCamera2D {
    pub const DEFAULT: Self = Self {
        center: Vec2::ZERO,
        world_size: Vec2::new(2.0, 2.0),
    };

    pub fn new(center: Vec2, world_size: Vec2) -> Result<Self, TextCameraError> {
        if !center.x.is_finite()
            || !center.y.is_finite()
            || !world_size.x.is_finite()
            || !world_size.y.is_finite()
            || world_size.x <= 0.0
            || world_size.y <= 0.0
        {
            return Err(TextCameraError::InvalidWorldSize);
        }
        Ok(Self { center, world_size })
    }

    fn uniform(self) -> TextCameraUniform {
        TextCameraUniform {
            center: [self.center.x, self.center.y],
            clip_scale: [2.0 / self.world_size.x, 2.0 / self.world_size.y],
        }
    }
}

impl Default for TextCamera2D {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCameraError {
    InvalidWorldSize,
}

impl std::fmt::Display for TextCameraError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWorldSize => {
                formatter.write_str("text camera world size must be finite and positive")
            }
        }
    }
}

impl std::error::Error for TextCameraError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextGpuUploadStats {
    pub bytes_uploaded: usize,
    pub buffer_reallocations: usize,
}

/// Caller-owned observation of actual glyph instance buffer writes.
///
/// The byte slice is borrowed only for the duration of the call so an upper
/// renderer can reuse its existing upload-write fingerprint without this crate
/// allocating a parallel trace. Normal uploads do not install a trace.
pub type TextUploadTrace<'a> =
    dyn FnMut(GlyphAtlasPlane, Range<usize>, wgpu::BufferAddress, &[u8]) + 'a;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextGpuDrawStats {
    pub draw_calls: usize,
    pub instances_drawn: usize,
    pub deferred_items: usize,
}

impl std::ops::AddAssign for TextGpuDrawStats {
    fn add_assign(&mut self, rhs: Self) {
        self.draw_calls = self.draw_calls.saturating_add(rhs.draw_calls);
        self.instances_drawn = self.instances_drawn.saturating_add(rhs.instances_drawn);
        self.deferred_items = self.deferred_items.saturating_add(rhs.deferred_items);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextGpuDrawError {
    UnsupportedSampleCount(u32),
    MissingAtlasPlane(GlyphAtlasPlane),
    MissingAtlasPage {
        plane: GlyphAtlasPlane,
        page: u32,
    },
    /// Retained for compatibility with callers compiled against the page-identity
    /// foundation. Live page-aware bindings no longer emit this error.
    UnsupportedAtlasPage {
        plane: GlyphAtlasPlane,
        page: u32,
    },
    InstanceRangeOutOfBounds {
        plane: GlyphAtlasPlane,
        end: u32,
        available: u32,
    },
}

impl std::fmt::Display for TextGpuDrawError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSampleCount(count) => {
                write!(formatter, "text glyph pipeline does not support sample count {count}")
            }
            Self::MissingAtlasPlane(plane) => {
                write!(formatter, "{plane:?} glyph atlas texture is not resident")
            }
            Self::MissingAtlasPage { plane, page } => {
                write!(formatter, "{plane:?} glyph atlas page {page} is not resident")
            }
            Self::UnsupportedAtlasPage { plane, page } => write!(
                formatter,
                "{plane:?} glyph atlas page {page} is not supported by the current GPU binding model"
            ),
            Self::InstanceRangeOutOfBounds {
                plane,
                end,
                available,
            } => write!(
                formatter,
                "{plane:?} glyph instance range ends at {end}, but only {available} instances are uploaded"
            ),
        }
    }
}

impl std::error::Error for TextGpuDrawError {}

/// GPU submission state for atlas-backed retained text glyphs.
///
/// This object intentionally owns only the fast glyph lane. `PreparedTextItem::Vector`
/// and `PreparedTextItem::OutlineRun` are reported as deferred so the parent renderer
/// can route them through the shared path pipeline at the exact painter-order point.
pub struct TextGlyphGpuRenderer {
    mask_pipeline_single_sample: wgpu::RenderPipeline,
    color_pipeline_single_sample: wgpu::RenderPipeline,
    mask_pipeline_msaa: wgpu::RenderPipeline,
    color_pipeline_msaa: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    mask_bind_groups: Vec<wgpu::BindGroup>,
    color_bind_groups: Vec<wgpu::BindGroup>,
    mask_buffer: wgpu::Buffer,
    color_buffer: wgpu::Buffer,
    mask_capacity_bytes: usize,
    color_capacity_bytes: usize,
    mask_instances: u32,
    color_instances: u32,
    camera: TextCamera2D,
}

impl TextGlyphGpuRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        camera: TextCamera2D,
    ) -> Self {
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon text camera uniform"),
            size: size_of::<TextCameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon text camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<TextCameraUniform>() as _),
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon text camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon glyph atlas bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Noon glyph atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("glyph.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon text glyph pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let mask_pipeline_single_sample = create_glyph_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "fs_mask",
            1,
            "Noon mask glyph pipeline",
        );
        let color_pipeline_single_sample = create_glyph_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "fs_color",
            1,
            "Noon color glyph pipeline",
        );
        let mask_pipeline_msaa = create_glyph_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "fs_mask",
            TEXT_MSAA_SAMPLE_COUNT,
            "Noon mask glyph MSAA pipeline",
        );
        let color_pipeline_msaa = create_glyph_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "fs_color",
            TEXT_MSAA_SAMPLE_COUNT,
            "Noon color glyph MSAA pipeline",
        );

        Self {
            mask_pipeline_single_sample,
            color_pipeline_single_sample,
            mask_pipeline_msaa,
            color_pipeline_msaa,
            camera_buffer,
            camera_bind_group,
            atlas_layout,
            sampler,
            mask_bind_groups: Vec::new(),
            color_bind_groups: Vec::new(),
            mask_buffer: empty_instance_buffer(device, "Noon text mask glyph instances"),
            color_buffer: empty_instance_buffer(device, "Noon text color glyph instances"),
            mask_capacity_bytes: 0,
            color_capacity_bytes: 0,
            mask_instances: 0,
            color_instances: 0,
            camera,
        }
    }

    pub fn set_camera(&mut self, queue: &wgpu::Queue, camera: TextCamera2D) {
        self.camera = camera;
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera.uniform()),
        );
    }

    pub const fn camera(&self) -> TextCamera2D {
        self.camera
    }

    /// Drop texture bind groups when switching to a different persistent atlas.
    pub fn reset_atlas_bindings(&mut self) {
        self.mask_bind_groups.clear();
        self.color_bind_groups.clear();
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedTextFrame<'_>,
        atlas: &GpuGlyphAtlas,
    ) -> TextGpuUploadStats {
        self.upload_impl(device, queue, prepared, atlas, None, None, None)
    }

    /// Upload a full text frame while reporting its actual buffer writes to a
    /// caller-owned opt-in trace.
    pub fn upload_with_trace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedTextFrame<'_>,
        atlas: &GpuGlyphAtlas,
        trace: &mut TextUploadTrace<'_>,
    ) -> TextGpuUploadStats {
        self.upload_impl(device, queue, prepared, atlas, None, None, Some(trace))
    }

    /// Upload only changed instance ranges when the caller knows the GPU buffers
    /// already contain the immediately preceding prepared generation. Reallocation
    /// automatically falls back to a full-plane write because a new buffer has no
    /// prior contents to preserve.
    pub fn upload_ranges(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedTextFrame<'_>,
        atlas: &GpuGlyphAtlas,
        mask_ranges: &[Range<u32>],
        color_ranges: &[Range<u32>],
    ) -> TextGpuUploadStats {
        self.upload_impl(
            device,
            queue,
            prepared,
            atlas,
            Some(mask_ranges),
            Some(color_ranges),
            None,
        )
    }

    /// Upload dirty text ranges while reporting the actual writes to a
    /// caller-owned opt-in trace. Buffer growth still reports the required full
    /// plane write.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_ranges_with_trace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedTextFrame<'_>,
        atlas: &GpuGlyphAtlas,
        mask_ranges: &[Range<u32>],
        color_ranges: &[Range<u32>],
        trace: &mut TextUploadTrace<'_>,
    ) -> TextGpuUploadStats {
        self.upload_impl(
            device,
            queue,
            prepared,
            atlas,
            Some(mask_ranges),
            Some(color_ranges),
            Some(trace),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_impl(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedTextFrame<'_>,
        atlas: &GpuGlyphAtlas,
        mask_ranges: Option<&[Range<u32>]>,
        color_ranges: Option<&[Range<u32>]>,
        mut trace: Option<&mut TextUploadTrace<'_>>,
    ) -> TextGpuUploadStats {
        let mask_bytes = std::mem::size_of_val(prepared.mask_quads);
        let color_bytes = std::mem::size_of_val(prepared.color_quads);
        let mask_reallocated = ensure_capacity(
            device,
            &mut self.mask_buffer,
            &mut self.mask_capacity_bytes,
            mask_bytes,
            "Noon text mask glyph instances",
        );
        let color_reallocated = ensure_capacity(
            device,
            &mut self.color_buffer,
            &mut self.color_capacity_bytes,
            color_bytes,
            "Noon text color glyph instances",
        );

        let bytes_uploaded = upload_plane_instances(
            queue,
            &self.mask_buffer,
            prepared.mask_quads,
            mask_ranges,
            mask_reallocated,
            GlyphAtlasPlane::Mask,
            &mut trace,
        ) + upload_plane_instances(
            queue,
            &self.color_buffer,
            prepared.color_quads,
            color_ranges,
            color_reallocated,
            GlyphAtlasPlane::Color,
            &mut trace,
        );
        self.mask_instances = u32::try_from(prepared.mask_quads.len())
            .expect("mask glyph instance count exceeds u32 draw limits");
        self.color_instances = u32::try_from(prepared.color_quads.len())
            .expect("color glyph instance count exceeds u32 draw limits");
        self.refresh_atlas_bind_groups(device, atlas);

        TextGpuUploadStats {
            bytes_uploaded,
            buffer_reallocations: usize::from(mask_reallocated) + usize::from(color_reallocated),
        }
    }

    fn refresh_atlas_bind_groups(&mut self, device: &wgpu::Device, atlas: &GpuGlyphAtlas) {
        refresh_plane_bind_groups(
            device,
            &self.atlas_layout,
            &self.sampler,
            &mut self.mask_bind_groups,
            atlas,
            GlyphAtlasPlane::Mask,
            "Noon mask glyph atlas bind group",
        );
        refresh_plane_bind_groups(
            device,
            &self.atlas_layout,
            &self.sampler,
            &mut self.color_bind_groups,
            atlas,
            GlyphAtlasPlane::Color,
            "Noon color glyph atlas bind group",
        );
    }

    pub fn draw_item<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        item: &PreparedTextItem,
        sample_count: u32,
    ) -> Result<TextGpuDrawStats, TextGpuDrawError> {
        let PreparedTextItem::GlyphBatch {
            plane,
            page,
            instance_range,
            ..
        } = item
        else {
            return Ok(TextGpuDrawStats {
                deferred_items: 1,
                ..TextGpuDrawStats::default()
            });
        };
        if instance_range.is_empty() {
            return Ok(TextGpuDrawStats::default());
        }

        let (buffer, bind_groups, available, pipeline) = match plane {
            GlyphAtlasPlane::Mask => (
                &self.mask_buffer,
                &self.mask_bind_groups,
                self.mask_instances,
                self.pipeline(GlyphAtlasPlane::Mask, sample_count)?,
            ),
            GlyphAtlasPlane::Color => (
                &self.color_buffer,
                &self.color_bind_groups,
                self.color_instances,
                self.pipeline(GlyphAtlasPlane::Color, sample_count)?,
            ),
        };
        let page_index =
            usize::try_from(*page).map_err(|_| TextGpuDrawError::MissingAtlasPage {
                plane: *plane,
                page: *page,
            })?;
        let bind_group = bind_groups.get(page_index).ok_or({
            if *page == 0 && bind_groups.is_empty() {
                TextGpuDrawError::MissingAtlasPlane(*plane)
            } else {
                TextGpuDrawError::MissingAtlasPage {
                    plane: *plane,
                    page: *page,
                }
            }
        })?;
        if instance_range.end > available {
            return Err(TextGpuDrawError::InstanceRangeOutOfBounds {
                plane: *plane,
                end: instance_range.end,
                available,
            });
        }

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..6, instance_range.clone());
        Ok(TextGpuDrawStats {
            draw_calls: 1,
            instances_drawn: instance_range.len(),
            deferred_items: 0,
        })
    }

    pub fn draw_ordered_glyphs<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedRetainedTextFrame<'_>,
        sample_count: u32,
    ) -> Result<TextGpuDrawStats, TextGpuDrawError> {
        let mut stats = TextGpuDrawStats::default();
        for item in prepared.items {
            stats += self.draw_item(pass, item, sample_count)?;
        }
        Ok(stats)
    }

    pub const fn mask_capacity_bytes(&self) -> usize {
        self.mask_capacity_bytes
    }

    pub const fn color_capacity_bytes(&self) -> usize {
        self.color_capacity_bytes
    }

    fn pipeline(
        &self,
        plane: GlyphAtlasPlane,
        sample_count: u32,
    ) -> Result<&wgpu::RenderPipeline, TextGpuDrawError> {
        match (plane, sample_count) {
            (GlyphAtlasPlane::Mask, 1) => Ok(&self.mask_pipeline_single_sample),
            (GlyphAtlasPlane::Color, 1) => Ok(&self.color_pipeline_single_sample),
            (GlyphAtlasPlane::Mask, TEXT_MSAA_SAMPLE_COUNT) => Ok(&self.mask_pipeline_msaa),
            (GlyphAtlasPlane::Color, TEXT_MSAA_SAMPLE_COUNT) => Ok(&self.color_pipeline_msaa),
            (_, count) => Err(TextGpuDrawError::UnsupportedSampleCount(count)),
        }
    }
}

fn upload_plane_instances(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    instances: &[GlyphQuadInstance],
    ranges: Option<&[Range<u32>]>,
    reallocated: bool,
    plane: GlyphAtlasPlane,
    trace: &mut Option<&mut TextUploadTrace<'_>>,
) -> usize {
    if instances.is_empty() {
        return 0;
    }
    if reallocated || ranges.is_none() {
        let bytes = bytemuck::cast_slice(instances);
        if let Some(trace) = trace.as_deref_mut() {
            trace(plane, 0..instances.len(), 0, bytes);
        }
        queue.write_buffer(buffer, 0, bytes);
        return bytes.len();
    }

    let mut bytes_uploaded = 0;
    for range in ranges.expect("checked above") {
        if range.is_empty() {
            continue;
        }
        let start = range.start as usize;
        let end = range.end as usize;
        assert!(
            end <= instances.len(),
            "dirty glyph upload range exceeds prepared instances"
        );
        let bytes = bytemuck::cast_slice(&instances[start..end]);
        let offset = start
            .checked_mul(size_of::<GlyphQuadInstance>())
            .expect("text glyph upload offset overflow") as u64;
        if let Some(trace) = trace.as_deref_mut() {
            trace(plane, start..end, offset, bytes);
        }
        queue.write_buffer(buffer, offset, bytes);
        bytes_uploaded += bytes.len();
    }
    bytes_uploaded
}

fn refresh_plane_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    bind_groups: &mut Vec<wgpu::BindGroup>,
    atlas: &GpuGlyphAtlas,
    plane: GlyphAtlasPlane,
    label: &'static str,
) {
    let page_count = atlas.page_count(plane);
    if bind_groups.len() > page_count {
        bind_groups.truncate(page_count);
    }
    while bind_groups.len() < page_count {
        let page = u32::try_from(bind_groups.len()).expect("glyph atlas page count exceeds u32");
        let view = atlas
            .texture_view_for_page(plane, page)
            .expect("resident glyph atlas page must expose its texture view");
        bind_groups.push(create_atlas_bind_group(
            device, layout, sampler, view, label,
        ));
    }
}

fn glyph_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<GlyphQuadInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &GLYPH_INSTANCE_ATTRIBUTES,
    }
}

fn create_glyph_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    fragment_entry: &'static str,
    sample_count: u32,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_glyph"),
            compilation_options: Default::default(),
            buffers: &[Some(glyph_instance_layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(TEXT_BLEND_STATE),
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

fn create_atlas_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn empty_instance_buffer(device: &wgpu::Device, label: &'static str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn ensure_capacity(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    capacity_bytes: &mut usize,
    required_bytes: usize,
    label: &'static str,
) -> bool {
    if required_bytes <= *capacity_bytes {
        return false;
    }
    let minimum = required_bytes.max(size_of::<GlyphQuadInstance>());
    let new_capacity = minimum
        .checked_next_power_of_two()
        .expect("text glyph instance buffer capacity overflow");
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: new_capacity as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    *capacity_bytes = new_capacity;
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{FontResourceHandle, FontResourceId, TextResourceHandle, TextResourceId};
    use noon_text_atlas::{GlyphAtlasEntry, GpuGlyphAtlas};
    use noon_text_raster::{
        GlyphRaster, GlyphRasterFormat, GlyphRasterImage, GlyphRasterKey, GlyphRasterPlacement,
    };

    use super::*;

    fn raster_key(glyph_id: u16) -> GlyphRasterKey {
        GlyphRasterKey {
            font: FontResourceHandle {
                arena: 0,
                id: FontResourceId::new(0),
                version: 0,
            },
            glyph_id,
            pixel_size_bits: 32.0f32.to_bits(),
            variation_fingerprint: 0,
        }
    }

    fn mask_raster(width: u32, height: u32, value: u8) -> GlyphRaster {
        GlyphRaster::Image(GlyphRasterImage {
            format: GlyphRasterFormat::Alpha8,
            placement: GlyphRasterPlacement {
                left: 0,
                top: height as i32,
                width,
                height,
            },
            data: Arc::from(vec![value; (width * height) as usize]),
        })
    }

    fn prepared_mask_frame<'a>(
        quads: &'a [GlyphQuadInstance],
        items: &'a [PreparedTextItem],
    ) -> PreparedRetainedTextFrame<'a> {
        PreparedRetainedTextFrame {
            time: 0.0,
            mask_quads: quads,
            color_quads: &[],
            items,
            stats: Default::default(),
        }
    }

    fn test_quad() -> GlyphQuadInstance {
        GlyphQuadInstance {
            origin: [0.0, 0.0],
            axis_x: [1.0, 0.0],
            axis_y: [0.0, 1.0],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: [1.0; 4],
        }
    }

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(0),
            version: 0,
        }
    }

    fn noop_target(
        device: &wgpu::Device,
        label: &'static str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        (target, view)
    }

    #[test]
    fn noop_device_compiles_uploads_and_draws_mask_glyphs() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = GpuGlyphAtlas::new(16).unwrap();
        let key = raster_key(1);
        let raster = GlyphRaster::Image(GlyphRasterImage {
            format: GlyphRasterFormat::Alpha8,
            placement: GlyphRasterPlacement {
                left: 0,
                top: 2,
                width: 2,
                height: 2,
            },
            data: Arc::from([255, 200, 100, 0]),
        });
        let GlyphAtlasEntry::Image(image) = atlas.insert(&device, &queue, key, &raster).unwrap()
        else {
            panic!("visible mask glyph must allocate an atlas image");
        };
        let quads = [GlyphQuadInstance {
            origin: [-0.5, -0.5],
            axis_x: [1.0, 0.0],
            axis_y: [0.0, 1.0],
            uv_min: image.uv_min,
            uv_max: image.uv_max,
            color: [1.0, 1.0, 1.0, 1.0],
        }];
        let items = [PreparedTextItem::GlyphBatch {
            object_index: 0,
            text: text_handle(),
            run_index: 0,
            plane: GlyphAtlasPlane::Mask,
            page: image.page,
            instance_range: 0..1,
        }];
        let prepared = prepared_mask_frame(&quads, &items);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        let upload = renderer.upload(&device, &queue, &prepared, &atlas);
        assert_eq!(upload.bytes_uploaded, size_of::<GlyphQuadInstance>());
        assert_eq!(upload.buffer_reallocations, 1);

        let (_target, view) = noop_target(&device, "Noon text glyph noop render target");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Noon text glyph noop encoder"),
        });
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let stats = {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon text glyph noop pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            renderer
                .draw_ordered_glyphs(&mut pass, &prepared, 1)
                .unwrap()
        };
        queue.submit(Some(encoder.finish()));
        assert_eq!(stats.draw_calls, 1);
        assert_eq!(stats.instances_drawn, 1);
        assert_eq!(stats.deferred_items, 0);
    }

    #[test]
    fn noop_device_draws_from_nonzero_atlas_page() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();
        for glyph_id in 1..=2 {
            let image = mask_raster(4, 2, 100 + glyph_id as u8);
            let GlyphAtlasEntry::Image(entry) = atlas
                .insert(&device, &queue, raster_key(glyph_id), &image)
                .unwrap()
            else {
                panic!("visible mask glyph must allocate an atlas image");
            };
            assert_eq!(entry.page, 0);
        }
        let raster = mask_raster(1, 1, 255);
        let GlyphAtlasEntry::Image(page_one) = atlas
            .insert(&device, &queue, raster_key(3), &raster)
            .unwrap()
        else {
            panic!("visible mask glyph must allocate an atlas image");
        };
        assert_eq!(page_one.page, 1);
        assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);

        let quads = [GlyphQuadInstance {
            origin: [-0.5, -0.5],
            axis_x: [1.0, 0.0],
            axis_y: [0.0, 1.0],
            uv_min: page_one.uv_min,
            uv_max: page_one.uv_max,
            color: [1.0; 4],
        }];
        let items = [PreparedTextItem::GlyphBatch {
            object_index: 0,
            text: text_handle(),
            run_index: 0,
            plane: GlyphAtlasPlane::Mask,
            page: page_one.page,
            instance_range: 0..1,
        }];
        let prepared = prepared_mask_frame(&quads, &items);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        renderer.upload(&device, &queue, &prepared, &atlas);
        assert_eq!(renderer.mask_bind_groups.len(), 2);

        let (_target, view) = noop_target(&device, "Noon text page-one noop target");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let stats = {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon text page-one noop pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            renderer.draw_item(&mut pass, &items[0], 1).unwrap()
        };
        queue.submit(Some(encoder.finish()));
        assert_eq!(stats.draw_calls, 1);
        assert_eq!(stats.instances_drawn, 1);
    }

    #[test]
    fn repeated_upload_reuses_instance_capacity() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = GpuGlyphAtlas::new(16).unwrap();
        let key = raster_key(1);
        let raster = GlyphRaster::Image(GlyphRasterImage {
            format: GlyphRasterFormat::Alpha8,
            placement: GlyphRasterPlacement {
                left: 0,
                top: 1,
                width: 1,
                height: 1,
            },
            data: Arc::from([255]),
        });
        let GlyphAtlasEntry::Image(image) = atlas.insert(&device, &queue, key, &raster).unwrap()
        else {
            panic!("visible glyph must allocate an atlas image");
        };
        let quads = [GlyphQuadInstance {
            origin: [0.0, 0.0],
            axis_x: [1.0, 0.0],
            axis_y: [0.0, 1.0],
            uv_min: image.uv_min,
            uv_max: image.uv_max,
            color: [1.0; 4],
        }];
        let items = [PreparedTextItem::GlyphBatch {
            object_index: 0,
            text: text_handle(),
            run_index: 0,
            plane: GlyphAtlasPlane::Mask,
            page: image.page,
            instance_range: 0..1,
        }];
        let prepared = prepared_mask_frame(&quads, &items);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        assert_eq!(
            renderer
                .upload(&device, &queue, &prepared, &atlas)
                .buffer_reallocations,
            1
        );
        assert_eq!(
            renderer
                .upload(&device, &queue, &prepared, &atlas)
                .buffer_reallocations,
            0
        );
    }

    #[test]
    fn dirty_range_upload_writes_only_requested_instances() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let atlas = GpuGlyphAtlas::new(16).unwrap();
        let quads: [GlyphQuadInstance; 4] = std::array::from_fn(|_| test_quad());
        let prepared = prepared_mask_frame(&quads, &[]);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        renderer.upload(&device, &queue, &prepared, &atlas);

        let stats = renderer.upload_ranges(&device, &queue, &prepared, &atlas, &[1..2], &[]);
        assert_eq!(stats.buffer_reallocations, 0);
        assert_eq!(stats.bytes_uploaded, size_of::<GlyphQuadInstance>());
    }

    #[test]
    fn dirty_range_trace_reports_the_exact_written_plane_and_bytes() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let atlas = GpuGlyphAtlas::new(16).unwrap();
        let quads: [GlyphQuadInstance; 4] = std::array::from_fn(|_| test_quad());
        let prepared = prepared_mask_frame(&quads, &[]);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        renderer.upload(&device, &queue, &prepared, &atlas);

        let mut writes = Vec::new();
        let mut trace = |plane, range, offset, bytes: &[u8]| {
            writes.push((plane, range, offset, bytes.len()));
        };
        let stats = renderer.upload_ranges_with_trace(
            &device,
            &queue,
            &prepared,
            &atlas,
            &[1..3],
            &[],
            &mut trace,
        );

        let stride = size_of::<GlyphQuadInstance>();
        assert_eq!(stats.bytes_uploaded, stride * 2);
        assert_eq!(
            writes,
            vec![(GlyphAtlasPlane::Mask, 1..3, stride as u64, stride * 2)]
        );
    }

    #[test]
    fn dirty_range_upload_falls_back_to_full_plane_after_reallocation() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let atlas = GpuGlyphAtlas::new(16).unwrap();
        let quads: [GlyphQuadInstance; 4] = std::array::from_fn(|_| test_quad());
        let prepared = prepared_mask_frame(&quads, &[]);
        let mut renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );

        let mut writes = Vec::new();
        let mut trace = |plane, range, offset, bytes: &[u8]| {
            writes.push((plane, range, offset, bytes.len()));
        };
        let stats = renderer.upload_ranges_with_trace(
            &device,
            &queue,
            &prepared,
            &atlas,
            &[1..2],
            &[],
            &mut trace,
        );
        assert_eq!(stats.buffer_reallocations, 1);
        assert_eq!(
            stats.bytes_uploaded,
            quads.len() * size_of::<GlyphQuadInstance>()
        );
        assert_eq!(
            writes,
            vec![(
                GlyphAtlasPlane::Mask,
                0..quads.len(),
                0,
                quads.len() * size_of::<GlyphQuadInstance>()
            )]
        );
    }

    #[test]
    fn non_glyph_items_remain_deferred_in_painter_order() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        let item = PreparedTextItem::Vector {
            object_index: 2,
            text: text_handle(),
            vector_index: 3,
            reveal: 1.0,
            morph: 0.0,
        };

        let (_target, view) = noop_target(&device, "Noon text deferred noop render target");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let stats = {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            renderer.draw_item(&mut pass, &item, 1).unwrap()
        };
        assert_eq!(stats.draw_calls, 0);
        assert_eq!(stats.deferred_items, 1);
    }

    #[test]
    fn unsupported_sample_count_is_rejected_before_draw() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let renderer = TextGlyphGpuRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextCamera2D::DEFAULT,
        );
        assert!(matches!(
            renderer.pipeline(GlyphAtlasPlane::Mask, 2),
            Err(TextGpuDrawError::UnsupportedSampleCount(2))
        ));
    }
}
