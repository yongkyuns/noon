//! Thin finite-media calls on the existing canonical authoring context.

use super::{CanonicalAuthoringSceneContext, WasmValueTrackerHandle};
use crate::{
    authoring_error::{js_error, AuthoringFailure},
    WasmAudioClipHandle, WasmAudioClipOptions, WasmAudioSource, WasmAuthoringMobjectHandle,
};
use noon::{PlaybackControl, RateFunction};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    pub fn audio(
        &mut self,
        source: &WasmAudioSource,
        options: &WasmAudioClipOptions,
    ) -> Result<WasmAudioClipHandle, JsValue> {
        self.inner
            .audio(&source.resource, options.options)
            .map(|handle| WasmAudioClipHandle { handle })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = declareValue)]
    pub fn declare_value(
        &mut self,
        tracker: &WasmValueTrackerHandle,
        target: f64,
        run_time: f64,
        rate_function: &str,
    ) -> Result<(), JsValue> {
        let rate = RateFunction::from_semantic_id(rate_function).ok_or_else(|| {
            js_error(AuthoringFailure::new(
                "invalid_input",
                "animation.invalid_rate_function",
                "unsupported scalar rate function",
            ))
        })?;
        let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
        self.inner
            .declare_value(tracker, target, run_time, rate)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = onClickPlayback)]
    pub fn on_click_playback(
        &mut self,
        target: &WasmAuthoringMobjectHandle,
        action: &str,
    ) -> Result<(), JsValue> {
        let action = match action {
            "toggle_playback" => PlaybackControl::TogglePlayback,
            "restart" => PlaybackControl::Restart,
            "previous_section" => PlaybackControl::PreviousSection,
            "next_section" => PlaybackControl::NextSection,
            "toggle_audio_muted" => PlaybackControl::ToggleAudioMuted,
            _ => {
                return Err(js_error(AuthoringFailure::new(
                    "invalid_input",
                    "authoring.playback_control",
                    "unknown playback control",
                )))
            }
        };
        self.inner
            .on_click_playback(target.semantic_mobject(), action)
            .map_err(js_error)
    }
}
