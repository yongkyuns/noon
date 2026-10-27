//! Isolated 2D source draws for offscreen consumers.
//!
//! Captures borrow canonical per-object draw ranges and existing meshes/atlases.
//! Only instance data is copied, to remove the final owning-scope opacity. This
//! module has no effect definitions, scheduling, semantic identity or filter graph.

use super::{
    raster_image_gpu::{CapturedImageBinding, RasterImageDrawError},
    raster_image_prepare::ImageUniform,
    retained_text::{
        PreparedRetainedGpuFrame, RetainedDrawError, RetainedRenderItem, RetainedTextGpuState,
    },
    GpuRenderer, ResolvedOrderedBatch, PATH_SAMPLE_COUNT,
};
use crate::{
    text::{atlas::GlyphAtlasPlane, GlyphQuadInstance, PreparedTextItem},
    OrderedRenderBatch, PackedStyle, PackedTransform, PreparedFrame, RenderPrimitive,
};
use noon_core::{RasterImageResourceHandle, Rect, Vec2};

mod filtered;

/// A versioned disposable renderer output, consumed without resampling it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FilteredCapture {
    pub object: noon_core::ObjectId,
    pub texture: wgpu::Texture,
    pub revision: u64,
    pub opacity: f32,
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub bounds: Rect,
}

pub(super) struct SourceReplacement {
    pub members: std::ops::Range<usize>,
    pub capture: FilteredCapture,
}

#[derive(Debug)]
pub(super) struct FilteredBinding {
    texture: wgpu::Texture,
    offset: u64,
    binding: wgpu::BindGroup,
}

