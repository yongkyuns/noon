//! Finite analytic-source integration of the shared glow operator.
//!
//! Sources use the renderer's existing analytic pipelines. The final tile replaces
//! exactly one stable primitive draw; it is not a foreground overlay. These caches
//! are indexed by existing packed-instance slots and retain no semantic authority.
//! #1897 owns feeding these effective requests from shared lowering/publication.

use std::{collections::BTreeMap, mem::size_of};

use bytemuck::{Pod, Zeroable};
use noon_core::{GeometryRef, Glow, GlowSource, ObjectId, Transform2D, Vec2};

use super::{
    Camera2D, CameraUniform, DrawStats, GeometryBinding, GlowCapture, GlowCaptureTile, GlowFilter,
    GlowParameters, GlowPixelBounds, GlowPrepareError, GlowRasterStats, GlowScope, GpuRenderer,
    ResolvedOrderedBatch, PATH_SAMPLE_COUNT,
};
use crate::{CircleInstance, PackedStyle, PreparedFrame, RectangleInstance, RenderPrimitive};

/// One effective renderer request. Object identity/attachment values and animation
/// phase must come from the normal runtime publication, not a host-owned store.
#[derive(Clone, Copy, Debug)]
pub struct AnalyticGlowRequest {
    pub object_index: usize,
    pub definition: Glow,
    /// Total retained capture + filter texture budget for this renderer, not just
    /// the current object. Source buffers and shared pipeline metadata are separate.
    /// Resources retained by already-submitted frames require an additional host
    /// in-flight budget; this counter covers the current renderer-owned cache.
    pub texture_budget_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalyticGlowStats {
    pub source_passes: usize,
    pub source_bytes_uploaded: usize,
    pub source_texture_allocations: usize,
    pub placement_bytes_uploaded: usize,
    pub filter: GlowRasterStats,
    pub retained_texture_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SourceInstance {
    Circle(CircleInstance),
    Rectangle(RectangleInstance),
}
impl SourceInstance {
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
        }
    }
    fn style_mut(&mut self) -> &mut PackedStyle {
        match self {
            Self::Circle(value) => &mut value.style,
            Self::Rectangle(value) => &mut value.style,
        }
    }
    fn geometry(self) -> GeometryRef {
        match self {
            Self::Circle(value) => GeometryRef::circle(value.radius),
            Self::Rectangle(value) => GeometryRef::rectangle(value.size[0], value.size[1]),
        }
    }
    fn silhouette(mut self) -> Self {
        let style = self.style_mut();
        style.fill = [1.0; 4];
        style.fill_enabled = 1;
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Placement {
    origin: [i32; 2],
    size: [u32; 2],
    viewport: [u32; 2],
    padding: [u32; 2],
}

#[derive(Debug)]
struct CaptureImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
impl CaptureImage {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon analytic glow source capture"),
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
struct ResidentGlow {
    object: ObjectId,
    tile: GlowCaptureTile,
    camera: Camera2D,
    viewport: [u32; 2],
    effective: SourceInstance,
    source_instance: SourceInstance,
    source: CaptureImage,
    silhouette: Option<CaptureImage>,
    source_buffer: wgpu::Buffer,
    silhouette_buffer: Option<wgpu::Buffer>,
    camera_buffer: wgpu::Buffer,
    camera_binding: wgpu::BindGroup,
    placement: Placement,
    placement_buffer: wgpu::Buffer,
    placement_binding: Option<wgpu::BindGroup>,
    filter: GlowScope,
    revision: u64,
    invalidated: bool,
    enabled: bool,
}

#[derive(Debug)]
struct PlacementPrograms {
    layout: wgpu::BindGroupLayout,
    single: wgpu::RenderPipeline,
    multisampled: wgpu::RenderPipeline,
}

#[derive(Debug, Default)]
pub(super) struct AnalyticGlowGpu {
    // RenderPrimitive is already a derived identity. Restrict keys to the two
    // admitted analytic kinds rather than allocating another object-ID domain.
    scopes: BTreeMap<(u8, usize), ResidentGlow>,
    owners: BTreeMap<ObjectId, (u8, usize)>,
    filter: GlowFilter,
    placement: Option<PlacementPrograms>,
    bytes: u64,
}

/// Sparse request for one runtime-owned row, never a host copy of effect values.
#[derive(Clone, Copy, Debug)]
pub struct PublishedAnalyticGlowRequest {
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
    pub fn prepare_published_analytic_glow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        publication: &noon_runtime::RendererPublication<'_>,
        request: PublishedAnalyticGlowRequest,
    ) -> Result<AnalyticGlowStats, GlowPrepareError> {
        let frame = publication.frame();
        let row = frame
            .objects
            .get(request.object_index)
            .ok_or(GlowPrepareError::UnsupportedCapture)?;
        if frame.time != prepared.time {
            return Err(GlowPrepareError::PublicationMismatch);
        }
        let Some(glow) = row
            .glow
            .as_ref()
            .filter(|_| frame.is_present(request.object_index))
        else {
            self.remove_analytic_glow(row.id);
            return Ok(AnalyticGlowStats {
                retained_texture_bytes: self.analytic_glow_texture_bytes(),
                ..Default::default()
            });
        };
        let observation = prepared
            .observe_object(request.object_index)
            .map_err(|_| GlowPrepareError::UnsupportedCapture)?;
        let key = match observation.primitive {
            RenderPrimitive::Circle => (0, observation.instance_index),
            RenderPrimitive::Rectangle => (1, observation.instance_index),
            _ => return Err(GlowPrepareError::UnsupportedCapture),
        };
        let source =
            SourceInstance::prepared(prepared, key).ok_or(GlowPrepareError::UnsupportedCapture)?;
        if observation.object != row.id
            || observation.transform != frame.render_transform(request.object_index).into()
            || observation.style != crate::pack_style(row)
            || frame.render_geometry(request.object_index) != Some(&source.geometry())
            || frame.reveal(request.object_index) != 1.0
        {
            return Err(GlowPrepareError::PublicationMismatch);
        }
        self.prepare_analytic_glow(
            device,
            queue,
            encoder,
            prepared,
            AnalyticGlowRequest {
                object_index: request.object_index,
                definition: glow.definition,
                texture_budget_bytes: request.texture_budget_bytes,
            },
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
    pub fn prepare_analytic_glow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedFrame<'_>,
        request: AnalyticGlowRequest,
    ) -> Result<AnalyticGlowStats, GlowPrepareError> {
        let observation = prepared
            .observe_object(request.object_index)
            .map_err(|_| GlowPrepareError::UnsupportedCapture)?;
        let kind = match observation.primitive {
            RenderPrimitive::Circle => 0,
            RenderPrimitive::Rectangle => 1,
            _ => return Err(GlowPrepareError::UnsupportedCapture),
        };
        // observe_object already requires a present resident slot. Some(false)
        // means only its base paint is suppressed; a silhouette halo can still
        // contribute. None is the unsupported partial-visibility projection.
        if observation.submission_membership.is_none()
            || !self.inset_views.is_empty()
            || !matches!(
                self.target_format,
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            )
        {
            return Err(GlowPrepareError::UnsupportedCapture);
        }
        let key = (kind, observation.instance_index);
        let effective =
            SourceInstance::prepared(prepared, key).ok_or(GlowPrepareError::UnsupportedCapture)?;
        if observation.style.stroke_enabled & 1 != 0
            || observation.style.fill_enabled == 0
            || matches!(effective, SourceInstance::Circle(value) if value.padding[0] != 1.0)
        {
            return Err(GlowPrepareError::UnsupportedCapture);
        }
        if !observation
            .style
            .fill
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            return Err(GlowPrepareError::UnsupportedCapture);
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
        let parameters = GlowParameters {
            definition: request.definition,
            output_height: self.viewport_size[1],
            world_view_height: f64::from(self.camera.world_size.y),
            scope_opacity: observation.style.opacity,
            scratch_budget_bytes: request.texture_budget_bytes,
        };
        let bounds = GlowPixelBounds::projected_analytic(
            &effective.geometry(),
            transform,
            self.camera,
            self.viewport_size,
        )?;
        let tile = GlowCaptureTile::prepare(
            bounds,
            self.viewport_size,
            parameters,
            device.limits().max_texture_dimension_2d,
            request.texture_budget_bytes,
        )?;
        let Some(mut tile) = tile else {
            // Retain warmed storage for neutral fades; exact ordinary submission
            // is restored. Absent neutral requests allocate no glow cache.
            if let Some(glows) = self.analytic_glows.as_deref_mut() {
                if let Some(scope) = glows
                    .scopes
                    .get_mut(&key)
                    .filter(|s| s.object == observation.object)
                {
                    scope.enabled = false;
                }
            }
            return Ok(AnalyticGlowStats {
                retained_texture_bytes: self.analytic_glow_texture_bytes(),
                ..Default::default()
            });
        };
        let old = self
            .analytic_glows
            .as_deref()
            .and_then(|g| g.scopes.get(&key));
        // Stable high-water capacity avoids reallocating captures whenever a
        // fractionally translated footprint alternates between adjacent sizes.
        // Extra padding is transparent; resolution/kernel quality is unchanged.
        let previous_size = old
            .filter(|scope| scope.object == observation.object)
            .map_or([0; 2], |scope| scope.tile.size);
        for (axis, previous) in previous_size.into_iter().enumerate() {
            let reserved = tile.size[axis]
                .saturating_add(1)
                .next_multiple_of(16)
                .min(device.limits().max_texture_dimension_2d);
            tile.size[axis] = reserved.max(previous);
            tile.visible_size[axis] = (tile.size[axis] - tile.local_origin[axis])
                .min(self.viewport_size[axis] - tile.viewport_origin[axis]);
        }
        tile.capture_and_scratch_bytes = u64::from(tile.size[0])
            .checked_mul(u64::from(tile.size[1]))
            .and_then(|pixels| {
                pixels.checked_mul(if request.definition.source() == GlowSource::Silhouette {
                    20
                } else {
                    16
                })
            })
            .ok_or(GlowPrepareError::ScratchBudgetExceeded)?;
        let old_bytes = old.map_or(0, |s| s.tile.capture_and_scratch_bytes);
        let relocated_bytes = self
            .analytic_glows
            .as_deref()
            .and_then(|g| {
                g.owners
                    .get(&observation.object)
                    .filter(|previous| **previous != key)
                    .and_then(|previous| g.scopes.get(previous))
            })
            .map_or(0, |s| s.tile.capture_and_scratch_bytes);
        let total = self
            .analytic_glow_texture_bytes()
            .checked_sub(old_bytes)
            .and_then(|v| v.checked_sub(relocated_bytes))
            .and_then(|v| v.checked_add(tile.capture_and_scratch_bytes))
            .ok_or(GlowPrepareError::ScratchBudgetExceeded)?;
        if total > request.texture_budget_bytes {
            return Err(GlowPrepareError::ScratchBudgetExceeded);
        }
        let mut source_instance = effective;
        source_instance.style_mut().opacity = 1.0; // apply final scope opacity once
        let needs_silhouette = request.definition.source() == GlowSource::Silhouette;
        let capture_camera = tile_camera(self.camera, self.viewport_size, tile)?;
        let camera_uniform = capture_camera.uniform(tile.size);
        let placement = Placement {
            origin: tile.origin,
            size: tile.size,
            viewport: self.viewport_size,
            padding: [0; 2],
        };
        // Everything that can reject input is checked before installing caches.
        let glows = self.analytic_glows.get_or_insert_with(Default::default);
        let programs = glows
            .placement
            .get_or_insert_with(|| PlacementPrograms::new(device, self.target_format));
        let replace = glows.scopes.get(&key).is_none_or(|s| {
            s.object != observation.object
                || s.tile.size != tile.size
                || s.silhouette.is_some() != needs_silhouette
        });
        let mut stats = AnalyticGlowStats::default();
        if replace {
            let source = CaptureImage::new(device, self.target_format, tile.size);
            let silhouette =
                needs_silhouette.then(|| CaptureImage::new(device, self.target_format, tile.size));
            let source_buffer = instance_buffer(device);
            let silhouette_buffer = needs_silhouette.then(|| instance_buffer(device));
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
                ResidentGlow {
                    object: observation.object,
                    tile,
                    camera: self.camera,
                    viewport: self.viewport_size,
                    effective,
                    source_instance,
                    source,
                    silhouette,
                    source_buffer,
                    silhouette_buffer,
                    camera_buffer,
                    camera_binding,
                    placement,
                    placement_buffer,
                    placement_binding: None,
                    filter: GlowScope::default(),
                    revision: 0,
                    invalidated: true,
                    enabled: true,
                },
            ) {
                glows.owners.remove(&previous.object);
            }
            stats.source_texture_allocations = 1 + usize::from(needs_silhouette);
        }
        if let Some(previous_key) = glows
            .owners
            .insert(observation.object, key)
            .filter(|previous| *previous != key)
        {
            glows.scopes.remove(&previous_key);
        }
        let scope = glows.scopes.get_mut(&key).expect("admitted glow scope");
        let source_changed = scope.invalidated
            || scope.source_instance != source_instance
            || scope.tile.origin != tile.origin
            || scope.camera != self.camera
            || scope.viewport != self.viewport_size;
        if source_changed {
            queue.write_buffer(&scope.source_buffer, 0, source_instance.bytes());
            queue.write_buffer(&scope.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
            stats.source_bytes_uploaded = size_of::<CircleInstance>() + size_of::<CameraUniform>();
            let pipeline = if kind == 0 {
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
            );
            stats.source_passes = 1;
            if let (Some(image), Some(buffer)) = (&scope.silhouette, &scope.silhouette_buffer) {
                queue.write_buffer(buffer, 0, source_instance.silhouette().bytes());
                capture_source(
                    encoder,
                    &image.view,
                    pipeline,
                    &scope.camera_binding,
                    &self.quad_buffer,
                    buffer,
                );
                stats.source_bytes_uploaded += size_of::<CircleInstance>();
                stats.source_passes += 1;
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
        let upload = glows.filter.prepare(
            device,
            queue,
            &mut scope.filter,
            GlowCapture {
                source: &scope.source.texture,
                silhouette: scope.silhouette.as_ref().map(|s| &s.texture),
                revision: scope.revision,
            },
            parameters,
        )?;
        let encoded = glows.filter.encode(encoder, &mut scope.filter);
        stats.filter = GlowRasterStats {
            blur_passes: encoded.blur_passes,
            composite_passes: encoded.composite_passes,
            ..upload
        };
        if replace || upload.texture_allocations != 0 || scope.placement_binding.is_none() {
            scope.placement_binding = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                            scope.filter.output_view().expect("active glow output"),
                        ),
                    },
                ],
            }));
        }
        glows.bytes = total;
        scope.effective = effective;
        scope.source_instance = source_instance;
        scope.tile = tile;
        scope.camera = self.camera;
        scope.viewport = self.viewport_size;
        scope.placement = placement;
        scope.enabled = true;
        stats.retained_texture_bytes = glows.bytes;
        Ok(stats)
    }

    /// Retire an exact derived object binding and release its local textures.
    /// Existing scene identity/lifecycle owners decide when this is called.
    pub fn remove_analytic_glow(&mut self, object: ObjectId) {
        if let Some(glows) = self.analytic_glows.as_deref_mut() {
            if let Some(key) = glows.owners.remove(&object) {
                if let Some(scope) = glows.scopes.remove(&key) {
                    glows.bytes -= scope.tile.capture_and_scratch_bytes;
                }
            }
        }
    }

    pub fn analytic_glow_texture_bytes(&self) -> u64 {
        self.analytic_glows.as_deref().map_or(0, |g| g.bytes)
    }

    /// Call when recorded capture/filter commands were abandoned rather than
    /// submitted. Renderer/device recreation naturally drops the entire cache.
    pub fn invalidate_analytic_glows(&mut self) {
        if let Some(glows) = self.analytic_glows.as_deref_mut() {
            for scope in glows.scopes.values_mut() {
                scope.invalidated = true;
                scope.filter.invalidate();
            }
        }
    }

    pub(super) fn has_analytic_glows(&self) -> bool {
        self.analytic_glows
            .as_deref()
            .is_some_and(|g| g.scopes.values().any(|s| s.enabled))
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
        let glows = self.analytic_glows.as_deref()?;
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
                Some(scope.effective),
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
            pass.set_pipeline(if single_sample {
                &programs.single
            } else {
                &programs.multisampled
            });
            pass.set_bind_group(
                0,
                scope
                    .placement_binding
                    .as_ref()
                    .expect("prepared glow image"),
                &[],
            );
            pass.draw(0..6, 0..1);
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
    tile: GlowCaptureTile,
) -> Result<Camera2D, GlowPrepareError> {
    let pixel = [
        f64::from(camera.world_size.x) / f64::from(viewport[0]),
        f64::from(camera.world_size.y) / f64::from(viewport[1]),
    ];
    let center = [
        f64::from(camera.center.x)
            + (f64::from(tile.origin[0]) + f64::from(tile.size[0]) * 0.5
                - f64::from(viewport[0]) * 0.5)
                * pixel[0],
        f64::from(camera.center.y)
            - (f64::from(tile.origin[1]) + f64::from(tile.size[1]) * 0.5
                - f64::from(viewport[1]) * 0.5)
                * pixel[1],
    ];
    Camera2D::new(
        Vec2::new(center[0] as f32, center[1] as f32),
        Vec2::new(
            (f64::from(tile.size[0]) * pixel[0]) as f32,
            (f64::from(tile.size[1]) * pixel[1]) as f32,
        ),
    )
    .map_err(|_| GlowPrepareError::InvalidProjection)
}

fn instance_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Noon isolated analytic glow source"),
        size: size_of::<CircleInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
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
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, camera, &[]);
    pass.set_vertex_buffer(0, quad.slice(..));
    pass.set_vertex_buffer(1, instance.slice(..));
    pass.draw(0..6, 0..1);
}

impl PlacementPrograms {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
        }
    }
}

#[cfg(test)]
mod tests;
