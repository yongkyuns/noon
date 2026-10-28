//! Retained linear RGBA16Float bloom and explicit SDR display resolution.
//!
//! Renderer prerequisite for #1897. The caller owns scene emission, linear
//! compositing, padding, submission and final presentation. This module neither
//! turns an encoded LDR scene into HDR nor establishes a creator-facing profile.
use std::{mem::size_of, sync::Arc};

use bytemuck::{Pod, Zeroable};

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DISPLAY: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u64 = 36;
const SHADER: &str = include_str!("hdr_bloom.wgsl");

/// Linear, alpha-associated radiance. The caller guarantees finite nonnegative
/// RGB <= 64 * alpha, alpha in [0, 1], and transparent black at alpha zero.
/// Brightness above one is meaningful and is never clamped before resolution.
#[derive(Clone, Copy, Debug)]
pub struct HdrBloomCapture<'a> {
    pub source: &'a wgpu::Texture,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct HdrBloomParameters {
    /// Standard deviation in final physical pixels, [0, 64].
    pub sigma_pixels: f64,
    /// Threshold of alpha-associated linear Rec.709 luminance, [0, 64].
    pub threshold: f32,
    /// Added bloom energy multiplier, [0, 8]. Zero still resolves the HDR base.
    pub intensity: f32,
    /// Four float images and one unorm image. Source/in-flight images excluded.
    pub scratch_budget_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdrBloomPrepareError {
    InvalidRadius,
    InvalidThreshold,
    InvalidIntensity,
    UnsupportedCapture,
    UnsupportedFloatAttachment,
    ExtentExceedsDevice,
    ScratchBudgetExceeded,
    WrongOwner,
    WrongDevice,
}
impl std::fmt::Display for HdrBloomPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot prepare linear HDR bloom: {self:?}")
    }
}
impl std::error::Error for HdrBloomPrepareError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HdrBloomStats {
    pub pipeline_compiles: usize,
    pub kernel_builds: usize,
    pub texture_allocations: usize,
    pub buffer_allocations: usize,
    pub bind_group_creations: usize,
    pub bytes_uploaded: usize,
    pub scratch_bytes: u64,
    /// Extract, horizontal, vertical, combine, display resolve, respectively.
    pub passes: [usize; 5],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct BloomUniform {
    size: [u32; 2],
    radius: u32,
    reserved: u32,
    control: [f32; 4],
    weights: [[f32; 4]; 64],
}
impl BloomUniform {
    fn new(p: HdrBloomParameters, size: [u32; 2], cached: Option<&Self>) -> Self {
        let mut result = Self {
            size,
            radius: (3.0 * p.sigma_pixels).ceil() as u32,
            reserved: 0,
            control: [p.threshold, p.intensity, 0.0, 0.0],
            weights: [[0.0; 4]; 64],
        };
        if let Some(cached) = cached {
            result.weights = cached.weights;
        } else if p.sigma_pixels == 0.0 {
            result.weights[0][0] = 1.0;
        } else {
            let weight = |x: u32| (-0.5 * (f64::from(x) / p.sigma_pixels).powi(2)).exp();
            let mass = 1.0 + 2.0 * (1..=result.radius).map(weight).sum::<f64>();
            for x in 0..=result.radius as usize {
                result.weights[x / 4][x % 4] = (weight(x as u32) / mass) as f32;
            }
        }
        result
    }
}

#[derive(Debug)]
struct Image {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
impl Image {
    fn new(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon linear HDR bloom image"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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
    images: [Image; 5],
    uniform: wgpu::Buffer,
    bindings: [wgpu::BindGroup; 5],
    source: wgpu::Texture,
}
/// Disposable GPU state. A scope belongs to one filter/device until cleared.
#[derive(Debug, Default)]
pub struct HdrBloomScope {
    resources: Option<Resources>,
    owner: Option<Arc<()>>,
    uniform: Option<BloomUniform>,
    sigma: Option<f64>,
    revision: Option<u64>,
    dirty: [bool; 5],
}
impl HdrBloomScope {
    /// Valid after encoding and submitting all current dirty work.
    pub fn linear_output(&self) -> Option<&wgpu::Texture> {
        self.resources.as_ref().map(|r| &r.images[3].texture)
    }
    /// Encoded-premultiplied SDR bytes, for a non-sRGB attachment/presentation
    /// path. Applying another output transfer would be incorrect.
    pub fn display_output(&self) -> Option<&wgpu::Texture> {
        self.resources.as_ref().map(|r| &r.images[4].texture)
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.resources.as_ref().map_or(0, |r| {
            u64::from(r.size[0]) * u64::from(r.size[1]) * BYTES_PER_PIXEL
        })
    }
    /// Required if an encoded command buffer was abandoned before submission.
    pub fn invalidate(&mut self) {
        self.dirty.fill(true);
    }
    /// Retire every derived texture/binding. Programs stay with the filter.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
#[derive(Debug)]
struct Programs {
    layout: wgpu::BindGroupLayout,
    pipelines: [wgpu::RenderPipeline; 5],
}
#[derive(Debug)]
pub struct HdrBloomFilter {
    device: wgpu::Device,
    float_targets: bool,
    owner: Arc<()>,
    programs: Option<Programs>,
}
impl HdrBloomFilter {
    /// The adapter must be the one that created `device`. Query its actual
    /// format usages, including downlevel/WebGL restrictions. Unsupported
    /// adapters get a typed preparation error with no pipelines or allocations.
    pub fn new(adapter: &wgpu::Adapter, device: &wgpu::Device) -> Self {
        let usages = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        Self {
            device: device.clone(),
            float_targets: adapter
                .get_texture_format_features(HDR)
                .allowed_usages
                .contains(usages),
            owner: Arc::new(()),
            programs: None,
        }
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scope: &mut HdrBloomScope,
        capture: HdrBloomCapture<'_>,
        p: HdrBloomParameters,
    ) -> Result<HdrBloomStats, HdrBloomPrepareError> {
        use HdrBloomPrepareError as E;
        if !p.sigma_pixels.is_finite() || !(0.0..=64.0).contains(&p.sigma_pixels) {
            return Err(E::InvalidRadius);
        }
        if !p.threshold.is_finite() || !(0.0..=64.0).contains(&p.threshold) {
            return Err(E::InvalidThreshold);
        }
        if !p.intensity.is_finite() || !(0.0..=8.0).contains(&p.intensity) {
            return Err(E::InvalidIntensity);
        }
        if scope
            .owner
            .as_ref()
            .is_some_and(|owner| !Arc::ptr_eq(owner, &self.owner))
        {
            return Err(E::WrongOwner);
        }
        if self.device != *device {
            return Err(E::WrongDevice);
        }
        let source = capture.source;
        if source.format() != HDR
            || source.dimension() != wgpu::TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.sample_count() != 1
            || !source
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(E::UnsupportedCapture);
        }
        let usages = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        if !self.float_targets
            || !HDR
                .guaranteed_format_features(device.features())
                .allowed_usages
                .contains(usages)
            || device.limits().max_color_attachments < 1
            || device.limits().max_color_attachment_bytes_per_sample < 8
        {
            return Err(E::UnsupportedFloatAttachment);
        }
        let size = [source.width(), source.height()];
        if size.contains(&0)
            || size
                .into_iter()
                .any(|x| x > device.limits().max_texture_dimension_2d)
        {
            return Err(E::ExtentExceedsDevice);
        }
        let bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|n| n.checked_mul(BYTES_PER_PIXEL))
            .filter(|n| *n <= p.scratch_budget_bytes)
            .ok_or(E::ScratchBudgetExceeded)?;
        // All rejections preserve the previous coherent scope and program cache.
        let kernel_changed = scope.sigma != Some(p.sigma_pixels);
        let uniform =
            BloomUniform::new(p, size, scope.uniform.as_ref().filter(|_| !kernel_changed));
        let mut stats = HdrBloomStats {
            scratch_bytes: bytes,
            kernel_builds: usize::from(kernel_changed),
            ..Default::default()
        };
        let programs = self.programs.get_or_insert_with(|| {
            stats.pipeline_compiles = 5;
            Programs::from_source(device, SHADER)
        });
        let recreate = scope.resources.as_ref().is_none_or(|r| r.size != size);
        let source_changed = scope.resources.as_ref().is_none_or(|r| r.source != *source);
        if recreate {
            scope.resources = Some(programs.resources(device, source, size));
            stats.texture_allocations = 5;
            stats.buffer_allocations = 1;
            stats.bind_group_creations = 5;
        } else if source_changed {
            let r = scope.resources.as_mut().expect("prepared HDR resources");
            let view = source.create_view(&Default::default());
            r.bindings[0] = programs.binding(device, &r.uniform, [&view, &view]);
            r.bindings[3] = programs.binding(device, &r.uniform, [&view, &r.images[2].view]);
            r.source = source.clone();
            stats.bind_group_creations = 2;
        }
        let r = scope.resources.as_ref().expect("prepared HDR resources");
        if recreate || kernel_changed {
            queue.write_buffer(&r.uniform, 0, bytemuck::bytes_of(&uniform));
            stats.bytes_uploaded = size_of::<BloomUniform>();
        } else if scope.uniform != Some(uniform) {
            queue.write_buffer(&r.uniform, 16, bytemuck::cast_slice(&uniform.control));
            stats.bytes_uploaded = 16;
        }
        let input_changed = recreate || source_changed || scope.revision != Some(capture.revision);
        let threshold_changed = scope
            .uniform
            .is_none_or(|old| old.control[0] != p.threshold);
        let intensity_changed = scope
            .uniform
            .is_none_or(|old| old.control[1] != p.intensity);
        scope.dirty[0] |= input_changed || threshold_changed;
        scope.dirty[1] |= scope.dirty[0] || kernel_changed;
        scope.dirty[2] |= scope.dirty[1];
        scope.dirty[3] |=
            input_changed || intensity_changed || (p.intensity != 0.0 && scope.dirty[2]);
        scope.dirty[4] |= scope.dirty[3];
        scope.owner = Some(self.owner.clone());
        scope.uniform = Some(uniform);
        scope.sigma = Some(p.sigma_pixels);
        scope.revision = Some(capture.revision);
        Ok(stats)
    }
    /// No polling, submission or host clock. A clean scope encodes no passes.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut HdrBloomScope,
    ) -> HdrBloomStats {
        let mut stats = HdrBloomStats {
            scratch_bytes: scope.scratch_bytes(),
            ..Default::default()
        };
        let Some(r) = scope.resources.as_ref() else {
            return stats;
        };
        assert!(
            scope
                .owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner)),
            "HDR bloom scope belongs to another filter"
        );
        let programs = self.programs.as_ref().expect("prepared HDR programs");
        let enabled = scope.uniform.expect("prepared HDR parameters").control[1] != 0.0;
        for at in 0..5 {
            if scope.dirty[at] && (at >= 3 || enabled) {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Noon linear HDR bloom pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &r.images[at].view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&programs.pipelines[at]);
                pass.set_bind_group(0, &r.bindings[at], &[]);
                pass.draw(0..3, 0..1);
                scope.dirty[at] = false;
                stats.passes[at] = 1;
            }
        }
        stats
    }
}
impl Programs {
    fn from_source(device: &wgpu::Device, source: &str) -> Self {
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
            label: Some("Noon HDR bloom layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<BloomUniform>() as u64),
                    },
                    count: None,
                },
                texture(1),
                texture(2),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon HDR bloom pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon HDR bloom"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipelines = std::array::from_fn(|at| {
            let entry = [
                "fs_extract",
                "fs_horizontal",
                "fs_vertical",
                "fs_combine",
                "fs_resolve",
            ][at];
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
                    targets: &[Some(wgpu::ColorTargetState {
                        format: if at == 4 { DISPLAY } else { HDR },
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
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
        views: [&wgpu::TextureView; 2],
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon HDR bloom binding"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(views[1]),
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
        let images =
            std::array::from_fn(|at| Image::new(device, size, if at == 4 { DISPLAY } else { HDR }));
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon HDR bloom parameters"),
            size: size_of::<BloomUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = source.create_view(&Default::default());
        let bindings = [
            self.binding(device, &uniform, [&view, &view]),
            self.binding(device, &uniform, [&images[0].view, &images[0].view]),
            self.binding(device, &uniform, [&images[1].view, &images[1].view]),
            self.binding(device, &uniform, [&view, &images[2].view]),
            self.binding(device, &uniform, [&images[3].view, &images[3].view]),
        ];
        Resources {
            size,
            images,
            uniform,
            bindings,
            source: source.clone(),
        }
    }
}
#[cfg(test)]
mod tests;
