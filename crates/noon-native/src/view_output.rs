//! Native view resources only. The current Runtime publication owns profile,
//! camera and Bloom values; this cache owns no authored or scheduling state.
use super::NativeHostError;
use noon::integration::RendererPublication;
use noon_core::ViewColorProfile;
use noon_render_wgpu::{
    EncodedDisplayPresenter, GpuRenderer, HdrBloomCapture, HdrBloomFilter, HdrBloomScope,
    HdrBloomStats, OutputTransfer, OverlayGpuState,
};

pub(super) struct NativeViewOutput {
    filter: HdrBloomFilter,
    scope: HdrBloomScope,
    presenter: Option<EncodedDisplayPresenter>,
    source: Option<(wgpu::Texture, wgpu::TextureView)>,
    supported: bool,
    surface_format: wgpu::TextureFormat,
    source_revision: u64,
}
// Retained primary-view targets at fixed quality, including source and MSAA.
const NATIVE_HDR_TARGET_BUDGET: u64 = 256 * 1024 * 1024;

fn error(message: &str) -> NativeHostError {
    NativeHostError::Gpu(message.to_owned())
}
impl NativeViewOutput {
    pub fn new(
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let supported = features.allowed_usages.contains(
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        ) && features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            && features.flags.sample_count_supported(4)
            && device.limits().max_color_attachment_bytes_per_sample >= 8
            && matches!(
                surface_format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Rgba8UnormSrgb
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Bgra8UnormSrgb
            );
        Self {
            filter: HdrBloomFilter::new(adapter, device),
            scope: HdrBloomScope::default(),
            presenter: None,
            source: None,
            supported,
            surface_format,
            source_revision: 0,
        }
    }
    /// Source 8 + four-sample scene attachment 32 + Bloom/output scratch 36
    /// bytes/pixel. Surface, scene assets/effect scopes and in-flight leases are
    /// separate owners; this is the retained primary-view target budget.
    pub fn validate_extent(device: &wgpu::Device, size: [u32; 2]) -> Result<(), NativeHostError> {
        let bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| {
                pixels.checked_mul(40).and_then(|scene| {
                    HdrBloomFilter::required_scratch_bytes(size)?.checked_add(scene)
                })
            })
            .ok_or_else(|| error("native HDR target extent overflow"))?;
        if size.contains(&0)
            || size
                .iter()
                .any(|&x| x > device.limits().max_texture_dimension_2d)
            || bytes > NATIVE_HDR_TARGET_BUDGET
        {
            return Err(error(
                "native HDR view exceeds the retained target budget or device extent",
            ));
        }
        Ok(())
    }
    pub fn is_hdr(&self) -> bool {
        self.source.is_some()
    }
    /// Validate before changing target resources or rebuilding the ordinary
    /// renderer. A global/initial profile checks all spatial rows once; later
    /// publications check only their local changed rows.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
        insets_active: bool,
        size: [u32; 2],
    ) -> Result<ViewColorProfile, NativeHostError> {
        let profile = publication.color_profile();
        if profile == ViewColorProfile::Compatibility {
            if self.source.take().is_some() {
                self.scope.clear();
                if let Some(presenter) = &mut self.presenter {
                    presenter.clear();
                }
            }
            return Ok(profile);
        }
        if !self.supported || insets_active || publication.primary_view().is_none() {
            return Err(error(
                "native HDR requires a supported primary 2D float view without insets",
            ));
        }
        let spatial = |row: &noon::integration::FrameObjectState| row.spatial.is_some();
        let has_spatial = if !self.is_hdr() || publication.changes().is_all() {
            publication.frame().objects.iter().any(spatial)
        } else {
            publication
                .changes()
                .object_indices()
                .iter()
                .any(|&i| spatial(&publication.frame().objects[i]))
        };
        if has_spatial {
            return Err(error("native HDR spatial output is not qualified"));
        }
        Self::validate_extent(device, size)?;
        if self
            .source
            .as_ref()
            .is_none_or(|(s, _)| [s.width(), s.height()] != size)
        {
            let source = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon native linear authored view"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = source.create_view(&Default::default());
            self.source = Some((source, view));
            self.scope.clear();
        }
        if self.presenter.is_none() {
            self.presenter = Some(
                EncodedDisplayPresenter::new(device, self.surface_format, OutputTransfer::Direct)
                    .map_err(|e| error(&e.to_string()))?,
            );
        }
        Ok(profile)
    }
    pub fn scene_view<'a>(&'a self, surface: &'a wgpu::TextureView) -> &'a wgpu::TextureView {
        self.source.as_ref().map_or(surface, |(_, v)| v)
    }
    pub fn invalidate(&mut self) {
        self.scope.invalidate();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &mut GpuRenderer,
        publication: &RendererPublication<'_>,
        world_view_height: f64,
        overlay: &OverlayGpuState,
        surface: &wgpu::Texture,
    ) -> Result<HdrBloomStats, NativeHostError> {
        let Some((source, _)) = &self.source else {
            return Ok(HdrBloomStats::default());
        };
        let revision = self
            .source_revision
            .checked_add(1)
            .ok_or_else(|| error("native HDR source revision exhausted"))?;
        let result = (|| {
            let hdr = self
                .filter
                .prepare_view(
                    device,
                    queue,
                    &mut self.scope,
                    publication,
                    HdrBloomCapture { source, revision },
                    world_view_height,
                    NATIVE_HDR_TARGET_BUDGET,
                )
                .map_err(|e| error(&e.to_string()))?;
            self.filter.encode(encoder, &mut self.scope);
            let display = self.scope.display_output().expect("prepared view output");
            renderer
                .encode_display_overlay(device, encoder, display, overlay)
                .map_err(|e| error(&e.to_string()))?;
            let presenter = self.presenter.as_mut().expect("prepared native presenter");
            presenter
                .prepare(display)
                .map_err(|e| error(&e.to_string()))?;
            presenter
                .encode(encoder, surface)
                .map_err(|e| error(&e.to_string()))?;
            Ok(hdr)
        })();
        match result {
            Ok(work) => {
                self.source_revision = revision;
                Ok(work)
            }
            Err(e) => {
                self.scope.invalidate();
                Err(e)
            }
        }
    }
}
#[cfg(test)]
mod tests;
