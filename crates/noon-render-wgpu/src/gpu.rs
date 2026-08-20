use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use noon_core::Vec2;
use wgpu::util::DeviceExt;

use crate::{
    CircleInstance, LineInstance, PathInstance, PathVertex, PreparedFrame, RectangleInstance,
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

const LINE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 9] = [
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
];

const PATH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32];
const PATH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = [
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
];

struct AnalyticPipelineDescriptor {
    vertex_entry: &'static str,
    fragment_entry: &'static str,
    label: &'static str,
    instance_layout: wgpu::VertexBufferLayout<'static>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadStats {
    pub bytes_uploaded: usize,
    pub buffer_reallocations: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    pub draw_calls: usize,
    pub instances_drawn: usize,
}

#[derive(Debug)]
pub struct GpuRenderer {
    circle_pipeline: wgpu::RenderPipeline,
    rectangle_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    path_pipeline: wgpu::RenderPipeline,
    quad_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera: Camera2D,
    viewport_size: [u32; 2],
    target_format: wgpu::TextureFormat,
    path_msaa_texture: wgpu::Texture,
    path_msaa_view: wgpu::TextureView,
    circle_buffer: wgpu::Buffer,
    rectangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    path_vertex_buffer: wgpu::Buffer,
    path_index_buffer: wgpu::Buffer,
    path_instance_buffer: wgpu::Buffer,
    circle_capacity_bytes: usize,
    rectangle_capacity_bytes: usize,
    line_capacity_bytes: usize,
    path_vertex_capacity_bytes: usize,
    path_index_capacity_bytes: usize,
    path_instance_capacity_bytes: usize,
}

