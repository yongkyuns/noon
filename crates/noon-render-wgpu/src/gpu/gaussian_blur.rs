//! Retained RGBA Gaussian raster operator for #1897's blur/shadow integration.
//!
//! Input and output are encoded-premultiplied LDR. The host supplies a padded
//! source and its revision, places the result in painter order, and owns queue
//! submission. This operator owns no semantic identity, scope graph or clock.
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u64 = 28; // six packed intermediates and one RGBA8 output

#[derive(Clone, Copy, Debug)]
pub struct GaussianBlurCapture<'a> {
    /// Encoded-premultiplied source, before final scope opacity.
    pub source: &'a wgpu::Texture,
    /// Existing source-content revision; advance after in-place source draws.
    pub revision: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct GaussianBlurParameters {
    /// Gaussian standard deviation in final physical output pixels, in [0, 64].
    /// The caller resolves authored units using the full output, before cropping.
    pub sigma_pixels: f64,
    pub scope_opacity: f32,
    /// This operator's retained textures; excludes source and in-flight frames.
    pub scratch_budget_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GaussianBlurPrepareError {
    InvalidRadius,
    InvalidOpacity,
    UnsupportedCapture,
    InsufficientColorAttachments,
    ExtentExceedsDevice,
    ScratchBudgetExceeded,
}

impl std::fmt::Display for GaussianBlurPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot prepare RGBA Gaussian blur: {self:?}")
    }
}
impl std::error::Error for GaussianBlurPrepareError {}

/// Work/resource observations; these do not establish physical-device pacing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GaussianBlurStats {
    pub pipeline_compiles: usize,
    pub kernel_builds: usize,
    pub texture_allocations: usize,
    pub buffer_allocations: usize,
    pub bind_group_creations: usize,
    pub bytes_uploaded: usize,
    pub scratch_bytes: u64,
    pub blur_passes: usize,
    pub output_passes: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct BlurUniform {
    size: [u32; 2],
    radius: u32,
    reserved: u32,
    control: [f32; 4],
    weights: [[f32; 4]; 64],
}

impl BlurUniform {
    fn new(parameters: GaussianBlurParameters, size: [u32; 2], cached: Option<&Self>) -> Self {
        let sigma = parameters.sigma_pixels;
        let mut result = Self {
            size,
            radius: (3.0 * sigma).ceil() as u32,
            reserved: 0,
            control: [parameters.scope_opacity, 0.0, 0.0, 0.0],
            weights: [[0.0; 4]; 64],
        };
        if let Some(cached) = cached {
            result.weights = cached.weights;
        } else if sigma == 0.0 {
            result.weights[0][0] = 1.0;
        } else {
            let weight = |offset: u32| (-0.5 * (f64::from(offset) / sigma).powi(2)).exp();
            let mass = 1.0 + 2.0 * (1..=result.radius).map(weight).sum::<f64>();
            for at in 0..=result.radius as usize {
                result.weights[at / 4][at % 4] = (weight(at as u32) / mass) as f32;
            }
        }
        result
    }

    fn same_kernel(&self, other: &Self) -> bool {
        self.size == other.size && self.radius == other.radius && self.weights == other.weights
    }
}

#[derive(Debug)]
struct Image {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Image {
    fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon RGBA Gaussian image"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self { texture, view }
    }
}

#[derive(Debug)]
struct Resources {
    size: [u32; 2],
    horizontal: [Image; 3],
    vertical: [Image; 3],
    output: Image,
    uniform: wgpu::Buffer,
    bindings: [wgpu::BindGroup; 3],
    source: wgpu::Texture,
}

/// Disposable derived state for one caller-owned capture. Clear on device or
/// renderer recreation. Each scope stays with its creating filter/device.
#[derive(Debug, Default)]
pub struct GaussianBlurScope {
    resources: Option<Resources>,
    uniform: Option<BlurUniform>,
    sigma: Option<f64>,
    source_revision: Option<u64>,
    blur_dirty: bool,
    output_dirty: bool,
    active: bool,
}

