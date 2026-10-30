//! Source capture, filtering and painter placement for the shared glow operator.
//!
//! Analytic sources retain their fast capture; other 2D sources share isolated
//! prepared draw capture. The final tile replaces one stable object range; it is not a foreground overlay. These caches
//! are indexed by existing packed-instance slots and retain no semantic authority.
//! Effective requests come from shared lowering/publication (#1897).

use std::{collections::BTreeMap, mem::size_of, sync::Arc};

use super::GpuLease as CaptureSourceLease;

use bytemuck::{Pod, Zeroable};
use noon_core::{EffectDefinition, GeometryRef, GlowSource, ObjectId, Transform2D, Vec2};

use super::{
    Camera2D, CameraUniform, DrawStats, EffectCaptureTile, EffectPixelBounds, EffectPrepareError,
    EffectRasterStats, EffectStackFilter, EffectStackParameters, EffectStackScope, GeometryBinding,
    GlowCapture, GpuRenderer, ResolvedOrderedBatch, PATH_SAMPLE_COUNT,
};
use crate::{CircleInstance, PackedStyle, PreparedFrame, RectangleInstance, RenderPrimitive};

mod backdrop;
mod inputs;

/// One effective renderer request. Object identity/attachment values and animation
/// phase must come from the normal runtime publication, not a host-owned store.
#[derive(Clone, Copy, Debug)]
pub struct EffectCaptureRequest<'a> {
    pub object_index: usize,
    pub definitions: &'a [EffectDefinition],
    /// Maps from full output pixels; capture translates by the padded tile origin.
    pub shader: Option<&'a super::ShaderSampleContext>,
    /// Total retained capture + filter texture budget for this renderer, not just
    /// the current object. Source buffers and shared pipeline metadata are separate.
    /// Resources retained by already-submitted frames require an additional host
    /// in-flight budget; this counter covers the current renderer-owned cache.
    pub texture_budget_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectCaptureStats {
    pub source_passes: usize,
    pub source_bytes_uploaded: usize,
    pub source_texture_allocations: usize,
    pub placement_bytes_uploaded: usize,
    pub filter: EffectRasterStats,
    pub retained_texture_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum SourceInstance {
    Circle(CircleInstance),
    Rectangle(RectangleInstance),
    Draws {
        source: CaptureSourceLease<super::source_capture::PreparedSource>,
        silhouette: Option<CaptureSourceLease<super::source_capture::PreparedSource>>,
    },
}
impl SourceInstance {
    fn same_source(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Draws {
                    source: a,
                    silhouette: x,
                },
                Self::Draws {
                    source: b,
                    silhouette: y,
                },
            ) => {
                (CaptureSourceLease::ptr_eq(a, b) || a == b)
                    && match (x, y) {
                        (Some(x), Some(y)) => CaptureSourceLease::ptr_eq(x, y) || x == y,
                        (None, None) => true,
                        _ => false,
                    }
            }
            _ => self == other,
        }
    }
    fn set_capture_region(&mut self, tile: EffectCaptureTile) -> Result<(), EffectPrepareError> {
        if let Self::Draws { source, silhouette } = self {
            for source in std::iter::once(source).chain(silhouette.iter_mut()) {
                if source.needs_capture_region(tile.origin, tile.size) {
                    CaptureSourceLease::make_mut(source)
                        .set_capture_region(tile.origin, tile.size)
                        .ok_or(EffectPrepareError::CaptureCoordinatesOutOfRange)?;
                }
            }
        }
        Ok(())
    }
    fn prepared(prepared: &PreparedFrame<'_>, key: (u8, usize)) -> Option<Self> {
        match key {
            (0, index) => prepared.circles.get(index).copied().map(Self::Circle),
            (1, index) => prepared.rectangles.get(index).copied().map(Self::Rectangle),
            _ => None,
        }
    }
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Circle(value) => bytemuck::bytes_of(value),
            Self::Rectangle(value) => bytemuck::bytes_of(value),
            Self::Draws { source, .. } => &source.bytes,
        }
    }
    fn silhouette_bytes_len(&self) -> usize {
        match self {
            Self::Draws { silhouette, .. } => {
                silhouette.as_ref().map_or(0, |source| source.bytes.len())
            }
            _ => self.bytes().len(),
        }
    }
    fn style_mut(&mut self) -> &mut PackedStyle {
        match self {
            Self::Circle(value) => &mut value.style,
            Self::Rectangle(value) => &mut value.style,
            Self::Draws { .. } => unreachable!("prepared draws already have scope opacity removed"),
        }
    }
    fn geometry(&self) -> GeometryRef {
        match self {
            Self::Circle(value) => GeometryRef::circle(value.radius),
            Self::Rectangle(value) => GeometryRef::rectangle(value.size[0], value.size[1]),
            Self::Draws { .. } => unreachable!("prepared draw bounds are already projected"),
        }
    }
    fn silhouette(&self) -> Self {
        let mut value = self.clone();
        let style = value.style_mut();
        style.fill = [1.0; 4];
        style.fill_enabled = 1;
        value
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct Placement {
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub viewport: [u32; 2],
    pub opacity: f32,
    pub padding: u32,
}

#[derive(Debug)]
struct CaptureImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
impl CaptureImage {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon isolated effect source"),
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
struct ResidentEffect {
    object: ObjectId,
    tile: EffectCaptureTile,
    // Raster projection depends on the requested footprint, never on spare
    // high-water texture capacity retained from an earlier effect profile.
    capture_viewport: [u32; 2],
    // Finite contribution before capacity rounding/high-water reuse. Ancestor
    // bounds must never feed retained transparent storage back into capture size.
    contribution_bounds: EffectPixelBounds,
    original_bounds: EffectPixelBounds,
    camera: Camera2D,
    viewport: [u32; 2],
    effective: SourceInstance,
    source_instance: SourceInstance,
    source: CaptureImage,
    silhouette: Option<CaptureImage>,
    original_mask: super::SampleScope,
    original_masked: bool,
    source_buffer: wgpu::Buffer,
    silhouette_buffer: Option<wgpu::Buffer>,
    source_image_bindings: Vec<super::raster_image_gpu::CapturedImageBinding>,
    silhouette_image_bindings: Vec<super::raster_image_gpu::CapturedImageBinding>,
    source_filtered_bindings: Vec<super::source_capture::FilteredBinding>,
    silhouette_filtered_bindings: Vec<super::source_capture::FilteredBinding>,
    camera_buffer: wgpu::Buffer,
    camera_binding: wgpu::BindGroup,
    text_camera_binding: Option<wgpu::BindGroup>,
    multisampled: Option<wgpu::TextureView>,
    placement: Placement,
    placement_buffer: wgpu::Buffer,
    placement_binding: Option<wgpu::BindGroup>,
    placement_output: Option<wgpu::Texture>,
    original_opacity: f32,
    placement_erase: Option<wgpu::Texture>,
    placement_erase_binding: Option<wgpu::BindGroup>,
    filter: EffectStackScope,
    revision: u64,
    output_revision: u64,
    invalidated: bool,
    enabled: bool,
}

#[derive(Debug)]
pub(super) struct PlacementPrograms {
    pub(super) layout: wgpu::BindGroupLayout,
    single: wgpu::RenderPipeline,
    pub(super) multisampled: wgpu::RenderPipeline,
    replacement: Option<[wgpu::RenderPipeline; 4]>,
}

#[derive(Debug, Default)]
pub(super) struct EffectCaptureGpu {
    // Existing analytic packed slots and mixed source indices are disposable
    // cache keys; owners retain their normal published object identities.
    scopes: BTreeMap<(u8, usize), ResidentEffect>,
    owners: BTreeMap<ObjectId, (u8, usize)>,
    pub(super) filter: EffectStackFilter,
    placement: Option<PlacementPrograms>,
    bytes: u64,
    backdrops: backdrop::BackdropGpu,
}

/// Sparse request for one runtime-owned row, never a host copy of effect values.
#[derive(Clone, Copy, Debug)]
pub struct PublishedEffectCaptureRequest {
    pub object_index: usize,
    pub texture_budget_bytes: u64,
}

impl GpuRenderer {
    /// Consume an effective attachment from the same borrowed runtime publication as
    /// source geometry/paint. Invoke for initially resident or dirty rows; no
    /// unrelated scene scan or private effect clock is introduced here.
    ///
    /// Absent/removed attachments retire their existing renderer derivations.
    /// Full Scene orchestration remains guarded until authored activation/live
    /// publication and all platform consumers are integrated (#1897).
    pub fn prepare_published_effect_capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        publication: &noon_runtime::RendererPublication<'_>,
        request: PublishedEffectCaptureRequest,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        let frame = publication.frame();
        let row = frame
            .objects
            .get(request.object_index)
            .ok_or(EffectPrepareError::UnsupportedCapture)?;
        if frame.time != prepared.time {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        let Some(effect) = row
            .effects
            .as_ref()
            .filter(|_| frame.is_drawable(request.object_index))
        else {
            self.remove_effect_capture(row.id);
            return Ok(EffectCaptureStats {
                retained_texture_bytes: self.effect_capture_texture_bytes(),
                ..Default::default()
            });
        };
        let observation = prepared
            .observe_object(request.object_index)
            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
        let key = match observation.primitive {
            RenderPrimitive::Circle => (0, observation.instance_index),
            RenderPrimitive::Rectangle => (1, observation.instance_index),
            _ => return Err(EffectPrepareError::UnsupportedCapture),
        };
        let source = SourceInstance::prepared(prepared, key)
            .ok_or(EffectPrepareError::UnsupportedCapture)?;
        if observation.object != row.id
            || observation.transform != frame.render_transform(request.object_index).into()
            || observation.style != crate::pack_style(row)
            || frame.render_geometry(request.object_index) != Some(&source.geometry())
            || frame.reveal(request.object_index) != 1.0
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        self.prepare_observed_effect_capture_input(
            device,
            queue,
            encoder,
            prepared,
            EffectCaptureRequest {
                shader: None,
                object_index: request.object_index,
                definitions: effect.definitions(),
                texture_budget_bytes: request.texture_budget_bytes,
            },
            observation,
            super::glow_filter::CaptureDemand::default(),
            effect
                .visibility()
                .and_then(|effect| effect.definition.as_noise_dissolve()),
        )
    }

    /// Capture/filter one effective primitive using the same command encoder as
    /// the subsequent ordinary scene encode. Call after packed-instance upload.
    /// This finite slice admits filled, unstroked, fully revealed planar sources.
    /// Unsupported source/view requests fail before mutating retained resources.
    ///
    /// Reprepare affected requests after a packed source or camera/view change.
    /// Only one publication for a given scope may be recorded before submission:
    /// queue writes must not overwrite uniforms used by earlier pending frames.
    /// If the encoder is abandoned, call invalidate_analytic_glows before reuse.
    pub fn prepare_effect_capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        request: EffectCaptureRequest<'_>,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        let observation = prepared
            .observe_object(request.object_index)
            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
        self.prepare_observed_effect_capture(device, queue, encoder, prepared, request, observation)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_observed_effect_capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        request: EffectCaptureRequest<'_>,
        observation: crate::PreparedGeometryObjectObservation,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        self.prepare_observed_effect_capture_input(
            device,
            queue,
            encoder,
            prepared,
            request,
            observation,
            super::glow_filter::CaptureDemand::default(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_observed_effect_capture_input(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        request: EffectCaptureRequest<'_>,
        observation: crate::PreparedGeometryObjectObservation,
        demand: super::glow_filter::CaptureDemand,
        visibility: Option<noon_core::NoiseDissolve>,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        // Prefix roles must enter through the explicit backdrop coordinator;
        // ordinary object/group capture must never reinterpret their input.
        if request
            .definitions
            .iter()
            .any(|d| d.input_role() == noon_core::EffectInputRole::PainterPrefix)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let kind = match observation.primitive {
            RenderPrimitive::Circle => 0,
            RenderPrimitive::Rectangle => 1,
            _ => return Err(EffectPrepareError::UnsupportedCapture),
        };
        // observe_object already requires a present resident slot. Some(false)
        // means only its base paint is suppressed; a silhouette halo can still
        // contribute. None is the unsupported partial-visibility projection.
        if observation.submission_membership.is_none()
            || !self.inset_views.is_empty()
            || !matches!(
                self.target_format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Rgba16Float
            )
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let key = (kind, observation.instance_index);
        let effective = SourceInstance::prepared(prepared, key)
            .ok_or(EffectPrepareError::UnsupportedCapture)?;
        if observation.style.stroke_enabled & 1 != 0
            || observation.style.fill_enabled == 0
            || matches!(&effective, SourceInstance::Circle(value) if value.padding[0] != 1.0)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if !observation
            .style
            .fill
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let transform = Transform2D {
            translation: Vec2::new(
                observation.transform.translation[0],
                observation.transform.translation[1],
            ),
            scale: Vec2::new(
                observation.transform.scale[0],
                observation.transform.scale[1],
            ),
            rotation: observation.transform.rotation,
        };
        let bounds = EffectPixelBounds::projected_analytic_in_view(
            &effective.geometry(),
            transform,
            self.camera,
            self.viewport_size,
            self.logical_viewport_size,
        )?;
        self.prepare_source_effect(
            device,
            queue,
            encoder,
            prepared,
            request,
            key,
            observation.object,
            effective,
            bounds,
            observation.style.opacity,
            None,
            demand,
            visibility,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_draw_source_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &super::retained_text::PreparedRetainedGpuFrame<'_>,
        text: &super::retained_text::RetainedTextGpuState,
        request: EffectCaptureRequest<'_>,
        object: ObjectId,
        opacity: f32,
        composed: Option<Arc<noon_core::ComposedSource>>,
        recapture: bool,
        demand: super::glow_filter::CaptureDemand,
        visibility: Option<noon_core::NoiseDissolve>,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        // Prefix roles must enter through the explicit backdrop coordinator;
        // ordinary object/group capture must never reinterpret their input.
        if request
            .definitions
            .iter()
            .any(|d| d.input_role() == noon_core::EffectInputRole::PainterPrefix)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if !matches!(
            self.target_format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Rgba16Float
        ) {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let mut own_capture = demand.isolate || (composed.is_some() && opacity != 1.0);
        for definition in request.definitions {
            let parameters = crate::EffectCaptureParameters {
                definition: definition.clone(),
                output_height: self.viewport_size[1],
                world_view_height: f64::from(self.camera.world_size.y),
                scope_opacity: opacity,
            };
            own_capture |=
                opacity > 0.0 && !parameters.is_neutral_at_sigma(parameters.sigma_pixels()?);
        }
        let needs_silhouette = demand.original_by_parent
            || (own_capture
                && request.definitions.iter().any(|definition| {
                    definition
                        .as_glow()
                        .is_some_and(|effect| effect.source() == GlowSource::Silhouette)
                }));
        let cached = composed.as_ref().filter(|_| !recapture).and_then(|_| {
            self.effect_captures
                .as_deref()?
                .scopes
                .get(&(2, request.object_index))
                .filter(|scope| scope.object == object)
                .and_then(|scope| match &scope.effective {
                    SourceInstance::Draws { source, silhouette }
                        if !needs_silhouette || silhouette.is_some() =>
                    {
                        Some((
                            source.clone(),
                            silhouette.clone().filter(|_| needs_silhouette),
                        ))
                    }
                    _ => None,
                })
        });
        let (source, silhouette) = if let Some(cached) = cached {
            cached
        } else {
            let members = composed.as_ref().map(|source| {
                source
                    .members()
                    .iter()
                    .map(|&id| (id, 1.0))
                    .collect::<Vec<_>>()
            });
            let leaf = [(object, opacity.max(f32::MIN_POSITIVE))];
            let members = members.as_deref().unwrap_or(&leaf);
            let replacements = if let Some(composed) = composed.as_ref().filter(|_| own_capture) {
                self.prepare_composed_replacements(
                    prepared,
                    request.object_index,
                    composed.members(),
                    false,
                )?
            } else {
                Vec::new()
            };
            let source = super::source_capture::PreparedSource::prepare_with_replacements(
                prepared,
                members,
                false,
                device.limits().min_uniform_buffer_offset_alignment as usize,
                &replacements,
            )
            .ok_or(EffectPrepareError::UnsupportedCapture)?;
            let silhouette = if needs_silhouette {
                Some(
                    super::source_capture::PreparedSource::prepare_with_replacements(
                        prepared,
                        members,
                        true,
                        device.limits().min_uniform_buffer_offset_alignment as usize,
                        &if let Some(composed) = &composed {
                            self.prepare_composed_replacements(
                                prepared,
                                request.object_index,
                                composed.members(),
                                true,
                            )?
                        } else {
                            Vec::new()
                        },
                    )
                    .ok_or(EffectPrepareError::UnsupportedCapture)?,
                )
            } else {
                None
            };
            (
                CaptureSourceLease::new(source),
                silhouette.map(CaptureSourceLease::new),
            )
        };
        // Share the logical-view qualification with analytic capture. Physical
        // sampling remains independent of CSS/DPR rounding.
        EffectPixelBounds::validate_planar_view(
            Transform2D::IDENTITY,
            self.camera,
            self.viewport_size,
            self.logical_viewport_size,
        )?;
        let Some(b) = source.bounds else {
            if let Some(scope) = self
                .effect_captures
                .as_deref_mut()
                .and_then(|glows| glows.scopes.get_mut(&(2, request.object_index)))
                .filter(|scope| scope.object == object)
            {
                scope.enabled = false;
            }
            return Ok(EffectCaptureStats {
                retained_texture_bytes: self.effect_capture_texture_bytes(),
                ..Default::default()
            });
        };
        let bounds = EffectPixelBounds::from_world_rect(b, self.camera, self.viewport_size);
        self.prepare_source_effect(
            device,
            queue,
            encoder,
            &prepared.geometry,
            request,
            (2, request.object_index),
            object,
            SourceInstance::Draws { source, silhouette },
            bounds,
            opacity,
            Some(text),
            super::glow_filter::CaptureDemand {
                isolate: demand.isolate || (composed.is_some() && opacity != 1.0),
                ..demand
            },
            visibility,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_source_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        request: EffectCaptureRequest<'_>,
        key: (u8, usize),
        object: ObjectId,
        mut effective: SourceInstance,
        bounds: EffectPixelBounds,
        opacity: f32,
        text: Option<&super::retained_text::RetainedTextGpuState>,
        demand: super::glow_filter::CaptureDemand,
        visibility: Option<noon_core::NoiseDissolve>,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        if self.target_format == wgpu::TextureFormat::Rgba16Float
            && (visibility.is_some()
                || request
                    .definitions
                    .iter()
                    .any(|effect| effect.as_backdrop_blur().is_some()))
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let texel_bytes = super::glow_filter::capture_texel_bytes(self.target_format)?;
        let parameters = EffectStackParameters {
            shader: None,
            working_format: self.target_format,
            output_height: self.viewport_size[1],
            world_view_height: f64::from(self.camera.world_size.y),
            scope_opacity: opacity,
            scratch_budget_bytes: request.texture_budget_bytes,
            source_bounds: None,
        };
        let tile = EffectCaptureTile::prepare_stack_input(
            bounds,
            self.viewport_size,
            request.definitions,
            parameters,
            device.limits().max_texture_dimension_2d,
            request.texture_budget_bytes,
            demand,
        )?;
        let Some(mut tile) = tile else {
            // Retain warmed storage for neutral fades; exact ordinary submission
            // is restored. Absent neutral requests allocate no glow cache.
            if let Some(glows) = self.effect_captures.as_deref_mut() {
                // Repacking can change `key` before an active prepare relocates
                // this source's warmed scope. Its owner mapping remains exact.
                let owned_key = glows.owners.get(&object).copied();
                if let Some(scope) = owned_key
                    .and_then(|owned_key| glows.scopes.get_mut(&owned_key))
                    .filter(|s| s.object == object)
                {
                    // Drop obsolete child-output leases even while this scope is
                    // neutral. Its warmed source image is reusable only when its
                    // normalized inputs still describe that captured image.
                    let mut source_tile = scope.tile;
                    source_tile.size = scope.capture_viewport;
                    effective.set_capture_region(source_tile)?;
                    let mut source_instance = effective.clone();
                    if key.0 != 2 {
                        source_instance.style_mut().opacity = 1.0;
                    }
                    if !scope.source_instance.same_source(&source_instance) {
                        scope.source_instance = source_instance;
                        scope.effective = effective;
                        scope.source_filtered_bindings.clear();
                        scope.silhouette_filtered_bindings.clear();
                        scope.source_image_bindings.clear();
                        scope.silhouette_image_bindings.clear();
                        scope.invalidated = true;
                    }
                    let source_bytes = u64::from(scope.tile.size[0])
                        * u64::from(scope.tile.size[1])
                        * texel_bytes
                        * (1 + u64::from(scope.silhouette.is_some())
                            + if scope.multisampled.is_some() {
                                u64::from(PATH_SAMPLE_COUNT)
                            } else {
                                0
                            });
                    let retained = source_bytes
                        + scope.filter.retained_profile_bytes(request.definitions)
                        + if request.definitions.is_empty() {
                            0
                        } else {
                            scope.original_mask.scratch_bytes()
                        };
                    let total = glows.bytes - scope.tile.capture_and_scratch_bytes + retained;
                    if total > request.texture_budget_bytes {
                        return Err(EffectPrepareError::ScratchBudgetExceeded);
                    }
                    if request.definitions.is_empty() {
                        scope.original_mask.clear();
                        scope.original_masked = false;
                    }
                    scope.filter.deactivate_profile(request.definitions);
                    scope.placement_binding = None;
                    scope.placement_output = None;
                    scope.tile.capture_and_scratch_bytes = retained;
                    glows.bytes = total;
                    scope.enabled = false;
                }
            }
            return Ok(EffectCaptureStats {
                retained_texture_bytes: self.effect_capture_texture_bytes(),
                ..Default::default()
            });
        };
        let mut source_tile = tile;
        let contribution_bounds = EffectPixelBounds {
            min: tile.origin.map(f64::from),
            max: std::array::from_fn(|axis| {
                f64::from(tile.origin[axis]) + f64::from(tile.size[axis])
            }),
        };
        let mut cost_per_pixel =
            tile.capture_and_scratch_bytes / (u64::from(tile.size[0]) * u64::from(tile.size[1]));
        let old_original_mask = self
            .effect_captures
            .as_deref()
            .and_then(|cache| cache.scopes.get(&key))
            .is_some_and(|scope| scope.original_mask.scratch_bytes() > 0);
        let retains_original = demand.original_by_parent
            || request.definitions.iter().any(|definition| {
                definition
                    .as_glow()
                    .is_some_and(|glow| glow.source() == GlowSource::Silhouette)
            });
        let retain_original_mask = retains_original
            && !request.definitions.is_empty()
            && (old_original_mask || (demand.original_by_parent && visibility.is_some()));
        if retain_original_mask {
            cost_per_pixel += texel_bytes;
        }
        let old = self
            .effect_captures
            .as_deref()
            .and_then(|g| g.scopes.get(&key));
        // Stable high-water capacity avoids reallocating captures whenever a
        // fractionally translated footprint alternates between adjacent sizes.
        // Extra padding is transparent; resolution/kernel quality is unchanged.
        let previous_size = old
            .filter(|scope| scope.object == object)
            .map_or([0; 2], |scope| scope.tile.size);
        for (axis, previous) in previous_size.into_iter().enumerate() {
            let reserved = tile.size[axis]
                .saturating_add(1)
                .next_multiple_of(16)
                .min(device.limits().max_texture_dimension_2d);
            // Preserve the existing profile-local block rounding, while
            // keeping a previous larger allocation out of the raster camera.
            source_tile.size[axis] = reserved;
            tile.size[axis] = reserved.max(previous);
        }
        tile.refresh_visible_region(self.viewport_size);
        let source_bounds = EffectPixelBounds {
            min: std::array::from_fn(|axis| bounds.min[axis] - f64::from(tile.origin[axis])),
            max: std::array::from_fn(|axis| bounds.max[axis] - f64::from(tile.origin[axis])),
        };
        let shader_context = request
            .shader
            .map(|context| context.for_capture(tile.origin));
        super::sample_filter::validate_effect_shader_context(
            request.definitions,
            shader_context.as_ref(),
            tile.size,
        )?;
        let _shader_programs = if request.definitions.iter().any(|d| d.as_shader().is_some()) {
            let cache = self
                .effect_captures
                .as_deref()
                .ok_or(EffectPrepareError::ShaderProgramNotPrepared)?;
            Some(cache.filter.pin_shader_programs(
                device,
                old.map(|old| &old.filter),
                request.definitions,
            )?)
        } else {
            None
        };
        if request
            .definitions
            .iter()
            .any(|definition| definition.as_pixelate().is_some())
        {
            super::sample_filter::checked_sample_origin(source_bounds)?;
        }
        if request.definitions.iter().any(|definition| {
            definition
                .as_noise_dissolve()
                .is_some_and(|value| !value.is_neutral())
        }) {
            // Check the final high-water capture domain before taking or
            // mutating any retained scope. A rejected tiny source must keep the
            // previous coherent outputs, including unrelated captures.
            super::sample_filter::checked_domain(tile.size, source_bounds)?;
        }
        tile.capture_and_scratch_bytes = u64::from(tile.size[0])
            .checked_mul(u64::from(tile.size[1]))
            .and_then(|pixels| {
                pixels.checked_mul(
                    cost_per_pixel
                        + if key.0 == 2 {
                            texel_bytes * u64::from(PATH_SAMPLE_COUNT)
                        } else {
                            0
                        },
                )
            })
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        let old_bytes = old.map_or(0, |s| s.tile.capture_and_scratch_bytes);
        let relocated_bytes = self
            .effect_captures
            .as_deref()
            .and_then(|g| {
                g.owners
                    .get(&object)
                    .filter(|previous| **previous != key)
                    .and_then(|previous| g.scopes.get(previous))
            })
            .map_or(0, |s| s.tile.capture_and_scratch_bytes);
        let total = self
            .effect_captures
            .as_deref()
            .map_or(0, |cache| cache.bytes)
            .checked_sub(old_bytes)
            .and_then(|v| v.checked_sub(relocated_bytes))
            .and_then(|v| v.checked_add(tile.capture_and_scratch_bytes))
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        if total > request.texture_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        effective.set_capture_region(source_tile)?;
        let mut source_instance = effective.clone();
        if key.0 != 2 {
            source_instance.style_mut().opacity = 1.0;
        } // apply final scope opacity once
        let needs_silhouette = demand.original_by_parent
            || request.definitions.iter().any(|definition| {
                definition
                    .as_glow()
                    .is_some_and(|effect| effect.source() == GlowSource::Silhouette)
            });
        let capture_camera = tile_camera(self.camera, self.viewport_size, source_tile)?;
        let camera_uniform = capture_camera
            .uniform(source_tile.size)
            .with_sample_projection(self.camera.uniform(self.viewport_size));
        let placement = Placement {
            origin: tile.origin,
            size: tile.size,
            viewport: self.viewport_size,
            opacity,
            padding: 0,
        };
        // Everything that can reject input is checked before installing caches.
        let mut glows = self.effect_captures.take().unwrap_or_default();
        let result = (|| {
            let programs = glows
                .placement
                .get_or_insert_with(|| PlacementPrograms::new(device, self.target_format));
            let replace = glows.scopes.get(&key).is_none_or(|s| {
                s.object != object
                    || s.tile.size != tile.size
                    || s.silhouette.is_some() != needs_silhouette
                    || s.source_buffer.size() < source_instance.bytes().len() as u64
                    || (needs_silhouette
                        && s.silhouette_buffer.as_ref().is_none_or(|buffer| {
                            buffer.size() < source_instance.silhouette_bytes_len() as u64
                        }))
            });
            let mut stats = EffectCaptureStats::default();
            if replace {
                let source = CaptureImage::new(device, self.target_format, tile.size);
                let silhouette = needs_silhouette
                    .then(|| CaptureImage::new(device, self.target_format, tile.size));
                let source_buffer = instance_buffer(device, source_instance.bytes().len());
                let silhouette_buffer = needs_silhouette
                    .then(|| instance_buffer(device, source_instance.silhouette_bytes_len()));
                let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon local glow camera"),
                    size: size_of::<CameraUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let camera_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Noon local glow camera binding"),
                    layout: &self.camera_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: camera_buffer.as_entire_binding(),
                    }],
                });
                let placement_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon glow painter placement"),
                    size: size_of::<Placement>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                if let Some(previous) = glows.scopes.insert(
                    key,
                    ResidentEffect {
                        object,
                        tile,
                        capture_viewport: source_tile.size,
                        contribution_bounds,
                        original_bounds: bounds,
                        camera: self.camera,
                        viewport: self.viewport_size,
                        effective: effective.clone(),
                        source_instance: source_instance.clone(),
                        source,
                        silhouette,
                        original_mask: super::SampleScope::default(),
                        original_masked: false,
                        source_buffer,
                        silhouette_buffer,
                        source_image_bindings: Vec::new(),
                        silhouette_image_bindings: Vec::new(),
                        source_filtered_bindings: Vec::new(),
                        silhouette_filtered_bindings: Vec::new(),
                        text_camera_binding: text
                            .map(|text| text.glyphs.capture_camera_binding(device, &camera_buffer)),
                        multisampled: (key.0 == 2).then(|| {
                            device
                                .create_texture(&wgpu::TextureDescriptor {
                                    label: Some("Noon isolated source MSAA"),
                                    size: wgpu::Extent3d {
                                        width: tile.size[0],
                                        height: tile.size[1],
                                        depth_or_array_layers: 1,
                                    },
                                    mip_level_count: 1,
                                    sample_count: PATH_SAMPLE_COUNT,
                                    dimension: wgpu::TextureDimension::D2,
                                    format: self.target_format,
                                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                    view_formats: &[],
                                })
                                .create_view(&Default::default())
                        }),
                        camera_buffer,
                        camera_binding,
                        placement,
                        placement_buffer,
                        placement_binding: None,
                        placement_output: None,
                        original_opacity: opacity,
                        placement_erase: None,
                        placement_erase_binding: None,
                        filter: EffectStackScope::default(),
                        revision: 0,
                        output_revision: 0,
                        invalidated: true,
                        enabled: true,
                    },
                ) {
                    glows.owners.remove(&previous.object);
                }
                stats.source_texture_allocations =
                    1 + usize::from(needs_silhouette) + usize::from(key.0 == 2);
            }
            if let Some(previous_key) = glows
                .owners
                .insert(object, key)
                .filter(|previous| *previous != key)
            {
                glows.scopes.remove(&previous_key);
            }
            if let SourceInstance::Draws { source, silhouette } = &source_instance {
                if source.has_replacements()
                    || silhouette.as_ref().is_some_and(|s| s.has_replacements())
                {
                    stats.filter.pipeline_compiles +=
                        programs.ensure_replacement(device, self.target_format);
                }
            }
            let scope = glows.scopes.get_mut(&key).expect("admitted glow scope");
            scope.original_opacity = opacity;
            scope.placement_erase = None;
            scope.placement_erase_binding = None;
            if !retain_original_mask {
                scope.original_mask.clear();
            }
            scope.original_bounds = bounds;
            let source_changed = scope.invalidated
                || !scope.source_instance.same_source(&source_instance)
                || scope.tile.origin != tile.origin
                || scope.capture_viewport != source_tile.size
                || scope.camera != self.camera
                || scope.viewport != self.viewport_size;
            if source_changed {
                queue.write_buffer(&scope.source_buffer, 0, source_instance.bytes());
                queue.write_buffer(&scope.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
                stats.source_bytes_uploaded =
                    source_instance.bytes().len() + size_of::<CameraUniform>();
                match &source_instance {
                    SourceInstance::Draws { source, silhouette } => {
                        let text = text.expect("isolated text capture state");
                        source.prepare_filtered_bindings(
                            device,
                            &programs.layout,
                            &scope.source_buffer,
                            &mut scope.source_filtered_bindings,
                        );
                        source
                            .prepare_image_bindings(
                                self,
                                device,
                                &scope.source_buffer,
                                &mut scope.source_image_bindings,
                            )
                            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
                        source
                            .encode(
                                self,
                                text,
                                prepared,
                                encoder,
                                &scope.source.view,
                                scope.multisampled.as_ref().expect("isolated capture MSAA"),
                                &scope.camera_binding,
                                scope
                                    .text_camera_binding
                                    .as_ref()
                                    .expect("isolated text camera"),
                                &scope.source_buffer,
                                &scope.source_image_bindings,
                                Some((programs, &scope.source_filtered_bindings)),
                                source_tile.size,
                            )
                            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
                        stats.source_passes = 1;
                        if let (Some(source), Some(image), Some(buffer)) =
                            (silhouette, &scope.silhouette, &scope.silhouette_buffer)
                        {
                            queue.write_buffer(buffer, 0, &source.bytes);
                            source.prepare_filtered_bindings(
                                device,
                                &programs.layout,
                                buffer,
                                &mut scope.silhouette_filtered_bindings,
                            );
                            source
                                .prepare_image_bindings(
                                    self,
                                    device,
                                    buffer,
                                    &mut scope.silhouette_image_bindings,
                                )
                                .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
                            source
                                .encode(
                                    self,
                                    text,
                                    prepared,
                                    encoder,
                                    &image.view,
                                    scope.multisampled.as_ref().expect("isolated capture MSAA"),
                                    &scope.camera_binding,
                                    scope
                                        .text_camera_binding
                                        .as_ref()
                                        .expect("isolated text camera"),
                                    buffer,
                                    &scope.silhouette_image_bindings,
                                    Some((programs, &scope.silhouette_filtered_bindings)),
                                    source_tile.size,
                                )
                                .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
                            stats.source_passes += 1;
                        }
                        stats.source_bytes_uploaded = source.bytes.len()
                            + size_of::<CameraUniform>()
                            + silhouette.as_ref().map_or(0, |s| s.bytes.len());
                    }
                    _ => {
                        let pipeline = if key.0 == 0 {
                            &self.circle_pipeline_single_sample
                        } else {
                            &self.rectangle_pipeline_single_sample
                        };
                        capture_source(
                            encoder,
                            &scope.source.view,
                            pipeline,
                            &scope.camera_binding,
                            &self.quad_buffer,
                            &scope.source_buffer,
                            source_tile.size,
                        );
                        stats.source_passes = 1;
                        if let (Some(image), Some(buffer)) =
                            (&scope.silhouette, &scope.silhouette_buffer)
                        {
                            queue.write_buffer(buffer, 0, source_instance.silhouette().bytes());
                            capture_source(
                                encoder,
                                &image.view,
                                pipeline,
                                &scope.camera_binding,
                                &self.quad_buffer,
                                buffer,
                                source_tile.size,
                            );
                            stats.source_bytes_uploaded += source_instance.bytes().len();
                            stats.source_passes += 1;
                        }
                    }
                }
                scope.revision = scope
                    .revision
                    .checked_add(1)
                    .expect("glow capture revision exhausted");
                scope.invalidated = false;
            }
            if replace || scope.placement != placement {
                queue.write_buffer(&scope.placement_buffer, 0, bytemuck::bytes_of(&placement));
                stats.placement_bytes_uploaded = size_of::<Placement>();
            }
            // Private Visibility clips the original stream independently of
            // authored paint filters. It shares the painted mask's source domain
            // and operator; final scope opacity still belongs to placement.
            scope.original_masked = demand.original_by_parent
                && visibility.is_some_and(|definition| !definition.is_neutral());
            let capture = GlowCapture {
                source: &scope.source.texture,
                silhouette: scope.silhouette.as_ref().map(|s| &s.texture),
                revision: scope.revision,
            };
            let filter_parameters = EffectStackParameters {
                shader: shader_context.as_ref(),
                working_format: self.target_format,
                output_height: parameters.output_height,
                world_view_height: parameters.world_view_height,
                // Keep the captured/filter image independent of final
                // placement opacity, including when a parent consumes it.
                scope_opacity: 1.0,
                scratch_budget_bytes: request.texture_budget_bytes,
                source_bounds: Some(source_bounds),
            };
            // A parent/prefix needs a real image. Otherwise the final pointwise
            // Glow composition can run in the existing painter-placement draw.
            // Keep the planner's materialized reservation for later consumers.
            let placement_only = !demand.required_by_parent
                && !demand.original_by_parent
                && !demand.isolate
                && visibility.is_none()
                && matches!(request.definitions, [EffectDefinition::Glow(_)])
                && EffectStackFilter::supports_glow_placement(device, self.target_format);
            let upload = if placement_only {
                glows.filter.prepare_glow_placement(
                    device,
                    queue,
                    encoder,
                    &mut scope.filter,
                    capture,
                    request.definitions,
                    filter_parameters,
                    &programs.layout,
                    demand.defer_glow_blur,
                )?
            } else {
                glows.filter.prepare_encode(
                    device,
                    queue,
                    encoder,
                    &mut scope.filter,
                    capture,
                    request.definitions,
                    filter_parameters,
                )?
            };
            // The authored stack checks every later pass, including the same
            // Visibility domain/profile, before this additional mask draw.
            let original_work = if scope.original_masked {
                let image = scope
                    .silhouette
                    .as_ref()
                    .ok_or(EffectPrepareError::MissingSilhouette)?;
                glows.filter.sample.prepare_encode(
                    device,
                    queue,
                    encoder,
                    &mut scope.original_mask,
                    super::GaussianBlurCapture {
                        source: &image.texture,
                        revision: scope.revision,
                    },
                    super::SampleParameters {
                        shader: None,
                        definition: visibility.expect("active private visibility").into(),
                        output_height: parameters.output_height,
                        world_view_height: parameters.world_view_height,
                        source_bounds,
                        scope_opacity: 1.0,
                        scratch_budget_bytes: u64::from(tile.size[0])
                            * u64::from(tile.size[1])
                            * texel_bytes,
                    },
                )?
            } else {
                EffectRasterStats::default()
            };
            let pipeline_compiles = stats.filter.pipeline_compiles;
            stats.filter = upload;
            stats.filter.pipeline_compiles += pipeline_compiles;
            stats.filter.composite_passes += original_work.composite_passes;
            stats.filter.bytes_uploaded += original_work.bytes_uploaded;
            stats.filter.texture_allocations += original_work.texture_allocations;
            stats.filter.buffer_allocations += original_work.buffer_allocations;
            stats.filter.bind_group_creations += original_work.bind_group_creations;
            stats.filter.pipeline_compiles += original_work.pipeline_compiles;
            stats.filter.scratch_bytes += scope.original_mask.scratch_bytes();
            let output = scope
                .filter
                .output()
                .unwrap_or(&scope.source.texture)
                .clone();
            if source_changed
                || stats.filter.composite_passes > 0
                || (scope.filter.placement_glow().is_some() && stats.filter.bytes_uploaded != 0)
                || scope.placement.opacity != placement.opacity
                || scope.placement_output.as_ref() != Some(&output)
            {
                scope.output_revision = scope
                    .output_revision
                    .checked_add(1)
                    .expect("effect output revision exhausted");
            }
            if scope.placement_output.as_ref() != Some(&output) || scope.placement_binding.is_none()
            {
                scope.placement_binding =
                    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Noon composed glow tile binding"),
                        layout: &programs.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: scope.placement_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(
                                    scope.filter.output_view().unwrap_or(&scope.source.view),
                                ),
                            },
                        ],
                    }));
                scope.placement_output = Some(output);
            }
            glows.bytes = total;
            scope.effective = effective;
            scope.source_instance = source_instance;
            scope.tile = tile;
            scope.capture_viewport = source_tile.size;
            scope.contribution_bounds = contribution_bounds;
            scope.camera = self.camera;
            scope.viewport = self.viewport_size;
            scope.placement = placement;
            scope.enabled = true;
            stats.retained_texture_bytes = glows.bytes;
            Ok(stats)
        })();
        self.effect_captures = result.is_ok().then_some(glows);
        result
    }

    pub(in crate::gpu) fn encode_deferred_glow_blurs(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        owners: Vec<ObjectId>,
    ) -> EffectCaptureStats {
        let Some(cache) = self.effect_captures.as_mut() else {
            return EffectCaptureStats::default();
        };
        let jobs = owners
            .into_iter()
            .filter_map(|owner| {
                let key = *cache.owners.get(&owner)?;
                cache.scopes.get_mut(&key)?.filter.take_placement_blur()
            })
            .collect();
        EffectCaptureStats {
            filter: cache.filter.encode_glow_blurs(device, encoder, jobs),
            ..EffectCaptureStats::default()
        }
    }

    /// Retire an exact derived object binding and release its local textures.
    /// Existing scene identity/lifecycle owners decide when this is called.
    pub fn remove_effect_capture(&mut self, object: ObjectId) {
        if let Some(glows) = self.effect_captures.as_deref_mut() {
            glows.backdrops.remove(object);
            if let Some(key) = glows.owners.remove(&object) {
                if let Some(scope) = glows.scopes.remove(&key) {
                    glows.bytes -= scope.tile.capture_and_scratch_bytes;
                }
            }
        }
    }

    pub fn effect_capture_texture_bytes(&self) -> u64 {
        self.effect_captures
            .as_deref()
            .map_or(0, |g| g.bytes + g.backdrops.bytes())
    }

    /// Call when recorded capture/filter commands were abandoned rather than
    /// submitted. Renderer/device recreation naturally drops the entire cache.
    pub fn invalidate_effect_captures(&mut self) {
        self.retained_effects.invalidate();
        if let Some(glows) = self.effect_captures.as_deref_mut() {
            glows.backdrops.invalidate();
            for scope in glows.scopes.values_mut() {
                scope.invalidated = true;
                scope.filter.invalidate();
                scope.original_mask.invalidate();
            }
        }
    }

    pub(super) fn has_effect_captures(&self) -> bool {
        self.effect_captures
            .as_deref()
            .is_some_and(|g| g.scopes.values().any(|s| s.enabled))
    }

    fn has_own_draw_source_effect(&self, object: ObjectId) -> bool {
        self.effect_captures.as_deref().is_some_and(|g| {
            g.owners
                .get(&object)
                .filter(|key| key.0 == 2)
                .and_then(|key| g.scopes.get(key))
                .is_some_and(|scope| scope.enabled)
        })
    }

    pub(super) fn has_draw_source_effect(&self, object: ObjectId) -> bool {
        self.retained_effects
            .composed_placement(object, |owner| self.has_own_draw_source_effect(owner))
            .is_some()
            || self.has_own_draw_source_effect(object)
    }

    pub(super) fn skip_composed_source(&self, object: ObjectId) -> bool {
        self.retained_effects
            .composed_placement(object, |owner| self.has_own_draw_source_effect(owner))
            .is_some_and(|(_, anchor)| !anchor)
    }

    pub(super) fn draw_source_effect<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        object: ObjectId,
        single_sample: bool,
    ) -> bool {
        let object = match self
            .retained_effects
            .composed_placement(object, |owner| self.has_own_draw_source_effect(owner))
        {
            Some((owner, true)) => owner,
            Some((_, false)) => return false,
            None => object,
        };
        let Some(glows) = self.effect_captures.as_deref() else {
            return false;
        };
        let Some(scope) = glows
            .owners
            .get(&object)
            .filter(|key| key.0 == 2)
            .and_then(|key| glows.scopes.get(key))
            .filter(|scope| scope.enabled)
        else {
            return false;
        };
        assert_eq!(
            scope.camera, self.camera,
            "source capture must follow camera changes"
        );
        assert_eq!(
            scope.viewport, self.viewport_size,
            "source capture must follow resize"
        );
        let programs = glows.placement.as_ref().expect("prepared placement");
        let placement = scope
            .placement_binding
            .as_ref()
            .expect("prepared source output");
        if !glows
            .filter
            .draw_glow_placement(pass, &scope.filter, placement, single_sample)
        {
            programs.draw(
                pass,
                placement,
                scope.placement_erase_binding.as_ref(),
                single_sample,
            );
        }

        true
    }

    /// Split only batches intersecting existing effect slots. Normal spans keep
    /// the ordinary contribution filter. A transparent silhouette source retains
    /// its anchor, even when its base paint would otherwise be suppressed.
    pub(super) fn draw_analytic_glow_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        resolved: &ResolvedOrderedBatch,
        single_sample: bool,
        binding: &mut Option<GeometryBinding>,
    ) -> Option<DrawStats> {
        let glows = self.effect_captures.as_deref()?;
        let kind = match resolved.batch.primitive {
            RenderPrimitive::Circle => 0,
            RenderPrimitive::Rectangle => 1,
            _ => return None,
        };
        let range = resolved.batch.instance_range.clone();
        let mut scopes = glows
            .scopes
            .range((kind, range.start as usize)..(kind, range.end as usize))
            .filter(|((_, index), scope)| {
                scope.enabled
                    && match kind {
                        0 => prepared.circle_ids.get(*index) == Some(&scope.object),
                        _ => prepared.rectangle_ids.get(*index) == Some(&scope.object),
                    }
            })
            .peekable();
        scopes.peek()?;
        let mut stats = DrawStats::default();
        let mut next = range.start;
        for ((_, index), scope) in scopes {
            // Never present stale data after a view or effective source edit.
            // The shared publication integration must reprepare these local rows.
            assert_eq!(
                scope.camera, self.camera,
                "glow must be reprepared after camera changes"
            );
            assert_eq!(
                scope.viewport, self.viewport_size,
                "glow must be reprepared after resize"
            );
            assert_eq!(
                Some(scope.effective.clone()),
                SourceInstance::prepared(prepared, (kind, *index)),
                "glow must be reprepared after an effective source change"
            );
            let instance_index =
                u32::try_from(*index).expect("prepared analytic index exceeds GPU limits");
            if next < instance_index {
                let mut segment = resolved.clone();
                segment.batch.instance_range = next..instance_index;
                stats +=
                    self.draw_plain_ordered_batch(pass, prepared, &segment, single_sample, binding);
            }
            let programs = glows.placement.as_ref().expect("prepared glow programs");
            let placement = scope
                .placement_binding
                .as_ref()
                .expect("prepared glow image");
            if !glows
                .filter
                .draw_glow_placement(pass, &scope.filter, placement, single_sample)
            {
                programs.draw(
                    pass,
                    placement,
                    scope.placement_erase_binding.as_ref(),
                    single_sample,
                );
            }
            stats.draw_calls += 1;
            stats.instances_drawn += 1;
            // The tile's layout differs from the ordinary geometry layout.
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            *binding = None;
            next = instance_index + 1;
        }
        if next < range.end {
            let mut segment = resolved.clone();
            segment.batch.instance_range = next..range.end;
            stats +=
                self.draw_plain_ordered_batch(pass, prepared, &segment, single_sample, binding);
        }
        Some(stats)
    }
}

