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
struct PaintUniform {
    bounds: [f32; 4],
    endpoints: [f32; 4],
    metadata: [u32; 4],
    transform: [f32; 4],
    rotation: [f32; 4],
    stops: [StopUniform; 16],
    image_u: [f32; 4],
    image_v: [f32; 4],
}

impl PaintUniform {
    fn set_colors(&mut self, colors: [noon_core::Color; 2], profile: noon_core::ViewColorProfile) {
        for (index, color) in colors.into_iter().enumerate() {
            let color = crate::color_profile::authored_color(color, profile);
            self.stops[index].color = [
                color.red * color.alpha,
                color.green * color.alpha,
                color.blue * color.alpha,
                color.alpha,
            ];
            self.stops[index].position[0] = index as f32;
        }
    }

    fn set_stops(
        &mut self,
        stops: &[noon_core::GradientStop],
        profile: noon_core::ViewColorProfile,
    ) {
        self.metadata[0] = stops.len() as u32;
        for (target, stop) in self.stops.iter_mut().zip(stops) {
            let c = crate::color_profile::authored_color(stop.color(), profile);
            target.color = [
                c.red * c.alpha,
                c.green * c.alpha,
                c.blue * c.alpha,
                c.alpha,
            ];
            target.position[0] = stop.position() as f32;
        }
    }

    fn new(
        value: &crate::paint::PreparedPaint,
        images: Option<&super::raster_image_gpu::RasterImageGpuRenderer>,
    ) -> Self {
        let mut result = Self::zeroed();
        let extent = value.bounds.max - value.bounds.min;
        result.bounds = [value.bounds.min.x, value.bounds.min.y, extent.x, extent.y];
        let transform = value.transform;
        result.transform = [
            transform.translation.x,
            transform.translation.y,
            transform.scale.x,
            transform.scale.y,
        ];
        result.rotation = [transform.rotation.cos(), transform.rotation.sin(), 0.0, 0.0];
        match value.definition.as_ref() {
            noon_core::SemanticPaint::LinearGradient(definition) => {
                let start = definition.start();
                let end = definition.end();
                result.endpoints = [
                    start[0] as f32,
                    start[1] as f32,
                    end[0] as f32,
                    end[1] as f32,
                ];
                result.set_stops(definition.stops(), value.color_profile);
            }
            noon_core::SemanticPaint::RadialGradient(definition) => {
                let center = definition.center();
                let radii = definition.radii();
                result.endpoints = [
                    center[0] as f32,
                    center[1] as f32,
                    radii[0] as f32,
                    radii[1] as f32,
                ];
                result.metadata[1] = 2;
                result.set_stops(definition.stops(), value.color_profile);
            }
            noon_core::SemanticPaint::Noise(noise) => {
                let (cell, fraction) = noise.phase_parts();
                let frequency = noise.frequency();
                result.metadata = [2, 3, noise.seed(), cell[0] | (cell[1] << 16)];
                result.image_u = [
                    frequency[0] as f32,
                    frequency[1] as f32,
                    fraction[0] as f32,
                    fraction[1] as f32,
                ];
                result.set_colors(noise.colors(), value.color_profile);
            }
            noon_core::SemanticPaint::Hatch(hatch) => {
                let rows = noon_compile::project_hatch_mapping(hatch)
                    .expect("checked local hatch mapping");
                result.metadata = [2, 4, u32::from(hatch.cross()), 0];
                result.image_u = rows[0];
                result.image_v = rows[1];
                result.set_colors(hatch.colors(), value.color_profile);
            }
            noon_core::SemanticPaint::Image(image) => {
                let dimensions = images
                    .and_then(|images| images.texture_dimensions(image.resource()))
                    .expect("prepared image texture dependency uploaded before paint uniforms");
                let rows = noon_compile::project_image_mapping(
                    image.mapping(),
                    [f64::from(extent.x), f64::from(extent.y)],
                    dimensions,
                )
                .expect("checked image mapping");
                result.image_u = [
                    rows[0][0],
                    rows[0][1],
                    rows[0][2],
                    f32::from(image.mapping().fit() == noon_core::ImageFit::Contain),
                ];
                result.image_v = [rows[1][0], rows[1][1], rows[1][2], 0.0];
                result.metadata[1] = 1;
                result.metadata[2] = match image.sampling() {
                    noon_core::RasterImageSampling::Nearest => 0,
                    noon_core::RasterImageSampling::Linear => 1,
                    noon_core::RasterImageSampling::Bicubic => 2,
                };
                result.metadata[3] = match image.mapping().address() {
                    noon_core::ImageAddressMode::Transparent => 0,
                    noon_core::ImageAddressMode::Clamp => 1,
                    noon_core::ImageAddressMode::Repeat => 2,
                };
            }
            noon_core::SemanticPaint::Solid(_) => {
                unreachable!("solid paint uses ordinary geometry carrier")
            }
        }
        result
    }
}