impl GaussianBlurScope {
    /// Valid after encoding and submitting the latest dirty work. None means
    /// use the ordinary source path (zero radius/opacity), without a copied image.
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.active.then(|| {
            &self
                .resources
                .as_ref()
                .expect("active blur resources")
                .output
                .texture
        })
    }
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.active.then(|| {
            &self
                .resources
                .as_ref()
                .expect("active blur resources")
                .output
                .view
        })
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.resources.as_ref().map_or(0, |r| {
            u64::from(r.size[0]) * u64::from(r.size[1]) * BYTES_PER_PIXEL
        })
    }
    /// Retry all derived commands after abandoning an encoder before submission.
    pub fn invalidate(&mut self) {
        self.blur_dirty = true;
        self.output_dirty = true;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
struct Programs {
    layout: wgpu::BindGroupLayout,
    pipelines: [wgpu::RenderPipeline; 3],
}

/// Lazy renderer-local programs, shared across scopes. No float attachment
/// capability, sampler, device polling or queue submission is required here.
#[derive(Debug, Default)]
pub struct GaussianBlurFilter {
    programs: Option<Programs>,
}

impl GaussianBlurFilter {
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scope: &mut GaussianBlurScope,
        capture: GaussianBlurCapture<'_>,
        parameters: GaussianBlurParameters,
    ) -> Result<GaussianBlurStats, GaussianBlurPrepareError> {
        let sigma = parameters.sigma_pixels;
        if !sigma.is_finite() || !(0.0..=64.0).contains(&sigma) {
            return Err(GaussianBlurPrepareError::InvalidRadius);
        }
        if !parameters.scope_opacity.is_finite() || !(0.0..=1.0).contains(&parameters.scope_opacity)
        {
            return Err(GaussianBlurPrepareError::InvalidOpacity);
        }
        let mut stats = GaussianBlurStats::default();
        if sigma == 0.0 || parameters.scope_opacity == 0.0 {
            if scope.scratch_bytes() > parameters.scratch_budget_bytes {
                scope.clear();
            }
            scope.active = false;
            stats.scratch_bytes = scope.scratch_bytes();
            return Ok(stats);
        }
        let source = capture.source;
        if !matches!(
            source.format(),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ) || source.dimension() != wgpu::TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.sample_count() != 1
            || !source
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(GaussianBlurPrepareError::UnsupportedCapture);
        }
        let size = [source.width(), source.height()];
        if device.limits().max_color_attachments < 3
            || device.limits().max_color_attachment_bytes_per_sample < 12
        {
            return Err(GaussianBlurPrepareError::InsufficientColorAttachments);
        }
        if size.contains(&0)
            || size
                .into_iter()
                .any(|v| v > device.limits().max_texture_dimension_2d)
        {
            return Err(GaussianBlurPrepareError::ExtentExceedsDevice);
        }
        let scratch_bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| pixels.checked_mul(BYTES_PER_PIXEL))
            .filter(|bytes| *bytes <= parameters.scratch_budget_bytes)
            .ok_or(GaussianBlurPrepareError::ScratchBudgetExceeded)?;
        let uniform = BlurUniform::new(
            parameters,
            size,
            scope
                .uniform
                .as_ref()
                .filter(|_| scope.sigma == Some(sigma)),
        );
        // Every input rejection above leaves the prior coherent scope intact.
        stats.kernel_builds = usize::from(scope.sigma != Some(sigma));
        let programs = self.programs.get_or_insert_with(|| {
            stats.pipeline_compiles = 3;
            Programs::new(device)
        });
        let recreate = scope.resources.as_ref().is_none_or(|r| r.size != size);
        let source_changed = scope.resources.as_ref().is_none_or(|r| r.source != *source);
        if recreate {
            scope.resources = Some(programs.resources(device, source, size));
            stats.texture_allocations = 7;
            stats.buffer_allocations = 1;
            stats.bind_group_creations = 3;
        } else if source_changed {
            let r = scope.resources.as_mut().expect("retained blur resources");
            let view = source.create_view(&Default::default());
            r.bindings[0] = programs.binding(device, &r.uniform, [&view; 3]);
            r.source = source.clone();
            stats.bind_group_creations = 1;
        }
        let r = scope.resources.as_ref().expect("prepared blur resources");
        let kernel_changed = scope
            .uniform
            .as_ref()
            .is_none_or(|old| !old.same_kernel(&uniform));
        if recreate || kernel_changed {
            queue.write_buffer(&r.uniform, 0, bytemuck::bytes_of(&uniform));
            stats.bytes_uploaded = size_of::<BlurUniform>();
        } else if scope.uniform != Some(uniform) {
            queue.write_buffer(&r.uniform, 16, bytemuck::cast_slice(&uniform.control));
            stats.bytes_uploaded = 16;
        }
        scope.blur_dirty |= recreate
            || source_changed
            || kernel_changed
            || scope.source_revision != Some(capture.revision);
        scope.output_dirty |= scope.blur_dirty || scope.uniform != Some(uniform);
        scope.uniform = Some(uniform);
        scope.sigma = Some(sigma);
        scope.source_revision = Some(capture.revision);
        scope.active = true;
        stats.scratch_bytes = scratch_bytes;
        Ok(stats)
    }

    /// Encodes only dirty passes. If the encoder is abandoned, invalidate the
    /// scope before retrying. The host owns padding, painter placement and fences.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut GaussianBlurScope,
    ) -> GaussianBlurStats {
        let mut stats = GaussianBlurStats {
            scratch_bytes: scope.scratch_bytes(),
            ..Default::default()
        };
        if !scope.active {
            return stats;
        }
        let programs = self.programs.as_ref().expect("active blur programs");
        let r = scope.resources.as_ref().expect("active blur resources");
        if scope.blur_dirty {
            encode_pass(
                encoder,
                r.horizontal.each_ref(),
                &programs.pipelines[0],
                &r.bindings[0],
            );
            encode_pass(
                encoder,
                r.vertical.each_ref(),
                &programs.pipelines[1],
                &r.bindings[1],
            );
            stats.blur_passes = 2;
        }
        if scope.output_dirty {
            encode_pass(encoder, [&r.output], &programs.pipelines[2], &r.bindings[2]);
            stats.output_passes = 1;
        }
        scope.blur_dirty = false;
        scope.output_dirty = false;
        stats
    }
}