fn tile_camera(
    camera: Camera2D,
    viewport: [u32; 2],
    tile: EffectCaptureTile,
) -> Result<Camera2D, EffectPrepareError> {
    capture_camera(camera, viewport, tile.origin, tile.size)
}

pub(super) fn capture_camera(
    camera: Camera2D,
    viewport: [u32; 2],
    origin: [i32; 2],
    size: [u32; 2],
) -> Result<Camera2D, EffectPrepareError> {
    let pixel = [
        f64::from(camera.world_size.x) / f64::from(viewport[0]),
        f64::from(camera.world_size.y) / f64::from(viewport[1]),
    ];
    let center = [
        f64::from(camera.center.x)
            + (f64::from(origin[0]) + f64::from(size[0]) * 0.5 - f64::from(viewport[0]) * 0.5)
                * pixel[0],
        f64::from(camera.center.y)
            - (f64::from(origin[1]) + f64::from(size[1]) * 0.5 - f64::from(viewport[1]) * 0.5)
                * pixel[1],
    ];
    Camera2D::new(
        Vec2::new(center[0] as f32, center[1] as f32),
        Vec2::new(
            (f64::from(size[0]) * pixel[0]) as f32,
            (f64::from(size[1]) * pixel[1]) as f32,
        ),
    )
    .map_err(|_| EffectPrepareError::InvalidProjection)
}

