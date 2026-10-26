//! One shared program and one retained uniform per continuous painted row.
use std::{collections::HashMap, mem::size_of};

use bytemuck::{Pod, Zeroable};
use noon_core::ObjectId;

use super::{
    analytic_instance_layout, create_pipeline, AnalyticPipelineDescriptor, DrawStats,
    GeometryBinding, GpuRenderer, PreparedFrame, RenderPrimitive, UploadStats, PATH_SAMPLE_COUNT,
};

#[cfg(test)]
mod tests;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct StopUniform {
    color: [f32; 4],
    position: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct GradientUniform {
    bounds: [f32; 4],
    endpoints: [f32; 4],
    metadata: [u32; 4],
    transform: [f32; 4],
    rotation: [f32; 4],
    stops: [StopUniform; 16],
}

impl GradientUniform {
    fn new(value: &crate::linear_gradient::PreparedGradient) -> Self {
        let mut result = Self::zeroed();
        let extent = value.bounds.max - value.bounds.min;
        result.bounds = [value.bounds.min.x, value.bounds.min.y, extent.x, extent.y];
        let noon_core::SemanticPaint::LinearGradient(definition) = value.definition.as_ref() else {
            unreachable!("checked gradient renderer profile");
        };
        let start = definition.start();
        let end = definition.end();
        result.endpoints = [
            start[0] as f32,
            start[1] as f32,
            end[0] as f32,
            end[1] as f32,
        ];
        result.metadata[0] = definition.stops().len() as u32;
        let transform = value.transform;
        result.transform = [
            transform.translation.x,
            transform.translation.y,
            transform.scale.x,
            transform.scale.y,
        ];
        result.rotation = [transform.rotation.cos(), transform.rotation.sin(), 0.0, 0.0];
        for (target, stop) in result.stops.iter_mut().zip(definition.stops()) {
            let c = stop.color();
            target.color = [
                c.red * c.alpha,
                c.green * c.alpha,
                c.blue * c.alpha,
                c.alpha,
            ];
            target.position[0] = stop.position() as f32;
        }
        result
    }
}

#[derive(Debug)]
struct ResidentGradient {
    value: GradientUniform,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

#[derive(Debug)]
pub(super) struct LinearGradientGpu {
    layout: wgpu::BindGroupLayout,
    // Circle and rectangle, each in multisample and single-sample variants.
    pipelines: [wgpu::RenderPipeline; 4],
    path_pipelines: [wgpu::RenderPipeline; 2],
    residents: HashMap<ObjectId, ResidentGradient>,
}

impl LinearGradientGpu {
    fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon linear paint uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<GradientUniform>() as u64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon linear paint pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout)],
            immediate_size: 0,
        });
        let source = format!(
            "{}\n{}\n{}\n{}\n{}",
            include_str!("../analytic.wgsl"),
            include_str!("../cairo_color.wgsl"),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("../gradient.wgsl"),
            include_str!("../gradient_analytic.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon linear paint shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipelines = std::array::from_fn(|index| {
            let circle = index < 2;
            create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                if index % 2 == 0 { PATH_SAMPLE_COUNT } else { 1 },
                AnalyticPipelineDescriptor {
                    vertex_entry: if circle { "vs_circle" } else { "vs_rectangle" },
                    fragment_entry: if circle {
                        "fs_gradient_circle"
                    } else {
                        "fs_gradient_rectangle"
                    },
                    label: "Noon continuous fill pipeline",
                    instance_layout: analytic_instance_layout(),
                },
            )
        });
        let path_source = format!(
            "{}\n{}\n{}\n{}\n{}",
            include_str!("../path.wgsl"),
            include_str!("../cairo_color.wgsl"),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("../gradient.wgsl"),
            include_str!("../gradient_path.wgsl")
        );
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon path linear paint shader"),
            source: wgpu::ShaderSource::Wgsl(path_source.into()),
        });
        let path_pipelines = std::array::from_fn(|index| {
            super::create_path_pipeline_with_instance_layout(
                device,
                &pipeline_layout,
                &path_shader,
                format,
                super::PathPipelineDescriptor {
                    vertex_layout: super::polygon_path_vertex_layout(),
                    instance_layout: super::path_instance_layout(),
                    vertex_entry: "vs_gradient_path",
                    fragment_entry: if index == 1 {
                        "fs_gradient_path_stroke"
                    } else {
                        "fs_gradient_path"
                    },
                    label: "Noon path continuous fill pipeline",
                },
            )
        });
        Self {
            layout,
            pipelines,
            path_pipelines,
            residents: HashMap::new(),
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &PreparedFrame<'_>,
    ) -> UploadStats {
        let mut stats = UploadStats::default();
        if frame.stats.full_rebuilds != 0 {
            self.residents
                .retain(|id, _| frame.fill_gradients.contains_key(id));
        }
        for id in frame.gradient_changes {
            let Some(paint) = frame.fill_gradients.get(id) else {
                self.residents.remove(id);
                continue;
            };
            let value = GradientUniform::new(paint);
            if let Some(resident) = self.residents.get_mut(id) {
                if resident.value == value {
                    continue;
                }
                queue.write_buffer(&resident.buffer, 0, bytemuck::bytes_of(&value));
                resident.value = value;
            } else {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon continuous fill uniform"),
                    size: size_of::<GradientUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Noon continuous fill binding"),
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&value));
                self.residents.insert(
                    *id,
                    ResidentGradient {
                        value,
                        buffer,
                        binding,
                    },
                );
                stats.buffer_reallocations += 1;
            }
            stats.bytes_uploaded += size_of::<GradientUniform>();
        }
        stats
    }
}

