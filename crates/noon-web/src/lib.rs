//! Browser-facing persistent Noon runtime.
//!
//! The core player is ordinary Rust so its state transitions are testable on
//! native CI. A thin wasm-bindgen wrapper exposes the same semantics to JavaScript.

#![forbid(unsafe_code)]

use noon_compile::{CompileError, CompilePatchError, CompiledScene};
use noon_core::{PatchError, SceneDefinition};
use noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};
use noon_runtime::{EvaluationError, FrameState, SceneInstance};

#[derive(Debug)]
pub enum PlayerError {
    Ir(IrError),
    Compile(CompileError),
    Patch(PatchError),
    CompilePatch(CompilePatchError),
    Evaluation(EvaluationError),
    Sequence { expected: u64, actual: u64 },
    SequenceExhausted,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => write!(formatter, "{error}"),
            Self::Compile(error) => write!(formatter, "scene compilation failed: {error}"),
            Self::Patch(error) => write!(formatter, "scene patch failed: {error}"),
            Self::CompilePatch(error) => write!(formatter, "runtime patch failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "scene evaluation failed: {error}"),
            Self::Sequence { expected, actual } => {
                write!(formatter, "expected patch sequence {expected}, got {actual}")
            }
            Self::SequenceExhausted => formatter.write_str("patch sequence space exhausted"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl From<IrError> for PlayerError {
    fn from(value: IrError) -> Self {
        Self::Ir(value)
    }
}

impl From<CompileError> for PlayerError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<PatchError> for PlayerError {
    fn from(value: PatchError) -> Self {
        Self::Patch(value)
    }
}

impl From<CompilePatchError> for PlayerError {
    fn from(value: CompilePatchError) -> Self {
        Self::CompilePatch(value)
    }
}

impl From<EvaluationError> for PlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SceneInstance,
    next_sequence: u64,
}

impl ScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        Ok(Self {
            definition,
            instance: SceneInstance::new(compiled),
            next_sequence: 0,
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.seek(time)?)
    }

    pub fn apply_patch_batch_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let batch = decode_patch_batch(json)?;
        if batch.sequence != self.next_sequence {
            return Err(PlayerError::Sequence {
                expected: self.next_sequence,
                actual: batch.sequence,
            });
        }

        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(PlayerError::SequenceExhausted)?;
        let mut definition = self.definition.clone();
        let mut instance = self.instance.clone();

        for patch in &batch.patches {
            definition.apply_patch(patch.clone())?;
            instance.apply_patch(patch)?;
        }

        self.definition = definition;
        self.instance = instance;
        self.next_sequence = next_sequence;
        Ok(self.instance.frame())
    }

    pub fn scene_json(&self) -> Result<String, PlayerError> {
        Ok(encode_scene(&self.definition)?)
    }

    pub fn frame(&self) -> &FrameState {
        self.instance.frame()
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn object_count(&self) -> usize {
        self.instance.frame().objects.len()
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::ScenePlayer;

    #[wasm_bindgen(js_name = ScenePlayer)]
    pub struct WasmScenePlayer {
        inner: ScenePlayer,
    }

    #[wasm_bindgen(js_class = ScenePlayer)]
    impl WasmScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(scene_json: &str) -> Result<WasmScenePlayer, JsValue> {
            Ok(Self {
                inner: ScenePlayer::from_scene_json(scene_json).map_err(js_error)?,
            })
        }

        pub fn seek(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner.seek(time).map_err(js_error)?;
            Ok(())
        }

        pub fn apply_patch_batch(&mut self, json: &str) -> Result<(), JsValue> {
            self.inner.apply_patch_batch_json(json).map_err(js_error)?;
            Ok(())
        }

        pub fn time(&self) -> f64 {
            self.inner.frame().time
        }

        pub fn object_count(&self) -> usize {
            self.inner.object_count()
        }

        pub fn next_sequence(&self) -> u64 {
            self.inner.next_sequence()
        }

        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, ObjectId, ScenePatch, Style, Transform2D, Vec2,
    };
    use noon_ir::{encode_patch_batch, encode_scene, PatchBatch};

    use super::*;

    fn player() -> ScenePlayer {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let json = encode_scene(&scene).expect("scene must serialize");
        ScenePlayer::from_scene_json(&json).expect("player must load")
    }

    #[test]
    fn player_loads_and_seeks_without_reexecuting_frontend_code() {
        let mut player = player();
        player.seek(3.25).expect("seek must succeed");
        assert_eq!(player.frame().time, 3.25);
        assert_eq!(player.object_count(), 1);
    }

    #[test]
    fn ordered_patch_batch_preserves_playhead_and_advances_sequence() {
        let mut player = player();
        player.seek(2.0).expect("seek must succeed");
        let batch = PatchBatch::new(
            0,
            vec![ScenePatch::SetTransform {
                object: ObjectId::new(0),
                transform: Transform2D {
                    translation: Vec2::new(5.0, -2.0),
                    ..Transform2D::IDENTITY
                },
            }],
        );
        let json = encode_patch_batch(&batch).expect("batch must serialize");

        player
            .apply_patch_batch_json(&json)
            .expect("patch batch must apply");

        assert_eq!(player.frame().time, 2.0);
        assert_eq!(player.frame().objects[0].transform.translation, Vec2::new(5.0, -2.0));
        assert_eq!(player.next_sequence(), 1);
    }

    #[test]
    fn patch_batch_is_transactional_when_later_patch_fails() {
        let mut player = player();
        let before_scene = player.scene_json().expect("scene must serialize");
        let before_frame = player.frame().clone();
        let batch = PatchBatch::new(
            0,
            vec![
                ScenePatch::SetStyle {
                    object: ObjectId::new(0),
                    style: Style {
                        opacity: 0.25,
                        ..Style::default()
                    },
                },
                ScenePatch::SetTransform {
                    object: ObjectId::new(999),
                    transform: Transform2D::IDENTITY,
                },
            ],
        );
        let json = encode_patch_batch(&batch).expect("batch must serialize");

        assert!(player.apply_patch_batch_json(&json).is_err());
        assert_eq!(player.scene_json().expect("scene must serialize"), before_scene);
        assert_eq!(player.frame(), &before_frame);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn out_of_order_patch_batch_is_rejected_without_mutation() {
        let mut player = player();
        let before = player.scene_json().expect("scene must serialize");
        let json = encode_patch_batch(&PatchBatch::new(3, Vec::new()))
            .expect("batch must serialize");

        assert!(matches!(
            player.apply_patch_batch_json(&json),
            Err(PlayerError::Sequence {
                expected: 0,
                actual: 3
            })
        ));
        assert_eq!(player.scene_json().expect("scene must serialize"), before);
        assert_eq!(player.next_sequence(), 0);
    }
}
