//! Alpha-correct replacement of a clipped, processed painter prefix.
//!
//! This is a renderer operator, not an authoring API or a backdrop dependency
//! planner. The finite color profile is the renderer's encoded-premultiplied
//! RGBA8 profile, with no additional transfer. The caller supplies inputs including the
//! view's clear color in the prefix. Radius-zero treatments must bypass this
//! intermediate path when exact ordinary-source identity is required.
use super::analytic_glow::Placement;
use super::{EffectPrepareError, EffectRasterStats, GaussianBlurCapture};
use std::mem::size_of;

#[cfg(test)]
mod tests;

/// Three explicitly distinct input roles, on one physical pixel grid.
#[derive(Clone, Copy, Debug)]
pub struct BackdropCompositeInputs<'a> {
    pub processed_prefix: GaussianBlurCapture<'a>,
    pub target_paint: GaussianBlurCapture<'a>,
    pub target_coverage: GaussianBlurCapture<'a>,
}
impl<'a> BackdropCompositeInputs<'a> {
    fn captures(self) -> [GaussianBlurCapture<'a>; 3] {
        [
            self.processed_prefix,
            self.target_paint,
            self.target_coverage,
        ]
    }
}

/// Placement uses final output pixels; opacity applies to the entire panel once.
#[derive(Clone, Copy, Debug)]
pub struct BackdropCompositeParameters {
    pub origin: [i32; 2],
    pub viewport: [u32; 2],
    pub scope_opacity: f32,
    /// Both retained RGBA8 outputs, eight bytes per capture pixel. Inputs and
    /// in-flight host images are separately charged by the enclosing planner.
    pub scratch_budget_bytes: u64,
}
#[derive(Debug)]
struct Resources {
    contribution: wgpu::Texture,
    erase: wgpu::Texture,
    views: [wgpu::TextureView; 2],
    buffer: wgpu::Buffer,
    prepare_binding: wgpu::BindGroup,
    placement_bindings: [wgpu::BindGroup; 2],
    sources: [wgpu::Texture; 3],
}
#[derive(Debug, Default)]
pub struct BackdropCompositeScope {
    resources: Option<Resources>,
    placement: Option<Placement>,
    input_revisions: Option<[u64; 3]>,
    output_revision: u64,
    active: bool,
    invalidated: bool,
}
impl BackdropCompositeScope {
    pub fn contribution(&self) -> Option<&wgpu::Texture> {
        self.resources
            .as_ref()
            .filter(|_| self.active)
            .map(|r| &r.contribution)
    }
    pub fn erase(&self) -> Option<&wgpu::Texture> {
        self.resources
            .as_ref()
            .filter(|_| self.active)
            .map(|r| &r.erase)
    }
    pub fn revision(&self) -> Option<u64> {
        self.active.then_some(self.output_revision)
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.resources.as_ref().map_or(0, |r| {
            u64::from(r.contribution.width()) * u64::from(r.contribution.height()) * 8
        })
    }
    /// Required after abandoning an encoder that contained preparation work.
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
#[derive(Debug)]
struct Program {
    prepare_layout: wgpu::BindGroupLayout,
    placement_layout: wgpu::BindGroupLayout,
    prepare: wgpu::RenderPipeline,
    erase: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
}
/// Lazy shared pipelines. This finite operator places onto a single-sample
/// RGBA8/BGRA8 unorm attachment using erase then additive draws, without
/// framebuffer fetch, dual-source blending or host readback.
#[derive(Debug)]
pub struct BackdropCompositeFilter {
    format: wgpu::TextureFormat,
    program: Option<Program>,
}
impl BackdropCompositeFilter {
    pub fn new(format: wgpu::TextureFormat) -> Result<Self, EffectPrepareError> {
        if !supported_format(format) {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        Ok(Self {
            format,
            program: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut BackdropCompositeScope,
        inputs: BackdropCompositeInputs<'_>,
        parameters: BackdropCompositeParameters,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let captures = inputs.captures();
        let size = captures[0].source.size();
        let placement = checked_placement(device, &captures, parameters)?;
        // Reject feedback, including a resized or inactive scope, before any
        // writes, resource replacement or command encoding.
        if scope.resources.as_ref().is_some_and(|r| {
            captures
                .iter()
                .any(|c| c.source == &r.contribution || c.source == &r.erase)
        }) {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if parameters.scope_opacity == 0.0 {
            if scope.scratch_bytes() > parameters.scratch_budget_bytes {
                return Err(EffectPrepareError::ScratchBudgetExceeded);
            }
            scope.active = false;
            return Ok(EffectRasterStats {
                scratch_bytes: scope.scratch_bytes(),
                ..Default::default()
            });
        }
        let bytes = u64::from(size.width) * u64::from(size.height) * 8;
        if bytes > parameters.scratch_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        let replace = scope
            .resources
            .as_ref()
            .is_none_or(|r| r.contribution.size() != size);
        let rebind = scope.resources.as_ref().is_some_and(|r| {
            r.sources
                .iter()
                .zip(captures)
                .any(|(old, new)| old != new.source)
        });
        let revisions = captures.map(|c| c.revision);
        // Placement-only changes upload one uniform but do not rerun the pixel
        // operator. Its pixel equation depends only on inputs and scope opacity.
        let dirty = replace
            || rebind
            || scope.invalidated
            || scope
                .placement
                .is_none_or(|p| p.opacity != placement.opacity)
            || scope.input_revisions != Some(revisions);
        let revision = if dirty {
            scope
                .output_revision
                .checked_add(1)
                .ok_or(EffectPrepareError::RevisionExhausted)?
        } else {
            scope.output_revision
        };
        let mut stats = EffectRasterStats {
            scratch_bytes: bytes,
            ..Default::default()
        };
        let program = self.program.get_or_insert_with(|| {
            stats.pipeline_compiles = 3;
            Program::new(device, self.format)
        });
        if replace {
            let image = |label| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            };
            let contribution = image("Noon backdrop contribution");
            let erase = image("Noon backdrop erase coverage");
            let views = [&contribution, &erase].map(|t| t.create_view(&Default::default()));
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Noon backdrop composite placement"),
                size: size_of::<Placement>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let prepare_binding = program.bind_inputs(device, &buffer, captures);
            let placement_bindings = views
                .each_ref()
                .map(|view| program.bind_placement(device, &buffer, view));
            scope.resources = Some(Resources {
                contribution,
                erase,
                views,
                buffer,
                prepare_binding,
                placement_bindings,
                sources: captures.map(|c| c.source.clone()),
            });
            stats.texture_allocations = 2;
            stats.buffer_allocations = 1;
            stats.bind_group_creations = 3;
        }
        let r = scope
            .resources
            .as_mut()
            .expect("prepared backdrop resources");
        if !replace && rebind {
            r.prepare_binding = program.bind_inputs(device, &r.buffer, captures);
            r.sources = captures.map(|c| c.source.clone());
            stats.bind_group_creations += 1;
        }
        if replace || scope.placement != Some(placement) {
            queue.write_buffer(&r.buffer, 0, bytemuck::bytes_of(&placement));
            stats.bytes_uploaded = size_of::<Placement>();
        }
        if dirty {
            let attachments = r.views.each_ref().map(|view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon backdrop contribution and coverage"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&program.prepare);
            pass.set_bind_group(0, &r.prepare_binding, &[]);
            pass.draw(0..3, 0..1);
            stats.composite_passes = 1;
        }
        scope.placement = Some(placement);
        scope.input_revisions = Some(revisions);
        scope.output_revision = revision;
        scope.invalidated = false;
        scope.active = true;
        Ok(stats)
    }

    /// Place immediately at the target's existing painter anchor, in a pass
    /// matching this filter's format and the prepared viewport. The caller owns
    /// the attachment, encoder and submission; no sampling of that attachment
    /// occurs. Use the same filter that successfully prepared the scope.
    pub fn encode(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scope: &BackdropCompositeScope,
    ) -> EffectRasterStats {
        if !scope.active {
            return EffectRasterStats::default();
        }
        let program = self.program.as_ref().expect("prepared backdrop filter");
        let r = scope.resources.as_ref().expect("prepared backdrop scope");
        pass.set_pipeline(&program.erase);
        pass.set_bind_group(0, &r.placement_bindings[1], &[]);
        pass.draw(0..6, 0..1);
        pass.set_pipeline(&program.add);
        pass.set_bind_group(0, &r.placement_bindings[0], &[]);
        pass.draw(0..6, 0..1);
        EffectRasterStats {
            composite_passes: 2,
            scratch_bytes: scope.scratch_bytes(),
            ..Default::default()
        }
    }
}
fn supported_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
    )
}
fn checked_placement(
    device: &wgpu::Device,
    inputs: &[GaussianBlurCapture<'_>; 3],
    p: BackdropCompositeParameters,
) -> Result<Placement, EffectPrepareError> {
    if !p.scope_opacity.is_finite() || !(0.0..=1.0).contains(&p.scope_opacity) {
        return Err(EffectPrepareError::InvalidScopeOpacity);
    }
    let size = inputs[0].source.size();
    for c in inputs {
        let t = c.source;
        if !supported_format(t.format())
            || t.dimension() != wgpu::TextureDimension::D2
            || t.sample_count() != 1
            || t.depth_or_array_layers() != 1
            || t.mip_level_count() != 1
            || !t.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if t.size() != size {
            return Err(EffectPrepareError::CaptureSizeMismatch);
        }
    }
    if p.viewport.contains(&0) {
        return Err(EffectPrepareError::InvalidProjection);
    }
    if p.viewport
        .into_iter()
        .chain([size.width, size.height])
        .any(|v| v > device.limits().max_texture_dimension_2d)
    {
        return Err(EffectPrepareError::ExtentExceedsDevice);
    }
    if p.origin.into_iter().any(|v| v.unsigned_abs() > 1_048_576) {
        return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
    }
    Ok(Placement {
        origin: p.origin,
        size: [size.width, size.height],
        viewport: p.viewport,
        opacity: p.scope_opacity,
        padding: 0,
    })
}
impl Program {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("backdrop_composite.wgsl"));
        Self::from_shader(device, format, &shader)
    }
    fn from_shader(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader: &wgpu::ShaderModule,
    ) -> Self {
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: if binding == 0 {
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Placement>() as u64),
                }
            } else {
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }
            },
            count: None,
        };
        let prepare_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon backdrop inputs"),
            entries: &[entry(0), entry(1), entry(2), entry(3)],
        });
        let placement_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon backdrop output"),
            entries: &[entry(0), entry(1)],
        });
        let target = |format, blend| {
            Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let pipeline = |label,
                        layout: &wgpu::BindGroupLayout,
                        vertex,
                        fragment,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets,
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let prepare = pipeline(
            "Noon backdrop prepare",
            &prepare_layout,
            "vs_fullscreen",
            "fs_prepare",
            &[
                target(wgpu::TextureFormat::Rgba8Unorm, None),
                target(wgpu::TextureFormat::Rgba8Unorm, None),
            ],
        );
        let erase_component = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let add_component = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let erase = pipeline(
            "Noon backdrop erase",
            &placement_layout,
            "vs_tile",
            "fs_tile",
            &[target(
                format,
                Some(wgpu::BlendState {
                    color: erase_component,
                    alpha: erase_component,
                }),
            )],
        );
        let add = pipeline(
            "Noon backdrop add",
            &placement_layout,
            "vs_tile",
            "fs_tile",
            &[target(
                format,
                Some(wgpu::BlendState {
                    color: add_component,
                    alpha: add_component,
                }),
            )],
        );
        Self {
            prepare_layout,
            placement_layout,
            prepare,
            erase,
            add,
        }
    }
    fn bind_inputs(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
        captures: [GaussianBlurCapture<'_>; 3],
    ) -> wgpu::BindGroup {
        let views = captures.map(|c| c.source.create_view(&Default::default()));
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon backdrop input binding"),
            layout: &self.prepare_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
            ],
        })
    }
    fn bind_placement(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon backdrop placement binding"),
            layout: &self.placement_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }
}
