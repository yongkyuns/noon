//! Batch independent equal-size masks without sharing their images or kernels.
use super::*;

pub(in crate::gpu) struct BlurJob {
    size: [u32; 2],
    views: [wgpu::TextureView; 2],
    bindings: [wgpu::BindGroup; 2],
}

#[derive(Debug)]
pub(super) struct GlowBlurBatch {
    pipelines: [wgpu::RenderPipeline; 2],
}

impl GlowFilter {
    pub(in crate::gpu) fn take_blur_job(scope: &mut GlowScope) -> Option<BlurJob> {
        if !scope.active || !scope.blur_dirty {
            return None;
        }
        let r = scope.resources.as_ref()?;
        let job = BlurJob {
            size: r.size,
            views: [r.horizontal.view.clone(), r.vertical.view.clone()],
            bindings: [r.horizontal_binding.clone(), r.vertical_binding.clone()],
        };
        scope.blur_dirty = false;
        Some(job)
    }

    pub(in crate::gpu) fn encode_blur_jobs(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        mut jobs: Vec<BlurJob>,
    ) -> EffectRasterStats {
        let mut stats = EffectRasterStats::default();
        if jobs.is_empty() {
            return stats;
        }
        let limits = device.limits();
        let supported = limits.max_bind_groups >= 4
            && limits.max_uniform_buffers_per_shader_stage >= 4
            && limits.max_sampled_textures_per_shader_stage >= 4
            && limits.max_color_attachments >= 4
            && limits.max_color_attachment_bytes_per_sample >= 16;
        jobs.sort_unstable_by_key(|job| job.size);
        let programs = self.programs.as_mut().expect("prepared Glow programs");
        let mut at = 0;
        while at < jobs.len() {
            if supported && jobs.len() - at >= 4 && jobs[at].size == jobs[at + 3].size {
                let batch = programs.batch.get_or_insert_with(|| {
                    stats.pipeline_compiles += 2;
                    GlowBlurBatch::new(device, &programs.blur_layout)
                });
                batch.encode(encoder, &jobs[at..at + 4]);
                at += 4;
            } else {
                let job = &jobs[at];
                for (axis, pipeline) in [&programs.horizontal, &programs.vertical]
                    .into_iter()
                    .enumerate()
                {
                    encode_pass(
                        encoder,
                        &job.views[axis],
                        pipeline,
                        &job.bindings[axis],
                        "Noon independent Glow mask",
                    );
                }
                at += 1;
            }
            stats.blur_passes += 2;
        }
        stats
    }
}

impl GlowBlurBatch {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon four independent Glow masks"),
            source: wgpu::ShaderSource::Wgsl(include_str!("batch.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon four independent Glow masks"),
            bind_group_layouts: &[Some(layout); 4],
            immediate_size: 0,
        });
        let targets = std::array::from_fn::<_, 4, _>(|_| {
            Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        });
        Self {
            pipelines: ["fs_horizontal", "fs_vertical"].map(|entry| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Noon four independent Glow masks"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_fullscreen"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &targets,
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            }),
        }
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder, jobs: &[BlurJob]) {
        for (axis, pipeline) in self.pipelines.iter().enumerate() {
            let attachments = std::array::from_fn::<_, 4, _>(|index| {
                let job = &jobs[index];
                Some(wgpu::RenderPassColorAttachment {
                    view: &job.views[axis],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon four independent Glow masks"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            for (index, job) in jobs.iter().enumerate() {
                pass.set_bind_group(index as u32, &job.bindings[axis], &[]);
            }
            pass.draw(0..3, 0..1);
        }
    }
}
