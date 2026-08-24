//! Renderer-independent frame snapshots used by deterministic replay tests and tools.

use noon_runtime::FrameState;
use serde_json::{json, Value};

use crate::{PlayerError, ScenePlayer};

/// Normalize the observable runtime frame into a stable JSON value.
///
/// Dense runtime indices and cache state are deliberately omitted. Object order,
/// semantic IDs, evaluated geometry/properties, presence/reveal/morph state, and
/// render geometry are retained because they affect user-visible scene behavior.
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
        "time": frame.time,
        "objects": objects,
    })
}

pub fn normalized_frame_json(frame: &FrameState) -> String {
    serde_json::to_string(&normalized_frame_value(frame))
        .expect("normalized frame contains only JSON-serializable values")
}

/// Evaluate a scene by seeking directly to `time` and return its normalized frame.
pub fn scene_snapshot_json(scene_json: &str, time: f64) -> Result<String, PlayerError> {
    let mut player = ScenePlayer::from_scene_json(scene_json)?;
    player.seek(time)?;
    Ok(normalized_frame_json(player.frame()))
}

/// Evaluate a scene through the supplied playhead sequence and return the final frame.
///
/// The sequence may move backward. This intentionally exercises the same
/// `advance_to` rewind behavior used by interactive scrubbing.
pub fn playback_snapshot_json(scene_json: &str, times: &[f64]) -> Result<String, PlayerError> {
    let mut player = ScenePlayer::from_scene_json(scene_json)?;
    for &time in times {
        player.advance_to(time)?;
    }
    Ok(normalized_frame_json(player.frame()))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{playback_snapshot_json, scene_snapshot_json};

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
}
