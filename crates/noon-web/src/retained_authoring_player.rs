use noon::RetainedScene;
use noon_core::{
    Camera2DState, FontResourceArena, GeometryResourceArena, ObjectId, TextResourceArena,
};
use noon_runtime::{EvaluationError, RetainedFrameState, RetainedSceneInstance};

use crate::{
    ClockError, MixedRetainedAuthoringError, MixedRetainedAuthoringScene, PlaybackClock,
    RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError,
    RetainedResourceBundle, RetainedResourceTransportError,
};

/// Deterministic execution owner for one mixed legacy-geometry + retained-text scene.
///
/// The authoring sidecar ends at construction. Runtime evaluation consumes one
/// [`RetainedSceneInstance`], while the resource arenas stay next to it so renderer
/// preparation can resolve text/font/vector resources without putting those payloads
/// on the Python or per-frame execution wire.
#[derive(Clone, Debug)]
pub struct RetainedAuthoringPlayer {
    scene: RetainedScene,
    runtime: RetainedSceneInstance,
    encoder: RetainedExecutionDeltaEncoder,
    resource_bundle: Vec<u8>,
    camera_object: Option<ObjectId>,
    snapshot_sent: bool,
}

impl RetainedAuthoringPlayer {
    pub fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        let mixed =
            MixedRetainedAuthoringScene::from_json(legacy_scene_json, retained_document_json)?;
        Self::new(mixed, session)
    }

    pub fn new(
        mixed: MixedRetainedAuthoringScene,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        let camera_object = mixed.camera_object();
        let compiled = mixed.compile()?;
        let scene = mixed.into_scene();
        let bundle = RetainedResourceBundle::capture(
            scene
                .objects()
                .iter()
                .filter_map(|object| object.content.text()),
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )?;
        let resource_bundle = bundle.encode_binary()?;
        let runtime = RetainedSceneInstance::new(compiled);
        Ok(Self {
            scene,
            runtime,
            encoder: RetainedExecutionDeltaEncoder::new(session),
            resource_bundle,
            camera_object,
            snapshot_sent: false,
        })
    }

    pub const fn scene(&self) -> &RetainedScene {
        &self.scene
    }

    pub fn frame(&self) -> &RetainedFrameState {
        self.runtime.frame()
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        &self.resource_bundle
    }

    pub const fn texts(&self) -> &TextResourceArena {
        self.scene.texts()
    }

    pub const fn geometries(&self) -> &GeometryResourceArena {
        self.scene.geometries()
    }

    pub const fn fonts(&self) -> &FontResourceArena {
        self.scene.fonts()
    }

    /// Evaluate one absolute scene time and encode the renderer-facing retained delta.
    ///
    /// The first call always emits a complete snapshot. Forward evaluation then emits
    /// only dirty objects. A backward seek invalidates the retained runtime frame and
    /// therefore naturally becomes a complete retained snapshot without changing the
    /// session or object/resource identities.
    pub fn evaluate_delta(
        &mut self,
        time: f64,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedAuthoringPlayerError> {
        self.runtime.evaluate(time)?;
        let camera = self.camera_state()?;
        let changes = self.runtime.take_frame_changes();
        if !self.snapshot_sent {
            let delta = self.encoder.encode_snapshot(self.runtime.frame(), camera)?;
            self.snapshot_sent = true;
            return Ok(Some(delta));
        }
        Ok(self
            .encoder
            .encode_incremental(self.runtime.frame(), &changes, camera)?)
    }

    fn camera_state(&self) -> Result<Camera2DState, RetainedAuthoringPlayerError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object = self
            .runtime
            .frame()
            .objects
            .iter()
            .find(|object| object.id == camera_object)
            .ok_or(RetainedAuthoringPlayerError::InvalidCameraObject(
                camera_object,
            ))?;
        let geometry =
            object
                .geometry()
                .ok_or(RetainedAuthoringPlayerError::InvalidCameraObject(
                    camera_object,
                ))?;
        Camera2DState::from_frame_object(geometry, object.transform).ok_or(
            RetainedAuthoringPlayerError::InvalidCameraObject(camera_object),
        )
    }
}

/// Clocked browser-engine facade for mixed Python authoring output.
///
/// Construction performs the only retained source/resource compilation. The engine
/// transfers `resource_bundle_bytes()` once, then sends only retained frame deltas.
#[derive(Debug)]
pub struct RetainedAuthoringEnginePlayer {
    player: RetainedAuthoringPlayer,
    clock: PlaybackClock,
    legacy_scene_json: String,
    retained_document_json: String,
}

impl RetainedAuthoringEnginePlayer {
    pub fn new(
        legacy_scene_json: &str,
        retained_document_json: &str,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        Ok(Self {
            player: RetainedAuthoringPlayer::from_json(
                legacy_scene_json,
                retained_document_json,
                session,
            )?,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            legacy_scene_json: legacy_scene_json.to_owned(),
            retained_document_json: retained_document_json.to_owned(),
        })
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        self.player.resource_bundle_bytes()
    }