#[derive(Clone, Debug, PartialEq)]
enum SourceDraw {
    Filtered {
        capture: FilteredCapture,
        bytes: std::ops::Range<u64>,
    },
    Geometry {
        batch: OrderedRenderBatch,
        bytes: std::ops::Range<u64>,
        mesh: Option<usize>,
        // The value participates in capture invalidation even when the packed
        // carrier row is unchanged. GPU bindings remain renderer-owned.
        paint: Option<(noon_core::ObjectId, crate::paint::PreparedPaint)>,
    },
    Glyph {
        item: PreparedTextItem,
        bytes: std::ops::Range<u64>,
    },
    Image {
        object_index: usize,
        resource: RasterImageResourceHandle,
        bytes: std::ops::Range<u64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreparedSource {
    draws: Vec<SourceDraw>,
    pub bytes: Vec<u8>,
    pub bounds: Option<Rect>,
    capture_origin: [i32; 2],
    capture_viewport: [u32; 2],
    has_filtered: bool,
}

impl PreparedSource {
    #[cfg(test)]
    pub(super) fn prepare(
        frame: &PreparedRetainedGpuFrame<'_>,
        objects: &[(noon_core::ObjectId, f32)],
        silhouette: bool,
        uniform_alignment: usize,
    ) -> Option<Self> {
        Self::prepare_with_replacements(frame, objects, silhouette, uniform_alignment, &[])
    }

    /// Members must be a unique contiguous painter interval. Each divisor removes
    /// only the opacity owned by the final capture placement; a composed scope
    /// passes one to retain each child's own opacity inside its source image.
    /// Replacement images must already represent the requested painted/original
    /// stream, including child scope opacity exactly once.
    pub(super) fn prepare_with_replacements(
        frame: &PreparedRetainedGpuFrame<'_>,
        objects: &[(noon_core::ObjectId, f32)],
        silhouette: bool,
        uniform_alignment: usize,
        replacements: &[SourceReplacement],
    ) -> Option<Self> {
        if objects.is_empty() || !uniform_alignment.is_power_of_two() {
            return None;
        }
        let mut source = Self {
            draws: Vec::new(),
            bytes: Vec::new(),
            bounds: None,
            capture_origin: [0; 2],
            capture_viewport: [0; 2],
            has_filtered: false,
        };
        let mut previous_rank = None;
        for &(object, opacity) in objects {
            if !opacity.is_finite() || opacity <= 0.0 {
                return None;
            }
            let rank = frame.capture_rank(object)?;
            // Capture exactly one contiguous, unique painter interval. Never
            // sort an aliased/interleaved request into a convenient group.
            if previous_rank.is_some_and(|previous| rank != previous + 1) {
                return None;
            }
            previous_rank = Some(rank);
        }
        let mut previous_end = 0;
        for replacement in replacements {
            if replacement.members.is_empty()
                || replacement.members.start < previous_end
                || replacement.members.end > objects.len()
            {
                return None;
            }
            previous_end = replacement.members.end;
        }
        let mut replacements = replacements.iter().peekable();
        let mut skip_until = 0;
        for (index, &(object, opacity)) in objects.iter().enumerate() {
            if index < skip_until {
                continue;
            }
            if replacements
                .peek()
                .is_some_and(|entry| entry.members.start == index)
            {
                let replacement = replacements.next()?;
                source.push_filtered(&replacement.capture, uniform_alignment)?;
                skip_until = replacement.members.end;
                continue;
            }
            for item in frame.capture_items(object)? {
                match item {
                    RetainedRenderItem::Geometry { batch, .. } => {
                        let at = batch.instance_range.start as usize;
                        let packed = match batch.primitive {
                            RenderPrimitive::Circle => frame.geometry.circle_ids.get(at),
                            RenderPrimitive::Rectangle => frame.geometry.rectangle_ids.get(at),
                            RenderPrimitive::Path { .. } => frame.geometry.path_ids.get(at),
                            _ => None,
                        };
                        // Retained mixed frames use private scratch identities. The
                        // canonical source draw range proves their packed mapping.
                        let paint = packed.filter(|_| !silhouette).and_then(|id| {
                            frame
                                .geometry
                                .fill_paints
                                .get(id)
                                .map(|value| (*id, value.clone()))
                        });
                        let start = source.bytes.len() as u64;
                        let mut local = batch.clone();
                        local.instance_range = 0..batch.instance_range.len() as u32;
                        let mesh = if let RenderPrimitive::Path { batch: at } = batch.primitive {
                            Some(frame.geometry.path_batch_cache_indices[at])
                        } else {
                            None
                        };
                        for at in batch.instance_range.clone() {
                            let at = at as usize;
                            macro_rules! pack {
                                ($value:expr, $bounds:expr) => {{
                                    let mut value = $value;
                                    source.include(transformed($bounds, value.transform)?);
                                    normalize(&mut value.style, opacity, silhouette);
                                    source.bytes.extend_from_slice(bytemuck::bytes_of(&value));
                                }};
                            }
                            match batch.primitive {
                                RenderPrimitive::Circle => {
                                    let v = frame.geometry.circles[at];
                                    let pad = stroke_pad(v.style, v.transform);
                                    let extent = v.radius + pad;
                                    pack!(
                                        v,
                                        Rect::new(
                                            Vec2::new(-extent, -extent),
                                            Vec2::new(extent, extent)
                                        )
                                    );
                                }
                                RenderPrimitive::Rectangle => {
                                    let v = frame.geometry.rectangles[at];
                                    let pad = stroke_pad(v.style, v.transform);
                                    let half =
                                        Vec2::new(v.size[0] * 0.5 + pad, v.size[1] * 0.5 + pad);
                                    pack!(v, Rect::new(half * -1.0, half));
                                }
                                RenderPrimitive::Line => {
                                    let v = frame.geometry.lines[at];
                                    let pad = stroke_pad(v.style, v.transform);
                                    let bounds = Rect::new(
                                        Vec2::new(
                                            v.start[0].min(v.end[0]) - pad,
                                            v.start[1].min(v.end[1]) - pad,
                                        ),
                                        Vec2::new(
                                            v.start[0].max(v.end[0]) + pad,
                                            v.start[1].max(v.end[1]) + pad,
                                        ),
                                    );
                                    pack!(v, bounds);
                                }
                                RenderPrimitive::Path { batch: path } => {
                                    let mesh = &frame.geometry.path_mesh_cache
                                        [frame.geometry.path_batch_cache_indices[path]];
                                    let Some([a, b]) = mesh.bounds else {
                                        // A zero-progress reveal can have an empty
                                        // tessellation while retaining its painter anchor.
                                        continue;
                                    };
                                    // Cached tessellation bounds already include stroke joins/caps.
                                    pack!(
                                        frame.geometry.paths[at],
                                        Rect::new(
                                            Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                                            Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y))
                                        )
                                    );
                                }
                                RenderPrimitive::MegaPath { .. } => return None,
                            }
                        }
                        if source.bytes.len() as u64 > start {
                            source.draws.push(SourceDraw::Geometry {
                                batch: local,
                                bytes: start..source.bytes.len() as u64,
                                mesh,
                                paint,
                            });
                        }
                    }
                    RetainedRenderItem::Glyph {
                        object_index,
                        run_index,
                        ..
                    } => {
                        for item in frame.text.glyph_batches(*object_index, *run_index) {
                            let PreparedTextItem::GlyphBatch {
                                plane,
                                instance_range,
                                ..
                            } = item
                            else {
                                return None;
                            };
                            let quads = match plane {
                                GlyphAtlasPlane::Mask => frame.text.mask_quads,
                                GlyphAtlasPlane::Color => frame.text.color_quads,
                            };
                            let start = source.bytes.len() as u64;
                            for quad in quads
                                .get(instance_range.start as usize..instance_range.end as usize)?
                            {
                                let mut quad = *quad;
                                source.include(quad_bounds(quad)?);
                                quad.color[3] /= opacity;
                                if silhouette {
                                    quad.color = [1.0; 4];
                                }
                                source.bytes.extend_from_slice(bytemuck::bytes_of(&quad));
                            }
                            let mut local = item.clone();
                            if let PreparedTextItem::GlyphBatch {
                                instance_range: range,
                                ..
                            } = &mut local
                            {
                                *range = 0..instance_range.len() as u32;
                            }
                            source.draws.push(SourceDraw::Glyph {
                                item: local,
                                bytes: start..source.bytes.len() as u64,
                            });
                        }
                    }
                    RetainedRenderItem::Image { object_index, .. } => {
                        // Each image keeps its immutable ordinary texture and its
                        // own aligned uniform range in this capture's one buffer.
                        let image = frame.capture_image(*object_index)?;
                        if image.object != object {
                            return None;
                        }
                        let half = Vec2::new(
                            image.content.width() as f32 * 0.5,
                            image.content.height() as f32 * 0.5,
                        );
                        source.include(transformed(
                            Rect::new(half * -1.0, half),
                            image.uniform.transform(),
                        )?);
                        // Texture alpha is intrinsic coverage, not paint modulation.
                        // Both modes preserve cutouts; silhouette ignores fill alpha.
                        let uniform = image.uniform.captured(
                            image.paint_opacity,
                            image.object_opacity / opacity,
                            silhouette,
                        );
                        let start = source
                            .bytes
                            .len()
                            .checked_next_multiple_of(uniform_alignment)?;
                        source.bytes.resize(start, 0);
                        source.bytes.extend_from_slice(bytemuck::bytes_of(&uniform));
                        source.draws.push(SourceDraw::Image {
                            object_index: *object_index,
                            resource: image.content.resource(),
                            bytes: start as u64..source.bytes.len() as u64,
                        });
                    }
                }
            }
        }
        Some(source)
    }

