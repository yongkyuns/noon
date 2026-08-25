#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputTransfer {
    #[default]
    Direct,
    /// The browser WebGL drawing buffer applies an sRGB store transfer even
    /// though wgpu exposes the surface as `Rgba8Unorm`. Keep scene rendering in
    /// encoded UNORM space, then decode exactly once before presentation.
    BrowserWebGlSrgb,
}

impl OutputTransfer {
    pub const fn for_browser_backend(backend: wgpu::Backend) -> Self {
        match backend {
            wgpu::Backend::Gl => Self::BrowserWebGlSrgb,
            _ => Self::Direct,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PresentationBridge {
    transfer: OutputTransfer,
    scene_format: wgpu::TextureFormat,
    viewport_size: [u32; 2],
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    bind_group: Option<wgpu::BindGroup>,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl PresentationBridge {
    pub(crate) fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        transfer: OutputTransfer,
        viewport_size: [u32; 2],
    ) -> Self {
        // Browser WebGL exposes an ordinary `Rgba8Unorm` wgpu surface, but its
        // drawing buffer still applies the browser's sRGB presentation transfer.
        // Resolve that backend fact here so every canvas host gets the same color
        // contract without duplicating adapter plumbing in noon-web.
        #[cfg(target_arch = "wasm32")]
        let transfer = if transfer == OutputTransfer::Direct {
            OutputTransfer::for_browser_backend(device.adapter_info().backend)
        } else {
            transfer
        };

        let scene_format = match transfer {
            OutputTransfer::Direct => surface_format,
            OutputTransfer::BrowserWebGlSrgb => surface_format.remove_srgb_suffix(),
        };
        let mut result = Self {
            transfer,
            scene_format,
            viewport_size: [viewport_size[0].max(1), viewport_size[1].max(1)],
            texture: None,
            view: None,
            bind_group_layout: None,
            bind_group: None,
            pipeline: None,
        };
        if transfer == OutputTransfer::BrowserWebGlSrgb {
            result.initialize_decode_pipeline(device, surface_format);
            result.recreate_scene_target(device);
        }
        result
    }

    pub(crate) const fn scene_format(&self) -> wgpu::TextureFormat {
        self.scene_format
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, viewport_size: [u32; 2]) {
        let viewport_size = [viewport_size[0].max(1), viewport_size[1].max(1)];
        if self.viewport_size == viewport_size {
            return;
        }
        self.viewport_size = viewport_size;
        if self.transfer == OutputTransfer::BrowserWebGlSrgb {
            self.recreate_scene_target(device);
        }
    }

    pub(crate) fn scene_view<'a>(
        &'a self,
        surface_view: &'a wgpu::TextureView,
    ) -> &'a wgpu::TextureView {
        self.view.as_ref().unwrap_or(surface_view)
    }

    pub(crate) fn encode_present(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        let (Some(pipeline), Some(bind_group)) = (&self.pipeline, &self.bind_group) else {
            return;
        };
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: surface_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon browser presentation transfer pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn initialize_decode_pipeline(
        &mut self,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon browser presentation bind group layout"),
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
            label: Some("Noon browser presentation pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("presentation.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Noon browser sRGB presentation pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_present"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_present"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
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
        self.bind_group_layout = Some(bind_group_layout);
        self.pipeline = Some(pipeline);
    }

    fn recreate_scene_target(&mut self, device: &wgpu::Device) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon encoded browser scene target"),
            size: wgpu::Extent3d {
                width: self.viewport_size[0],
                height: self.viewport_size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.scene_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon browser presentation bind group"),
            layout: self
                .bind_group_layout
                .as_ref()
                .expect("decode presentation requires a bind group layout"),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        self.texture = Some(texture);
        self.view = Some(view);
        self.bind_group = Some(bind_group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_browser_gl_needs_the_presentation_decode() {
        assert_eq!(
            OutputTransfer::for_browser_backend(wgpu::Backend::Gl),
            OutputTransfer::BrowserWebGlSrgb
        );
        assert_eq!(
            OutputTransfer::for_browser_backend(wgpu::Backend::BrowserWebGpu),
            OutputTransfer::Direct
        );
        assert_eq!(
            OutputTransfer::for_browser_backend(wgpu::Backend::Vulkan),
            OutputTransfer::Direct
        );
    }

    #[test]
    fn webgl_scene_target_strips_only_the_surface_srgb_suffix() {
        assert_eq!(
            wgpu::TextureFormat::Rgba8UnormSrgb.remove_srgb_suffix(),
            wgpu::TextureFormat::Rgba8Unorm
        );
        assert_eq!(
            wgpu::TextureFormat::Rgba8Unorm.remove_srgb_suffix(),
            wgpu::TextureFormat::Rgba8Unorm
        );
    }
}
