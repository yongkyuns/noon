use noon::{MathTypst, RetainedScene, TextAuthoringError, Typst};
use noon_compile::{RetainedCompileError, RetainedCompiledScene};
use noon_core::Camera2DState;
use noon_runtime::{EvaluationError, RetainedSceneInstance};

use crate::{
    ClockError, PlaybackClock, RetainedAuthoringDocument, RetainedExecutionDeltaEncoder,
    RetainedExecutionTransportError, RetainedResourceBundle, RetainedResourceTransportError,
};

/// Engine-side owner for source-authored retained Typst/MathTypst scenes.
///
/// The authoring document stays source-level across the Python/browser boundary.
/// Construction compiles each source exactly once into ordinary retained text,
/// geometry and font arenas, substitutes the author-provided semantic `ObjectId`s
/// before retained scene compilation, captures one dependency-closed resource
/// bundle, then emits only compact retained execution deltas while playing.
#[derive(Debug)]
pub struct RetainedEngineScenePlayer {
    runtime: RetainedSceneInstance,
    clock: PlaybackClock,
    encoder: RetainedExecutionDeltaEncoder,
    resource_bundle: Vec<u8>,
    authoring_json: String,
}

impl RetainedEngineScenePlayer {
    pub fn new(
        authoring_json: &str,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, RetainedEngineError> {
        let document = RetainedAuthoringDocument::from_json(authoring_json)
            .map_err(RetainedEngineError::AuthoringDocument)?;
        let authoring_json = document
            .to_json()
            .map_err(RetainedEngineError::AuthoringDocument)?;

        let mut authored = document.objects;
        authored.sort_by_key(|object| object.order);

        let mut scene = RetainedScene::new();
        let mut objects = Vec::with_capacity(authored.len());
        for object in authored {
            let spec = object.text;
            if spec.math {
                scene.add_math_typst(
                    MathTypst::new(spec.source)
                        .with_font_size(spec.font_size)
                        .color(spec.color)
                        .set_opacity(spec.opacity)
                        .move_to(spec.transform.translation)
                        .scale_xy(spec.transform.scale)
                        .rotate(spec.transform.rotation),
                )?;
            } else {
                scene.add_typst(
                    Typst::new(spec.source)
                        .with_font_size(spec.font_size)
                        .color(spec.color)
                        .set_opacity(spec.opacity)
                        .move_to(spec.transform.translation)
                        .scale_xy(spec.transform.scale)
                        .rotate(spec.transform.rotation),
                )?;
            }

            let mut retained = scene
                .objects()
                .last()
                .expect("retained Typst insertion must append one object")
                .clone();
            retained.id = object.object;
            objects.push(retained);
        }

        let text_handles = objects.iter().filter_map(|object| object.content.text());
        let bundle = RetainedResourceBundle::capture(
            text_handles,
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )?;
        let resource_bundle = bundle.encode_binary()?;

        let compiled = RetainedCompiledScene::compile(&objects, scene.tracks())?;
        let runtime = RetainedSceneInstance::new(compiled);

        Ok(Self {
            runtime,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            encoder: RetainedExecutionDeltaEncoder::new(session),
            resource_bundle,
            authoring_json,
        })
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        &self.resource_bundle
    }

    pub fn authoring_json(&self) -> &str {
        &self.authoring_json
    }

    pub fn initial_delta_json(&mut self) -> Result<String, RetainedEngineError> {
        let delta = self
            .encoder
            .encode_snapshot(self.runtime.frame(), Camera2DState::default())?;
        // `RetainedSceneInstance` starts fully dirty. The explicit snapshot above
        // consumes that baseline so the first animation tick can remain incremental.
        self.runtime.take_frame_changes();
        Ok(serde_json::to_string(&delta)?)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, RetainedEngineError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.runtime.advance_to(scene_time)?;
        let changes = self.runtime.take_frame_changes();
        self.encoder
            .encode_incremental(self.runtime.frame(), &changes, Camera2DState::default())?
            .map(|delta| serde_json::to_string(&delta).map_err(RetainedEngineError::from))
            .transpose()
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), RetainedEngineError> {
        self.clock.set_loop_duration(duration)?;
        Ok(())
    }

    pub fn time(&self) -> f64 {
        self.runtime.frame().time
    }
}

#[derive(Debug)]
pub enum RetainedEngineError {
    AuthoringDocument(String),
    TextAuthoring(TextAuthoringError),
    Compile(RetainedCompileError),
    Resource(RetainedResourceTransportError),
    Transport(RetainedExecutionTransportError),
    Runtime(EvaluationError),
    Clock(ClockError),
    Json(serde_json::Error),
}

impl std::fmt::Display for RetainedEngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthoringDocument(error) => formatter.write_str(error),
            Self::TextAuthoring(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
            Self::Clock(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedEngineError {}

impl From<TextAuthoringError> for RetainedEngineError {
    fn from(value: TextAuthoringError) -> Self {
        Self::TextAuthoring(value)
    }
}

impl From<RetainedCompileError> for RetainedEngineError {
    fn from(value: RetainedCompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<RetainedResourceTransportError> for RetainedEngineError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

impl From<RetainedExecutionTransportError> for RetainedEngineError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<EvaluationError> for RetainedEngineError {
    fn from(value: EvaluationError) -> Self {
        Self::Runtime(value)
    }
}

impl From<ClockError> for RetainedEngineError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<serde_json::Error> for RetainedEngineError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::RetainedEngineScenePlayer;

    #[wasm_bindgen(js_name = RetainedEngineScenePlayer)]
    pub struct WasmRetainedEngineScenePlayer {
        inner: RetainedEngineScenePlayer,
    }

    #[wasm_bindgen(js_class = RetainedEngineScenePlayer)]
    impl WasmRetainedEngineScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            authoring_json: &str,
            loop_duration_seconds: f64,
            session: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                inner: RetainedEngineScenePlayer::new(
                    authoring_json,
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

        #[wasm_bindgen(js_name = authoringJson)]
        pub fn authoring_json(&self) -> String {
            self.inner.authoring_json().to_owned()
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
    use noon_core::{ObjectId, TextSourceKind, Transform2D};

    use super::*;
    use crate::{
        RetainedAuthoringTextObject, RetainedTypstAuthoringSpec, TransportObjectContent,
        TransportTextResourceHandle,
    };

    fn document_json() -> String {
        RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(41),
                order: 1,
                text: RetainedTypstAuthoringSpec::new("frac(x, 2)", true, 72.0).unwrap(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(7),
                order: 0,
                text: RetainedTypstAuthoringSpec {
                    source: "*Hello* from _Typst!_".to_owned(),
                    math: false,
                    font_size: 64.0,
                    transform: Transform2D::default(),
                    color: noon_core::YELLOW,
                    opacity: 0.75,
                },
            },
        ])
        .unwrap()
        .to_json()
        .unwrap()
    }

    #[test]
    fn source_document_compiles_once_and_preserves_explicit_identity_and_order() {
        let mut player = RetainedEngineScenePlayer::new(&document_json(), 4.0, 23).unwrap();
        let initial: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        assert!(initial.snapshot);
        assert_eq!(initial.objects.len(), 2);
        assert_eq!(initial.objects[0].object, ObjectId::new(7));
        assert_eq!(initial.objects[1].object, ObjectId::new(41));
        assert_eq!(initial.objects[0].order, 0);
        assert_eq!(initial.objects[1].order, 1);
        assert!(initial
            .objects
            .iter()
            .all(|object| matches!(object.content, TransportObjectContent::Text { .. })));

        let bundle = RetainedResourceBundle::decode_binary(player.resource_bundle_bytes()).unwrap();
        assert_eq!(bundle.text_count(), 2);
        assert!(bundle.font_count() >= 1);
        let installed = bundle.install().unwrap();
        for object in initial.objects {
            let TransportObjectContent::Text { text } = object.content else {
                panic!("retained source object must remain text-backed");
            };
            let local = installed.resolve_text_handle(text).unwrap();
            let resource = installed.texts().get(local).unwrap();
            assert!(matches!(
                resource.kind,
                TextSourceKind::Typst | TextSourceKind::MathTypst
            ));
        }
    }

    #[test]
    fn static_retained_scene_does_not_emit_per_frame_resource_or_object_payloads() {
        let mut player = RetainedEngineScenePlayer::new(&document_json(), 4.0, 5).unwrap();
        player.initial_delta_json().unwrap();
        assert!(player.tick_delta_json(0.0).unwrap().is_none());
        assert!(player.tick_delta_json(16.0).unwrap().is_none());
        assert!(!player.resource_bundle_bytes().is_empty());
    }

    #[test]
    fn bundle_wire_handles_are_distinct_from_renderer_local_handles_by_contract() {
        let mut player = RetainedEngineScenePlayer::new(&document_json(), 4.0, 2).unwrap();
        let initial: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        let installed = RetainedResourceBundle::decode_binary(player.resource_bundle_bytes())
            .unwrap()
            .install()
            .unwrap();
        for object in initial.objects {
            let TransportObjectContent::Text { text } = object.content else {
                continue;
            };
            let wire = TransportTextResourceHandle {
                id: text.id,
                version: text.version,
            };
            assert!(installed.resolve_text_handle(wire).is_some());
        }
    }
}