impl GpuRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
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
        let camera_uniform = camera.uniform(viewport_size);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Noon camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
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

        let shader = device.create_shader_module(wgpu::include_wgsl!("analytic.wgsl"));
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
            AnalyticPipelineDescriptor {
                vertex_entry: "vs_line",
                fragment_entry: "fs_line",
                label: "Noon line pipeline",
                instance_layout: line_instance_layout(),
            },
        );
        let path_shader = device.create_shader_module(wgpu::include_wgsl!("path.wgsl"));
        let path_pipeline =
            create_path_pipeline(device, &pipeline_layout, &path_shader, target_format);
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Noon unit quad"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let circle_buffer = empty_instance_buffer(device, "Noon circle instances");
        let rectangle_buffer = empty_instance_buffer(device, "Noon rectangle instances");
        let line_buffer = empty_instance_buffer(device, "Noon line instances");
        let path_vertex_buffer = empty_buffer(
            device,
            "Noon path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let path_index_buffer = empty_buffer(
            device,
            "Noon path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let path_instance_buffer = empty_instance_buffer(device, "Noon path instances");
        let (path_msaa_texture, path_msaa_view) =
            create_path_msaa_target(device, target_format, viewport_size);

        Self {
            circle_pipeline,
            rectangle_pipeline,
            line_pipeline,
            path_pipeline,
            quad_buffer,
            camera_buffer,
            camera_bind_group,
            camera,
            viewport_size,
            target_format,
            path_msaa_texture,
            path_msaa_view,
            circle_buffer,
            rectangle_buffer,
            line_buffer,
            path_vertex_buffer,
            path_index_buffer,
            path_instance_buffer,
            circle_capacity_bytes: 0,
            rectangle_capacity_bytes: 0,
            line_capacity_bytes: 0,
            path_vertex_capacity_bytes: 0,
            path_index_capacity_bytes: 0,
            path_instance_capacity_bytes: 0,
        }
    }

    pub fn set_camera(&mut self, queue: &wgpu::Queue, camera: Camera2D) {
        self.camera = camera;
        self.write_camera_uniform(queue);
    }

    pub fn set_viewport(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) {
        self.viewport_size = [width.max(1), height.max(1)];
        (self.path_msaa_texture, self.path_msaa_view) =
            create_path_msaa_target(device, self.target_format, self.viewport_size);
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

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
    ) -> UploadStats {
        let circle_bytes = std::mem::size_of_val(prepared.circles);
        let rectangle_bytes = std::mem::size_of_val(prepared.rectangles);
        let line_bytes = std::mem::size_of_val(prepared.lines);
        let path_vertex_bytes = std::mem::size_of_val(prepared.path_vertices);
        let path_index_bytes = std::mem::size_of_val(prepared.path_indices);
        let path_instance_bytes = std::mem::size_of_val(prepared.paths);
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
        buffer_reallocations += usize::from(path_vertex_reallocated);
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

        let bytes_uploaded = upload_dirty(
            queue,
            &self.circle_buffer,
            prepared.circles,
            prepared.circle_dirty_ranges,
            circle_reallocated,
        ) + upload_dirty(
            queue,
            &self.rectangle_buffer,
            prepared.rectangles,
            prepared.rectangle_dirty_ranges,
            rectangle_reallocated,
        ) + upload_dirty(
            queue,
            &self.line_buffer,
            prepared.lines,
            prepared.line_dirty_ranges,
            line_reallocated,
        ) + upload_full_if(
            queue,
            &self.path_vertex_buffer,
            prepared.path_vertices,
            prepared.path_geometry_dirty || path_vertex_reallocated,
        ) + upload_full_if(
            queue,
            &self.path_index_buffer,
            prepared.path_indices,
            prepared.path_geometry_dirty || path_index_reallocated,
        ) + upload_dirty(
            queue,
            &self.path_instance_buffer,
            prepared.paths,
            prepared.path_dirty_ranges,
            path_instance_reallocated,
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
        self.encode_inner(encoder, view, prepared, clear_color, None)
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
        self.encode_inner(encoder, view, prepared, clear_color, Some(query_set))
    }

    fn encode_inner(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
        query_set: Option<&wgpu::QuerySet>,
    ) -> DrawStats {
        let has_paths = prepared
            .path_batches
            .iter()
            .any(|batch| !batch.index_range.is_empty());
        let has_analytics = !prepared.circles.is_empty()
            || !prepared.rectangles.is_empty()
            || !prepared.lines.is_empty();
        let mut stats = DrawStats::default();

        if has_paths {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &self.path_msaa_view,
                depth_slice: None,
                resolve_target: Some(view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Discard,
                },
            })];
            let timestamp_writes = query_set.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: (!has_analytics).then_some(1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon multisampled path render pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            add_draw_stats(&mut stats, self.draw_paths(&mut pass, prepared));
        }

        if has_analytics || !has_paths {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if has_paths {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(clear_color)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })];
            let timestamp_writes = query_set.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: (!has_paths).then_some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon analytic render pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            add_draw_stats(&mut stats, self.draw(&mut pass, prepared));
        }
        stats
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));

        if !prepared.circles.is_empty() {
            let count = u32::try_from(prepared.circles.len())
                .expect("circle instance count exceeds wgpu draw limits");
            pass.set_pipeline(&self.circle_pipeline);
            pass.set_vertex_buffer(1, self.circle_buffer.slice(..));
            pass.draw(0..6, 0..count);
            stats.draw_calls += 1;
            stats.instances_drawn += prepared.circles.len();
        }

        if !prepared.rectangles.is_empty() {
            let count = u32::try_from(prepared.rectangles.len())
                .expect("rectangle instance count exceeds wgpu draw limits");
            pass.set_pipeline(&self.rectangle_pipeline);
            pass.set_vertex_buffer(1, self.rectangle_buffer.slice(..));
            pass.draw(0..6, 0..count);
            stats.draw_calls += 1;
            stats.instances_drawn += prepared.rectangles.len();
        }

        if !prepared.lines.is_empty() {
            let count = u32::try_from(prepared.lines.len())
                .expect("line instance count exceeds wgpu draw limits");
            pass.set_pipeline(&self.line_pipeline);
            pass.set_vertex_buffer(1, self.line_buffer.slice(..));
            pass.draw(0..6, 0..count);
            stats.draw_calls += 1;
            stats.instances_drawn += prepared.lines.len();
        }

        stats
    }

    fn draw_paths<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_pipeline(&self.path_pipeline);
        pass.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.path_instance_buffer.slice(..));
        pass.set_index_buffer(self.path_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for batch in prepared
            .path_batches
            .iter()
            .filter(|batch| !batch.index_range.is_empty())
        {
            pass.draw_indexed(batch.index_range.clone(), 0, batch.instance_range.clone());
            stats.draw_calls += 1;
            stats.instances_drawn += batch.instance_range.len();
        }
        stats
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
}

fn add_draw_stats(total: &mut DrawStats, next: DrawStats) {
    total.draw_calls += next.draw_calls;
    total.instances_drawn += next.instances_drawn;
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
        array_stride: size_of::<PathVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &PATH_VERTEX_ATTRIBUTES,
    }
}