    fn include(&mut self, bounds: Rect) {
        self.bounds = Some(self.bounds.map_or(bounds, |old| {
            Rect::new(
                Vec2::new(old.min.x.min(bounds.min.x), old.min.y.min(bounds.min.y)),
                Vec2::new(old.max.x.max(bounds.max.x), old.max.y.max(bounds.max.y)),
            )
        }));
    }

    pub(super) fn needs_capture_region(&self, origin: [i32; 2], size: [u32; 2]) -> bool {
        self.capture_origin != origin || (self.has_filtered && self.capture_viewport != size)
    }

    pub(super) fn set_capture_region(&mut self, origin: [i32; 2], size: [u32; 2]) -> Option<()> {
        if !self.needs_capture_region(origin, size) {
            return Some(());
        }
        self.capture_origin = origin;
        if self.has_filtered {
            self.capture_viewport = size;
        }
        for draw in &self.draws {
            if let SourceDraw::Image { bytes, .. } = draw {
                let bytes = &mut self.bytes[bytes.start as usize..bytes.end as usize];
                let uniform =
                    bytemuck::pod_read_unaligned::<ImageUniform>(bytes).with_capture_origin(origin);
                bytes.copy_from_slice(bytemuck::bytes_of(&uniform));
            }
            if let SourceDraw::Filtered { capture, bytes } = draw {
                let placement = super::analytic_glow::Placement {
                    origin: [
                        capture.origin[0].checked_sub(origin[0])?,
                        capture.origin[1].checked_sub(origin[1])?,
                    ],
                    size: capture.size,
                    viewport: size,
                    opacity: capture.opacity,
                    padding: 0,
                };
                self.bytes[bytes.start as usize..bytes.end as usize]
                    .copy_from_slice(bytemuck::bytes_of(&placement));
            }
        }
        Some(())
    }

