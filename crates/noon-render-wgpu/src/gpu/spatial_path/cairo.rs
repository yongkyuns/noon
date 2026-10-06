//! Optional Cairo path inputs. Geometry stays retained; poses and family bounds
//! update the small draw uniform, while camera/light updates use shared buffers.

use super::{DrawId, PathGpuState, PathKey, SpatialPathError};
use bytemuck::{Pod, Zeroable};
use noon_compile::CompiledCairoPathAppearance;
use noon_core::SemanticVec3;
use noon_geometry::CairoPathGeometry;
use std::collections::HashMap;

#[derive(Debug)]
pub(super) struct ResidentDraw {
    pub value: Uniform,
    pub buffer: wgpu::Buffer,
    pub binding: wgpu::BindGroup,
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub geometry: HashMap<PathKey, CairoPathGeometry>,
    pub draws: HashMap<DrawId, ResidentDraw>,
    pub pipelines: Option<Pipelines>,
}

#[derive(Debug)]
pub(super) struct Pipelines {
    layout: wgpu::BindGroupLayout,
    pub world: [wgpu::RenderPipeline; 2],
}

impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        gpu: &PathGpuState,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon retained Cairo path appearance"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniform>() as u64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon Cairo path pipeline layout"),
            bind_group_layouts: &[
                Some(&gpu.camera_layout),
                Some(&gpu.fixed_camera_layout),
                Some(&layout),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon retained Cairo path shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../spatial_path.wgsl"),
                    "\n",
                    include_str!("../spatial_math.wgsl"),
                    "\n",
                    include_str!("../cairo_lighting.wgsl"),
                    "\n",
                    include_str!("cairo.wgsl"),
                    "\n",
                    include_str!("../../cairo_color.wgsl")
                )
                .into(),
            ),
        });
        let world = [1, samples].map(|samples| {
            super::spatial_path_pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                samples,
                true,
                &super::PATH_VERTEX_ATTRIBUTES,
                &super::PATH_INSTANCE_ATTRIBUTES,
                true,
            )
        });
        Self { layout, world }
    }

    pub(super) fn retain(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        value: Uniform,
    ) -> ResidentDraw {
        let buffer = super::create_buffer_with_data(
            device,
            queue,
            Some("Noon retained Cairo path draw inputs"),
            bytemuck::bytes_of(&value),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon retained Cairo path draw inputs"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        ResidentDraw {
            value,
            buffer,
            binding,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct Uniform {
    points_and_spans: [[f32; 4]; 6],
    gradient_start: [f32; 4],
    gradient_end: [f32; 4],
    metadata: [f32; 4],
}

impl Uniform {
    pub(super) fn lower(
        geometry: CairoPathGeometry,
        appearance: &CompiledCairoPathAppearance,
    ) -> Result<Self, SpatialPathError> {
        let c = geometry.corners;
        let point = |v: SemanticVec3| [v.x as f32, v.y as f32, v.z as f32, 0.];
        let mut result = Self {
            points_and_spans: [
                c.p0,
                c.p6,
                c.span_p3_p0,
                c.span_p12_p0,
                c.span_p9_p6,
                c.span_p3_p6,
            ]
            .map(point),
            gradient_start: [0.; 4],
            gradient_end: [0.; 4],
            metadata: [
                appearance.sheen_factor as f32,
                if geometry.world_up_normal { 1. } else { 0. },
                0.,
                0.,
            ],
        };
        if let Some(direction) = appearance.gradient_direction {
            let bounds = appearance
                .world_family_bounds
                .ok_or(SpatialPathError::MissingAnchor)?;
            let critical = |d: f64, min: f64, max: f64| {
                if d > 0. {
                    max
                } else if d < 0. {
                    min
                } else {
                    min * 0.5 + max * 0.5
                }
            };
            result.gradient_start = point(SemanticVec3::new(
                critical(-direction.x, bounds.min.x, bounds.max.x),
                critical(-direction.y, bounds.min.y, bounds.max.y),
                critical(-direction.z, bounds.min.z, bounds.max.z),
            ));
            result.gradient_end = point(SemanticVec3::new(
                critical(direction.x, bounds.min.x, bounds.max.x),
                critical(direction.y, bounds.min.y, bounds.max.y),
                critical(direction.z, bounds.min.z, bounds.max.z),
            ));
            result.metadata[2] = 1.;
        }
        if result
            .points_and_spans
            .iter()
            .flatten()
            .chain(result.gradient_start.iter())
            .chain(result.gradient_end.iter())
            .chain(result.metadata.iter())
            .any(|v| !v.is_finite())
        {
            return Err(SpatialPathError::UnrepresentableVertex);
        }
        Ok(result)
    }
}
