//! Retained ordered composition of the qualified finite LDR operators.
//! This is derived GPU work over a caller-owned padded capture, not a scene,
//! attachment registry, scheduler or clock. Authored scope admission and capture
//! bounds remain with their existing compiler/renderer owners.
use noon_core::{EffectDefinition, GlowSource};

use super::{
    gaussian_blur::{
        GaussianBlurCapture, GaussianBlurFilter, GaussianBlurParameters, GaussianBlurScope,
    },
    shadow_filter::{ShadowFilter, ShadowScope},
    DissolveFilter, DissolveParameters, DissolveScope, EffectCaptureParameters, EffectPixelBounds,
    EffectPrepareError, EffectRasterStats, GlowCapture, GlowFilter, GlowParameters, GlowScope,
};

/// The fixed-quality finite operator profile; no silent truncation of a stack.
pub use noon_core::MAX_EFFECT_STACK_LEN;

#[derive(Clone, Copy, Debug)]
pub struct EffectStackParameters {
    pub output_height: u32,
    pub world_view_height: f64,
    /// Applied once, by the final non-neutral pass.
    pub scope_opacity: f32,
    /// All currently retained filter images, including warm neutral slots.
    /// Source/mask images and in-flight host resources are accounted separately.
    pub scratch_budget_bytes: u64,
    /// Original source extent in capture pixels, excluding padding and rounded
    /// capacity. None describes a raw capture whose entire image is the source.
    pub source_bounds: Option<EffectPixelBounds>,
}

impl EffectStackParameters {
    fn dissolve(
        self,
        definition: noon_core::NoiseDissolve,
        source: &wgpu::Texture,
        opacity: f32,
        budget: u64,
    ) -> DissolveParameters {
        DissolveParameters {
            definition,
            source_bounds: self.source_bounds.unwrap_or(EffectPixelBounds {
                min: [0.0; 2],
                max: [f64::from(source.width()), f64::from(source.height())],
            }),
            scope_opacity: opacity,
            scratch_budget_bytes: budget,
        }
    }
}

#[derive(Debug)]
enum PassFilter {
    Glow(Box<GlowScope>),
    Blur(Box<GaussianBlurScope>),
    Shadow(Box<ShadowScope>),
    Dissolve(Box<DissolveScope>),
}
impl PassFilter {
    fn new(definition: EffectDefinition) -> Self {
        match definition {
            EffectDefinition::Glow(_) => Self::Glow(Box::default()),
            EffectDefinition::Blur(_) => Self::Blur(Box::default()),
            EffectDefinition::Shadow(_) => Self::Shadow(Box::default()),
            EffectDefinition::NoiseDissolve(_) => Self::Dissolve(Box::default()),
        }
    }
    fn matches(&self, definition: EffectDefinition) -> bool {
        matches!(
            (self, definition),
            (Self::Glow(_), EffectDefinition::Glow(_))
                | (Self::Blur(_), EffectDefinition::Blur(_))
                | (Self::Shadow(_), EffectDefinition::Shadow(_))
                | (Self::Dissolve(_), EffectDefinition::NoiseDissolve(_))
        )
    }
    fn output(&self) -> Option<&wgpu::Texture> {
        match self {
            Self::Glow(v) => v.output(),
            Self::Blur(v) => v.output(),
            Self::Shadow(v) => v.output(),
            Self::Dissolve(v) => v.output(),
        }
    }
    fn output_view(&self) -> Option<&wgpu::TextureView> {
        match self {
            Self::Glow(v) => v.output_view(),
            Self::Blur(v) => v.output_view(),
            Self::Shadow(v) => v.output_view(),
            Self::Dissolve(v) => v.output_view(),
        }
    }
    fn scratch_bytes(&self) -> u64 {
        match self {
            Self::Glow(v) => v.scratch_bytes(),
            Self::Blur(v) => v.scratch_bytes(),
            Self::Shadow(v) => v.scratch_bytes(),
            Self::Dissolve(v) => v.scratch_bytes(),
        }
    }
    fn invalidate(&mut self) {
        match self {
            Self::Glow(v) => v.invalidate(),
            Self::Blur(v) => v.invalidate(),
            Self::Shadow(v) => v.invalidate(),
            Self::Dissolve(v) => v.invalidate(),
        }
    }
}

