//! Explicit editor/session overlay rendering, outside the scene painter stream.
//!
//! This is a disposable one-instance GPU cache, not a selected-target authority.
//! Ordinary encode APIs never consume it, including when the same renderer is reused.

use super::{CircleInstance, DrawStats, GpuRenderer, RectangleInstance, UploadStats};
use noon_core::{Color, GeometryRef, Style, Transform2D};

/// One prepared analytic fill overlay. No semantic ID, scene row, resource index,
/// painter anchor or serialized scene participates in this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalyticOverlay {
    instance: OverlayInstance,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OverlayInstance {
    Circle(CircleInstance),
    Rectangle(RectangleInstance),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPrepareError {
    UnsupportedGeometry,
    InvalidGeometry,
    InvalidTransform,
    InvalidColor,
}
impl std::fmt::Display for OverlayPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid analytic overlay: {self:?}")
    }
}
impl std::error::Error for OverlayPrepareError {}

impl AnalyticOverlay {
    /// Prepare a tint of the exact effective fill, not its axis-aligned bounds.
    /// Reflections and nonuniform scales keep the normal analytic renderer path.
    pub fn new(
        geometry: &GeometryRef,
        transform: Transform2D,
        color: Color,
    ) -> Result<Self, OverlayPrepareError> {
        if ![
            transform.translation.x,
            transform.translation.y,
            transform.scale.x,
            transform.scale.y,
            transform.rotation,
        ]
        .into_iter()
        .all(f32::is_finite)
            || transform.scale.x == 0.0
            || transform.scale.y == 0.0
        {
            return Err(OverlayPrepareError::InvalidTransform);
        }
        if ![color.red, color.green, color.blue, color.alpha]
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        {
            return Err(OverlayPrepareError::InvalidColor);
        }
        let style = Style {
            fill: Some(color),
            stroke: None,
            ..Style::default()
        }
        .into();
        match geometry {
            GeometryRef::Circle { radius } if radius.is_finite() && *radius > 0.0 => Ok(Self {
                instance: OverlayInstance::Circle(CircleInstance {
                    transform: transform.into(),
                    style,
                    radius: *radius,
                    padding: [1.0, 0.0, 0.0],
                }),
            }),
            GeometryRef::Rectangle { size }
                if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 =>
            {
                Ok(Self {
                    instance: OverlayInstance::Rectangle(RectangleInstance {
                        transform: transform.into(),
                        style,
                        size: [size.x, size.y],
                        padding: [0.0; 2],
                    }),
                })
            }
            GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. } => {
                Err(OverlayPrepareError::InvalidGeometry)
            }
            _ => Err(OverlayPrepareError::UnsupportedGeometry),
        }
    }

    fn bytes(&self) -> &[u8] {
        match &self.instance {
            OverlayInstance::Circle(instance) => bytemuck::bytes_of(instance),
            OverlayInstance::Rectangle(instance) => bytemuck::bytes_of(instance),
        }
    }
}

/// One lazily allocated, fixed-capacity instance buffer. Updating or hiding an
/// overlay never touches stable scene buffers or renderer preparation caches.
#[derive(Debug, Default)]
pub struct OverlayGpuState {
    current: Option<AnalyticOverlay>,
    buffer: Option<wgpu::Buffer>,
}
impl OverlayGpuState {
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        overlay: Option<AnalyticOverlay>,
    ) -> UploadStats {
        if self.current == overlay {
            return UploadStats::default();
        }
        let mut stats = UploadStats::default();
        if let Some(value) = overlay.as_ref() {
            let buffer = self.buffer.get_or_insert_with(|| {
                stats.buffer_reallocations = 1;
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon session overlay instance"),
                    size: std::mem::size_of::<CircleInstance>() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            queue.write_buffer(buffer, 0, value.bytes());
            stats.bytes_uploaded = value.bytes().len();
        }
        self.current = overlay;
        stats
    }

    pub fn capacity_bytes(&self) -> usize {
        self.buffer
            .as_ref()
            .map_or(0, |_| std::mem::size_of::<CircleInstance>())
    }
}

impl GpuRenderer {
    /// Runs after all ordinary scene/transient draws and their MSAA resolve,
    /// before the single output-transfer pass. No painter order is rewritten.
    pub(super) fn encode_overlay(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        overlay: Option<&OverlayGpuState>,
    ) -> DrawStats {
        let Some(OverlayGpuState {
            current: Some(value),
            buffer: Some(buffer),
        }) = overlay
        else {
            return DrawStats::default();
        };
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: self.presentation.scene_view(view),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon session overlay pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(match value.instance {
            OverlayInstance::Circle(_) => &self.circle_pipeline_single_sample,
            OverlayInstance::Rectangle(_) => &self.rectangle_pipeline_single_sample,
        });
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        pass.draw(0..6, 0..1);
        DrawStats {
            draw_calls: 1,
            instances_drawn: 1,
        }
    }
}

#[cfg(test)]
mod tests;
