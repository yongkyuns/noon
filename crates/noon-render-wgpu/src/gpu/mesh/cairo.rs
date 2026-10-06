//! Optional retained Cairo endpoint-gradient data. Ordinary mesh vertices and
//! instances keep their existing layouts; one immutable uniform is shared by
//! every Cairo instance of the resource.

use super::{create_buffer_with_data, pipeline, PipelineKind, SpatialPrepareError};
use bytemuck::{Pod, Zeroable};
use noon_core::CairoSurfaceAppearance;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct Uniform {
    points_and_spans: [[f32; 4]; 6],
}

impl Uniform {
    pub(super) fn lower(value: &CairoSurfaceAppearance) -> Result<Self, SpatialPrepareError> {
        let result = Self {
            points_and_spans: [
                value.p0,
                value.p6,
                value.span_p3_p0,
                value.span_p12_p0,
                value.span_p9_p6,
                value.span_p3_p6,
            ]
            .map(|p| [p.x as f32, p.y as f32, p.z as f32, 0.0]),
        };
        if result
            .points_and_spans
            .iter()
            .flatten()
            .all(|v| v.is_finite())
        {
            Ok(result)
        } else {
            Err(SpatialPrepareError::UnrepresentableVertex)
        }
    }
}

#[derive(Debug)]
pub(super) struct Pipelines {
    appearance_layout: wgpu::BindGroupLayout,
    opaque: [wgpu::RenderPipeline; 2],
    transparent: [wgpu::RenderPipeline; 2],
    boundary: [wgpu::RenderPipeline; 2],
}

impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let appearance_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon retained Cairo face appearance"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon Cairo face pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&appearance_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon retained Cairo face shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../mesh.wgsl"),
                    "\n",
                    include_str!("cairo.wgsl")
                )
                .into(),
            ),
        });
        let make = |transparent, kind| {
            [1, 4].map(|samples| {
                pipeline(device, &layout, &shader, format, samples, transparent, kind)
            })
        };
        Self {
            appearance_layout,
            opaque: make(false, PipelineKind::CairoMesh),
            transparent: make(true, PipelineKind::CairoMesh),
            boundary: make(true, PipelineKind::CairoBoundary),
        }
    }

    pub(super) fn retain(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        value: Uniform,
    ) -> wgpu::BindGroup {
        let buffer = create_buffer_with_data(
            device,
            queue,
            Some("Noon immutable Cairo face geometry"),
            bytemuck::bytes_of(&value),
            wgpu::BufferUsages::UNIFORM,
        );
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon immutable Cairo face geometry"),
            layout: &self.appearance_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    pub(super) fn select(
        &self,
        sample_count: u32,
        transparent: bool,
        boundary: bool,
    ) -> &wgpu::RenderPipeline {
        let index = usize::from(sample_count != 1);
        if boundary {
            &self.boundary[index]
        } else if transparent {
            &self.transparent[index]
        } else {
            &self.opaque[index]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::SemanticVec3;

    #[test]
    fn optional_appearance_has_a_bounded_uniform_and_checks_gpu_precision() {
        let mut value = CairoSurfaceAppearance {
            p0: SemanticVec3::ZERO,
            p6: SemanticVec3::new(1., 1., 0.),
            span_p3_p0: SemanticVec3::new(1., 0., 0.),
            span_p12_p0: SemanticVec3::new(0., 1., 0.),
            span_p9_p6: SemanticVec3::new(-1., 0., 0.),
            span_p3_p6: SemanticVec3::new(0., -1., 0.),
        };
        assert_eq!(std::mem::size_of::<Uniform>(), 96);
        assert_eq!(std::mem::size_of::<super::super::Vertex>(), 24);
        assert_eq!(std::mem::size_of::<super::super::Instance>(), 128);
        let uniform = Uniform::lower(&value).unwrap();
        assert_eq!(uniform.points_and_spans[1], [1., 1., 0., 0.]);
        value.p6.x = f64::MAX;
        assert_eq!(
            Uniform::lower(&value),
            Err(SpatialPrepareError::UnrepresentableVertex)
        );
    }
}