#[derive(Debug)]
struct RetainedPass {
    filter: PassFilter,
    // Content revision of this disposable intermediate, never authored time.
    revision: u64,
}

/// Disposable resources for one ordered capture. Each slot has independent
/// uniforms and intermediates; shared programs never overwrite another pass.
#[derive(Debug, Default)]
pub struct EffectStackScope {
    passes: Vec<RetainedPass>,
    output: Option<usize>,
}
impl EffectStackScope {
    /// None means ordinary source submission, including an entirely neutral stack.
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.output
            .and_then(|index| self.passes[index].filter.output())
    }
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.output
            .and_then(|index| self.passes[index].filter.output_view())
    }
    pub fn output_revision(&self) -> Option<u64> {
        self.output.map(|index| self.passes[index].revision)
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.passes
            .iter()
            .map(|pass| pass.filter.scratch_bytes())
            .sum()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn invalidate(&mut self) {
        for pass in &mut self.passes {
            pass.filter.invalidate();
        }
    }
    pub(super) fn retained_profile_bytes(&self, effects: &[EffectDefinition]) -> u64 {
        self.passes
            .iter()
            .zip(effects)
            .filter(|(pass, effect)| pass.filter.matches(**effect))
            .map(|(pass, _)| pass.filter.scratch_bytes())
            .sum()
    }
    /// An invisible/neutral capture still retires removed or replaced slots.
    /// Surviving warm resources remain charged; no GPU work is necessary.
    pub(super) fn deactivate_profile(&mut self, effects: &[EffectDefinition]) {
        self.passes.truncate(effects.len());
        for (pass, &effect) in self.passes.iter_mut().zip(effects) {
            if !pass.filter.matches(effect) {
                pass.filter = PassFilter::new(effect);
            }
        }
        self.output = None;
    }
}

/// Shared lazy programs for the same Glow, RGBA Gaussian and Shadow operators
/// used by ordinary source capture. Caller owns device, encoding and submission.
#[derive(Debug, Default)]
pub struct EffectStackFilter {
    glow: GlowFilter,
    blur: GaussianBlurFilter,
    shadow: ShadowFilter,
    dissolve: DissolveFilter,
}

