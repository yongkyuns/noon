//! Coordinator for selected-prefix input, target paint/coverage, and placement.
//! All state here is disposable GPU derivation of an admitted publication.
use super::*;
use crate::gpu::retained_text::{PreparedRetainedGpuFrame, RetainedTextGpuState};
use crate::gpu::{
    BackdropCompositeFilter, BackdropCompositeInputs, BackdropCompositeParameters,
    BackdropCompositeScope, GaussianBlurCapture, GaussianBlurFilter, GaussianBlurParameters,
    GaussianBlurScope, PainterPrefixCaptureFilter, PainterPrefixCaptureRequest,
    PainterPrefixCaptureScope,
};

#[derive(Debug)]
struct ReplacementBinding {
    contribution: wgpu::Texture,
    erase: wgpu::Texture,
    buffer: wgpu::Buffer,
    bindings: [wgpu::BindGroup; 2],
}
#[derive(Debug, Default)]
struct Scope {
    prefix: PainterPrefixCaptureScope,
    blur: GaussianBlurScope,
    composite: BackdropCompositeScope,
    blur_revision: u64,
    placement: Option<ReplacementBinding>,
}
impl Scope {
    fn bytes(&self) -> u64 {
        self.prefix.scratch_bytes() + self.blur.scratch_bytes() + self.composite.scratch_bytes()
    }
    fn invalidate(&mut self) {
        self.prefix.invalidate();
        self.blur.invalidate();
        self.composite.invalidate();
    }
}
#[derive(Debug, Default)]
pub(super) struct BackdropGpu {
    scopes: BTreeMap<ObjectId, Scope>,
    bytes: u64,
    prefix: PainterPrefixCaptureFilter,
    blur: GaussianBlurFilter,
    composite: Option<BackdropCompositeFilter>,
}
impl BackdropGpu {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn remove(&mut self, object: ObjectId) {
        if let Some(scope) = self.scopes.remove(&object) {
            self.bytes -= scope.bytes();
        }
    }
    pub fn invalidate(&mut self) {
        for scope in self.scopes.values_mut() {
            scope.invalidate();
        }
    }
}
impl GpuRenderer {
    pub(in crate::gpu) fn backdrop_texture_bytes(&self) -> u64 {
        self.effect_captures
            .as_deref()
            .map_or(0, |cache| cache.backdrops.bytes())
    }
    pub(in crate::gpu) fn retire_backdrop_output(&mut self, object: ObjectId) {
        if let Some(cache) = self.effect_captures.as_deref_mut() {
            cache.backdrops.remove(object);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::gpu) fn prepare_backdrop_output(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text: &RetainedTextGpuState,
        index: usize,
        object: ObjectId,
        definition: EffectDefinition,
        mut request: PainterPrefixCaptureRequest,
        budget: u64,
    ) -> Result<EffectCaptureStats, EffectPrepareError> {
        let sigma = crate::EffectCaptureParameters {
            definition,
            output_height: self.viewport_size[1],
            world_view_height: f64::from(self.camera.world_size.y),
            scope_opacity: 1.0,
        }
        .sigma_pixels()?;
        // An offscreen target can be absent unless a later prefix needs it.
        // The common capture planner has already made that clipping decision.
        if !self
            .effect_captures
            .as_deref()
            .and_then(|cache| cache.scopes.get(&(2, index)))
            .is_some_and(|scope| scope.object == object && scope.enabled)
        {
            self.retire_backdrop_output(object);
            return Ok(EffectCaptureStats {
                retained_texture_bytes: self.effect_capture_texture_bytes(),
                ..Default::default()
            });
        }
        let cache = self
            .effect_captures
            .as_deref_mut()
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        let resident = cache
            .scopes
            .get(&(2, index))
            .filter(|s| s.object == object && s.enabled)
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        request.origin = resident.tile.origin;
        request.size = resident.tile.size;
        let paint = resident
            .placement_output
            .as_ref()
            .ok_or(EffectPrepareError::PublicationMismatch)?
            .clone();
        let mask = if resident.original_masked {
            resident.original_mask.output()
        } else {
            resident.silhouette.as_ref().map(|s| &s.texture)
        }
        .ok_or(EffectPrepareError::MissingSilhouette)?
        .clone();
        let paint_revision = resident.output_revision;
        let opacity = resident.original_opacity;
        let common_bytes = cache.bytes;
        let mut gpu = std::mem::take(&mut cache.backdrops);
        let previous = gpu.scopes.get(&object).map_or(0, Scope::bytes);
        let mut result = (|| {
            let others = common_bytes
                .checked_add(gpu.bytes() - previous)
                .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
            let pixels = u64::from(request.size[0]) * u64::from(request.size[1]);
            let total = others
                .checked_add(
                    pixels
                        .checked_mul(56)
                        .ok_or(EffectPrepareError::ScratchBudgetExceeded)?,
                )
                .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
            if total > budget {
                return Err(EffectPrepareError::ScratchBudgetExceeded);
            }
            request.scratch_budget_bytes = pixels * 20;
            let scope = gpu.scopes.entry(object).or_default();
            let prefix = gpu.prefix.prepare_encode(
                device,
                queue,
                encoder,
                self,
                text,
                prepared,
                &mut scope.prefix,
                &request,
            )?;
            let blur = gpu
                .blur
                .prepare(
                    device,
                    queue,
                    &mut scope.blur,
                    GaussianBlurCapture {
                        source: scope
                            .prefix
                            .output()
                            .ok_or(EffectPrepareError::PublicationMismatch)?,
                        revision: scope
                            .prefix
                            .revision()
                            .ok_or(EffectPrepareError::PublicationMismatch)?,
                    },
                    GaussianBlurParameters {
                        sigma_pixels: sigma,
                        scope_opacity: 1.0,
                        scratch_budget_bytes: pixels * 28,
                    },
                )
                .map_err(EffectPrepareError::Blur)?;
            let blur_work = gpu.blur.encode(encoder, &mut scope.blur);
            if blur_work.output_passes > 0 {
                scope.blur_revision = scope
                    .blur_revision
                    .checked_add(1)
                    .expect("backdrop blur revision exhausted");
            }
            if gpu.composite.is_none() {
                gpu.composite = Some(BackdropCompositeFilter::new(self.target_format)?);
            }
            let composite = gpu.composite.as_mut().unwrap().prepare_encode(
                device,
                queue,
                encoder,
                &mut scope.composite,
                BackdropCompositeInputs {
                    processed_prefix: GaussianBlurCapture {
                        source: scope
                            .blur
                            .output()
                            .ok_or(EffectPrepareError::PublicationMismatch)?,
                        revision: scope.blur_revision,
                    },
                    target_paint: GaussianBlurCapture {
                        source: &paint,
                        revision: paint_revision,
                    },
                    // Private Visibility can change without repainting original geometry.
                    target_coverage: GaussianBlurCapture {
                        source: &mask,
                        revision: paint_revision,
                    },
                },
                BackdropCompositeParameters {
                    origin: request.origin,
                    viewport: self.viewport_size,
                    scope_opacity: opacity,
                    scratch_budget_bytes: pixels * 8,
                },
            )?;
            let contribution = scope
                .composite
                .contribution()
                .ok_or(EffectPrepareError::PublicationMismatch)?
                .clone();
            let erase = scope
                .composite
                .erase()
                .ok_or(EffectPrepareError::PublicationMismatch)?
                .clone();
            let cache = self.effect_captures.as_deref_mut().unwrap();
            let programs = cache
                .placement
                .as_mut()
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            let compiles = programs.ensure_replacement(device, self.target_format);
            let resident = cache.scopes.get_mut(&(2, index)).unwrap();
            // C and erasure already include final opacity; original coverage is
            // still borrowed with its original opacity by any coverage consumer.
            let placement = Placement {
                opacity: 1.0,
                ..resident.placement
            };
            queue.write_buffer(
                &resident.placement_buffer,
                0,
                bytemuck::bytes_of(&placement),
            );
            let bind = |texture: &wgpu::Texture| {
                let view = texture.create_view(&Default::default());
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Noon backdrop replacement placement"),
                    layout: &programs.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: resident.placement_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                    ],
                })
            };
            let rebind = scope.placement.as_ref().is_none_or(|old| {
                old.contribution != contribution
                    || old.erase != erase
                    || old.buffer != resident.placement_buffer
            });
            if rebind {
                scope.placement = Some(ReplacementBinding {
                    contribution: contribution.clone(),
                    erase: erase.clone(),
                    buffer: resident.placement_buffer.clone(),
                    bindings: [bind(&contribution), bind(&erase)],
                });
            }
            let bindings = &scope.placement.as_ref().unwrap().bindings;
            resident.placement_binding = Some(bindings[0].clone());
            resident.placement_erase_binding = Some(bindings[1].clone());
            resident.placement_output = Some(contribution);
            resident.placement_erase = Some(erase);
            resident.placement = placement;
            resident.contribution_bounds = EffectPixelBounds {
                min: resident.original_bounds.min.map(|v| v - 1.0),
                max: resident.original_bounds.max.map(|v| v + 1.0),
            };
            if composite.composite_passes > 0 {
                resident.output_revision = resident
                    .output_revision
                    .checked_add(1)
                    .expect("backdrop output revision exhausted");
            }
            Ok(EffectCaptureStats {
                source_passes: prefix.composite_passes,
                placement_bytes_uploaded: size_of::<Placement>(),
                filter: EffectRasterStats {
                    pipeline_compiles: prefix.pipeline_compiles
                        + blur.pipeline_compiles
                        + composite.pipeline_compiles
                        + compiles,
                    texture_allocations: prefix.texture_allocations
                        + blur.texture_allocations
                        + composite.texture_allocations,
                    buffer_allocations: prefix.buffer_allocations
                        + blur.buffer_allocations
                        + composite.buffer_allocations,
                    bind_group_creations: prefix.bind_group_creations
                        + blur.bind_group_creations
                        + composite.bind_group_creations
                        + if rebind { 2 } else { 0 },
                    bytes_uploaded: prefix.bytes_uploaded
                        + blur.bytes_uploaded
                        + composite.bytes_uploaded,
                    blur_passes: blur_work.blur_passes,
                    composite_passes: blur_work.output_passes + composite.composite_passes,
                    scratch_bytes: 0,
                    ..Default::default()
                },
                retained_texture_bytes: 0,
                ..Default::default()
            })
        })();
        gpu.bytes = gpu.bytes - previous + gpu.scopes.get(&object).map_or(0, Scope::bytes);
        if let Ok(stats) = &mut result {
            stats.filter.scratch_bytes = gpu.bytes;
            stats.retained_texture_bytes = common_bytes + gpu.bytes;
        }
        self.effect_captures.as_deref_mut().unwrap().backdrops = gpu;
        result
    }
}

#[cfg(test)]
mod tests;
