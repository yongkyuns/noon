//! Disposable pointwise mask resources. Source bounds are relative to capture
//! pixels, before padding/capacity rounding. No scene or clock lives here.
use super::{EffectPixelBounds, EffectPrepareError, EffectRasterStats, GaussianBlurCapture};
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub struct DissolveParameters {
    pub definition: noon_core::NoiseDissolve,
    pub source_bounds: EffectPixelBounds,
    pub scope_opacity: f32,
    /// Retained output only; input and in-flight host images are separate.
    pub scratch_budget_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Uniform {
    control: [f32; 4],
    domain: [f32; 4],
    seed: [u32; 4],
}
#[derive(Debug)]
struct Resources {
    output: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
    source: wgpu::Texture,
}
#[derive(Debug, Default)]
pub struct DissolveScope {
    resources: Option<Resources>,
    uniform: Option<Uniform>,
    revision: Option<u64>,
    invalidated: bool,
}
impl DissolveScope {
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.resources.as_ref().map(|r| &r.output)
    }
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.resources.as_ref().map(|r| &r.view)
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.output()
            .map_or(0, |t| u64::from(t.width()) * u64::from(t.height()) * 4)
    }
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
#[derive(Debug)]
struct Program {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
#[derive(Debug, Default)]
pub struct DissolveFilter {
    program: Option<Program>,
}
impl DissolveFilter {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut DissolveScope,
        capture: GaussianBlurCapture<'_>,
        parameters: DissolveParameters,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let bounds = parameters.source_bounds;
        if !parameters.scope_opacity.is_finite() || !(0.0..=1.0).contains(&parameters.scope_opacity)
        {
            return Err(EffectPrepareError::InvalidScopeOpacity);
        }
        if !bounds.min.into_iter().chain(bounds.max).all(f64::is_finite)
            || (0..2).any(|a| bounds.min[a] >= bounds.max[a])
        {
            return Err(EffectPrepareError::InvalidSourceBounds);
        }
        let domain = [
            bounds.min[0],
            bounds.min[1],
            1.0 / (bounds.max[0] - bounds.min[0]),
            1.0 / (bounds.max[1] - bounds.min[1]),
        ];
        if domain.into_iter().any(|v| !(v as f32).is_finite())
            || (0..2).any(|a| (f64::from(domain[a] as f32) - domain[a]).abs() > 1.0 / 256.0)
            || domain[2..].iter().any(|v| (*v as f32) <= 0.0)
        {
            return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
        }
        // Keep normalized lattice coordinates finite, integer-addressable and
        // within the declared 1/256-cell coordinate precision before GPU writes.
        for (axis, size) in [capture.source.width(), capture.source.height()]
            .into_iter()
            .enumerate()
        {
            for pixel in [0.5, f64::from(size) - 0.5] {
                let exact = (pixel - domain[axis]) * domain[axis + 2] * 16.0;
                let encoded =
                    ((pixel as f32 - domain[axis] as f32) * domain[axis + 2] as f32) * 16.0;
                if !encoded.is_finite()
                    || exact.abs() > 65536.0
                    || (f64::from(encoded) - exact).abs() > 1.0 / 256.0
                {
                    return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
                }
            }
        }
        if !matches!(
            capture.source.format(),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ) || capture.source.dimension() != wgpu::TextureDimension::D2
            || capture.source.sample_count() != 1
            || capture.source.depth_or_array_layers() != 1
            || capture.source.mip_level_count() != 1
            || !capture
                .source
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
            || scope
                .output()
                .is_some_and(|output| output == capture.source)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let bytes = u64::from(capture.source.width()) * u64::from(capture.source.height()) * 4;
        if bytes > parameters.scratch_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        let definition = parameters.definition;
        let uniform = Uniform {
            control: [
                definition.progress() as f32,
                definition.softness() as f32,
                parameters.scope_opacity,
                0.0,
            ],
            domain: domain.map(|v| v as f32),
            seed: [definition.seed(), 0, 0, 0],
        };
        let mut stats = EffectRasterStats {
            scratch_bytes: bytes,
            ..Default::default()
        };
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
                label: Some("Noon dissolve result"),
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
                label: Some("Noon dissolve uniform"),
                size: size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let binding = program.bind(device, &buffer, capture.source);
            scope.resources = Some(Resources {
                output,
                view,
                buffer,
                binding,
                source: capture.source.clone(),
            });
            stats.texture_allocations += 1;
            stats.buffer_allocations += 1;
            stats.bind_group_creations += 1;
        }
        let r = scope
            .resources
            .as_mut()
            .expect("prepared dissolve resources");
        let rebind = r.source != *capture.source;
        if rebind {
            r.binding = program.bind(device, &r.buffer, capture.source);
            r.source = capture.source.clone();
            stats.bind_group_creations += 1;
        }
        let dirty = replace
            || rebind
            || scope.invalidated
            || scope.uniform != Some(uniform)
            || scope.revision != Some(capture.revision);
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
                label: Some("Noon dissolve mask"),
                color_attachments: &[attachment],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&program.pipeline);
            pass.set_bind_group(0, &r.binding, &[]);
            pass.draw(0..3, 0..1);
            stats.composite_passes = 1;
        }
        scope.uniform = Some(uniform);
        scope.revision = Some(capture.revision);
        scope.invalidated = false;
        Ok(stats)
    }
}
impl Program {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon dissolve bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<Uniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon dissolve layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("dissolve_filter.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Noon dissolve mask"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_dissolve"),
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
    ) -> wgpu::BindGroup {
        let view = source.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon dissolve binding"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        })
    }
}