fn encode_pass<const N: usize>(
    encoder: &mut wgpu::CommandEncoder,
    images: [&Image; N],
    pipeline: &wgpu::RenderPipeline,
    binding: &wgpu::BindGroup,
) {
    let attachments = images.map(|image| {
        Some(wgpu::RenderPassColorAttachment {
            view: &image.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Noon RGBA Gaussian pass"),
        color_attachments: &attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, binding, &[]);
    pass.draw(0..3, 0..1);
}

impl Programs {
    fn new(device: &wgpu::Device) -> Self {
        let uniform = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<BlurUniform>() as u64),
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
            label: Some("Noon RGBA Gaussian bindings"),
            entries: &[uniform, texture(1), texture(2), texture(3)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon RGBA Gaussian pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("gaussian_blur.wgsl"));
        let pipelines = std::array::from_fn(|at| {
            let entry = ["fs_horizontal", "fs_vertical", "fs_output"][at];
            let target = Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            });
            let targets = if at == 2 {
                vec![target]
            } else {
                vec![target.clone(), target.clone(), target]
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        Self { layout, pipelines }
    }

    fn binding(
        &self,
        device: &wgpu::Device,
        uniform: &wgpu::Buffer,
        images: [&wgpu::TextureView; 3],
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon RGBA Gaussian binding"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(images[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(images[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(images[2]),
                },
            ],
        })
    }

    fn resources(
        &self,
        device: &wgpu::Device,
        source: &wgpu::Texture,
        size: [u32; 2],
    ) -> Resources {
        let horizontal = std::array::from_fn(|_| Image::new(device, size));
        let vertical = std::array::from_fn(|_| Image::new(device, size));
        let output = Image::new(device, size);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon RGBA Gaussian uniform"),
            size: size_of::<BlurUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = source.create_view(&Default::default());
        let bindings = [
            self.binding(device, &uniform, [&view; 3]),
            self.binding(
                device,
                &uniform,
                horizontal.each_ref().map(|image| &image.view),
            ),
            self.binding(
                device,
                &uniform,
                vertical.each_ref().map(|image| &image.view),
            ),
        ];
        Resources {
            size,
            horizontal,
            vertical,
            output,
            uniform,
            bindings,
            source: source.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