    /// Rebind only when an immutable image resource or the capture buffer changes.
    /// Uniform-only edits keep both the texture and this GPU binding resident.
    pub(super) fn prepare_image_bindings(
        &self,
        renderer: &GpuRenderer,
        device: &wgpu::Device,
        instances: &wgpu::Buffer,
        bindings: &mut Vec<CapturedImageBinding>,
    ) -> Result<(), RasterImageDrawError> {
        let mut count = 0;
        for draw in &self.draws {
            let SourceDraw::Image {
                object_index,
                resource,
                bytes,
            } = draw
            else {
                continue;
            };
            if bindings
                .get(count)
                .is_none_or(|old| old.resource != *resource || old.offset != bytes.start)
            {
                let binding = renderer
                    .images
                    .as_ref()
                    .ok_or(RasterImageDrawError::NotUploaded)?
                    .bind_captured(device, *object_index, *resource, instances, bytes.start)?;
                if count == bindings.len() {
                    bindings.push(binding);
                } else {
                    bindings[count] = binding;
                }
            }
            count += 1;
        }
        bindings.truncate(count);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode(
        &self,
        renderer: &GpuRenderer,
        text: &RetainedTextGpuState,
        prepared: &PreparedFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        multisampled: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        text_camera: &wgpu::BindGroup,
        instances: &wgpu::Buffer,
        images: &[CapturedImageBinding],
        filtered: Option<(&wgpu::RenderPipeline, &[FilteredBinding])>,
    ) -> Result<(), RetainedDrawError> {
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: multisampled,
            depth_slice: None,
            resolve_target: Some(target),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Discard,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon isolated 2D source"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut binding = None;
        let mut images = images.iter();
        let mut filtered_index = 0;
        for draw in &self.draws {
            match draw {
                SourceDraw::Filtered { .. } => {
                    let (pipeline, filtered) =
                        filtered.ok_or(RetainedDrawError::EffectNotPrepared)?;
                    let tile = filtered
                        .get(filtered_index)
                        .ok_or(RetainedDrawError::EffectNotPrepared)?;
                    filtered_index += 1;
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &tile.binding, &[]);
                    pass.draw(0..6, 0..1);
                    binding = None;
                }
                SourceDraw::Geometry {
                    batch,
                    bytes,
                    paint,
                    ..
                } => {
                    pass.set_bind_group(0, camera, &[]);
                    if let Some((object, _)) = paint {
                        renderer
                            .draw_paint_batch(
                                &mut pass,
                                prepared,
                                batch.primitive,
                                batch.instance_range.clone(),
                                false,
                                &mut binding,
                                *object,
                                Some(instances.slice(bytes.clone())),
                            )
                            .ok_or(RetainedDrawError::GradientNotUploaded)?;
                        continue;
                    }
                    renderer.draw_isolated_ordered_batch(
                        &mut pass,
                        prepared,
                        &ResolvedOrderedBatch {
                            batch: batch.clone(),
                            mega: None,
                        },
                        false,
                        &mut binding,
                        Some(instances.slice(bytes.clone())),
                    );
                }
                SourceDraw::Glyph { item, bytes } => {
                    text.glyphs.draw_captured_item(
                        &mut pass,
                        item,
                        PATH_SAMPLE_COUNT,
                        text_camera,
                        Some(instances.slice(bytes.clone())),
                    )?;
                    binding = None;
                }
                SourceDraw::Image { .. } => {
                    renderer
                        .images
                        .as_ref()
                        .ok_or(RasterImageDrawError::NotUploaded)?
                        .draw_captured(
                            &mut pass,
                            camera,
                            images.next().ok_or(RasterImageDrawError::NotUploaded)?,
                            PATH_SAMPLE_COUNT,
                        );
                    binding = None;
                }
            }
        }
        Ok(())
    }
}