impl EffectStackFilter {
    /// Every active pass consumes the preceding encoded-premultiplied RGBA8
    /// output. Painted glow uses that input alpha; silhouette glow uses the
    /// original declared geometry mask. Transparent border and sample grid are
    /// unchanged. Caller must provide the full cumulative padded source extent.
    ///
    /// All profile/capability/budget checks precede resource changes or queue
    /// writes, including checks for later passes. Clean preparation allocates,
    /// uploads and encodes nothing. Abandoned command encoders require invalidate.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut EffectStackScope,
        capture: GlowCapture<'_>,
        effects: &[EffectDefinition],
        parameters: EffectStackParameters,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let PreparedStack {
            budgets,
            active,
            last,
        } = preflight(device, scope, capture, effects, parameters)?;
        scope.passes.truncate(effects.len());
        if last.is_none() && scope.passes.is_empty() {
            scope.output = None;
            return Ok(EffectRasterStats::default());
        }
        for (index, &effect) in effects.iter().enumerate() {
            if index == scope.passes.len() {
                scope.passes.push(RetainedPass {
                    filter: PassFilter::new(effect),
                    revision: 0,
                });
            } else if !scope.passes[index].filter.matches(effect) {
                scope.passes[index].filter = PassFilter::new(effect);
            }
        }
        let mut stats = EffectRasterStats::default();
        let mut input = capture.source.clone();
        let mut revision = capture.revision;
        if last.is_some() {
            for (index, (&effect, &budget)) in effects.iter().zip(&budgets).enumerate() {
                if !active[index] {
                    continue;
                }
                let pass = &mut scope.passes[index];
                let opacity = if Some(index) == last {
                    parameters.scope_opacity
                } else {
                    1.0
                };
                let capture = GlowCapture {
                    source: &input,
                    silhouette: capture.silhouette,
                    revision,
                };
                let work = match (&mut pass.filter, effect) {
                    (PassFilter::Dissolve(filter), EffectDefinition::NoiseDissolve(definition)) => {
                        self.dissolve.prepare_encode(
                            device,
                            queue,
                            encoder,
                            filter,
                            GaussianBlurCapture {
                                source: capture.source,
                                revision,
                            },
                            parameters.dissolve(definition, capture.source, opacity, budget),
                        )?
                    }
                    (PassFilter::Glow(filter), EffectDefinition::Glow(definition)) => {
                        let upload = self.glow.prepare(
                            device,
                            queue,
                            filter,
                            capture,
                            GlowParameters {
                                definition,
                                output_height: parameters.output_height,
                                world_view_height: parameters.world_view_height,
                                scope_opacity: opacity,
                                scratch_budget_bytes: budget,
                            },
                        )?;
                        let encoded = self.glow.encode(encoder, filter);
                        EffectRasterStats {
                            blur_passes: encoded.blur_passes,
                            composite_passes: encoded.composite_passes,
                            ..upload
                        }
                    }
                    (PassFilter::Blur(filter), EffectDefinition::Blur(definition)) => {
                        let sigma_pixels = definition
                            .radius()
                            .to_output_pixels(
                                parameters.output_height,
                                parameters.world_view_height,
                            )
                            .map_err(EffectPrepareError::Parameter)?;
                        let upload = self
                            .blur
                            .prepare(
                                device,
                                queue,
                                filter,
                                GaussianBlurCapture {
                                    source: capture.source,
                                    revision,
                                },
                                GaussianBlurParameters {
                                    sigma_pixels,
                                    scope_opacity: opacity,
                                    scratch_budget_bytes: budget,
                                },
                            )
                            .map_err(EffectPrepareError::Blur)?;
                        let encoded = self.blur.encode(encoder, filter);
                        EffectRasterStats {
                            pipeline_compiles: upload.pipeline_compiles,
                            kernel_builds: upload.kernel_builds,
                            texture_allocations: upload.texture_allocations,
                            buffer_allocations: upload.buffer_allocations,
                            bind_group_creations: upload.bind_group_creations,
                            bytes_uploaded: upload.bytes_uploaded,
                            scratch_bytes: upload.scratch_bytes,
                            blur_passes: encoded.blur_passes,
                            composite_passes: encoded.output_passes,
                        }
                    }
                    (PassFilter::Shadow(filter), EffectDefinition::Shadow(definition)) => {
                        self.shadow.prepare_encode(
                            &mut self.blur,
                            device,
                            queue,
                            encoder,
                            filter,
                            GaussianBlurCapture {
                                source: capture.source,
                                revision,
                            },
                            definition,
                            parameters.output_height,
                            parameters.world_view_height,
                            opacity,
                            budget,
                        )?
                    }
                    _ => unreachable!("validated finite filter kind"),
                };
                if work.composite_passes != 0 {
                    pass.revision += 1;
                }
                revision = pass.revision;
                input = pass
                    .filter
                    .output()
                    .expect("active retained pass output")
                    .clone();
                accumulate(&mut stats, work);
            }
        }
        scope.output = last;
        stats.scratch_bytes = scope.scratch_bytes();
        Ok(stats)
    }
}

struct PreparedStack {
    budgets: [u64; MAX_EFFECT_STACK_LEN],
    active: [bool; MAX_EFFECT_STACK_LEN],
    last: Option<usize>,
}

