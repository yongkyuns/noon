//! Renderer-independent frame snapshots used by deterministic replay tests and tools.

use noon_compile::{CompileError, CompiledScene};
use noon_ir::{decode_scene, IrError};
use noon_runtime::{EvaluationError, FrameState, SlottedSceneInstance};
use serde_json::{json, Value};

fn normalize_playhead(time: f64) -> f64 {
    const SCALE: f64 = 1_000_000_000_000.0;
    (time * SCALE).round() / SCALE
}

/// Normalize the observable runtime frame into a stable JSON value.
///
/// Dense runtime indices and cache state are deliberately omitted. Object order,
/// semantic IDs, evaluated geometry/properties, presence/reveal/morph state, and
/// render geometry are retained because they affect user-visible scene behavior.
/// The f64 playhead is rounded to picosecond precision so mathematically equivalent
/// timestamp construction paths do not create false mismatches; evaluated scene
/// properties remain unrounded.
pub fn normalized_frame_value(frame: &FrameState) -> Value {
    let objects = frame
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            json!({
                "id": object.id.get(),
                "geometry": &object.geometry,
                "transform": object.transform,
                "style": object.style,
                "appearance": object.appearance,
                "present": frame.presences[index],
                "reveal": frame.reveals[index],
                "morph": frame.morphs[index],
                "render_geometry": frame.render_geometries[index].as_ref(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "time": normalize_playhead(frame.time),
        "objects": objects,
    })
}

pub fn normalized_frame_json(frame: &FrameState) -> String {
    serde_json::to_string(&normalized_frame_value(frame))
        .expect("normalized frame contains only JSON-serializable values")
}

fn normalized_frames_equal(left: &FrameState, right: &FrameState) -> bool {
    normalize_playhead(left.time) == normalize_playhead(right.time)
        && left.objects == right.objects
        && left.presences == right.presences
        && left.reveals == right.reveals
        && left.morphs == right.morphs
        && left.render_geometries == right.render_geometries
}

#[derive(Debug)]
pub enum ReplayRuntimeError {
    Ir(IrError),
    Compile(CompileError),
    Evaluation(EvaluationError),
}

impl std::fmt::Display for ReplayRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ReplayRuntimeError {}

impl From<IrError> for ReplayRuntimeError {
    fn from(value: IrError) -> Self {
        Self::Ir(value)
    }
}

impl From<CompileError> for ReplayRuntimeError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<EvaluationError> for ReplayRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

fn runtime_from_scene_json(scene_json: &str) -> Result<SlottedSceneInstance, ReplayRuntimeError> {
    let definition = decode_scene(scene_json)?;
    let compiled = CompiledScene::compile(&definition)?;
    Ok(SlottedSceneInstance::new(compiled))
}

/// Evaluate a scene by seeking directly to `time` and return its normalized frame.
pub fn scene_snapshot_json(scene_json: &str, time: f64) -> Result<String, ReplayRuntimeError> {
    let mut runtime = runtime_from_scene_json(scene_json)?;
    runtime.seek(time)?;
    Ok(normalized_frame_json(runtime.frame()))
}

/// Evaluate a scene through the supplied playhead sequence and return the final frame.
///
/// The sequence may move backward. This intentionally exercises the same
/// `advance_to` rewind behavior used by interactive scrubbing.
pub fn playback_snapshot_json(
    scene_json: &str,
    times: &[f64],
) -> Result<String, ReplayRuntimeError> {
    let mut runtime = runtime_from_scene_json(scene_json)?;
    for &time in times {
        runtime.advance_to(time)?;
    }
    Ok(normalized_frame_json(runtime.frame()))
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
    Runtime(ReplayRuntimeError),
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
            Self::Runtime(error) => error.fmt(formatter),
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

impl From<ReplayRuntimeError> for ReplayVerificationError {
    fn from(value: ReplayRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<EvaluationError> for ReplayVerificationError {
    fn from(value: EvaluationError) -> Self {
        Self::Runtime(ReplayRuntimeError::Evaluation(value))
    }
}

/// Verify direct seek, incremental playback, and rewind equivalence for one scene.
///
/// The scene is decoded and compiled once. Three persistent runtime instances then
/// exercise the independent direct, forward, and rewind paths for every target.
/// Observable frame state is compared in Rust instead of serializing large snapshots
/// through the WASM boundary merely to compare them in JavaScript.
pub fn verify_scene_replay(
    scene_json: &str,
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

    let mut direct = runtime_from_scene_json(scene_json)?;
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
        if !normalized_frames_equal(direct.frame(), forward.frame()) {
            return Err(ReplayVerificationError::Diverged {
                mode: ReplayVerificationMode::Forward,
                target,
            });
        }

        rewind.advance_to(0.0)?;
        for time in [0.0, target.max(0.25) + 0.4, 0.1, target] {
            rewind.advance_to(time)?;
        }
        if !normalized_frames_equal(direct.frame(), rewind.frame()) {
            return Err(ReplayVerificationError::Diverged {
                mode: ReplayVerificationMode::Rewind,
                target,
            });
        }
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{playback_snapshot_json, scene_snapshot_json, verify_scene_replay};

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen(js_name = evaluateSceneSnapshot)]
    pub fn evaluate_scene_snapshot(scene_json: &str, time: f64) -> Result<String, JsValue> {
        scene_snapshot_json(scene_json, time).map_err(js_error)
    }

    #[wasm_bindgen(js_name = evaluateScenePlaybackSnapshot)]
    pub fn evaluate_scene_playback_snapshot(
        scene_json: &str,
        times_json: &str,
    ) -> Result<String, JsValue> {
        let times: Vec<f64> = serde_json::from_str(times_json).map_err(js_error)?;
        playback_snapshot_json(scene_json, &times).map_err(js_error)
    }

    #[wasm_bindgen(js_name = verifySceneReplay)]
    pub fn verify_scene_replay_wasm(
        scene_json: &str,
        targets_json: &str,
        forward_sample_count: u32,
    ) -> Result<(), JsValue> {
        let targets: Vec<f64> = serde_json::from_str(targets_json).map_err(js_error)?;
        verify_scene_replay(scene_json, &targets, forward_sample_count as usize).map_err(js_error)
    }
}