pub fn path_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<PathInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &PATH_INSTANCE_ATTRIBUTES,
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    descriptor: AnalyticPipelineDescriptor,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(descriptor.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(descriptor.vertex_entry),
            compilation_options: Default::default(),
            buffers: &[quad_vertex_layout(), descriptor.instance_layout],
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
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_path_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Noon vector path pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_path"),
            compilation_options: Default::default(),
            buffers: &[path_vertex_layout(), path_instance_layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_path"),
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

    let new_capacity = required_bytes
        .checked_next_power_of_two()
        .unwrap_or(required_bytes)
        .max(256);
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: new_capacity as wgpu::BufferAddress,
        usage,
        mapped_at_creation: false,
    });
    *capacity_bytes = new_capacity;
    true
}

fn upload_full_if<T: Pod>(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    values: &[T],
    upload: bool,
) -> usize {
    if values.is_empty() || !upload {
        return 0;
    }
    let bytes = bytemuck::cast_slice(values);
    queue.write_buffer(buffer, 0, bytes);
    bytes.len()
}

fn upload_dirty<T: Pod>(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    instances: &[T],
    dirty_ranges: &[std::ops::Range<usize>],
    force_full_upload: bool,
) -> usize {
    if instances.is_empty() {
        return 0;
    }
    if force_full_upload {
        let bytes = bytemuck::cast_slice(instances);
        queue.write_buffer(buffer, 0, bytes);
        return bytes.len();
    }

    let stride = size_of::<T>();
    let mut bytes_uploaded = 0;
    for range in dirty_ranges {
        let bytes = bytemuck::cast_slice(&instances[range.clone()]);
        queue.write_buffer(buffer, (range.start * stride) as wgpu::BufferAddress, bytes);
        bytes_uploaded += bytes.len();
    }
    bytes_uploaded
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D, VectorPath};
    use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

    use crate::FramePreparer;

    use super::*;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    fn test_frame() -> FrameState {
        FrameState {
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(1),
                    geometry: GeometryRef::circle(0.25),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                },
                FrameObjectState {
                    id: ObjectId::new(3),
                    geometry: GeometryRef::line(Vec2::new(-0.5, 0.0), Vec2::new(0.5, 0.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                },
                FrameObjectState {
                    id: ObjectId::new(2),
                    geometry: GeometryRef::rectangle(0.5, 0.25),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                },
            ],
            reveals: vec![1.0; 3],
        }
    }

    fn test_frame_with_path() -> FrameState {
        let path = VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.5))
            .quadratic_to(Vec2::new(0.0, 0.75), Vec2::new(0.5, -0.5))
            .close();
        FrameState {
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(1),
                    geometry: GeometryRef::path(path),
                    transform: Transform2D::IDENTITY,
                    style: Style {
                        stroke: Some(noon_core::Color::WHITE),
                        stroke_width: 0.1,
                        ..Style::default()
                    },
                },
                FrameObjectState {
                    id: ObjectId::new(2),
                    geometry: GeometryRef::circle(0.2),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                },
            ],
            reveals: vec![1.0; 2],
        }
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
        assert_eq!(line_layout.attributes.len(), 9);
        assert_eq!(line_layout.attributes[3].offset, 72);
        assert_eq!(line_layout.attributes[8].offset, 80);
        assert_eq!(line_layout.attributes[8].shader_location, 9);

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

        let path_instance_layout = path_instance_layout();
        assert_eq!(path_instance_layout.array_stride, 72);
        assert_eq!(
            path_instance_layout.step_mode,
            wgpu::VertexStepMode::Instance
        );
        assert_eq!(path_instance_layout.attributes.len(), 7);
        assert_eq!(path_instance_layout.attributes[0].shader_location, 3);
        assert_eq!(path_instance_layout.attributes[6].shader_location, 9);
    }

    #[test]
    fn analytic_shader_uses_derivative_based_edge_coverage() {
        let shader = include_str!("analytic.wgsl");
        assert!(shader.contains("fwidth(signed_distance)"));
        assert!(shader.contains("smoothstep(-half_width, half_width, signed_distance)"));
        assert!(shader.contains("local_units_per_pixel"));
        assert!(shader.contains("rectangle_signed_distance"));
        assert!(shader.contains("capsule_signed_distance"));
        assert!(shader.contains("mix(premultiplied(fill), premultiplied(stroke)"));
        assert!(
            shader
                .find("let stroke_coverage =")
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
        let mut renderer = GpuRenderer::new(&device, FORMAT);
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
    fn noop_device_validates_timestamp_profiled_draw_encoding() {
        let descriptor = wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        };
        let (device, queue) = wgpu::Device::noop(&descriptor);
        let mut renderer = GpuRenderer::new(&device, FORMAT);
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
        let mut renderer = GpuRenderer::new(&device, FORMAT);
        renderer.set_viewport(&device, &queue, 64, 64);
        let frame = test_frame_with_path();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);

        let upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(upload.buffer_reallocations, 4);
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
