use std::mem::size_of;

use wgpu::util::DeviceExt;

use crate::{CircleInstance, PreparedFrame, RectangleInstance};

const QUAD_VERTICES: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 1] =
    wgpu::vertex_attr_array![0 => Float32x2];

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
    quad_buffer: wgpu::Buffer,
    circle_buffer: wgpu::Buffer,
    rectangle_buffer: wgpu::Buffer,
    circle_capacity_bytes: usize,
    rectangle_capacity_bytes: usize,
}

impl GpuRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        assert_eq!(
            size_of::<CircleInstance>(),
            size_of::<RectangleInstance>(),
            "analytic instance layouts must stay identical"
        );

        let shader = device.create_shader_module(wgpu::include_wgsl!("analytic.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon analytic pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let circle_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "vs_circle",
            "fs_circle",
            "Noon circle pipeline",
        );
        let rectangle_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            "vs_rectangle",
            "fs_rectangle",
            "Noon rectangle pipeline",
        );
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Noon unit quad"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let circle_buffer = empty_instance_buffer(device, "Noon circle instances");
        let rectangle_buffer = empty_instance_buffer(device, "Noon rectangle instances");

        Self {
            circle_pipeline,
            rectangle_pipeline,
            quad_buffer,
            circle_buffer,
            rectangle_buffer,
            circle_capacity_bytes: 0,
            rectangle_capacity_bytes: 0,
        }
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedFrame<'_>,
    ) -> UploadStats {
        let circle_bytes = bytemuck::cast_slice(prepared.circles);
        let rectangle_bytes = bytemuck::cast_slice(prepared.rectangles);
        let mut buffer_reallocations = 0;

        if ensure_capacity(
            device,
            &mut self.circle_buffer,
            &mut self.circle_capacity_bytes,
            circle_bytes.len(),
            "Noon circle instances",
        ) {
            buffer_reallocations += 1;
        }
        if ensure_capacity(
            device,
            &mut self.rectangle_buffer,
            &mut self.rectangle_capacity_bytes,
            rectangle_bytes.len(),
            "Noon rectangle instances",
        ) {
            buffer_reallocations += 1;
        }

        if !circle_bytes.is_empty() {
            queue.write_buffer(&self.circle_buffer, 0, circle_bytes);
        }
        if !rectangle_bytes.is_empty() {
            queue.write_buffer(&self.rectangle_buffer, 0, rectangle_bytes);
        }

        UploadStats {
            bytes_uploaded: circle_bytes.len() + rectangle_bytes.len(),
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
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon analytic render pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw(&mut pass, prepared)
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
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

        stats
    }

    pub const fn circle_capacity_bytes(&self) -> usize {
        self.circle_capacity_bytes
    }

    pub const fn rectangle_capacity_bytes(&self) -> usize {
        self.rectangle_capacity_bytes
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

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    vertex_entry: &str,
    fragment_entry: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: Default::default(),
            buffers: &[quad_vertex_layout(), analytic_instance_layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

fn empty_instance_buffer(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
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
    label: &str,
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
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    *capacity_bytes = new_capacity;
    true
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
    use noon_runtime::{FrameObjectState, FrameState};

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
                    id: ObjectId::new(2),
                    geometry: GeometryRef::rectangle(0.5, 0.25),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                },
            ],
        }
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
    }

    #[test]
    fn noop_device_validates_pipelines_upload_and_draw_encoding() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, FORMAT);
        let frame = test_frame();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);

        let first_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(first_upload.buffer_reallocations, 2);
        assert_eq!(
            first_upload.bytes_uploaded,
            size_of::<CircleInstance>() + size_of::<RectangleInstance>()
        );
        assert!(renderer.circle_capacity_bytes() >= size_of::<CircleInstance>());
        assert!(renderer.rectangle_capacity_bytes() >= size_of::<RectangleInstance>());

        let second_upload = renderer.upload(&device, &queue, &prepared);
        assert_eq!(second_upload.buffer_reallocations, 0);

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

        assert_eq!(draw.draw_calls, 2);
        assert_eq!(draw.instances_drawn, 2);
    }
}
