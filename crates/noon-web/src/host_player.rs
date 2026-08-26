use noon_core::{
    HostCallbackId, HostCallbackRegistry, HostCallbackRegistryError, HostCallbackSlot,
    MutationTransaction, ObjectId, SignalId,
};
use noon_ir::{decode_patch_batch, decode_timed_semantic_scene, IrError, TimedSemanticIrError};
use noon_runtime::{
    HostCallbackAttachError, HostCommitError, HostDrivenScene, TimedSceneInstance,
    TimedSceneRuntimeError,
};
use serde_json::{json, Value};

#[derive(Debug)]
pub enum HostPlayerError {
    Ir(IrError),
    TimedIr(TimedSemanticIrError),
    Attach(HostCallbackAttachError),
    Registry(HostCallbackRegistryError),
    Runtime(TimedSceneRuntimeError),
    Commit(HostCommitError),
    CallbackJson(String),
    Sequence { expected: u64, actual: u64 },
    SequenceExhausted,
}

impl std::fmt::Display for HostPlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => error.fmt(formatter),
            Self::TimedIr(error) => error.fmt(formatter),
            Self::Attach(error) => error.fmt(formatter),
            Self::Registry(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
            Self::Commit(error) => error.fmt(formatter),
            Self::CallbackJson(message) => formatter.write_str(message),
            Self::Sequence { expected, actual } => {
                write!(
                    formatter,
                    "expected callback patch sequence {expected}, got {actual}"
                )
            }
            Self::SequenceExhausted => {
                formatter.write_str("callback patch sequence space exhausted")
            }
        }
    }
}

impl std::error::Error for HostPlayerError {}

impl From<IrError> for HostPlayerError {
    fn from(value: IrError) -> Self {
        Self::Ir(value)
    }
}

impl From<TimedSemanticIrError> for HostPlayerError {
    fn from(value: TimedSemanticIrError) -> Self {
        Self::TimedIr(value)
    }
}

impl From<HostCallbackAttachError> for HostPlayerError {
    fn from(value: HostCallbackAttachError) -> Self {
        Self::Attach(value)
    }
}

impl From<HostCallbackRegistryError> for HostPlayerError {
    fn from(value: HostCallbackRegistryError) -> Self {
        Self::Registry(value)
    }
}

