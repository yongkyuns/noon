use std::sync::Arc;

use noon_core::VideoFrameSource;
use wasm_bindgen::prelude::*;

use super::{typed_js_error, CanonicalAuthoringSceneContext};
use crate::{
    authoring_error::AuthoringFailure, video_asset::BrowserVideoSource, WasmAudioSource,
    WasmAuthoringMobjectHandle,
};

/// Immutable browser media prepared by the codec host. The host retains the
/// compressed bytes, while the scene retains the index and optional mixed PCM.
#[wasm_bindgen]
pub struct WasmVideoSource {
    source: Arc<BrowserVideoSource>,
    audio: Option<(noon::AudioResource, f64)>,
}

#[wasm_bindgen]
impl WasmVideoSource {
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: u32,
        width: u32,
        height: u32,
        timestamps: Vec<f64>,
        duration: f64,
        soundtrack: Option<WasmAudioSource>,
        audio_offset: f64,
    ) -> Result<WasmVideoSource, JsValue> {
        let invalid = |message| {
            typed_js_error(AuthoringFailure::new(
                "invalid_input",
                "media.invalid_video",
                message,
            ))
        };
        if !audio_offset.is_finite() {
            return Err(invalid("video soundtrack offset must be finite".into()));
        }
        let source =
            BrowserVideoSource::new(id, width, height, timestamps, duration).map_err(invalid)?;
        Ok(Self {
            source: Arc::new(source),
            audio: soundtrack.map(|audio| (audio.resource, audio_offset)),
        })
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    pub fn video(
        &mut self,
        asset: &WasmVideoSource,
        start_time: f64,
        source_start: f64,
        source_end: Option<f64>,
        height: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.inner
            .require_pre_execution_media_authoring()
            .map_err(typed_js_error)?;
        let end = source_end.unwrap_or(asset.source.frame_index().duration());
        noon::integration::place_video(
            &mut self.inner.scene,
            asset.source.clone(),
            asset.audio.as_ref().map(|(audio, offset)| (audio, *offset)),
            start_time,
            source_start..end,
            height,
        )
        .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
        .map_err(typed_js_error)
    }
}
