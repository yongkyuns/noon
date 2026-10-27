//! Disposable shadow composition; the existing RGBA Gaussian owns convolution.
use super::gaussian_blur::{
    GaussianBlurCapture, GaussianBlurFilter, GaussianBlurParameters, GaussianBlurScope,
};
use super::{EffectPrepareError, EffectRasterStats};
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Uniform {
    tint: [f32; 4],
    offset_opacity: [f32; 4],
}
#[derive(Debug)]
struct Resources {
    output: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
    source: wgpu::Texture,
    blurred: wgpu::Texture,
}
#[derive(Debug, Default)]
pub(super) struct ShadowScope {
    gaussian: GaussianBlurScope,
    resources: Option<Resources>,
    uniform: Option<Uniform>,
    revision: Option<u64>,
    invalidated: bool,
}
impl ShadowScope {
    #[cfg(test)]
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.resources.as_ref().map(|r| &r.output)
    }
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.resources.as_ref().map(|r| &r.view)
    }
    pub fn invalidate(&mut self) {
        self.gaussian.invalidate();
        self.invalidated = true;
    }
}
#[derive(Debug)]
struct Program {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
#[derive(Debug, Default)]
pub(super) struct ShadowFilter {
    program: Option<Program>,
}
impl ShadowFilter {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        gaussian: &mut GaussianBlurFilter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut ShadowScope,
        capture: GaussianBlurCapture<'_>,
        definition: noon_core::Shadow,
        output_height: u32,
        world_view_height: f64,
        opacity: f32,
        budget: u64,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let sigma = definition
            .radius()
            .to_output_pixels(output_height, world_view_height)
            .map_err(EffectPrepareError::Parameter)?;
        let offset = definition
            .offset()
            .to_output_pixels(output_height, world_view_height)
            .map_err(EffectPrepareError::Parameter)?;
        let output_bytes = u64::from(capture.source.width())
            .checked_mul(u64::from(capture.source.height()))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(EffectPrepareError::InvalidScopeOpacity);
        }
        if offset
            .into_iter()
            .any(|v| !((v as f32).is_finite()) || (f64::from(v as f32) - v).abs() > 1.0 / 256.0)
        {
            return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
        }
        if !matches!(
            capture.source.format(),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ) || capture.source.dimension() != wgpu::TextureDimension::D2
            || capture.source.sample_count() != 1
            || capture.source.depth_or_array_layers() != 1
            || !capture
                .source
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let gaussian_budget = budget
            .checked_sub(output_bytes)
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        let prepared = gaussian
            .prepare(
                device,
                queue,
                &mut scope.gaussian,
                capture,
                GaussianBlurParameters {
                    sigma_pixels: sigma,
                    scope_opacity: 1.0,
                    scratch_budget_bytes: gaussian_budget,
                },
            )
            .map_err(EffectPrepareError::Blur)?;
        let encoded = gaussian.encode(encoder, &mut scope.gaussian);
        let mut stats = EffectRasterStats {
            pipeline_compiles: prepared.pipeline_compiles,
            kernel_builds: prepared.kernel_builds,
            texture_allocations: prepared.texture_allocations,
            buffer_allocations: prepared.buffer_allocations,
            bind_group_creations: prepared.bind_group_creations,
            bytes_uploaded: prepared.bytes_uploaded,
            scratch_bytes: prepared.scratch_bytes + output_bytes,
            blur_passes: encoded.blur_passes,
            composite_passes: encoded.output_passes,
        };
        let tint = definition.color();
        let uniform = Uniform {
            tint: [tint.red, tint.green, tint.blue, tint.alpha],
            offset_opacity: [offset[0] as f32, -(offset[1] as f32), opacity, 0.0],
        };
        let blurred = scope.gaussian.output().unwrap_or(capture.source);
        let program = self.program.get_or_insert_with(|| {
            stats.pipeline_compiles += 1;
            Program::new(device)
        });
        let replace = scope
            .resources
            .as_ref()
            .is_none_or(|r| r.output.size() != capture.source.size());
        if replace {
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon shadow result"),
                size: capture.source.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = output.create_view(&Default::default());
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Noon shadow uniform"),
                size: size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let binding = program.bind(device, &buffer, capture.source, blurred);
            scope.resources = Some(Resources {
                output,
                view,
                buffer,
                binding,
                source: capture.source.clone(),
                blurred: blurred.clone(),
            });
            stats.texture_allocations += 1;
            stats.buffer_allocations += 1;
            stats.bind_group_creations += 1;
        }
        let r = scope.resources.as_mut().expect("prepared shadow resources");
        let rebind = r.source != *capture.source || r.blurred != *blurred;
        if rebind {
            r.binding = program.bind(device, &r.buffer, capture.source, blurred);
            r.source = capture.source.clone();
            r.blurred = blurred.clone();
            stats.bind_group_creations += 1;
        }
        let dirty = replace
            || rebind
            || scope.invalidated
            || scope.uniform != Some(uniform)
            || scope.revision != Some(capture.revision)
            || encoded.output_passes != 0;
        if replace || scope.uniform != Some(uniform) {
            queue.write_buffer(&r.buffer, 0, bytemuck::bytes_of(&uniform));
            stats.bytes_uploaded += size_of::<Uniform>();
        }
        if dirty {
            let attachment = Some(wgpu::RenderPassColorAttachment {
                view: &r.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon shadow composition"),
                color_attachments: &[attachment],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&program.pipeline);
            pass.set_bind_group(0, &r.binding, &[]);
            pass.draw(0..3, 0..1);
            stats.composite_passes += 1;
        }
        scope.uniform = Some(uniform);
        scope.revision = Some(capture.revision);
        scope.invalidated = false;
        Ok(stats)
    }
}
impl Program {
    fn new(device: &wgpu::Device) -> Self {
        let uniform = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<Uniform>() as u64),
            },
            count: None,
        };
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon shadow bindings"),
            entries: &[uniform, texture(1), texture(2)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon shadow layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shadow_filter.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Noon shadow composition"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { layout, pipeline }
    }
    fn bind(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
        source: &wgpu::Texture,
        blurred: &wgpu::Texture,
    ) -> wgpu::BindGroup {
        let source = source.create_view(&Default::default());
        let blurred = blurred.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon shadow binding"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&blurred),
                },
            ],
        })
    }
}