fn preflight(
    device: &wgpu::Device,
    scope: &EffectStackScope,
    capture: GlowCapture<'_>,
    effects: &[EffectDefinition],
    parameters: EffectStackParameters,
) -> Result<PreparedStack, EffectPrepareError> {
    if effects.len() > MAX_EFFECT_STACK_LEN {
        return Err(EffectPrepareError::StackExceedsProfile);
    }
    if !parameters.scope_opacity.is_finite() || !(0.0..=1.0).contains(&parameters.scope_opacity) {
        return Err(EffectPrepareError::InvalidScopeOpacity);
    }
    noon_core::EffectRadius::Scene(0.0)
        .to_output_pixels(parameters.output_height, parameters.world_view_height)
        .map_err(EffectPrepareError::Parameter)?;
    let limits = device.limits();
    let size = [capture.source.width(), capture.source.height()];
    if scope.passes.iter().any(|p| {
        p.filter
            .output()
            .is_some_and(|output| output == capture.source)
    }) {
        return Err(EffectPrepareError::UnsupportedCapture);
    }
    let pixels = u64::from(size[0]) * u64::from(size[1]);
    let mut budgets = [0; MAX_EFFECT_STACK_LEN];
    let mut active = [false; MAX_EFFECT_STACK_LEN];
    let mut total = 0_u64;
    let mut last = None;
    for (index, &effect) in effects.iter().enumerate() {
        let values = EffectCaptureParameters {
            definition: effect,
            output_height: parameters.output_height,
            world_view_height: parameters.world_view_height,
            scope_opacity: parameters.scope_opacity,
        };
        let sigma = values.sigma_pixels()?;
        if values.is_neutral_at_sigma(sigma) || parameters.scope_opacity == 0.0 {
            budgets[index] = scope
                .passes
                .get(index)
                .filter(|pass| pass.filter.matches(effect))
                .map_or(0, |pass| pass.filter.scratch_bytes());
        } else {
            super::glow_filter::validate_capture(capture.source)?;
            values.offset_pixels()?;
            if size.contains(&0) || size.iter().any(|&v| v > limits.max_texture_dimension_2d) {
                return Err(EffectPrepareError::ExtentExceedsDevice);
            }
            if matches!(
                effect,
                EffectDefinition::Blur(_) | EffectDefinition::Shadow(_)
            ) && sigma > 0.0
                && (limits.max_color_attachments < 3
                    || limits.max_color_attachment_bytes_per_sample < 12)
            {
                return Err(EffectPrepareError::Blur(
                    super::GaussianBlurPrepareError::InsufficientColorAttachments,
                ));
            }
            if effect
                .as_glow()
                .is_some_and(|v| v.source() == GlowSource::Silhouette)
            {
                let mask = capture
                    .silhouette
                    .ok_or(EffectPrepareError::MissingSilhouette)?;
                super::glow_filter::validate_capture(mask)?;
                if [mask.width(), mask.height()] != size {
                    return Err(EffectPrepareError::CaptureSizeMismatch);
                }
            }
            if scope
                .passes
                .get(index)
                .is_some_and(|pass| pass.revision == u64::MAX)
            {
                return Err(EffectPrepareError::RevisionExhausted);
            }
            let cost = values.filter_bytes_per_pixel();
            budgets[index] = pixels
                .checked_mul(cost)
                .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
            if let EffectDefinition::NoiseDissolve(definition) = effect {
                super::dissolve_filter::validate_input(
                    GaussianBlurCapture {
                        source: capture.source,
                        revision: capture.revision,
                    },
                    parameters.dissolve(
                        definition,
                        capture.source,
                        parameters.scope_opacity,
                        budgets[index],
                    ),
                )?;
            }
            active[index] = true;
            last = Some(index);
        }
        total = total
            .checked_add(budgets[index])
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
    }
    if total > parameters.scratch_budget_bytes {
        return Err(EffectPrepareError::ScratchBudgetExceeded);
    }
    Ok(PreparedStack {
        budgets,
        active,
        last,
    })
}

fn accumulate(total: &mut EffectRasterStats, value: EffectRasterStats) {
    total.pipeline_compiles += value.pipeline_compiles;
    total.kernel_builds += value.kernel_builds;
    total.texture_allocations += value.texture_allocations;
    total.buffer_allocations += value.buffer_allocations;
    total.bind_group_creations += value.bind_group_creations;
    total.bytes_uploaded += value.bytes_uploaded;
    total.blur_passes += value.blur_passes;
    total.composite_passes += value.composite_passes;
}

#[cfg(test)]
mod tests;
