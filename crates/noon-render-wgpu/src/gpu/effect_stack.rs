//! Retained ordered composition of finite compatibility operators and linear Blur.
//! This is derived GPU work over a caller-owned padded capture, not a scene,
//! attachment registry, scheduler or clock. Authored scope admission and capture
//! bounds remain with their existing compiler/renderer owners.
use noon_core::{EffectDefinition, GlowSource};

use super::{
    gaussian_blur::{
        GaussianBlurCapture, GaussianBlurFilter, GaussianBlurParameters, GaussianBlurScope,
    },
    shadow_filter::{ShadowFilter, ShadowScope},
    EffectCaptureParameters, EffectPixelBounds, EffectPrepareError, EffectRasterStats, GlowCapture,
    GlowFilter, GlowParameters, GlowScope, SampleFilter, SampleParameters, SampleScope,
};

/// The fixed-quality finite operator profile; no silent truncation of a stack.
pub use noon_core::MAX_EFFECT_STACK_LEN;

#[derive(Clone, Copy, Debug)]
pub struct EffectStackParameters {
    /// Derived raster working format, shared by capture and every active pass.
    pub working_format: wgpu::TextureFormat,
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
        definition: EffectDefinition,
        source: &wgpu::Texture,
        opacity: f32,
        budget: u64,
    ) -> SampleParameters {
        SampleParameters {
            definition,
            output_height: self.output_height,
            world_view_height: self.world_view_height,
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
    Sample(Box<SampleScope>),
}
impl PassFilter {
    fn new(definition: EffectDefinition) -> Self {
        match definition {
            EffectDefinition::Glow(_) => Self::Glow(Box::default()),
            EffectDefinition::Blur(_) | EffectDefinition::BackdropBlur(_) => {
                Self::Blur(Box::default())
            }
            EffectDefinition::Shadow(_) => Self::Shadow(Box::default()),
            EffectDefinition::NoiseDissolve(_) | EffectDefinition::Pixelate(_) => {
                Self::Sample(Box::default())
            }
        }
    }
    fn matches(&self, definition: EffectDefinition) -> bool {
        matches!(
            (self, definition),
            (Self::Glow(_), EffectDefinition::Glow(_))
                | (
                    Self::Blur(_),
                    EffectDefinition::Blur(_) | EffectDefinition::BackdropBlur(_)
                )
                | (Self::Shadow(_), EffectDefinition::Shadow(_))
                | (
                    Self::Sample(_),
                    EffectDefinition::NoiseDissolve(_) | EffectDefinition::Pixelate(_)
                )
        )
    }
    fn output(&self) -> Option<&wgpu::Texture> {
        match self {
            Self::Glow(v) => v.output(),
            Self::Blur(v) => v.output(),
            Self::Shadow(v) => v.output(),
            Self::Sample(v) => v.output(),
        }
    }
    fn output_view(&self) -> Option<&wgpu::TextureView> {
        match self {
            Self::Glow(v) => v.output_view(),
            Self::Blur(v) => v.output_view(),
            Self::Shadow(v) => v.output_view(),
            Self::Sample(v) => v.output_view(),
        }
    }
    fn scratch_bytes(&self) -> u64 {
        match self {
            Self::Glow(v) => v.scratch_bytes(),
            Self::Blur(v) => v.scratch_bytes(),
            Self::Shadow(v) => v.scratch_bytes(),
            Self::Sample(v) => v.scratch_bytes(),
        }
    }
    fn invalidate(&mut self) {
        match self {
            Self::Glow(v) => v.invalidate(),
            Self::Blur(v) => v.invalidate(),
            Self::Shadow(v) => v.invalidate(),
            Self::Sample(v) => v.invalidate(),
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
    // One disposable pointwise destination can survive removal of a trailing
    // mask while this capture still owns authored effects. No semantic identity
    // or mask parameters live here; an empty/retired scope releases it normally.
    warm_sample: Option<Box<SampleScope>>,
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
            .sum::<u64>()
            + self
                .warm_sample
                .as_ref()
                .map_or(0, |mask| mask.scratch_bytes())
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
            .sum::<u64>()
            + self.sample_candidate_bytes(effects)
    }
    fn sample_candidate_bytes(&self, effects: &[EffectDefinition]) -> u64 {
        if effects.is_empty() {
            return 0;
        }
        self.passes
            .iter()
            .skip(effects.len())
            .rev()
            .find_map(|pass| match &pass.filter {
                PassFilter::Sample(mask) => Some(mask.scratch_bytes()),
                _ => None,
            })
            .or_else(|| self.warm_sample.as_ref().map(|mask| mask.scratch_bytes()))
            .unwrap_or(0)
    }
    fn sample_reuse_index(&self, effects: &[EffectDefinition]) -> Option<usize> {
        (self.sample_candidate_bytes(effects) > 0)
            .then(|| {
                effects.iter().enumerate().find_map(|(index, effect)| {
                    (matches!(
                        effect,
                        EffectDefinition::NoiseDissolve(_) | EffectDefinition::Pixelate(_)
                    ) && self
                        .passes
                        .get(index)
                        .is_none_or(|pass| !pass.filter.matches(*effect)))
                    .then_some(index)
                })
            })
            .flatten()
    }
    fn sync_profile(&mut self, effects: &[EffectDefinition]) {
        if effects.is_empty() {
            self.clear();
            return;
        }
        for pass in self.passes.drain(effects.len().min(self.passes.len())..) {
            if let PassFilter::Sample(mask) = pass.filter {
                self.warm_sample = Some(mask);
            }
        }
        for (index, &effect) in effects.iter().enumerate() {
            if self
                .passes
                .get(index)
                .is_some_and(|pass| pass.filter.matches(effect))
            {
                continue;
            }
            let filter = if matches!(
                effect,
                EffectDefinition::NoiseDissolve(_) | EffectDefinition::Pixelate(_)
            ) {
                self.warm_sample
                    .take()
                    .map(|mut mask| {
                        mask.invalidate();
                        PassFilter::Sample(mask)
                    })
                    .unwrap_or_else(|| PassFilter::new(effect))
            } else {
                PassFilter::new(effect)
            };
            let pass = RetainedPass {
                filter,
                revision: 0,
            };
            if index == self.passes.len() {
                self.passes.push(pass);
            } else {
                self.passes[index] = pass;
            }
        }
    }
    /// Surviving warm resources remain charged; no GPU work is necessary.
    pub(super) fn deactivate_profile(&mut self, effects: &[EffectDefinition]) {
        if self.passes.is_empty() {
            self.output = None;
            return;
        }
        self.sync_profile(effects);
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
    pub(super) sample: SampleFilter,
}

impl EffectStackFilter {
    /// Each pass consumes the preceding output in the declared working format.
    /// Compatibility operators use encoded-premultiplied RGBA8; linear Blur
    /// preserves associated RGBA16Float radiance. Painted glow uses input alpha;
    /// silhouette glow uses the original geometry mask. Caller provides the full
    /// cumulative padded extent on the unchanged transparent-border sample grid.
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
        if last.is_none() && scope.passes.is_empty() {
            scope.output = None;
            return Ok(EffectRasterStats::default());
        }
        scope.sync_profile(effects);
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
                    (
                        PassFilter::Sample(filter),
                        definition @ (EffectDefinition::NoiseDissolve(_)
                        | EffectDefinition::Pixelate(_)),
                    ) => self.sample.prepare_encode(
                        device,
                        queue,
                        encoder,
                        filter,
                        GaussianBlurCapture {
                            source: capture.source,
                            revision,
                        },
                        parameters.dissolve(definition, capture.source, opacity, budget),
                    )?,
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
                    (PassFilter::Blur(filter), definition)
                        if definition.as_blur().is_some()
                            || definition.as_backdrop_blur().is_some() =>
                    {
                        let sigma_pixels = definition
                            .radius()
                            .expect("Gaussian radius")
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
    }) || scope
        .warm_sample
        .as_ref()
        .and_then(|mask| mask.output())
        .is_some_and(|output| output == capture.source)
    {
        return Err(EffectPrepareError::UnsupportedCapture);
    }
    let linear = capture.source.format() == wgpu::TextureFormat::Rgba16Float;
    if linear != (parameters.working_format == wgpu::TextureFormat::Rgba16Float) {
        return Err(EffectPrepareError::UnsupportedCapture);
    }
    let pixels = u64::from(size[0]) * u64::from(size[1]);
    let mut budgets = [0; MAX_EFFECT_STACK_LEN];
    let mut active = [false; MAX_EFFECT_STACK_LEN];
    let mut total = 0_u64;
    let sample_reuse = scope.sample_reuse_index(effects);
    let warm_sample_bytes = scope.sample_candidate_bytes(effects);
    let mut last = None;
    for (index, &effect) in effects.iter().enumerate() {
        let values = EffectCaptureParameters {
            definition: effect,
            output_height: parameters.output_height,
            world_view_height: parameters.world_view_height,
            scope_opacity: parameters.scope_opacity,
        };
        let cost = values.filter_bytes_per_pixel(parameters.working_format)?;
        let sigma = values.sigma_pixels()?;
        if values.is_neutral_at_sigma(sigma) || parameters.scope_opacity == 0.0 {
            budgets[index] = scope
                .passes
                .get(index)
                .filter(|pass| pass.filter.matches(effect))
                .map_or_else(
                    || {
                        if sample_reuse == Some(index) {
                            warm_sample_bytes
                        } else {
                            0
                        }
                    },
                    |pass| pass.filter.scratch_bytes(),
                );
        } else {
            super::glow_filter::validate_working_capture(capture.source)?;
            values.offset_pixels()?;
            if size.contains(&0) || size.iter().any(|&v| v > limits.max_texture_dimension_2d) {
                return Err(EffectPrepareError::ExtentExceedsDevice);
            }
            if matches!(
                effect,
                EffectDefinition::Blur(_) | EffectDefinition::Shadow(_)
            ) && sigma > 0.0
                && (limits.max_color_attachments < if linear { 1 } else { 3 }
                    || limits.max_color_attachment_bytes_per_sample < if linear { 8 } else { 12 })
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
            budgets[index] = pixels
                .checked_mul(cost)
                .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
            if let definition @ (EffectDefinition::NoiseDissolve(_)
            | EffectDefinition::Pixelate(_)) = effect
            {
                super::sample_filter::validate_input(
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
    if sample_reuse.is_none() {
        total = total
            .checked_add(warm_sample_bytes)
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