    pub fn legacy_scene_json(&self) -> &str {
        &self.legacy_scene_json
    }

    pub fn retained_document_json(&self) -> &str {
        &self.retained_document_json
    }

    pub fn initial_delta_json(&mut self) -> Result<String, RetainedAuthoringPlayerError> {
        let delta = self
            .player
            .evaluate_delta(0.0)?
            .expect("first retained evaluation must emit a snapshot");
        Ok(serde_json::to_string(&delta)?)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, RetainedAuthoringPlayerError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.player
            .evaluate_delta(scene_time)?
            .map(|delta| serde_json::to_string(&delta).map_err(RetainedAuthoringPlayerError::from))
            .transpose()
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), RetainedAuthoringPlayerError> {
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
    ) -> Result<Option<String>, RetainedAuthoringPlayerError> {
        let mut clock = self.clock.clone();
        clock.seek(scene_time)?;
        let delta = self
            .player
            .evaluate_delta(scene_time)?
            .map(|delta| serde_json::to_string(&delta).map_err(RetainedAuthoringPlayerError::from))
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
pub enum RetainedAuthoringPlayerError {
    Authoring(MixedRetainedAuthoringError),
    Resource(RetainedResourceTransportError),
    Evaluation(EvaluationError),
    Transport(RetainedExecutionTransportError),
    Clock(ClockError),
    Json(serde_json::Error),
    InvalidCameraObject(ObjectId),
}

impl std::fmt::Display for RetainedAuthoringPlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::Clock(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
            Self::InvalidCameraObject(object) => write!(
                formatter,
                "retained camera object {} is missing or not a supported 2D frame",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedAuthoringPlayerError {}

impl From<MixedRetainedAuthoringError> for RetainedAuthoringPlayerError {
    fn from(value: MixedRetainedAuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<RetainedResourceTransportError> for RetainedAuthoringPlayerError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

impl From<EvaluationError> for RetainedAuthoringPlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<RetainedExecutionTransportError> for RetainedAuthoringPlayerError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<ClockError> for RetainedAuthoringPlayerError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<serde_json::Error> for RetainedAuthoringPlayerError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::RetainedAuthoringEnginePlayer;

    #[wasm_bindgen(js_name = MixedRetainedEngineScenePlayer)]
    pub struct WasmMixedRetainedEngineScenePlayer {
        inner: RetainedAuthoringEnginePlayer,
    }

    #[wasm_bindgen(js_class = MixedRetainedEngineScenePlayer)]
    impl WasmMixedRetainedEngineScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            legacy_scene_json: &str,
            retained_document_json: &str,
            loop_duration_seconds: f64,
            session: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                inner: RetainedAuthoringEnginePlayer::new(
                    legacy_scene_json,
                    retained_document_json,
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

        #[wasm_bindgen(js_name = legacySceneJson)]
        pub fn legacy_scene_json(&self) -> String {
            self.inner.legacy_scene_json().to_owned()
        }

        #[wasm_bindgen(js_name = retainedDocumentJson)]
        pub fn retained_document_json(&self) -> String {
            self.inner.retained_document_json().to_owned()
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
        GeometryRef, ObjectContentRef, Property, RateFunction, SceneDefinition, TextSourceKind,
        TrackTiming, TrackValues, Vec2,
    };

    use crate::{
        RetainedAuthoringDocument, RetainedAuthoringTextObject, RetainedTextAuthoringSpec,
        RetainedTrackAuthoringSpec, RetainedTypstAuthoringSpec, TransportObjectContent,
    };

    use super::*;

    fn text_document(
        source: &str,
        math: bool,
        order: u32,
        object: ObjectId,
    ) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order,
            text: RetainedTypstAuthoringSpec::new(source, math, 48.0).unwrap(),
        }])
        .unwrap()
    }

    fn native_text_document(
        source: &str,
        order: u32,
        object: ObjectId,
    ) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order,
            text: RetainedTextAuthoringSpec::native(
                source,
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap()
    }

    #[test]
    fn first_frame_is_one_geometry_text_geometry_snapshot_with_live_resources() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("*Hello*", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 17).unwrap();

        let delta = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(delta.snapshot);
        assert_eq!(delta.sequence, 0);
        assert_eq!(
            delta
                .objects
                .iter()
                .map(|object| object.object)
                .collect::<Vec<_>>(),
            vec![circle, text_id, square]
        );
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        let TransportObjectContent::Text { text } = delta.objects[1].content else {
            panic!("middle retained object must stay text-backed");
        };
        let retained_handle = player.scene().objects()[1].content.text().unwrap();
        assert_eq!(text.id, retained_handle.id.get());
        assert_eq!(text.version, retained_handle.version);
        assert_eq!(
            player.texts().get(retained_handle).unwrap().kind,
            TextSourceKind::Typst
        );
        assert!(!player.fonts().is_empty());
        let bundle = RetainedResourceBundle::decode_binary(player.resource_bundle_bytes()).unwrap();
        assert_eq!(bundle.text_count(), 1);
        assert!(bundle.font_count() >= 1);
    }

    #[test]
    fn retained_native_text_scale_track_evaluates_without_replacing_resource_identity() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let base_scale = noon::NATIVE_POINT_TO_SCENE_SCALE;
        let track = RetainedTrackAuthoringSpec::new(
            text_id,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );
        let mixed = MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            native_text_document("Shrink", 0, text_id),
            vec![track],
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 21).unwrap();
        let text_handle = player.scene().objects()[0].content.text().unwrap();

