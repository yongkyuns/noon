//! Output observations from the selected historical reader and its Rust clock.
//! Resource preparation is silent; the endpoint supplies the picture barrier.

use super::PreparedExecutionPlayer;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl PreparedExecutionPlayer {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = hasAudio))]
    pub fn has_audio(&self) -> bool {
        self.playback.reader.has_audio()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioResourceBundleBytes))]
    pub fn audio_resource_bundle_bytes(&self) -> Result<Vec<u8>, String> {
        crate::worker_audio::encode_prepared(&self.playback.reader, self.encoder.session())
    }

    /// Observe the current reader only after the host's retained picture barrier.
    /// Sleeping waits use the existing Rust clock; source progress contributes
    /// only the adopted availability bound. The monotonically issued sequence
    /// lets Window discard a late anchor after a newer pause, seek or refill.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioAnchorJson))]
    pub fn audio_anchor_json(
        &mut self,
        sampled_epoch_ms: f64,
        wall_time_ms: f64,
    ) -> Result<String, String> {
        if !sampled_epoch_ms.is_finite() || sampled_epoch_ms < 0.0 {
            return Err("audio anchor requires a finite non-negative host timestamp".into());
        }
        let time = self.prepared_playback_time_at(wall_time_ms)?;
        let available_until = self.playback.reader.frontier();
        let sequence = self
            .audio_sequence
            .checked_add(1)
            .ok_or("prepared audio anchor sequence exhausted")?;
        let anchor = serde_json::json!({
            "session": self.encoder.session(),
            "time": time,
            "playing": self.playback.playing() && !self.playback.buffering()
                && self.held_frontier.is_none() && time < available_until,
            "sampledEpochMs": sampled_epoch_ms,
            "availableUntil": available_until,
            "audioSequence": sequence,
        })
        .to_string();
        self.audio_sequence = sequence;
        Ok(anchor)
    }
}

#[cfg(test)]
mod tests;
