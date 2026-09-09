//! Renderer-independent frame snapshots used by deterministic replay tests and tools.

use noon::ExecutionSession;
use noon_runtime::{EvaluationError, FrameState};

fn normalize_playhead(time: f64) -> f64 {
    const SCALE: f64 = 1_000_000_000_000.0;
    (time * SCALE).round() / SCALE
}

fn normalized_frames_equal(left: &FrameState, right: &FrameState) -> bool {
    normalize_playhead(left.time) == normalize_playhead(right.time)
        && left.objects == right.objects
        && left.presences == right.presences
        && left.reveals == right.reveals
        && left.morphs == right.morphs
        && left.render_geometries == right.render_geometries
        && left.render_transforms == right.render_transforms
        && left.family_animations == right.family_animations
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayVerificationMode {
    Forward,
    Rewind,
}

impl std::fmt::Display for ReplayVerificationMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Forward => "forward playback",
            Self::Rewind => "rewind playback",
        })
    }
}

#[derive(Debug)]
pub enum ReplayVerificationError {
    Evaluation(EvaluationError),
    Fixture(String),
    InvalidForwardSampleCount(usize),
    NonFiniteTarget {
        index: usize,
        target: f64,
    },
    PlayheadDrift {
        target: f64,
        actual: f64,
    },
    Diverged {
        mode: ReplayVerificationMode,
        target: f64,
    },
}

impl std::fmt::Display for ReplayVerificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Fixture(error) => formatter.write_str(error),
            Self::InvalidForwardSampleCount(count) => {
                write!(
                    formatter,
                    "forward sample count must be at least two, got {count}"
                )
            }
            Self::NonFiniteTarget { index, target } => {
                write!(
                    formatter,
                    "replay target {index} must be finite, got {target}"
                )
            }
            Self::PlayheadDrift { target, actual } => write!(
                formatter,
                "direct replay playhead drifted from target {target} to {actual}"
            ),
            Self::Diverged { mode, target } => {
                write!(
                    formatter,
                    "{mode} diverged from direct seek at target {target}"
                )
            }
        }
    }
}

impl std::error::Error for ReplayVerificationError {}

impl From<EvaluationError> for ReplayVerificationError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Compare direct seek, incremental playback and rewind on independent runtime copies.
///
/// Cloning is qualification work only; the product still owns one mutable runtime.
/// Construction and lowering are typed Rust. No scene or frame data crosses a codec.
pub fn verify_execution_replay(
    session: &ExecutionSession,
    targets: &[f64],
    forward_sample_count: usize,
) -> Result<(), ReplayVerificationError> {
    if forward_sample_count < 2 {
        return Err(ReplayVerificationError::InvalidForwardSampleCount(
            forward_sample_count,
        ));
    }
    for (index, &target) in targets.iter().enumerate() {
        if !target.is_finite() {
            return Err(ReplayVerificationError::NonFiniteTarget { index, target });
        }
    }

    let mut direct = session.clone();
    let mut forward = direct.clone();
    let mut rewind = direct.clone();
    let denominator = (forward_sample_count - 1) as f64;

    for &target in targets {
        direct.seek(target)?;
        let actual = direct.frame().time;
        if (actual - target).abs() > 1.0e-12 {
            return Err(ReplayVerificationError::PlayheadDrift { target, actual });
        }

        forward.advance_to(0.0)?;
        for sample in 0..forward_sample_count {
            forward.advance_to(target * sample as f64 / denominator)?;
        }
        if !normalized_frames_equal(direct.frame(), forward.frame())
            || direct.painter_order() != forward.painter_order()
        {
            return Err(ReplayVerificationError::Diverged {
                mode: ReplayVerificationMode::Forward,
                target,
            });
        }

        rewind.advance_to(0.0)?;
        for time in [0.0, target.max(0.25) + 0.4, 0.1, target] {
            rewind.advance_to(time)?;
        }
        if !normalized_frames_equal(direct.frame(), rewind.frame())
            || direct.painter_order() != rewind.painter_order()
        {
            return Err(ReplayVerificationError::Diverged {
                mode: ReplayVerificationMode::Rewind,
                target,
            });
        }
    }

    Ok(())
}

