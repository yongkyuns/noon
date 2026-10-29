//! Disposable pointwise mask resources. Source bounds are relative to capture
//! pixels, before padding/capacity rounding. No scene or clock lives here.
use super::WeakGpuLease;
use super::{EffectPixelBounds, EffectPrepareError, EffectRasterStats, GaussianBlurCapture};
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

mod shader;
pub use shader::{PreparedShaderProgram, ShaderSampleContext};

#[cfg(test)]
pub(super) mod tests;

#[derive(Clone, Debug)]
pub struct SampleParameters<'a> {
    pub shader: Option<&'a ShaderSampleContext>,
    pub definition: noon_core::EffectDefinition,
    pub output_height: u32,
    pub world_view_height: f64,
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
pub struct SampleScope {
    resources: Option<Resources>,
    shader: Option<Box<shader::ShaderScope>>,
    uniform: Option<Uniform>,
    revision: Option<u64>,
    invalidated: bool,
}
impl SampleScope {
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
    pub(super) fn retire_shader_program(&mut self) {
        self.shader = None;
    }
}
#[derive(Debug)]
struct Program {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
#[derive(Debug, Default)]
pub struct SampleFilter {
    shader_programs: Vec<WeakGpuLease<PreparedShaderProgram>>,
    program: Option<Program>,
    // Real backend-failure tests corrupt generated WGSL after Core validation.
    // This hook and its storage are absent from production builds.
    #[cfg(test)]
    pub(in crate::gpu) test_shader_source: Option<fn(String) -> String>,
}
pub(super) fn checked_domain(
    size: [u32; 2],
    bounds: EffectPixelBounds,
) -> Result<[f32; 4], EffectPrepareError> {
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
    for (axis, size) in size.into_iter().enumerate() {
        for pixel in [0.5, f64::from(size) - 0.5] {
            let exact = (pixel - domain[axis]) * domain[axis + 2] * 16.0;
            let encoded = ((pixel as f32 - domain[axis] as f32) * domain[axis + 2] as f32) * 16.0;
            if !encoded.is_finite()
                || exact.abs() > 65536.0
                || (f64::from(encoded) - exact).abs() > 1.0 / 256.0
            {
                return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
            }
        }
    }
    Ok(domain.map(|v| v as f32))
}

fn validate_texture(
    capture: GaussianBlurCapture<'_>,
    parameters: &SampleParameters<'_>,
) -> Result<(), EffectPrepareError> {
    if !parameters.scope_opacity.is_finite() || !(0.0..=1.0).contains(&parameters.scope_opacity) {
        return Err(EffectPrepareError::InvalidScopeOpacity);
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
    {
        return Err(EffectPrepareError::UnsupportedCapture);
    }
    let bytes = u64::from(capture.source.width()) * u64::from(capture.source.height()) * 4;
    if bytes > parameters.scratch_budget_bytes {
        return Err(EffectPrepareError::ScratchBudgetExceeded);
    }
    Ok(())
}

fn checked_uniform(
    capture: GaussianBlurCapture<'_>,
    parameters: SampleParameters,
) -> Result<Uniform, EffectPrepareError> {
    validate_texture(capture, &parameters)?;
    let size = [capture.source.width(), capture.source.height()];
    if parameters.definition.as_pixelate().is_some() && size.into_iter().any(|v| v > 65536) {
        return Err(EffectPrepareError::ExtentExceedsDevice);
    }
    let (control, domain, seed) = match parameters.definition {
        noon_core::EffectDefinition::NoiseDissolve(v) => (
            [
                v.progress() as f32,
                v.softness() as f32,
                parameters.scope_opacity,
                0.0,
            ],
            checked_domain(size, parameters.source_bounds)?,
            [v.seed(), 0, 0, 0],
        ),
        noon_core::EffectDefinition::Pixelate(v) => {
            let cell =
                checked_cell_size(v, parameters.output_height, parameters.world_view_height)?;
            let bounds = parameters.source_bounds;
            let origin = checked_sample_origin(bounds)?;
            (
                [cell as f32, 0.0, parameters.scope_opacity, 1.0],
                [0.0; 4],
                [(cell * 256.0) as u32, origin[0] as u32, origin[1] as u32, 0],
            )
        }
        _ => return Err(EffectPrepareError::UnsupportedCapture),
    };
    Ok(Uniform {
        control,
        domain,
        seed,
    })
}

// Fixed 1/256-pixel domains keep ties independent of f32 division and backend.
pub(super) fn checked_sample_origin(
    bounds: EffectPixelBounds,
) -> Result<[i32; 2], EffectPrepareError> {
    if !bounds.min.into_iter().chain(bounds.max).all(f64::is_finite)
        || (0..2).any(|a| bounds.min[a] >= bounds.max[a])
    {
        return Err(EffectPrepareError::InvalidSourceBounds);
    }
    // This bound plus the backend texture limit keeps all signed grid
    // differences, products and doubled cell centres within i32.
    if bounds.min.into_iter().any(|v| v.abs() > 1_048_576.0) {
        return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
    }
    Ok(bounds.min.map(|v| (v * 256.0).round() as i32))
}

/// A bounded nearest-centre sampling profile, resolved before any GPU writes.
pub(super) fn checked_cell_size(
    definition: noon_core::Pixelate,
    height: u32,
    world: f64,
) -> Result<f64, EffectPrepareError> {
    let cell = definition
        .size()
        .to_output_pixels(height, world)
        .map_err(EffectPrepareError::Parameter)?;
    if cell > 128.0 {
        return Err(EffectPrepareError::CellSizeExceedsProfile);
    }
    Ok(((cell * 256.0).round() / 256.0).max(1.0))
}

pub(super) fn validate_input(
    capture: GaussianBlurCapture<'_>,
    parameters: SampleParameters,
) -> Result<(), EffectPrepareError> {
    if let Some(definition) = parameters.definition.as_shader() {
        shader::ShaderUniform::new(
            definition,
            [capture.source.width(), capture.source.height()],
            parameters.shader,
            parameters.scope_opacity,
        )?;
        validate_texture(capture, &parameters)
    } else {
        checked_uniform(capture, parameters).map(|_| ())
    }
}

pub(super) fn validate_effect_shader_context(
    definitions: &[noon_core::EffectDefinition],
    context: Option<&ShaderSampleContext>,
    size: [u32; 2],
) -> Result<(), EffectPrepareError> {
    for definition in definitions {
        if let Some(value) = definition.as_shader() {
            shader::ShaderUniform::new(value, size, context, 1.0)?;
        }
    }
    Ok(())
}

impl SampleFilter {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut SampleScope,
        capture: GaussianBlurCapture<'_>,
        parameters: SampleParameters<'_>,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        if scope
            .output()
            .is_some_and(|output| output == capture.source)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if parameters.definition.as_shader().is_some() {
            return self.prepare_shader_encode(device, queue, encoder, scope, capture, parameters);
        }
        let uniform = checked_uniform(capture, parameters)?;
        let mut compiles = 0;
        let program = self.program.get_or_insert_with(|| {
            compiles = 1;
            Program::new(device)
        });
        let program_changed = scope.shader.is_some();
        let uniform_changed = program_changed || scope.uniform != Some(uniform);
        let mut stats = scope.encode_sample(
            device,
            queue,
            encoder,
            capture,
            program,
            bytemuck::bytes_of(&uniform),
            uniform_changed,
            program_changed,
        );
        stats.pipeline_compiles += compiles;
        scope.shader = None;
        scope.uniform = Some(uniform);
        Ok(stats)
    }
}

impl SampleScope {
    #[allow(clippy::too_many_arguments)]
    fn encode_sample(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        capture: GaussianBlurCapture<'_>,
        program: &Program,
        uniform_bytes: &[u8],
        uniform_changed: bool,
        program_changed: bool,
    ) -> EffectRasterStats {
        let bytes = u64::from(capture.source.width()) * u64::from(capture.source.height()) * 4;
        let mut stats = EffectRasterStats {
            scratch_bytes: bytes,
            ..Default::default()
        };
        let replace = self
            .resources
            .as_ref()
            .is_none_or(|r| r.output.size() != capture.source.size());
        if replace {
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon sampled image result"),
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
                label: Some("Noon sampled image uniform"),
                size: uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let binding = program.bind(device, &buffer, capture.source);
            self.resources = Some(Resources {
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
        let r = self
            .resources
            .as_mut()
            .expect("prepared sampled image resources");
        let buffer_changed = r.buffer.size() != uniform_bytes.len() as u64;
        if buffer_changed {
            r.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Noon sampled image uniform"),
                size: uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            stats.buffer_allocations += 1;
        }
        let rebind = r.source != *capture.source || program_changed || buffer_changed;
        if rebind {
            r.binding = program.bind(device, &r.buffer, capture.source);
            r.source = capture.source.clone();
            stats.bind_group_creations += 1;
        }
        let dirty = replace
            || rebind
            || self.invalidated
            || uniform_changed
            || self.revision != Some(capture.revision);
        if replace || buffer_changed || uniform_changed {
            queue.write_buffer(&r.buffer, 0, uniform_bytes);
            stats.bytes_uploaded += uniform_bytes.len();
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
                label: Some("Noon sampled image mask"),
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
        self.revision = Some(capture.revision);
        self.invalidated = false;
        stats
    }
}
impl Program {
    fn new(device: &wgpu::Device) -> Self {
        Self::from_source(
            device,
            include_str!("sample_filter.wgsl"),
            size_of::<Uniform>() as u64,
        )
    }
    fn from_source(device: &wgpu::Device, source: &str, uniform_size: u64) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon sampled image bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(uniform_size),
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
            label: Some("Noon sampled image layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon sampled image program"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Noon sampled image mask"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sample"),
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
            label: Some("Noon sampled image binding"),
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
