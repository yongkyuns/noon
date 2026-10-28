//! Final presentation of already encoded, alpha-associated SDR bytes.
//! No scene conversion, filtering, authored state or time lives here.
use super::OutputTransfer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPresentationError {
    UnsupportedTarget,
    UnsupportedSource,
    ExtentMismatch,
    Unprepared,
}
impl std::fmt::Display for DisplayPresentationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot present encoded SDR output: {self:?}")
    }
}
impl std::error::Error for DisplayPresentationError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayPresentationStats {
    pub pipeline_compiles: usize,
    pub bind_group_creations: usize,
    pub passes: usize,
}

/// One finite output-format specialization and one retained source lease.
/// UNORM targets receive encoded bytes unchanged. sRGB attachments (and the
/// browser GL drawing buffer) decode once immediately before their store
/// transfer, which reconstructs those bytes. Alpha is never decoded or scaled.
/// No intermediate image, sampler, parameter buffer or per-frame upload.
#[derive(Debug)]
pub struct EncodedDisplayPresenter {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    decode_before_store: bool,
    program: Option<(wgpu::BindGroupLayout, wgpu::RenderPipeline)>,
    source: Option<wgpu::Texture>,
    binding: Option<wgpu::BindGroup>,
}
impl EncodedDisplayPresenter {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        transfer: OutputTransfer,
    ) -> Result<Self, DisplayPresentationError> {
        use wgpu::TextureFormat::*;
        if !matches!(
            format,
            Rgba8Unorm | Rgba8UnormSrgb | Bgra8Unorm | Bgra8UnormSrgb
        ) {
            return Err(DisplayPresentationError::UnsupportedTarget);
        }
        Ok(Self {
            device: device.clone(),
            format,
            decode_before_store: format.is_srgb() || transfer == OutputTransfer::BrowserWebGlSrgb,
            program: None,
            source: None,
            binding: None,
        })
    }

    /// Validate before replacing the current source or creating a pipeline.
    /// A changed source rebinds once; changed pixels in the same texture require
    /// no preparation work. Source/destination must belong to the constructing
    /// device; create a new presenter after device replacement. Submission and
    /// synchronization remain host-owned.
    pub fn prepare(
        &mut self,
        source: &wgpu::Texture,
    ) -> Result<DisplayPresentationStats, DisplayPresentationError> {
        let device = &self.device;
        if !matches!(
            source.format(),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ) || source.dimension() != wgpu::TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.sample_count() != 1
            || source.mip_level_count() != 1
            || !source
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(DisplayPresentationError::UnsupportedSource);
        }
        let mut stats = DisplayPresentationStats::default();
        if self.program.is_none() {
            self.program = Some(self.create_program(device));
            stats.pipeline_compiles = 1;
        }
        if self.source.as_ref() != Some(source) {
            let view = source.create_view(&Default::default());
            self.binding = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Noon encoded display source"),
                layout: &self.program.as_ref().expect("prepared program").0,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            }));
            self.source = Some(source.clone());
            stats.bind_group_creations = 1;
        }
        Ok(stats)
    }

    /// One full-size point load, with no scaling, blending or second tone map.
    /// Reject invalid destinations before adding any commands to the encoder.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
    ) -> Result<DisplayPresentationStats, DisplayPresentationError> {
        let source = self
            .source
            .as_ref()
            .ok_or(DisplayPresentationError::Unprepared)?;
        if target.format() != self.format
            || target.dimension() != wgpu::TextureDimension::D2
            || target.depth_or_array_layers() != 1
            || target.sample_count() != 1
            || target.mip_level_count() != 1
            || !target
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || target == source
        {
            return Err(DisplayPresentationError::UnsupportedTarget);
        }
        if target.size() != source.size() {
            return Err(DisplayPresentationError::ExtentMismatch);
        }
        let view = target.create_view(&Default::default());
        self.encode_view(encoder, &view);
        Ok(DisplayPresentationStats {
            passes: 1,
            ..Default::default()
        })
    }

    /// Retire the source lease/binding; keep the format program reusable.
    pub fn clear(&mut self) {
        self.source = None;
        self.binding = None;
    }

    // The existing browser bridge already owns/validates its surface extent and
    // format; its surface API supplies a view rather than a texture.
    pub(super) fn encode_view(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let (Some((_, pipeline)), Some(binding)) = (&self.program, &self.binding) else {
            return;
        };
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon encoded display presentation"),
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

    fn create_program(
        &self,
        device: &wgpu::Device,
    ) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon encoded display layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon encoded display pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("../presentation.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Noon encoded display pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_present"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if self.decode_before_store {
                    "fs_present"
                } else {
                    "fs_copy"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        (layout, pipeline)
    }
}
#[cfg(test)]
mod tests;