impl GpuRenderer {
    pub(super) fn upload_fill_gradients(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &PreparedFrame<'_>,
    ) -> UploadStats {
        if self.fill_gradients.is_none() && !frame.fill_gradients.is_empty() {
            self.fill_gradients = Some(Box::new(LinearGradientGpu::new(
                device,
                &self.camera_bind_group_layout,
                self.target_format,
            )));
        }
        self.fill_gradients
            .as_mut()
            .map_or_else(UploadStats::default, |gpu| gpu.upload(device, queue, frame))
    }

    pub(super) fn draw_gradient_instance<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        frame: &PreparedFrame<'_>,
        primitive: RenderPrimitive,
        index: u32,
        single_sample: bool,
        binding: &mut Option<GeometryBinding>,
    ) -> Option<DrawStats> {
        let id = match primitive {
            RenderPrimitive::Circle => *frame.circle_ids.get(index as usize)?,
            RenderPrimitive::Rectangle => *frame.rectangle_ids.get(index as usize)?,
            RenderPrimitive::Path { .. } => *frame.path_ids.get(index as usize)?,
            _ => return None,
        };
        if !frame.fill_gradients.contains_key(&id) {
            return None;
        }
        self.draw_gradient_batch(
            pass,
            frame,
            primitive,
            index..index + 1,
            single_sample,
            binding,
            id,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_gradient_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        frame: &PreparedFrame<'_>,
        primitive: RenderPrimitive,
        range: std::ops::Range<u32>,
        single_sample: bool,
        binding: &mut Option<GeometryBinding>,
        id: ObjectId,
        isolated: Option<wgpu::BufferSlice<'a>>,
    ) -> Option<DrawStats> {
        let (pipeline_index, buffer) = match primitive {
            RenderPrimitive::Circle => (0, &self.circle_buffer),
            RenderPrimitive::Rectangle => (2, &self.rectangle_buffer),
            RenderPrimitive::Path { .. } => (0, &self.path_instance_buffer),
            _ => return None,
        };
        let gpu = self.fill_gradients.as_ref()?;
        let resident = gpu.residents.get(&id)?;
        if let RenderPrimitive::Path { batch } = primitive {
            let path = &frame.path_batches[batch];
            let stroke = usize::from(path.coverage == crate::PathCoverage::PolygonalStroke);
            pass.set_pipeline(&gpu.path_pipelines[stroke]);
            pass.set_bind_group(1, &resident.binding, &[]);
            pass.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, isolated.unwrap_or_else(|| buffer.slice(..)));
            pass.set_index_buffer(self.path_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(path.index_range.clone(), 0, range.clone());
            *binding = None;
            return Some(DrawStats {
                draw_calls: 1,
                instances_drawn: range.len(),
            });
        }
        pass.set_pipeline(&gpu.pipelines[pipeline_index + usize::from(single_sample)]);
        pass.set_bind_group(1, &resident.binding, &[]);
        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
        pass.set_vertex_buffer(1, isolated.unwrap_or_else(|| buffer.slice(..)));
        pass.draw(0..6, range.clone());
        *binding = None;
        Some(DrawStats {
            draw_calls: 1,
            instances_drawn: range.len(),
        })
    }
}