/// Qualify the same Rust-authored fixtures on native and direct WASM.
pub fn verify_example_replay(
    example: &str,
    targets: &[f64],
    forward_sample_count: usize,
    stress_count: usize,
) -> Result<(), ReplayVerificationError> {
    use noon::example_scenes;
    let session = match example {
        "exact-property-tracks" => example_scenes::exact_property_tracks::session(),
        "specialized-geometry" => example_scenes::specialized_geometry::session(),
        "family-placement" => example_scenes::family_placement::session(),
        "painter-order" => example_scenes::painter_order_overlap::session(),
        "analytic-stress" => example_scenes::analytic_profile::session(
            stress_count,
            example_scenes::analytic_profile::Layout::Fit,
            16.0 / 9.0,
            2.0,
        ),
        "create-morph-fade" => create_morph_fade_session(),
        _ => Err(format!("unknown direct replay example: {example}")),
    }
    .map_err(ReplayVerificationError::Fixture)?;
    verify_execution_replay(&session, targets, forward_sample_count)
}

fn create_morph_fade_session() -> Result<ExecutionSession, String> {
    use noon::{
        AnimationCompositionRequest as Request, AnimationOptions, FadeEndpoint, RateFunction,
        Scene, SemanticAnimationCompositionKind, SemanticFadeDirection, TransformToRequest,
    };
    let mut scene = Scene::new();
    let mut entering = scene.circle(0.75).map_err(|error| error.to_string())?;
    entering
        .set_translation(-2.0, 0.0)
        .map_err(|error| error.to_string())?;
    let source = scene.square(1.5).map_err(|error| error.to_string())?;
    let target = scene.circle(0.75).map_err(|error| error.to_string())?;
    let mut leaving = scene.circle(0.75).map_err(|error| error.to_string())?;
    leaving
        .set_translation(2.0, 0.0)
        .map_err(|error| error.to_string())?;
    scene.add(&source).map_err(|error| error.to_string())?;
    scene.add(&leaving).map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    let options = AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::Linear);
    let request = Request::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        children: vec![
            Request::Create {
                target: &entering,
                options,
            },
            Request::TransformTo(TransformToRequest::point_correspondence(
                &source, &target, options,
            )),
            Request::Fade {
                target: &leaving,
                direction: SemanticFadeDirection::Out,
                endpoint: FadeEndpoint::default(),
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    // Activate through the ordinary live API: initial exact-track lowering is
    // intentionally narrower. Replay compares the activated execution channels;
    // logical segment completion/publication has its own shared live tests.
    scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .map_err(|error| error.to_string())?;
    Ok(session)
}

#[cfg(all(target_arch = "wasm32", debug_assertions))]
mod wasm {
    use wasm_bindgen::prelude::*;

    /// Only fixture selection and sample times cross the test-harness boundary.
    #[wasm_bindgen(js_name = verifyDirectExecutionReplay)]
    pub fn verify_direct_execution_replay(
        example: &str,
        targets: &[f64],
        forward_sample_count: u32,
        stress_count: u32,
    ) -> Result<(), JsValue> {
        super::verify_example_replay(
            example,
            targets,
            forward_sample_count as usize,
            stress_count as usize,
        )
        .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::normalized_frames_equal;

    #[test]
    fn replay_equality_includes_the_effective_renderer_coordinate_frame() {
        let session = noon::example_scenes::exact_property_tracks::session().unwrap();
        let expected = session.frame();
        let mut actual = expected.clone();
        assert!(normalized_frames_equal(&actual, expected));

        // A derived point-morph coordinate frame can differ while the semantic
        // object transform stays unchanged. The renderer consumes this override.
        let mut render_transform = actual.objects[0].transform;
        render_transform.translation.x += 1.0;
        actual.render_transforms[0] = Some(render_transform);
        assert_eq!(actual.objects, expected.objects);
        assert!(!normalized_frames_equal(&actual, expected));
    }
}