impl From<TimedSceneRuntimeError> for HostPlayerError {
    fn from(value: TimedSceneRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<HostCommitError> for HostPlayerError {
    fn from(value: HostCommitError) -> Self {
        Self::Commit(value)
    }
}

/// Browser-facing host-dynamic runtime using Noon's shared callback protocol.
///
/// Static and native-reactive scenes should keep using their existing fast-path
/// players. This player exists only when a host language owns arbitrary callback
/// code and therefore needs a coherent read phase followed by one atomic mutation
/// commit.
#[derive(Clone, Debug)]
pub struct HostScenePlayer {
    driven: HostDrivenScene,
    signal_ids: Vec<SignalId>,
    next_sequence: u64,
}

impl HostScenePlayer {
    pub fn from_json(scene_json: &str, callback_slots_json: &str) -> Result<Self, HostPlayerError> {
        let scene = decode_timed_semantic_scene(scene_json)?;
        let signal_ids = scene
            .semantic()
            .reactive()
            .signals()
            .iter()
            .map(|signal| signal.id)
            .collect();
        let registry = decode_callback_registry(callback_slots_json)?;
        let instance = TimedSceneInstance::from_timed(&scene)?;
        let driven = HostDrivenScene::from_timed(instance, &registry)?;
        Ok(Self {
            driven,
            signal_ids,
            next_sequence: 0,
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<(), HostPlayerError> {
        self.driven.seek(time)?;
        Ok(())
    }

    pub fn advance_to(&mut self, time: f64) -> Result<(), HostPlayerError> {
        self.driven.advance_to(time)?;
        Ok(())
    }

    pub fn callback_frame_json(&mut self) -> Result<String, HostPlayerError> {
        let frame = self.driven.callback_frame();
        let objects = frame
            .objects
            .into_iter()
            .map(|object| {
                json!({
                    "object": object.object.get(),
                    "transform": object.transform,
                    "style": object.style,
                    "presence": object.presence,
                    "appearance": object.appearance,
                    "reveal": object.reveal,
                    "morph": object.morph,
                })
            })
            .collect::<Vec<_>>();
        let signals = self
            .signal_ids
            .iter()
            .filter_map(|signal| {
                self.driven.reactive_value(*signal).map(|value| {
                    json!({
                        "signal": signal.get(),
                        "value": value,
                    })
                })
            })
            .collect::<Vec<_>>();
        let invocations = frame
            .invocations
            .into_iter()
            .map(|invocation| {
                json!({
                    "callback": invocation.callback.get(),
                    "object_indices": invocation.object_indices,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&json!({
            "time": frame.time,
            "delta_time": frame.delta_time,
            "objects": objects,
            "signals": signals,
            "invocations": invocations,
        }))
        .map_err(|error| HostPlayerError::CallbackJson(error.to_string()))
    }

    pub fn commit_patch_batch_json(&mut self, json: &str) -> Result<(), HostPlayerError> {
        let batch = decode_patch_batch(json)?;
        if batch.sequence != self.next_sequence {
            return Err(HostPlayerError::Sequence {
                expected: self.next_sequence,
                actual: batch.sequence,
            });
        }
        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(HostPlayerError::SequenceExhausted)?;
        let transaction = MutationTransaction::from_mutations(batch.patches);
        self.driven.commit(&transaction)?;
        self.next_sequence = next_sequence;
        Ok(())
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn time(&self) -> f64 {
        self.driven.scene().frame().time
    }
}

fn decode_callback_registry(json_text: &str) -> Result<HostCallbackRegistry, HostPlayerError> {
    let value: Value = serde_json::from_str(json_text)
        .map_err(|error| HostPlayerError::CallbackJson(error.to_string()))?;
    let slots = value
        .as_array()
        .ok_or_else(|| HostPlayerError::CallbackJson("callback slots must be an array".into()))?;
    let mut decoded = Vec::with_capacity(slots.len());
    for slot in slots {
        let record = slot.as_object().ok_or_else(|| {
            HostPlayerError::CallbackJson("callback slot must be an object".into())
        })?;
        let id = record.get("id").and_then(Value::as_u64).ok_or_else(|| {
            HostPlayerError::CallbackJson("callback slot id must be an integer".into())
        })?;
        let objects = record
            .get("objects")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                HostPlayerError::CallbackJson("callback slot objects must be an array".into())
            })?
            .iter()
            .map(|value| {
                value.as_u64().map(ObjectId::new).ok_or_else(|| {
                    HostPlayerError::CallbackJson("callback object id must be an integer".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        decoded.push(HostCallbackSlot {
            id: HostCallbackId::new(id),
            objects,
        });
    }
    Ok(HostCallbackRegistry::from_slots(decoded)?)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::HostScenePlayer;

    #[wasm_bindgen(js_name = HostScenePlayer)]
    pub struct WasmHostScenePlayer {
        inner: HostScenePlayer,
    }

    #[wasm_bindgen(js_class = HostScenePlayer)]
    impl WasmHostScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(scene_json: &str, callback_slots_json: &str) -> Result<Self, JsValue> {
            Ok(Self {
                inner: HostScenePlayer::from_json(scene_json, callback_slots_json)
                    .map_err(js_error)?,
            })
        }

        pub fn seek(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner.seek(time).map_err(js_error)
        }

        #[wasm_bindgen(js_name = advanceTo)]
        pub fn advance_to(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner.advance_to(time).map_err(js_error)
        }

        #[wasm_bindgen(js_name = callbackFrameJson)]
        pub fn callback_frame_json(&mut self) -> Result<String, JsValue> {
            self.inner.callback_frame_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = commitPatchBatch)]
        pub fn commit_patch_batch(&mut self, json: &str) -> Result<(), JsValue> {
            self.inner.commit_patch_batch_json(json).map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextSequence)]
        pub fn next_sequence(&self) -> u64 {
            self.inner.next_sequence()
        }

        pub fn time(&self) -> f64 {
            self.inner.time()
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, SceneDefinition};
    use noon_ir::encode_scene;

    use super::*;

    #[test]
    fn callback_frame_and_atomic_patch_batch_round_trip() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::circle(0.5));
        let scene_json = encode_scene(&definition).unwrap();
        let slots = format!(
            r#"[{{"id":0,"objects":[{},{}]}}]"#,
            first.get(),
            second.get()
        );
        let mut player = HostScenePlayer::from_json(&scene_json, &slots).unwrap();
        player.advance_to(0.25).unwrap();
        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();
        assert_eq!(frame["time"], 0.25);
        assert_eq!(frame["delta_time"], 0.25);
        assert_eq!(frame["objects"].as_array().unwrap().len(), 2);
        assert_eq!(frame["invocations"][0]["object_indices"], json!([0, 1]));

        let batch = format!(
            r#"{{"version":1,"sequence":0,"patches":[{{"set_transform":{{"object":{},"transform":{{"translation":{{"x":2.0,"y":-1.0}},"rotation":0.0,"scale":{{"x":1.0,"y":1.0}}}}}}}}]}}"#,
            second.get()
        );
        player.commit_patch_batch_json(&batch).unwrap();
        assert_eq!(player.next_sequence(), 1);
        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();
        assert_eq!(frame["objects"][1]["transform"]["translation"]["x"], 2.0);
        assert_eq!(frame["objects"][1]["transform"]["translation"]["y"], -1.0);
    }

    #[test]
    fn callback_frame_reports_timeline_evaluated_signal_values() {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let tracker = semantic.add_input(0.0_f32);
        let mut timeline = noon_core::SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                0.0,
                10.0,
                noon_core::TrackTiming::new(0.0, 2.0, noon_core::RateFunction::Linear),
            )
            .unwrap();
        let scene = noon_core::TimedSemanticScene::from_parts(semantic, timeline).unwrap();
        let scene_json = noon_ir::encode_timed_semantic_scene(&scene).unwrap();
        let slots = format!(r#"[{{"id":0,"objects":[{}]}}]"#, object.get());
        let mut player = HostScenePlayer::from_json(&scene_json, &slots).unwrap();
        player.advance_to(0.5).unwrap();
        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();
        assert_eq!(frame["signals"][0]["signal"], tracker.get());
        assert_eq!(frame["signals"][0]["value"]["scalar"], 2.5);
    }

    #[test]
    fn callback_patch_sequence_is_checked() {
        let mut definition = SceneDefinition::new();
        definition.add(GeometryRef::circle(1.0));
        let scene_json = encode_scene(&definition).unwrap();
        let mut player = HostScenePlayer::from_json(&scene_json, "[]").unwrap();
        let batch = r#"{"version":1,"sequence":4,"patches":[]}"#;
        assert!(matches!(
            player.commit_patch_batch_json(batch),
            Err(HostPlayerError::Sequence {
                expected: 0,
                actual: 4
            })
        ));
    }
}