#[derive(Debug)]
struct ResidentPaint {
    value: PaintUniform,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

#[derive(Debug)]
pub(super) struct PaintGpu {
    layout: wgpu::BindGroupLayout,
    // Circle and rectangle, each in multisample and single-sample variants.
    pipelines: [wgpu::RenderPipeline; 4],
    path_pipelines: [wgpu::RenderPipeline; 2],
    residents: HashMap<ObjectId, ResidentPaint>,
    dummy_image: wgpu::BindGroup,
}

impl PaintGpu {
    fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        image_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon shared paint uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<PaintUniform>() as u64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon shared paint pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout), Some(image_layout)],
            immediate_size: 0,
        });
        let source = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("../analytic.wgsl"),
            super::source_color_shader(format),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("raster_sampling.wgsl"),
            include_str!("../paint.wgsl"),
            include_str!("../paint_analytic.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon shared paint shader"),
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
                        "fs_paint_circle"
                    } else {
                        "fs_paint_rectangle"
                    },
                    label: "Noon continuous fill pipeline",
                    instance_layout: analytic_instance_layout(),
                },
            )
        });
        let path_source = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("../path.wgsl"),
            super::source_color_shader(format),
            include_str!("../polygon_coverage.wgsl"),
            include_str!("raster_sampling.wgsl"),
            include_str!("../paint.wgsl"),
            include_str!("../paint_path.wgsl")
        );
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon path shared paint shader"),
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
                    vertex_entry: "vs_paint_path",
                    fragment_entry: if index == 1 {
                        "fs_paint_path_stroke"
                    } else {
                        "fs_paint_path"
                    },
                    label: "Noon path continuous fill pipeline",
                },
            )
        });
        // Gradients share the image-capable program and bind a renderer-local
        // dummy texture. It has no semantic arena identity or pixel upload.
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon continuous paint dummy texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_image = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon continuous paint dummy binding"),
            layout: image_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &dummy_texture.create_view(&Default::default()),
                ),
            }],
        });
        Self {
            layout,
            pipelines,
            path_pipelines,
            residents: HashMap::new(),
            dummy_image,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &PreparedFrame<'_>,
        images: Option<&super::raster_image_gpu::RasterImageGpuRenderer>,
    ) -> UploadStats {
        let mut stats = UploadStats::default();
        if frame.stats.full_rebuilds != 0 {
            self.residents
                .retain(|id, _| frame.fill_paints.contains_key(id));
        }
        for id in frame.paint_changes {
            let Some(paint) = frame.fill_paints.get(id) else {
                self.residents.remove(id);
                continue;
            };
            let value = PaintUniform::new(paint, images);
            if let Some(resident) = self.residents.get_mut(id) {
                if resident.value == value {
                    continue;
                }
                queue.write_buffer(&resident.buffer, 0, bytemuck::bytes_of(&value));
                resident.value = value;
            } else {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon continuous fill uniform"),
                    size: size_of::<PaintUniform>() as u64,
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
                    ResidentPaint {
                        value,
                        buffer,
                        binding,
                    },
                );
                stats.buffer_reallocations += 1;
            }
            stats.bytes_uploaded += size_of::<PaintUniform>();
        }
        stats
    }
}

impl GpuRenderer {
    pub(super) fn upload_fill_paints(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &PreparedFrame<'_>,
    ) -> UploadStats {
        if self.fill_paints.is_none() && !frame.fill_paints.is_empty() {
            self.fill_paints = Some(Box::new(PaintGpu::new(
                device,
                &self.camera_bind_group_layout,
                &self.image_texture_layout,
                self.target_format,
            )));
        }
        self.fill_paints
            .as_mut()
            .map_or_else(UploadStats::default, |gpu| {
                gpu.upload(device, queue, frame, self.images.as_ref())
            })
    }

    pub(super) fn draw_paint_instance<'a>(
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
        if !frame.fill_paints.contains_key(&id) {
            return None;
        }
        self.draw_paint_batch(
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
    pub(super) fn draw_paint_batch<'a>(
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
        let gpu = self.fill_paints.as_ref()?;
        let resident = gpu.residents.get(&id)?;
        let texture = match frame.fill_paints.get(&id)?.definition.as_image() {
            Some(image) => self.images.as_ref()?.texture_binding(image.resource())?,
            None => &gpu.dummy_image,
        };
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