        let initial = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(initial.snapshot);
        let midpoint = player.evaluate_delta(0.5).unwrap().unwrap();
        assert!(!midpoint.snapshot);
        assert_eq!(midpoint.objects.len(), 1);
        assert_eq!(midpoint.objects[0].object, text_id);
        assert!((midpoint.objects[0].transform.scale.x - base_scale * 0.5).abs() < 1.0e-6);
        assert!((midpoint.objects[0].transform.scale.y - base_scale * 0.5).abs() < 1.0e-6);
        let TransportObjectContent::Text { text } = midpoint.objects[0].content else {
            panic!("scaled retained Text must stay text-backed");
        };
        assert_eq!(text.id, text_handle.id.get());
        assert_eq!(text.version, text_handle.version);

        let endpoint = player.evaluate_delta(1.0).unwrap().unwrap();
        assert_eq!(endpoint.objects.len(), 1);
        assert_eq!(endpoint.objects[0].transform.scale, Vec2::ZERO);
        assert_eq!(
            player.scene().objects()[0].content,
            ObjectContentRef::Text(text_handle)
        );
    }

    #[test]
    fn forward_evaluation_emits_only_dirty_geometry_and_keeps_text_identity_stable() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("stable", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 18).unwrap();
        let text_handle = player.scene().objects()[1].content.text().unwrap();

        player.evaluate_delta(0.0).unwrap().unwrap();
        let delta = player.evaluate_delta(0.5).unwrap().unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].object, circle);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        assert_eq!(
            player.scene().objects()[1].content,
            ObjectContentRef::Text(text_handle)
        );
    }

    #[test]
    fn backward_evaluation_reissues_snapshot_without_changing_text_resource_handle() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("seek", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 19).unwrap();
        let text_handle = player.scene().objects()[1].content.text().unwrap();

        player.evaluate_delta(0.0).unwrap().unwrap();
        player.evaluate_delta(0.75).unwrap().unwrap();
        let rewind = player.evaluate_delta(0.25).unwrap().unwrap();
        assert!(rewind.snapshot);
        assert_eq!(rewind.sequence, 2);
        let TransportObjectContent::Text { text } = rewind.objects[1].content else {
            panic!("rewind must preserve retained text content identity");
        };
        assert_eq!(text.id, text_handle.id.get());
        assert_eq!(text.version, text_handle.version);
    }

    #[test]
    fn retained_player_derives_camera_from_the_same_evaluated_object_stream() {
        let mut legacy = SceneDefinition::new();
        let camera = legacy.add(GeometryRef::rectangle(14.0, 8.0));
        assert!(legacy.set_camera_object(camera));
        legacy
            .animate_position(
                camera,
                Vec2::ZERO,
                Vec2::new(4.0, -2.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            RetainedAuthoringDocument::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 20).unwrap();

        let delta = player.evaluate_delta(0.5).unwrap().unwrap();
        assert_eq!(delta.camera.center, Vec2::new(2.0, -1.0));
        assert!((delta.camera.height - 8.0).abs() < 1.0e-6);
    }

    #[test]
    fn clocked_engine_player_emits_one_bundle_and_mixed_snapshot() {
        let mut legacy = SceneDefinition::new();
        legacy.add(GeometryRef::circle(0.25));
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = text_document("engine", false, 1, text_id);
        let legacy_json = noon_ir::encode_scene(&legacy).unwrap();
        let retained_json = retained.to_json().unwrap();
        let mut engine =
            RetainedAuthoringEnginePlayer::new(&legacy_json, &retained_json, 2.0, 31).unwrap();

        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        assert!(initial.snapshot);
        assert_eq!(initial.objects.len(), 2);
        assert!(!engine.resource_bundle_bytes().is_empty());
        assert_eq!(engine.legacy_scene_json(), legacy_json);
        assert_eq!(engine.retained_document_json(), retained_json);
    }

    #[test]
    fn retained_engine_playback_controls_keep_resources_and_session_stable() {
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
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = text_document("controls", false, 1, text_id);
        let legacy_json = noon_ir::encode_scene(&legacy).unwrap();
        let retained_json = retained.to_json().unwrap();
        let mut engine =
            RetainedAuthoringEnginePlayer::new(&legacy_json, &retained_json, 4.0, 37).unwrap();
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
}
