//! Reusable authored-view GPU resources only. The current Runtime publication owns profile,
//! camera and Bloom values; this cache owns no authored or scheduling state.
use super::{
    EncodedDisplayPresenter, GpuRenderer, HdrBloomCapture, HdrBloomFilter, HdrBloomScope,
    HdrBloomStats, OutputTransfer, OverlayGpuState,
};
use noon_core::ViewColorProfile;
use noon_runtime::RendererPublication;

pub struct AuthoredViewOutput {
    filter: HdrBloomFilter,
    scope: HdrBloomScope,
    presenter: Option<EncodedDisplayPresenter>,
    source: Option<(wgpu::Texture, wgpu::TextureView)>,
    supported: bool,
    surface_format: wgpu::TextureFormat,
    transfer: OutputTransfer,
    source_revision: u64,
}
// Retained primary-view targets at fixed quality, including source and MSAA.
const VIEW_OUTPUT_TARGET_BUDGET: u64 = 256 * 1024 * 1024;

/// An unsupported or over-budget view leaves the previous GPU output intact.
#[derive(Debug)]
pub struct ViewOutputError(String);
impl std::fmt::Display for ViewOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ViewOutputError {}
fn error(message: &str) -> ViewOutputError {
    ViewOutputError(message.to_owned())
}
impl AuthoredViewOutput {
    pub fn new(
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        transfer: OutputTransfer,
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
            transfer,
            source_revision: 0,
        }
    }
    /// Source 8 + four-sample scene attachment 32 + Bloom/output scratch 36
    /// bytes/pixel. Surface, scene assets/effect scopes and in-flight leases are
    /// separate owners; this is the retained primary-view target budget.
    pub fn validate_extent(device: &wgpu::Device, size: [u32; 2]) -> Result<(), ViewOutputError> {
        let bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| {
                pixels.checked_mul(40).and_then(|scene| {
                    HdrBloomFilter::required_scratch_bytes(size)?.checked_add(scene)
                })
            })
            .ok_or_else(|| error("authored HDR target extent overflow"))?;
        if size.contains(&0)
            || size
                .iter()
                .any(|&x| x > device.limits().max_texture_dimension_2d)
            || bytes > VIEW_OUTPUT_TARGET_BUDGET
        {
            return Err(error(
                "authored HDR view exceeds the retained target budget or device extent",
            ));
        }
        Ok(())
    }
    pub fn is_hdr(&self) -> bool {
        self.source.is_some()
    }
    pub fn supports_hdr(&self) -> bool {
        self.supported
    }
    pub fn source_texture(&self) -> Option<&wgpu::Texture> {
        self.source.as_ref().map(|(s, _)| s)
    }
    pub fn linear_output(&self) -> Option<&wgpu::Texture> {
        self.scope.linear_output()
    }
    pub fn display_output(&self) -> Option<&wgpu::Texture> {
        self.scope.display_output()
    }
    /// Retire disposable targets on profile transitions or host recovery.
    pub fn clear(&mut self) {
        self.source = None;
        self.scope.clear();
        if let Some(presenter) = &mut self.presenter {
            presenter.clear();
        }
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
    ) -> Result<ViewColorProfile, ViewOutputError> {
        let profile = publication.color_profile();
        crate::validate_emission_profile(
            publication.frame(),
            publication.changes(),
            profile,
            self.is_hdr() != (profile == ViewColorProfile::LinearHdr),
        )
        .map_err(|e| error(&e.to_string()))?;
        if profile == ViewColorProfile::Compatibility {
            if self.source.is_some() {
                self.clear();
            }
            return Ok(profile);
        }
        if !self.supported || insets_active || publication.primary_view().is_none() {
            return Err(error(
                "authored HDR requires a supported primary 2D float view without insets",
            ));
        }
        let spatial = |row: &noon_runtime::FrameObjectState| row.spatial.is_some();
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
            return Err(error("authored HDR spatial output is not qualified"));
        }
        Self::validate_extent(device, size)?;
        if self
            .source
            .as_ref()
            .is_none_or(|(s, _)| [s.width(), s.height()] != size)
        {
            let source = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon linear authored view"),
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
                EncodedDisplayPresenter::new(device, self.surface_format, self.transfer)
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
    ) -> Result<HdrBloomStats, ViewOutputError> {
        let Some((source, _)) = &self.source else {
            return Ok(HdrBloomStats::default());
        };
        let revision = self
            .source_revision
            .checked_add(1)
            .ok_or_else(|| error("authored HDR source revision exhausted"))?;
        let result = (|| {
            let mut hdr = self
                .filter
                .prepare_view(
                    device,
                    queue,
                    &mut self.scope,
                    publication,
                    HdrBloomCapture { source, revision },
                    world_view_height,
                    VIEW_OUTPUT_TARGET_BUDGET,
                )
                .map_err(|e| error(&e.to_string()))?;
            hdr.passes = self.filter.encode(encoder, &mut self.scope).passes;
            let display = self.scope.display_output().expect("prepared view output");
            renderer
                .encode_display_overlay(device, encoder, display, overlay)
                .map_err(|e| error(&e.to_string()))?;
            let presenter = self
                .presenter
                .as_mut()
                .expect("prepared authored presenter");
            let presentation = presenter
                .prepare(display)
                .map_err(|e| error(&e.to_string()))?;
            hdr.pipeline_compiles += presentation.pipeline_compiles;
            hdr.bind_group_creations += presentation.bind_group_creations;
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