fn instance_buffer(device: &wgpu::Device, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Noon isolated analytic glow source"),
        size: bytes.max(size_of::<CircleInstance>()).next_power_of_two() as u64,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::UNIFORM
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn capture_source(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    camera: &wgpu::BindGroup,
    quad: &wgpu::Buffer,
    instance: &wgpu::Buffer,
    viewport: [u32; 2],
) {
    let attachments = [Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    })];
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Noon padded analytic source capture"),
        color_attachments: &attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_viewport(0.0, 0.0, viewport[0] as f32, viewport[1] as f32, 0.0, 1.0);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, camera, &[]);
    pass.set_vertex_buffer(0, quad.slice(..));
    pass.set_vertex_buffer(1, instance.slice(..));
    pass.draw(0..6, 0..1);
}

impl PlacementPrograms {
    /// Ordinary scenes compile only their existing source-over programs.
    pub(super) fn ensure_replacement(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> usize {
        if self.replacement.is_some() {
            return 0;
        }
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon replacement tile placement"),
            bind_group_layouts: &[Some(&self.layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("analytic_glow.wgsl"));
        let pipeline = |count, erase| {
            let blend = wgpu::BlendComponent {
                src_factor: if erase {
                    wgpu::BlendFactor::Zero
                } else {
                    wgpu::BlendFactor::One
                },
                dst_factor: if erase {
                    wgpu::BlendFactor::OneMinusSrcAlpha
                } else {
                    wgpu::BlendFactor::One
                },
                operation: wgpu::BlendOperation::Add,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Noon replacement tile pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        self.replacement = Some([
            pipeline(1, true),
            pipeline(1, false),
            pipeline(PATH_SAMPLE_COUNT, true),
            pipeline(PATH_SAMPLE_COUNT, false),
        ]);
        4
    }

    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        contribution: &'a wgpu::BindGroup,
        erase: Option<&'a wgpu::BindGroup>,
        single_sample: bool,
    ) {
        if let Some(erase) = erase {
            let programs = self
                .replacement
                .as_ref()
                .expect("prepared replacement programs");
            let at = if single_sample { 0 } else { 2 };
            pass.set_pipeline(&programs[at]);
            pass.set_bind_group(0, erase, &[]);
            pass.draw(0..6, 0..1);
            pass.set_pipeline(&programs[at + 1]);
        } else {
            pass.set_pipeline(if single_sample {
                &self.single
            } else {
                &self.multisampled
            });
        }
        pass.set_bind_group(0, contribution, &[]);
        pass.draw(0..6, 0..1);
    }

    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon glow tile placement layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<Placement>() as u64),
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
            label: Some("Noon glow tile placement"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("analytic_glow.wgsl"));
        let pipeline = |count| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Noon glow tile pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            single: pipeline(1),
            multisampled: pipeline(PATH_SAMPLE_COUNT),
            layout,
            replacement: None,
        }
    }
}

#[cfg(test)]
mod tests;
