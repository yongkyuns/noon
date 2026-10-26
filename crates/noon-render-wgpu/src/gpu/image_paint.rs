//! Image fills reuse vector coverage and the ordinary immutable image texture cache.
//! Mapping edits update one retained uniform without regenerating geometry or pixels.
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
struct ImagePaintUniform {
    bounds: [f32; 4],
    fit: [f32; 4],
    transform: [f32; 4],
    rotation: [f32; 4],
    uv_transform: [f32; 4],
    uv_rotation: [f32; 4],
    metadata: [u32; 4],
}

impl ImagePaintUniform {
    fn new(value: &crate::image_paint::PreparedImagePaint, dimensions: [u32; 2]) -> Self {
        use noon_core::{ImageAddressMode, ImageFit, RasterImageSampling};
        let extent = value.bounds.max - value.bounds.min;
        let mapping = value.definition.mapping();
        let ratio = (f64::from(dimensions[0]) / f64::from(dimensions[1]))
            * (f64::from(extent.y) / f64::from(extent.x));
        let size = match mapping.fit() {
            ImageFit::Stretch => [1.0; 2],
            ImageFit::Cover if ratio >= 1.0 => [ratio, 1.0],
            ImageFit::Cover => [1.0, ratio.recip()],
            ImageFit::Contain if ratio >= 1.0 => [1.0, ratio.recip()],
            ImageFit::Contain => [ratio, 1.0],
        };
        let alignment = mapping.alignment();
        let uv = mapping.transform();
        let translation = uv.translation();
        let scale = uv.scale();
        let transform = value.transform;
        Self {
            bounds: [value.bounds.min.x, value.bounds.min.y, extent.x, extent.y],
            fit: [
                ((1.0 - size[0]) * alignment[0]) as f32,
                ((1.0 - size[1]) * alignment[1]) as f32,
                size[0] as f32,
                size[1] as f32,
            ],
            transform: [
                transform.translation.x,
                transform.translation.y,
                transform.scale.x,
                transform.scale.y,
            ],
            rotation: [transform.rotation.cos(), transform.rotation.sin(), 0.0, 0.0],
            uv_transform: [
                translation[0] as f32,
                translation[1] as f32,
                scale[0] as f32,
                scale[1] as f32,
            ],
            uv_rotation: [
                uv.rotation().cos() as f32,
                uv.rotation().sin() as f32,
                0.0,
                0.0,
            ],
            metadata: [
                u32::from(mapping.fit() == ImageFit::Contain),
                match mapping.address() {
                    ImageAddressMode::Transparent => 0,
                    ImageAddressMode::Clamp => 1,
                    ImageAddressMode::Repeat => 2,
                },
                match value.definition.sampling() {
                    RasterImageSampling::Nearest => 0,
                    RasterImageSampling::Linear => 1,
                    RasterImageSampling::Bicubic => 2,
                },
                0,
            ],
        }
    }
}

#[derive(Debug)]
struct ResidentImagePaint {
    value: ImagePaintUniform,
    resource: noon_core::RasterImageResourceHandle,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

#[derive(Debug)]
pub(super) struct ImagePaintGpu {
    layout: wgpu::BindGroupLayout,
    // Circle and rectangle, each in multisample and single-sample variants.
    pipelines: [wgpu::RenderPipeline; 4],
    path_pipelines: [wgpu::RenderPipeline; 2],
    residents: HashMap<ObjectId, ResidentImagePaint>,
}

impl ImagePaintGpu {
    fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon image paint uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ImagePaintUniform>() as u64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon image paint pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let source = format!(
            "{}\n{}\n{}\n{}\n{}",
            include_str!("../analytic.wgsl"),
            include_str!("../cairo_color.wgsl"),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("../image_paint.wgsl"),
            include_str!("../image_paint_analytic.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon image paint shader"),
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
                        "fs_image_paint_circle"
                    } else {
                        "fs_image_paint_rectangle"
                    },
                    label: "Noon image fill pipeline",
                    instance_layout: analytic_instance_layout(),
                },
            )
        });
        let path_source = format!(
            "{}\n{}\n{}\n{}\n{}",
            include_str!("../path.wgsl"),
            include_str!("../cairo_color.wgsl"),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("../image_paint.wgsl"),
            include_str!("../image_paint_path.wgsl")
        );
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon path image paint shader"),
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
                    vertex_entry: "vs_image_paint_path",
                    fragment_entry: if index == 1 {
                        "fs_image_paint_path_stroke"
                    } else {
                        "fs_image_paint_path"
                    },
                    label: "Noon path image fill pipeline",
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
        images: &super::raster_image_gpu::RasterImageGpuRenderer,
    ) -> UploadStats {
        let mut stats = UploadStats::default();
        if frame.stats.full_rebuilds != 0 {
            self.residents
                .retain(|id, _| frame.fill_image_paints.contains_key(id));
        }
        for id in frame.image_paint_changes {
            let Some(paint) = frame.fill_image_paints.get(id) else {
                self.residents.remove(id);
                continue;
            };
            let resource = paint.definition.resource();
            let dimensions = images
                .texture_dimensions(resource)
                .expect("image paint resource prepared before uniform upload");
            let value = ImagePaintUniform::new(paint, dimensions);
            if let Some(resident) = self.residents.get_mut(id) {
                resident.resource = resource;
                if resident.value == value {
                    continue;
                }
                queue.write_buffer(&resident.buffer, 0, bytemuck::bytes_of(&value));
                resident.value = value;
            } else {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon image fill uniform"),
                    size: size_of::<ImagePaintUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Noon image fill binding"),
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&value));
                self.residents.insert(
                    *id,
                    ResidentImagePaint {
                        value,
                        resource,
                        buffer,
                        binding,
                    },
                );
                stats.buffer_reallocations += 1;
            }
            stats.bytes_uploaded += size_of::<ImagePaintUniform>();
        }
        stats
    }
}

impl GpuRenderer {
    pub(super) fn upload_fill_image_paints(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &PreparedFrame<'_>,
    ) -> UploadStats {
        let Some(images) = self.images.as_ref() else {
            return UploadStats::default();
        };
        if self.fill_image_paints.is_none() && !frame.fill_image_paints.is_empty() {
            self.fill_image_paints = Some(Box::new(ImagePaintGpu::new(
                device,
                &self.camera_bind_group_layout,
                self.target_format,
                images.texture_layout(),
            )));
        }
        self.fill_image_paints
            .as_mut()
            .map_or_else(UploadStats::default, |gpu| {
                gpu.upload(device, queue, frame, images)
            })
    }

    pub(super) fn draw_image_paint_instance<'a>(
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
        if !frame.fill_image_paints.contains_key(&id) {
            return None;
        }
        Some(
            self.draw_image_paint_batch(
                pass,
                frame,
                primitive,
                index..index + 1,
                single_sample,
                binding,
                id,
                None,
            )
            .expect("image fills must be uploaded through upload_retained before drawing"),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_image_paint_batch<'a>(
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
        let gpu = self.fill_image_paints.as_ref()?;
        let resident = gpu.residents.get(&id)?;
        let texture = self.images.as_ref()?.texture_binding(resident.resource)?;
        if let RenderPrimitive::Path { batch } = primitive {
            let path = &frame.path_batches[batch];
            let stroke = usize::from(path.coverage == crate::PathCoverage::PolygonalStroke);
            pass.set_pipeline(&gpu.path_pipelines[stroke]);
            pass.set_bind_group(1, &resident.binding, &[]);
            pass.set_bind_group(2, texture, &[]);
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
        pass.set_bind_group(2, texture, &[]);
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
