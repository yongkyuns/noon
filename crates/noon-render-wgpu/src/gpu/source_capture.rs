//! Isolated 2D source draws for offscreen consumers.
//!
//! Captures borrow canonical per-object draw ranges and existing meshes/atlases.
//! Only instance data is copied, to remove the final owning-scope opacity. This
//! module has no effect definitions, scheduling, semantic identity or filter graph.

use super::{
    retained_text::{PreparedRetainedGpuFrame, RetainedRenderItem, RetainedTextGpuState},
    GpuRenderer, ResolvedOrderedBatch, PATH_SAMPLE_COUNT,
};
use crate::{
    text::{atlas::GlyphAtlasPlane, GlyphQuadInstance, PreparedTextItem},
    OrderedRenderBatch, PackedStyle, PackedTransform, PreparedFrame, RenderPrimitive,
};
use noon_core::{Rect, Vec2};

#[derive(Clone, Debug, PartialEq)]
enum SourceDraw {
    Geometry {
        batch: OrderedRenderBatch,
        bytes: std::ops::Range<u64>,
        mesh: Option<usize>,
    },
    Glyph {
        item: PreparedTextItem,
        bytes: std::ops::Range<u64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreparedSource {
    draws: Vec<SourceDraw>,
    pub bytes: Vec<u8>,
    pub bounds: Option<Rect>,
}

impl PreparedSource {
    pub(super) fn prepare(
        frame: &PreparedRetainedGpuFrame<'_>,
        object: noon_core::ObjectId,
        opacity: f32,
        silhouette: bool,
    ) -> Option<Self> {
        if !opacity.is_finite() || opacity <= 0.0 {
            return None;
        }
        let mut source = Self {
            draws: Vec::new(),
            bytes: Vec::new(),
            bounds: None,
        };
        for item in frame.capture_items(object)? {
            match item {
                RetainedRenderItem::Geometry { batch, .. } => {
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
                                let half = Vec2::new(v.size[0] * 0.5 + pad, v.size[1] * 0.5 + pad);
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
                        for quad in
                            quads.get(instance_range.start as usize..instance_range.end as usize)?
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
                RetainedRenderItem::Image { .. } => return None,
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
    ) -> Result<(), crate::text::TextGpuDrawError> {
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
        for draw in &self.draws {
            match draw {
                SourceDraw::Geometry { batch, bytes, .. } => {
                    pass.set_bind_group(0, camera, &[]);
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
            }
        }
        Ok(())
    }
}

fn normalize(style: &mut PackedStyle, opacity: f32, silhouette: bool) {
    style.opacity /= opacity;
    if silhouette {
        // A patterned Line uses disabled fill lanes as geometric parameters.
        // Silhouette capture may recolor ink, never rewrite its spacing.
        if style.fill_enabled & 4 == 0 {
            style.fill = [1.0; 4];
        }
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
