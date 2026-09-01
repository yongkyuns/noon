use noon_ir::{SceneSpec, SceneSpecError};

use crate::{
    ClockError, MixedRetainedAuthoringError, MixedRetainedAuthoringScene, PlaybackClock,
    RetainedAuthoringPlayer, RetainedAuthoringPlayerError,
};

/// Clocked retained execution owner constructed directly from canonical `SceneSpec`.
///
/// Compatibility adapters may still produce `SceneSpec` from older payloads, but this
/// execution boundary has no legacy geometry document or retained-text sidecar.
#[derive(Debug)]
pub struct CanonicalRetainedEnginePlayer {
    player: RetainedAuthoringPlayer,
    clock: PlaybackClock,
    scene_spec_json: String,
}

impl CanonicalRetainedEnginePlayer {
    pub fn new(
        scene_spec: SceneSpec,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, CanonicalRetainedEnginePlayerError> {
        let scene_spec_json = scene_spec.to_json()?;
        let mixed = MixedRetainedAuthoringScene::from_scene_spec(scene_spec)?;
        Ok(Self {
            player: RetainedAuthoringPlayer::new(mixed, session)?,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            scene_spec_json,
        })
    }

    pub fn from_json(
        scene_spec_json: &str,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, CanonicalRetainedEnginePlayerError> {
        Self::new(
            SceneSpec::from_json(scene_spec_json)?,
            loop_duration_seconds,
            session,
        )
    }

    pub fn scene_spec_json(&self) -> &str {
        &self.scene_spec_json
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        self.player.resource_bundle_bytes()
    }

    pub fn initial_delta_json(&mut self) -> Result<String, CanonicalRetainedEnginePlayerError> {
        let delta = self
            .player
            .evaluate_delta(0.0)?
            .expect("first retained evaluation must emit a snapshot");
        Ok(serde_json::to_string(&delta)?)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.player
            .evaluate_delta(scene_time)?
            .map(|delta| {
                serde_json::to_string(&delta).map_err(CanonicalRetainedEnginePlayerError::from)
            })
            .transpose()
    }

    pub fn set_loop_duration(
        &mut self,
        duration: f64,
    ) -> Result<(), CanonicalRetainedEnginePlayerError> {
        self.clock.set_loop_duration(duration)?;
        Ok(())
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn resume(&mut self) {
        self.clock.resume();
    }

    pub fn seek_delta_json(
        &mut self,
        scene_time: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        let mut clock = self.clock.clone();
        clock.seek(scene_time)?;
        let delta = self
            .player
            .evaluate_delta(scene_time)?
            .map(|delta| {
                serde_json::to_string(&delta).map_err(CanonicalRetainedEnginePlayerError::from)
            })
            .transpose()?;
        self.clock = clock;
        Ok(delta)
    }

    pub const fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    pub fn time(&self) -> f64 {
        self.player.frame().time
    }
}

#[derive(Debug)]
pub enum CanonicalRetainedEnginePlayerError {
    SceneSpec(SceneSpecError),
    Authoring(MixedRetainedAuthoringError),
    Player(RetainedAuthoringPlayerError),
    Clock(ClockError),
    Json(serde_json::Error),
}

impl std::fmt::Display for CanonicalRetainedEnginePlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneSpec(error) => error.fmt(formatter),
            Self::Authoring(error) => error.fmt(formatter),
            Self::Player(error) => error.fmt(formatter),
            Self::Clock(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CanonicalRetainedEnginePlayerError {}

impl From<SceneSpecError> for CanonicalRetainedEnginePlayerError {
    fn from(value: SceneSpecError) -> Self {
        Self::SceneSpec(value)
    }
}

impl From<MixedRetainedAuthoringError> for CanonicalRetainedEnginePlayerError {
    fn from(value: MixedRetainedAuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<RetainedAuthoringPlayerError> for CanonicalRetainedEnginePlayerError {
    fn from(value: RetainedAuthoringPlayerError) -> Self {
        Self::Player(value)
    }
}

impl From<ClockError> for CanonicalRetainedEnginePlayerError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<serde_json::Error> for CanonicalRetainedEnginePlayerError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::CanonicalRetainedEnginePlayer;

    #[wasm_bindgen(js_name = CanonicalRetainedEngineScenePlayer)]
    pub struct WasmCanonicalRetainedEngineScenePlayer {
        inner: CanonicalRetainedEnginePlayer,
    }

    #[wasm_bindgen(js_class = CanonicalRetainedEngineScenePlayer)]
    impl WasmCanonicalRetainedEngineScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            scene_spec_json: &str,
            loop_duration_seconds: f64,
            session: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                inner: CanonicalRetainedEnginePlayer::from_json(
                    scene_spec_json,
                    loop_duration_seconds,
                    session,
                )
                .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = resourceBundleBytes)]
        pub fn resource_bundle_bytes(&self) -> Vec<u8> {
            self.inner.resource_bundle_bytes().to_vec()
        }

        #[wasm_bindgen(js_name = initialDeltaJson)]
        pub fn initial_delta_json(&mut self) -> Result<String, JsValue> {
            self.inner.initial_delta_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = tickDeltaJson)]
        pub fn tick_delta_json(&mut self, timestamp_ms: f64) -> Result<Option<String>, JsValue> {
            self.inner.tick_delta_json(timestamp_ms).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setLoopDurationSeconds)]
        pub fn set_loop_duration_seconds(&mut self, duration: f64) -> Result<(), JsValue> {
            self.inner.set_loop_duration(duration).map_err(js_error)
        }

        pub fn pause(&mut self) {
            self.inner.pause();
        }

        pub fn resume(&mut self) {
            self.inner.resume();
        }

        #[wasm_bindgen(js_name = seekDeltaJson)]
        pub fn seek_delta_json(&mut self, scene_time: f64) -> Result<Option<String>, JsValue> {
            self.inner.seek_delta_json(scene_time).map_err(js_error)
        }

        #[wasm_bindgen(js_name = isPlaying)]
        pub fn is_playing(&self) -> bool {
            self.inner.is_playing()
        }

        #[wasm_bindgen(js_name = sceneSpecJson)]
        pub fn scene_spec_json(&self) -> String {
            self.inner.scene_spec_json().to_owned()
        }

        pub fn time(&self) -> f64 {
            self.inner.time()
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectId, SceneDefinition};
    use serde_json::json;

    use crate::{
        canonical_retained_scene_spec_json, RetainedAuthoringDocument,
        RetainedAuthoringEnginePlayer, RetainedAuthoringTextObject, RetainedExecutionDeltaEnvelope,
        RetainedTextAuthoringSpec,
    };

    use super::*;

    #[test]
    fn canonical_engine_matches_compatibility_snapshot_and_resources() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 1,
            text: RetainedTextAuthoringSpec::native(
                "Canonical engine",
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap();
        let legacy_json = noon_ir::encode_scene(&legacy).unwrap();
        let retained_json = retained.to_json().unwrap();
        let scene_spec_json =
            canonical_retained_scene_spec_json(&legacy_json, &retained_json).unwrap();

        let mut canonical =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 2.0, 41).unwrap();
        let mut compatibility =
            RetainedAuthoringEnginePlayer::new(&legacy_json, &retained_json, 2.0, 41).unwrap();
        let canonical_initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&canonical.initial_delta_json().unwrap()).unwrap();
        let compatibility_initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&compatibility.initial_delta_json().unwrap()).unwrap();

        assert_eq!(canonical_initial, compatibility_initial);
        assert_eq!(canonical_initial.objects[0].object, circle);
        assert_eq!(canonical_initial.objects[1].object, text_id);
        assert_eq!(
            canonical.resource_bundle_bytes(),
            compatibility.resource_bundle_bytes()
        );
        assert_eq!(canonical.scene_spec_json(), scene_spec_json);
    }

    #[test]
    fn canonical_engine_rejects_unsupported_scene_spec_version() {
        let invalid = json!({"version": 99, "objects": [], "tracks": []}).to_string();
        let error = CanonicalRetainedEnginePlayer::from_json(&invalid, 2.0, 47).unwrap_err();
        assert!(matches!(
            error,
            CanonicalRetainedEnginePlayerError::SceneSpec(SceneSpecError::UnsupportedVersion(99))
        ));
    }
}