fn normalize(style: &mut PackedStyle, opacity: f32, silhouette: bool) {
    style.opacity /= opacity;
    if silhouette {
        style.fill = [1.0; 4];
        style.stroke = [1.0; 4];
    }
}

fn stroke_pad(style: PackedStyle, transform: PackedTransform) -> f32 {
    if style.stroke_enabled & 1 == 0 {
        return 0.0;
    }
    let scale = if style.stroke_enabled & 2 != 0 {
        transform.scale[0].abs().min(transform.scale[1].abs())
    } else {
        1.0
    };
    style.stroke_width * 0.5 / scale
}

fn transformed(bounds: Rect, transform: PackedTransform) -> Option<Rect> {
    let (sin, cos) = transform.rotation.sin_cos();
    let mut result = Rect::new(
        Vec2::new(f32::INFINITY, f32::INFINITY),
        Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    );
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            let x = x * transform.scale[0];
            let y = y * transform.scale[1];
            let p = Vec2::new(
                cos * x - sin * y + transform.translation[0],
                sin * x + cos * y + transform.translation[1],
            );
            result.min.x = result.min.x.min(p.x);
            result.min.y = result.min.y.min(p.y);
            result.max.x = result.max.x.max(p.x);
            result.max.y = result.max.y.max(p.y);
        }
    }
    finite(result).then_some(result)
}

fn quad_bounds(quad: GlyphQuadInstance) -> Option<Rect> {
    let bounds = {
        let mut points = [Vec2::ZERO; 4];
        for (i, point) in points.iter_mut().enumerate() {
            *point = Vec2::new(
                quad.origin[0] + (i & 1) as f32 * quad.axis_x[0] + (i >> 1) as f32 * quad.axis_y[0],
                quad.origin[1] + (i & 1) as f32 * quad.axis_x[1] + (i >> 1) as f32 * quad.axis_y[1],
            );
        }
        Rect::new(
            Vec2::new(
                points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
                points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
            ),
            Vec2::new(
                points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
                points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
            ),
        )
    };
    finite(bounds).then_some(bounds)
}

fn finite(bounds: Rect) -> bool {
    [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .into_iter()
        .all(f32::is_finite)
}

#[cfg(test)]
mod tests;
