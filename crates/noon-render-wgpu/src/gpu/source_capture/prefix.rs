//! Bounded explicit painter-prefix capture using the ordinary retained draws.
//! Selection/dependency discovery belongs to the execution/publication caller.
//! This operator validates order and captures a supplied dependency closure; it
//! does not invent scene membership, discover dependencies, or own a clock.
use super::*;
use crate::gpu::analytic_glow::{capture_camera, PlacementPrograms};
use crate::gpu::{
    raster_image_gpu::CapturedImageBinding, Camera2D, CameraUniform, EffectPrepareError,
    EffectRasterStats,
};
use std::{mem::size_of, sync::Arc};

#[cfg(test)]
mod tests;

/// A derived selection from the execution-owned expanded-window query.
/// Include complete earlier isolated intervals when their filtered output is
/// needed. Advance source_revision after any selected input content/order edit,
/// including in-place mesh/text changes that do not change packed instances.
#[derive(Clone, Debug)]
pub struct PainterPrefixCaptureRequest {
    pub anchor: noon_core::ObjectId,
    pub candidates: Arc<[noon_core::ObjectId]>,
    pub source_revision: u64,
    pub origin: [i32; 2],
    pub size: [u32; 2],
    /// The same encoded-premultiplied clear used by the consuming view.
    pub clear_color: wgpu::Color,
    /// RGBA8 output plus four-sample source attachment, twenty bytes per pixel.
    /// Source buffers and in-flight host resources are separately accounted.
    pub scratch_budget_bytes: u64,
}
#[derive(Clone, Debug)]
struct Key {
    // Existing GPU resource identity, not a scene ID or new context allocator.
    renderer_camera: wgpu::Buffer,
    request: PainterPrefixCaptureRequest,
    camera: Camera2D,
    viewport: [u32; 2],
    format: wgpu::TextureFormat,
}
impl Key {
    fn same_content(&self, next: &Self) -> bool {
        let a = &self.request;
        let b = &next.request;
        a.anchor == b.anchor
            && a.source_revision == b.source_revision
            && a.origin == b.origin
            && a.size == b.size
            && a.clear_color == b.clear_color
            && (Arc::ptr_eq(&a.candidates, &b.candidates) || a.candidates == b.candidates)
            && self.camera == next.camera
            && self.viewport == next.viewport
            && self.format == next.format
    }
    fn same_camera(&self, next: &Self) -> bool {
        self.camera == next.camera
            && self.viewport == next.viewport
            && self.request.origin == next.request.origin
            && self.request.size == next.request.size
    }
}
#[derive(Clone, Debug)]
struct Resources {
    output: wgpu::Texture,
    view: wgpu::TextureView,
    multisampled: wgpu::TextureView,
    instances: wgpu::Buffer,
    camera: wgpu::Buffer,
    camera_binding: wgpu::BindGroup,
    text_camera: wgpu::BindGroup,
    images: Vec<CapturedImageBinding>,
    filtered: Vec<FilteredBinding>,
}
#[derive(Debug, Default)]
pub struct PainterPrefixCaptureScope {
    resources: Option<Resources>,
    source: Option<PreparedSource>,
    key: Option<Key>,
    revision: u64,
    invalidated: bool,
}
impl PainterPrefixCaptureScope {
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.resources.as_ref().map(|r| &r.output)
    }
    pub fn revision(&self) -> Option<u64> {
        self.key.as_ref().map(|_| self.revision)
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.output()
            .map_or(0, |t| u64::from(t.width()) * u64::from(t.height()) * 20)
    }
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
#[derive(Debug, Default)]
pub struct PainterPrefixCaptureFilter {
    placement: Option<(wgpu::TextureFormat, PlacementPrograms)>,
}
impl PainterPrefixCaptureFilter {
    /// Use after ordinary retained geometry/image upload. Prefix output is an
    /// input texture, never an implicit replacement of the target's own paint.
    /// The caller owns the encoder and submission. Abandon a failed encoder and
    /// invalidate the scope before retrying an otherwise abandoned successful
    /// preparation. Renderer replacement requires a fresh/cleared scope; device
    /// replacement also requires a new filter.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &GpuRenderer,
        text: &RetainedTextGpuState,
        prepared: &PreparedRetainedGpuFrame<'_>,
        scope: &mut PainterPrefixCaptureScope,
        request: &PainterPrefixCaptureRequest,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        if scope
            .key
            .as_ref()
            .is_some_and(|old| old.renderer_camera != renderer.camera_buffer)
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        let key = Key {
            renderer_camera: renderer.camera_buffer.clone(),
            request: request.clone(),
            camera: renderer.camera,
            viewport: renderer.viewport_size,
            format: renderer.target_format,
        };
        let bytes = checked_request(device, &key)?;
        if bytes > request.scratch_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        if !scope.invalidated && scope.key.as_ref().is_some_and(|old| old.same_content(&key)) {
            return Ok(EffectRasterStats {
                scratch_bytes: scope.scratch_bytes(),
                ..Default::default()
            });
        }
        let revision = scope
            .revision
            .checked_add(1)
            .ok_or(EffectPrepareError::RevisionExhausted)?;
        let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        // Resolve earlier retained inputs, then validate and pack the selection
        // once. An empty prefix is valid and still captures the view clear.
        let members = request
            .candidates
            .iter()
            .map(|&id| (id, 1.0))
            .collect::<Vec<_>>();
        let replacements = renderer.prepare_composed_replacements(
            prepared,
            usize::MAX,
            &request.candidates,
            false,
        )?;
        let mut source = PreparedSource::prepare_prefix(
            prepared,
            &members,
            alignment,
            &replacements,
            request.anchor,
            request.clear_color,
        )
        .ok_or(EffectPrepareError::PublicationMismatch)?;
        source
            .set_capture_region(request.origin, request.size)
            .ok_or(EffectPrepareError::CaptureCoordinatesOutOfRange)?;
        if scope.output().is_some_and(|output| {
            source.draws.iter().any(|draw|
            matches!(draw, SourceDraw::Filtered { capture, .. } if &capture.texture == output))
        }) {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let buffer_size = source
            .bytes
            .len()
            .max(size_of::<crate::CircleInstance>())
            .checked_next_power_of_two()
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)? as u64;
        if buffer_size > device.limits().max_buffer_size {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        let camera = capture_camera(key.camera, key.viewport, request.origin, request.size)?;
        let camera_uniform = camera
            .uniform(request.size)
            .with_sample_projection(key.camera.uniform(key.viewport));
        let mut stats = EffectRasterStats {
            scratch_bytes: bytes,
            ..Default::default()
        };
        // Stage replacement resources and bindings. A checked failure keeps the
        // old output, key, revision and binding lists coherent.
        let mut resources = if let Some(old) = scope.resources.as_ref() {
            old.clone()
        } else {
            stats.texture_allocations = 2;
            stats.buffer_allocations = 2;
            stats.bind_group_creations = 2;
            Resources::new(device, renderer, text, request.size, buffer_size)
        };
        let replace_images = resources.output.width() != request.size[0]
            || resources.output.height() != request.size[1]
            || resources.output.format() != key.format;
        if replace_images {
            let (output, view, multisampled) = images(device, key.format, request.size);
            resources.output = output;
            resources.view = view;
            resources.multisampled = multisampled;
            stats.texture_allocations += 2;
        }
        let new_buffer = resources.instances.size() < buffer_size;
        if new_buffer {
            resources.instances = instance_buffer(device, buffer_size);
            resources.images.clear();
            resources.filtered.clear();
            stats.buffer_allocations += 1;
        }
        let old_image_keys = resources
            .images
            .iter()
            .map(|b| (b.resource, b.offset))
            .collect::<Vec<_>>();
        source
            .prepare_image_bindings(
                renderer,
                device,
                &resources.instances,
                &mut resources.images,
            )
            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
        stats.bind_group_creations += resources
            .images
            .iter()
            .enumerate()
            .filter(|(i, b)| old_image_keys.get(*i) != Some(&(b.resource, b.offset)))
            .count();
        let filtered = if source.has_filtered {
            if self
                .placement
                .as_ref()
                .is_none_or(|(format, _)| *format != key.format)
            {
                self.placement = Some((key.format, PlacementPrograms::new(device, key.format)));
                stats.pipeline_compiles += 2;
            }
            let programs = &mut self
                .placement
                .as_mut()
                .expect("prefix placement programs")
                .1;
            if source.has_replacements() {
                stats.pipeline_compiles += programs.ensure_replacement(device, key.format);
            }
            let old_keys = resources
                .filtered
                .iter()
                .map(|b| (&b.texture, b.offset))
                .map(|(t, o)| (t.clone(), o))
                .collect::<Vec<_>>();
            source.prepare_filtered_bindings(
                device,
                &programs.layout,
                &resources.instances,
                &mut resources.filtered,
            );
            stats.bind_group_creations += resources
                .filtered
                .iter()
                .enumerate()
                .filter(|(i, b)| old_keys.get(*i) != Some(&(b.texture.clone(), b.offset)))
                .count();
            Some((&*programs, resources.filtered.as_slice()))
        } else {
            resources.filtered.clear();
            None
        };
        let source_changed = scope.invalidated
            || new_buffer
            || scope
                .source
                .as_ref()
                .is_none_or(|old| old.bytes != source.bytes);
        let camera_changed =
            scope.invalidated || scope.key.as_ref().is_none_or(|old| !old.same_camera(&key));
        scope.invalidated = true;
        if source_changed && !source.bytes.is_empty() {
            queue.write_buffer(&resources.instances, 0, &source.bytes);
            stats.bytes_uploaded += source.bytes.len();
        }
        if camera_changed {
            queue.write_buffer(&resources.camera, 0, bytemuck::bytes_of(&camera_uniform));
            stats.bytes_uploaded += size_of::<CameraUniform>();
        }
        prepared.text.atlas.encode_pending_uploads(encoder);
        source
            .encode(
                renderer,
                text,
                &prepared.geometry,
                encoder,
                &resources.view,
                &resources.multisampled,
                &resources.camera_binding,
                &resources.text_camera,
                &resources.instances,
                &resources.images,
                filtered,
                request.size,
            )
            .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
        stats.composite_passes = 1;
        scope.resources = Some(resources);
        scope.source = Some(source);
        scope.key = Some(key);
        scope.revision = revision;
        scope.invalidated = false;
        Ok(stats)
    }
}
fn checked_request(device: &wgpu::Device, key: &Key) -> Result<u64, EffectPrepareError> {
    let p = &key.request;
    let c = p.clear_color;
    if ![c.r, c.g, c.b, c.a]
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        || [c.r, c.g, c.b].into_iter().any(|v| v > c.a)
    {
        return Err(EffectPrepareError::InvalidCaptureClearColor);
    }
    if !matches!(
        key.format,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
    ) {
        return Err(EffectPrepareError::UnsupportedCapture);
    }
    if p.size.contains(&0) || key.viewport.contains(&0) {
        return Err(EffectPrepareError::InvalidProjection);
    }
    if p.size
        .into_iter()
        .chain(key.viewport)
        .any(|n| n > device.limits().max_texture_dimension_2d)
    {
        return Err(EffectPrepareError::ExtentExceedsDevice);
    }
    if p.origin.into_iter().any(|v| v.unsigned_abs() > 1_048_576) {
        return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
    }
    Ok(u64::from(p.size[0]) * u64::from(p.size[1]) * 20)
}
fn instance_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Noon painter prefix instances"),
        size,
        usage: wgpu::BufferUsages::UNIFORM
            | wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn images(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Noon painter prefix capture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    let multisampled = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon painter prefix source MSAA"),
            size: extent,
            mip_level_count: 1,
            sample_count: PATH_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    (output, view, multisampled)
}
impl Resources {
    fn new(
        device: &wgpu::Device,
        renderer: &GpuRenderer,
        text: &RetainedTextGpuState,
        size: [u32; 2],
        buffer_size: u64,
    ) -> Self {
        let (output, view, multisampled) = images(device, renderer.target_format, size);
        let instances = instance_buffer(device, buffer_size);
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon painter prefix camera"),
            size: size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon painter prefix camera binding"),
            layout: &renderer.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let text_camera = text.glyphs.capture_camera_binding(device, &camera);
        Self {
            output,
            view,
            multisampled,
            instances,
            camera,
            camera_binding,
            text_camera,
            images: Vec::new(),
            filtered: Vec::new(),
        }
    }
}
