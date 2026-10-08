//! Optional retained Cairo endpoint-gradient data. Ordinary mesh vertices and
//! instances keep their existing layouts; one immutable uniform is shared by
//! every Cairo instance of the resource.

use super::{create_buffer_with_data, pipeline, PipelineKind, SpatialPrepareError};
use bytemuck::{Pod, Zeroable};
use noon_core::{CairoSurfaceAppearance, SemanticVec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct Uniform {
    points_and_spans: [[f32; 4]; 6],
    // W is the sampled perimeter count; its sign selects proven planar support.
    center: [f32; 4],
    perimeter: [[f32; 4]; 16],
}

impl Uniform {
    pub(super) fn lower(
        value: &CairoSurfaceAppearance,
        positions: &[SemanticVec3],
        vertices: &[super::Vertex],
        planar_proxy: bool,
    ) -> Result<Self, SpatialPrepareError> {
        let center = if value.boundary_controls.is_some() {
            let [x, y, z] = vertices
                .first()
                .ok_or(SpatialPrepareError::UnrepresentableVertex)?
                .position;
            SemanticVec3::new(f64::from(x), f64::from(y), f64::from(z))
        } else {
            super::boundary::face_center(positions)
        };
        let mut perimeter = [[0.0; 4]; 16];
        let count = if value.boundary_controls.is_some() {
            let points = vertices
                .get(1..)
                .filter(|points| (3..=perimeter.len()).contains(&points.len()))
                .ok_or(SpatialPrepareError::UnrepresentableVertex)?;
            for (output, vertex) in perimeter.iter_mut().zip(points) {
                let [x, y, z] = vertex.position;
                *output = [x, y, z, 1.0];
            }
            points.len()
        } else {
            0
        };
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
            center: [
                center.x as f32,
                center.y as f32,
                center.z as f32,
                if planar_proxy {
                    -(count as f32)
                } else {
                    count as f32
                },
            ],
            perimeter,
        };
        if result
            .points_and_spans
            .iter()
            .flatten()
            .chain(result.center.iter())
            .chain(result.perimeter.iter().flatten())
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
                    include_str!("../spatial_math.wgsl"),
                    "\n",
                    include_str!("../cairo_lighting.wgsl"),
                    "\n",
                    include_str!("../../polygon_coverage.wgsl"),
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
            boundary_controls: None,
        };
        assert_eq!(std::mem::size_of::<Uniform>(), 368);
        assert_eq!(std::mem::size_of::<super::super::Vertex>(), 24);
        assert_eq!(std::mem::size_of::<super::super::Instance>(), 128);
        let positions = [SemanticVec3::ZERO, SemanticVec3::new(1., 1., 0.)];
        let uniform = Uniform::lower(&value, &positions, &[], false).unwrap();
        assert_eq!(uniform.points_and_spans[1], [1., 1., 0., 0.]);
        value.boundary_controls = Some([[SemanticVec3::ZERO; 2]; 4]);
        assert_eq!(
            Uniform::lower(&value, &positions, &[], false),
            Err(SpatialPrepareError::UnrepresentableVertex)
        );
        value.boundary_controls = None;
        value.p6.x = f64::MAX;
        assert_eq!(
            Uniform::lower(&value, &positions, &[], false),
            Err(SpatialPrepareError::UnrepresentableVertex)
        );
    }
}
