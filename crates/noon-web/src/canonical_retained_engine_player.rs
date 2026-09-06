use noon_ir::{SceneSpec, SceneSpecError};

use crate::{
    CanonicalRetainedFamilyAnimationScene, CanonicalRetainedFamilyAnimationSceneError, ClockError,
    MixedRetainedAuthoringError, MixedRetainedAuthoringScene, PlaybackClock,
    RetainedAuthoringPlayer, RetainedAuthoringPlayerError, RetainedFamilyExecutionPlayer,
    RetainedFamilyExecutionPlayerError,
};

/// Runtime selected behind the single canonical retained browser/WASM surface.
///
/// Frontends and workers do not branch on family scheduling. Canonical source content
/// determines the Rust execution owner once at construction, after which both variants
/// expose the same clocked delta/resource API.
#[derive(Debug)]
enum CanonicalRetainedExecutionPlayer {
    Ordinary(RetainedAuthoringPlayer),
    Family(RetainedFamilyExecutionPlayer),
}

impl CanonicalRetainedExecutionPlayer {
    fn resource_bundle_bytes(&self) -> &[u8] {
        match self {
            Self::Ordinary(player) => player.resource_bundle_bytes(),
            Self::Family(player) => player.resource_bundle_bytes(),
        }
    }

    fn evaluate_delta_json(
        &mut self,
        time: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        match self {
            Self::Ordinary(player) => player
                .evaluate_delta(time)?
                .map(|delta| {
                    serde_json::to_string(&delta).map_err(CanonicalRetainedEnginePlayerError::from)
                })
                .transpose(),
            Self::Family(player) => player
                .evaluate_delta(time)?
                .map(|delta| {
                    serde_json::to_string(&delta).map_err(CanonicalRetainedEnginePlayerError::from)
                })
                .transpose(),
        }
    }

    fn time(&self) -> f64 {
        match self {
            Self::Ordinary(player) => player.frame().time,
            Self::Family(player) => player.frame().time,
        }
    }
}

/// Clocked retained execution owner constructed directly from canonical `SceneSpec`.
///
/// Compatibility adapters may still produce `SceneSpec` from older payloads, but this
/// execution boundary has no legacy geometry document or retained-text sidecar. Family
/// animation requests are likewise consumed entirely inside Rust: the WASM class and
/// browser worker remain one canonical protocol regardless of selected execution owner.
#[derive(Debug)]
pub struct CanonicalRetainedEnginePlayer {
    player: CanonicalRetainedExecutionPlayer,
    clock: PlaybackClock,
    scene_spec_json: String,
}

