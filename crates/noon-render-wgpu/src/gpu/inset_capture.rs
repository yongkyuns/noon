//! Disposable camera captures in the existing renderer, sampled by the shared
//! image pipeline. Semantic scene/resource identity remains unchanged.
use super::retained_text::{
    PreparedRetainedGpuFrame, RetainedDrawError, RetainedDrawStats, RetainedTextGpuState,
};
use super::{
    raster_image_gpu::{ExternalImageBinding, RasterImageGpuRenderer},
    raster_image_prepare::ImageUniform,
    GpuRenderer, PATH_SAMPLE_COUNT,
};
use std::collections::HashSet;

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
