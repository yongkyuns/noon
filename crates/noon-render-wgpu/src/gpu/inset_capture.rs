//! Disposable camera captures in the existing renderer, sampled by the shared
//! image pipeline. Semantic scene/resource identity remains unchanged.
use super::retained_text::{
    PreparedRetainedGpuFrame, RetainedDrawError, RetainedDrawStats, RetainedTextGpuState,
};
use super::{
    raster_image_gpu::{ExternalImageBinding, RasterImageGpuRenderer},
    raster_image_prepare::ImageUniform,
    Camera2D, GpuRenderer, PATH_SAMPLE_COUNT,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InsetCaptureProjectionError {
    InvalidDisplay,
}

/// Resolve the full source raster size for one display that intersects the main pass.
///
/// The external image retains its authored world-space display rectangle, so the
/// normal main pass clips a partial display. Capture resolution is bounded by the
/// main surface just as a formerly fully-inside display was, and by the device's
/// texture limit.
pub(super) fn capture_raster_size(
    display_center: noon_core::Vec2,
    display_size: noon_core::Vec2,
    main_camera: Camera2D,
    surface: [u32; 2],
    max_texture_dimension_2d: u32,
) -> Result<Option<[u32; 2]>, InsetCaptureProjectionError> {
    if !display_center.x.is_finite()
        || !display_center.y.is_finite()
        || !display_size.x.is_finite()
        || !display_size.y.is_finite()
        || display_size.x <= 0.0
        || display_size.y <= 0.0
        || max_texture_dimension_2d == 0
    {
        return Err(InsetCaptureProjectionError::InvalidDisplay);
    }
    let limits = [
        surface[0].min(max_texture_dimension_2d),
        surface[1].min(max_texture_dimension_2d),
    ];
    if limits.contains(&0) {
        return Err(InsetCaptureProjectionError::InvalidDisplay);
    }

    let surface = noon_core::Vec2::new(surface[0] as f32, surface[1] as f32);
    let main_min = main_camera.center - main_camera.world_size * 0.5;
    let main_max = main_camera.center + main_camera.world_size * 0.5;
    let world_min = display_center - display_size * 0.5;
    let world_max = display_center + display_size * 0.5;
    let left = (world_min.x - main_min.x) / main_camera.world_size.x * surface.x;
    let right = (world_max.x - main_min.x) / main_camera.world_size.x * surface.x;
    let top = (main_max.y - world_max.y) / main_camera.world_size.y * surface.y;
    let bottom = (main_max.y - world_min.y) / main_camera.world_size.y * surface.y;
    if ![left, right, top, bottom].into_iter().all(f32::is_finite) || right <= left || bottom <= top
    {
        return Err(InsetCaptureProjectionError::InvalidDisplay);
    }
    if right <= 0.0 || bottom <= 0.0 || left >= surface.x || top >= surface.y {
        return Ok(None);
    }

    let extent = |value: f32, limit: u32| value.floor().max(1.0).min(limit as f32) as u32;
    Ok(Some([
        extent(right - left, limits[0]),
        extent(bottom - top, limits[1]),
    ]))
}

#[derive(Debug)]
pub(super) struct InsetCaptureTarget {
    pub size: [u32; 2],
    _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    _msaa: wgpu::Texture,
    pub msaa_view: wgpu::TextureView,
    pub image: ExternalImageBinding,
}

impl InsetCaptureTarget {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        images: &RasterImageGpuRenderer,
        uniform: ImageUniform,
    ) -> Self {
        let texture = |samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon retained inset capture"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let view = color.create_view(&Default::default());
        let msaa = texture(PATH_SAMPLE_COUNT, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let msaa_view = msaa.create_view(&Default::default());
        let image = images.bind_external(device, &view, uniform);
        Self {
            size,
            _texture: color,
            view,
            _msaa: msaa,
            msaa_view,
            image,
        }
    }
}

impl GpuRenderer {
    pub(super) fn encode_inset_captures(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text: &RetainedTextGpuState,
        samples: u32,
    ) -> Result<RetainedDrawStats, RetainedDrawError> {
        let mut stats = RetainedDrawStats::default();
        for (index, (inset, target)) in self.inset_views.iter().zip(&self.inset_targets).enumerate()
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: if samples == 1 {
                    &target.view
                } else {
                    &target.msaa_view
                },
                depth_slice: None,
                resolve_target: (samples != 1).then_some(&target.view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: if samples == 1 {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon inset scene capture"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut excluded = HashSet::new();
            if !inset.state.capture_own_display {
                excluded.insert(inset.state.display);
            }
            stats += self.draw_retained_items(
                &mut pass,
                prepared,
                text,
                samples,
                &self.inset_camera_bind_groups[index],
                Some(index),
                &excluded,
                false,
                &mut HashSet::new(),
            )?;
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::{capture_raster_size, InsetCaptureProjectionError};
    use crate::Camera2D;
    use noon_core::Vec2;

    fn camera() -> Camera2D {
        Camera2D::new(Vec2::ZERO, Vec2::new(10.0, 10.0)).unwrap()
    }

    #[test]
    fn capture_raster_keeps_the_full_partial_display_and_bounds_its_allocation() {
        assert_eq!(
            capture_raster_size(
                Vec2::new(3.0, 0.0),
                Vec2::new(2.0, 2.0),
                camera(),
                [100, 60],
                64,
            ),
            Ok(Some([20, 12])),
            "fully visible displays retain their exact projected raster size",
        );
        assert_eq!(
            capture_raster_size(
                Vec2::new(5.0, 0.0),
                Vec2::new(4.0, 2.0),
                camera(),
                [100, 60],
                64,
            ),
            Ok(Some([40, 12])),
            "the display projects from x=80 through x=120; its source must not crop to 20 pixels",
        );
        assert_eq!(
            capture_raster_size(
                Vec2::ZERO,
                Vec2::new(1000.0, 1000.0),
                camera(),
                [100, 60],
                64,
            ),
            Ok(Some([64, 60])),
            "capture allocation remains bounded by the surface and device limit",
        );
    }

    #[test]
    fn capture_raster_skips_fully_offscreen_displays_but_rejects_invalid_geometry() {
        assert_eq!(
            capture_raster_size(
                Vec2::new(8.0, 0.0),
                Vec2::new(2.0, 2.0),
                camera(),
                [100, 60],
                64,
            ),
            Ok(None),
        );
        assert_eq!(
            capture_raster_size(
                Vec2::new(f32::NAN, 0.0),
                Vec2::new(2.0, 2.0),
                camera(),
                [100, 60],
                64,
            ),
            Err(InsetCaptureProjectionError::InvalidDisplay),
        );
    }
}