impl CanonicalRetainedEnginePlayer {
    pub fn new(
        scene_spec: SceneSpec,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, CanonicalRetainedEnginePlayerError> {
        let has_family_animations = !scene_spec.family_animations.is_empty();
        let scene_spec_json = scene_spec.to_json()?;
        let player = if !has_family_animations {
            let mixed = MixedRetainedAuthoringScene::from_scene_spec(scene_spec)?;
            CanonicalRetainedExecutionPlayer::Ordinary(RetainedAuthoringPlayer::new(
                mixed, session,
            )?)
        } else {
            let lowered = CanonicalRetainedFamilyAnimationScene::from_scene_spec(scene_spec)?;
            let (scene, tracks, camera_object, animations) = lowered.into_parts();
            let animations = animations
                .into_iter()
                .map(|animation| animation.into_parts())
                .collect();
            CanonicalRetainedExecutionPlayer::Family(
                RetainedFamilyExecutionPlayer::new_many_with_tracks(
                    scene,
                    &tracks,
                    animations,
                    camera_object,
                    session,
                )?,
            )
        };

        Ok(Self {
            player,
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
        self.player
            .evaluate_delta_json(0.0)?
            .ok_or(CanonicalRetainedEnginePlayerError::MissingInitialSnapshot)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.player.evaluate_delta_json(scene_time)
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
        let delta = self.player.evaluate_delta_json(scene_time)?;
        self.clock = clock;
        Ok(delta)
    }

    pub const fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    pub fn time(&self) -> f64 {
        self.player.time()
    }
}

#[derive(Debug)]
pub enum CanonicalRetainedEnginePlayerError {
    SceneSpec(SceneSpecError),
    Authoring(MixedRetainedAuthoringError),
    Player(RetainedAuthoringPlayerError),
    FamilyScene(CanonicalRetainedFamilyAnimationSceneError),
    FamilyPlayer(RetainedFamilyExecutionPlayerError),
    MissingInitialSnapshot,
    Clock(ClockError),
    Json(serde_json::Error),
}

impl std::fmt::Display for CanonicalRetainedEnginePlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneSpec(error) => error.fmt(formatter),
            Self::Authoring(error) => error.fmt(formatter),
            Self::Player(error) => error.fmt(formatter),
            Self::FamilyScene(error) => error.fmt(formatter),
            Self::FamilyPlayer(error) => error.fmt(formatter),
            Self::MissingInitialSnapshot => formatter
                .write_str("canonical retained execution did not emit its initial snapshot"),
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

impl From<CanonicalRetainedFamilyAnimationSceneError> for CanonicalRetainedEnginePlayerError {
    fn from(value: CanonicalRetainedFamilyAnimationSceneError) -> Self {
        Self::FamilyScene(value)
    }
}

impl From<RetainedFamilyExecutionPlayerError> for CanonicalRetainedEnginePlayerError {
    fn from(value: RetainedFamilyExecutionPlayerError) -> Self {
        Self::FamilyPlayer(value)
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
    use noon_core::{
        FamilyAnimationLeafBinding, FamilyAnimationMode, FamilyAnimationRequest,
        FamilyAnimationSpec, GeometryRef, ObjectId, RateFunction, SceneDefinition, SemanticStore,
        TrackTiming, Vec2,
    };
    use noon_ir::SceneSpec;
    use serde_json::json;

    use crate::{
        canonical_retained_scene_spec_json, InstalledRetainedExecutionMirror,
        RetainedAuthoringDocument, RetainedAuthoringTextObject, RetainedExecutionDeltaEnvelope,
        RetainedFamilyExecutionDeltaEnvelope, RetainedTextAuthoringSpec,
        RetainedTransportApplyOutcome,
    };

    use super::*;

    fn canonical_scene_spec_json(legacy: &SceneDefinition, source: &str) -> (String, ObjectId) {
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: legacy.objects().len() as u32,
            text: RetainedTextAuthoringSpec::native(
                source,
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap();
        let legacy_json = noon_ir::encode_scene(legacy).unwrap();
        let retained_json = retained.to_json().unwrap();
        (
            canonical_retained_scene_spec_json(&legacy_json, &retained_json).unwrap(),
            text_id,
        )
    }

    fn family_scene_spec() -> (SceneSpec, ObjectId, ObjectId) {
        let mut legacy = SceneDefinition::new();
        let circle_id = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle_id,
                Vec2::ZERO,
                Vec2::new(4.0, 0.0),
                TrackTiming::new(0.0, 4.0, RateFunction::Linear),
            )
            .unwrap();
        let (scene_spec_json, text_id) = canonical_scene_spec_json(&legacy, "AB");
        let mut scene_spec = SceneSpec::from_json(&scene_spec_json).unwrap();

        let mut semantics = SemanticStore::new();
        let text_leaf = semantics.insert_authoring_object();
        let circle_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, text_leaf).unwrap();
        semantics.add_member(family, circle_leaf).unwrap();
        let family_spec = FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            1.0,
            2.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        scene_spec.family_animations.push(
            FamilyAnimationRequest::from_semantic_bindings(
                &semantics,
                family,
                family_spec,
                [
                    FamilyAnimationLeafBinding::new(circle_leaf, circle_id),
                    FamilyAnimationLeafBinding::new(text_leaf, text_id),
                ],
            )
            .unwrap(),
        );
        scene_spec.validate().unwrap();
        (scene_spec, text_id, circle_id)
    }

    fn append_family_request(
        scene_spec: &mut SceneSpec,
        start_time: f64,
        duration: f64,
        mode: FamilyAnimationMode,
    ) {
        let first = &scene_spec.family_animations[0];
        let spec = FamilyAnimationSpec::new(
            mode,
            start_time,
            duration,
            first.spec().lag_ratio,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        scene_spec.family_animations.push(
            FamilyAnimationRequest::new(first.target(), first.bindings().to_vec(), spec).unwrap(),
        );
        scene_spec.validate().unwrap();
    }

    fn assert_family_midpoint(
        mirror: &InstalledRetainedExecutionMirror,
        text_id: ObjectId,
        circle_id: ObjectId,
    ) {
        let plan = mirror.family_plan().unwrap().unwrap();
        assert_eq!(plan.leaves()[0].span().object, text_id);
        assert_eq!(plan.leaves()[1].span().object, circle_id);

        let retained = mirror.frame().unwrap();
        let text_index = retained
            .objects
            .iter()
            .position(|object| object.id == text_id)
            .unwrap();
        let circle_index = retained
            .objects
            .iter()
            .position(|object| object.id == circle_id)
            .unwrap();
        assert_eq!((circle_index, text_index), (0, 1));

        let family_frame = mirror.family_frame().unwrap().unwrap();
        let text = family_frame
            .planned_family_leaf(plan, text_index)
            .unwrap()
            .unwrap();
        let circle = family_frame
            .planned_family_leaf(plan, circle_index)
            .unwrap()
            .unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);

        let circle = &retained.objects[circle_index];
        assert_eq!(circle.transform.translation, Vec2::new(2.0, 0.0));
    }

    #[test]
    fn canonical_engine_emits_mixed_snapshot_and_resources() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let (scene_spec_json, text_id) = canonical_scene_spec_json(&legacy, "Canonical engine");

        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 2.0, 41).unwrap();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();

        assert!(initial.snapshot);
        assert_eq!(initial.objects.len(), 2);
        assert_eq!(initial.objects[0].object, circle);
        assert_eq!(initial.objects[1].object, text_id);
        assert!(!engine.resource_bundle_bytes().is_empty());
        assert_eq!(engine.scene_spec_json(), scene_spec_json);
    }

    #[test]
    fn canonical_engine_playback_controls_keep_resources_and_session_stable() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            )
            .unwrap();
        let (scene_spec_json, _) = canonical_scene_spec_json(&legacy, "controls");
        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 37).unwrap();
        let bundle = engine.resource_bundle_bytes().to_vec();

        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        engine.tick_delta_json(100.0).unwrap();
        engine.tick_delta_json(1_100.0).unwrap();
        assert_eq!(engine.time(), 1.0);

        engine.pause();
        assert!(!engine.is_playing());
        assert!(engine.tick_delta_json(5_100.0).unwrap().is_none());
        assert_eq!(engine.time(), 1.0);

        let rewind: RetainedExecutionDeltaEnvelope = serde_json::from_str(
            &engine
                .seek_delta_json(0.25)
                .unwrap()
                .expect("rewind snapshot"),
        )
        .unwrap();
        assert!(rewind.snapshot);
        assert_eq!(rewind.session, initial.session);
        assert_eq!(rewind.time, 0.25);
        assert_eq!(engine.resource_bundle_bytes(), bundle);
        assert!(engine.tick_delta_json(8_100.0).unwrap().is_none());
        assert_eq!(engine.time(), 0.25);

        engine.resume();
        assert!(engine.is_playing());
        assert!(engine.tick_delta_json(8_100.0).unwrap().is_none());
        engine.tick_delta_json(8_600.0).unwrap();
        assert_eq!(engine.time(), 0.75);
    }

    #[test]
    fn canonical_engine_selects_family_execution_and_preserves_tracks() {
        let (scene_spec, text_id, circle_id) = family_scene_spec();
        let scene_spec_json = scene_spec.to_json().unwrap();
        let mut engine = CanonicalRetainedEnginePlayer::new(scene_spec, 4.0, 51).unwrap();
        assert_eq!(engine.scene_spec_json(), scene_spec_json);

        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();
        let initial_json = engine.initial_delta_json().unwrap();
        let initial: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial_json).unwrap();
        assert!(initial.retained.snapshot);
        assert_eq!(initial.family_plans.len(), 1);
        assert!(!initial_json.contains("glyph"));
        let (outcome, changes) = mirror.apply_json(&initial_json).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());

        let midpoint_json = engine
            .seek_delta_json(2.0)
            .unwrap()
            .expect("family midpoint delta");
        let midpoint: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&midpoint_json).unwrap();
        assert!(!midpoint.retained.snapshot);
        assert!(midpoint.family_plans.is_empty());
        mirror.apply_json(&midpoint_json).unwrap();
        assert_family_midpoint(&mirror, text_id, circle_id);
    }

    #[test]
    fn canonical_family_direct_seek_matches_forward_state() {
        let (scene_spec, text_id, circle_id) = family_scene_spec();

        let mut forward = CanonicalRetainedEnginePlayer::new(scene_spec.clone(), 4.0, 61).unwrap();
        let mut forward_mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(forward.resource_bundle_bytes())
                .unwrap();
        forward_mirror
            .apply_json(&forward.initial_delta_json().unwrap())
            .unwrap();
        forward_mirror
            .apply_json(
                &forward
                    .seek_delta_json(2.0)
                    .unwrap()
                    .expect("forward midpoint delta"),
            )
            .unwrap();

        let mut direct = CanonicalRetainedEnginePlayer::new(scene_spec, 4.0, 62).unwrap();
        let mut direct_mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(direct.resource_bundle_bytes())
                .unwrap();
        let direct_json = direct
            .seek_delta_json(2.0)
            .unwrap()
            .expect("direct midpoint snapshot");
        let direct_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&direct_json).unwrap();
        assert!(direct_delta.retained.snapshot);
        direct_mirror.apply_json(&direct_json).unwrap();

        assert_family_midpoint(&forward_mirror, text_id, circle_id);
        assert_family_midpoint(&direct_mirror, text_id, circle_id);
        let forward_frame = forward_mirror.frame().unwrap();
        let direct_frame = direct_mirror.frame().unwrap();
        assert_eq!(
            crate::determinism::normalized_frame_value(forward_frame),
            crate::determinism::normalized_frame_value(direct_frame),
        );
        for (forward_object, direct_object) in
            forward_frame.objects.iter().zip(&direct_frame.objects)
        {
            assert_eq!(forward_object.text_bounds, direct_object.text_bounds);
            if let (Some(forward_text), Some(direct_text)) =
                (forward_object.text(), direct_object.text())
            {
                assert_ne!(forward_text.arena, direct_text.arena);
                assert_eq!(
                    forward_mirror.resources().texts().get(forward_text),
                    direct_mirror.resources().texts().get(direct_text),
                );
            }
        }
    }

    #[test]
    fn canonical_engine_runs_sequential_family_requests_with_exact_plan_identity() {
        let (mut scene_spec, text_id, circle_id) = family_scene_spec();
        append_family_request(&mut scene_spec, 3.0, 1.0, FamilyAnimationMode::Reveal);
        let mut engine = CanonicalRetainedEnginePlayer::new(scene_spec, 5.0, 71).unwrap();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();

        let initial = engine.initial_delta_json().unwrap();
        let initial_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial).unwrap();
        assert_eq!(initial_delta.family_plans.len(), 2);
        mirror.apply_json(&initial).unwrap();
        assert_eq!(mirror.family_plans().len(), 2);

        let first = engine
            .seek_delta_json(2.0)
            .unwrap()
            .expect("first family request state");
        mirror.apply_json(&first).unwrap();
        let retained = mirror.frame().unwrap();
        let text_index = retained
            .objects
            .iter()
            .position(|object| object.id == text_id)
            .unwrap();
        let circle_index = retained
            .objects
            .iter()
            .position(|object| object.id == circle_id)
            .unwrap();
        let first_frame = mirror.planned_family_frame().unwrap().unwrap();
        assert_eq!(first_frame.family_plan_index(text_index), Some(0));
        assert_eq!(first_frame.family_plan_index(circle_index), Some(0));

        let second = engine
            .seek_delta_json(3.5)
            .unwrap()
            .expect("second family request state");
        mirror.apply_json(&second).unwrap();
        let second_frame = mirror.planned_family_frame().unwrap().unwrap();
        assert_eq!(second_frame.family_plan_index(text_index), Some(1));
        assert_eq!(second_frame.family_plan_index(circle_index), Some(1));
    }

    #[test]
    fn canonical_engine_rejects_overlapping_family_ownership_on_same_object() {
        let (mut scene_spec, _, _) = family_scene_spec();
        append_family_request(&mut scene_spec, 2.5, 1.0, FamilyAnimationMode::Reveal);
        let error = CanonicalRetainedEnginePlayer::new(scene_spec, 4.0, 72).unwrap_err();
        assert!(matches!(
            error,
            CanonicalRetainedEnginePlayerError::FamilyPlayer(
                RetainedFamilyExecutionPlayerError::Runtime(
                    noon_runtime::RetainedFamilyPlanSetRuntimeError::OverlappingAnimations { .. }
                )
            )
        ));
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
