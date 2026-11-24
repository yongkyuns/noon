use super::encoded_display::EncodedDisplayPresenter;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputTransfer {
    #[default]
    Direct,
    /// Keep scene rendering in encoded UNORM space, then decode exactly once
    /// before an sRGB store. Browser WebGL also needs this for its implicit
    /// drawing-buffer transfer, even with an exposed `Rgba8Unorm` surface.
    SrgbStore,
}

impl OutputTransfer {
    fn for_target(backend: wgpu::Backend, format: wgpu::TextureFormat) -> Self {
        // A float radiance image is an offscreen scene input, never the browser
        // drawing buffer. Its output transfer belongs after HDR resolution.
        if matches!(format, wgpu::TextureFormat::Rgba16Float) {
            Self::Direct
        } else if format.is_srgb() {
            Self::SrgbStore
        } else if cfg!(target_arch = "wasm32") {
            Self::for_browser_backend(backend)
        } else {
            Self::Direct
        }
    }

    pub const fn for_browser_backend(backend: wgpu::Backend) -> Self {
        match backend {
            wgpu::Backend::Gl => Self::SrgbStore,
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
    presenter: Option<EncodedDisplayPresenter>,
}

impl PresentationBridge {
    pub(crate) fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        transfer: OutputTransfer,
        viewport_size: [u32; 2],
    ) -> Self {
        // Native sRGB attachments and the browser GL drawing buffer both apply
        // a store transfer. Resolve it once for every retained rendering host.
        let transfer = if transfer == OutputTransfer::Direct {
            OutputTransfer::for_target(device.adapter_info().backend, surface_format)
        } else {
            transfer
        };

        let scene_format = match transfer {
            OutputTransfer::Direct => surface_format,
            OutputTransfer::SrgbStore => surface_format.remove_srgb_suffix(),
        };
        let mut result = Self {
            transfer,
            scene_format,
            viewport_size: [viewport_size[0].max(1), viewport_size[1].max(1)],
            texture: None,
            view: None,
            presenter: None,
        };
        if transfer == OutputTransfer::SrgbStore {
            result.presenter = Some(
                EncodedDisplayPresenter::new(device, surface_format, transfer)
                    .expect("encoded presentation uses RGBA8/BGRA8 output"),
            );
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
        if self.transfer == OutputTransfer::SrgbStore {
            self.recreate_scene_target(device);
        }
    }

    pub(crate) fn scene_view<'a>(
        &'a self,
        surface_view: &'a wgpu::TextureView,
    ) -> &'a wgpu::TextureView {
        self.view.as_ref().unwrap_or(surface_view)
    }

    #[cfg(test)]
    pub(super) fn scene_target_for_tests(&self) -> Option<&wgpu::Texture> {
        self.texture.as_ref()
    }

    pub(crate) fn encode_present(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        if let Some(presenter) = &self.presenter {
            presenter.encode_view(encoder, surface_view);
        }
    }

    fn recreate_scene_target(&mut self, device: &wgpu::Device) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon retained scene target"),
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
        self.presenter
            .as_mut()
            .expect("presentation requires a presenter")
            .prepare(&texture)
            .expect("presentation scene target is encoded RGBA8");
        self.texture = Some(texture);
        self.view = Some(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_backend_defaults_select_only_the_implicit_gl_store() {
        assert_eq!(
            OutputTransfer::for_browser_backend(wgpu::Backend::Gl),
            OutputTransfer::SrgbStore
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
    fn target_formats_preserve_encoded_bytes_without_touching_float_radiance() {
        for backend in [
            wgpu::Backend::Gl,
            wgpu::Backend::BrowserWebGpu,
            wgpu::Backend::Metal,
        ] {
            assert_eq!(
                OutputTransfer::for_target(backend, wgpu::TextureFormat::Rgba16Float),
                OutputTransfer::Direct
            );
        }
        assert_eq!(
            OutputTransfer::for_target(wgpu::Backend::Gl, wgpu::TextureFormat::Rgba8Unorm),
            if cfg!(target_arch = "wasm32") {
                OutputTransfer::SrgbStore
            } else {
                OutputTransfer::Direct
            }
        );
        for backend in [
            wgpu::Backend::Metal,
            wgpu::Backend::Vulkan,
            wgpu::Backend::BrowserWebGpu,
        ] {
            for format in [
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ] {
                assert_eq!(
                    OutputTransfer::for_target(backend, format),
                    OutputTransfer::SrgbStore
                );
                assert_eq!(
                    OutputTransfer::for_target(backend, format.remove_srgb_suffix()),
                    OutputTransfer::Direct
                );
            }
        }
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
