//! Worker transport over the shared completed PrefixReader export capability.
//! The interpreter driver completes preparation before this player begins export.

use super::PreparedExecutionPlayer;
use crate::export_wire::{
    parse_frame_options, parse_observation, sample_json, sampling_json, source_json,
    ObservationWire, MAX_SAFE_JS_INTEGER,
};
use noon::integration::{
    CompletedPrefixExport, ExportFramePolicyStatus, MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use serde_json::{json, Value};

impl PreparedExecutionPlayer {
    fn validate_export_host(&self) -> Result<(), String> {
        if self.playback.playing()
            || self.playback.buffering()
            || self
                .source
                .as_ref()
                .is_some_and(|source| !source.finished())
        {
            return Err("completed-history export requires a paused reader with terminal source delivery adopted".into());
        }
        Ok(())
    }

    fn export_status(&self, export: &CompletedPrefixExport) -> Result<Value, String> {
        Ok(
            match export
                .status(&self.playback.reader)
                .map_err(|e| e.to_string())?
            {
                ExportFramePolicyStatus::NeedsSample(requested_time) => json!({
                    "kind": "needs_sample", "requested_time": requested_time,
                    "target_time": requested_time.min(export.source_end()),
                }),
                ExportFramePolicyStatus::SampleReady(sample) => {
                    sample_json(sample, self.playback.reader.runtime_identity())
                }
                ExportFramePolicyStatus::Complete(sampling) => {
                    json!({ "kind": "complete", "sampling": sampling_json(sampling) })
                }
            },
        )
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl PreparedExecutionPlayer {
    /// Borrow this same terminal-adopted history and its original soundtrack.
    /// Shared Rust validates the immutable owner before any sample is selected.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = beginFiniteExportJson))]
    pub fn begin_finite_export_json(&mut self, options_json: &str) -> Result<String, String> {
        if self.finite_export.is_some() {
            return Err("a finite export is already active".into());
        }
        self.validate_export_host()?;
        let (wire, options) = parse_frame_options(options_json)?;
        let export = CompletedPrefixExport::new(&self.playback.reader, options)
            .map_err(|e| e.to_string())?;
        let audio_grid = export.audio_grid();
        if audio_grid.is_some_and(|grid| grid.frames() > MAX_SAFE_JS_INTEGER) {
            return Err("browser export PCM count exceeds exact transport integers".into());
        }
        let reply = json!({
            "source": source_json("completed_history", self.playback.reader.runtime_identity(), export.owner()),
            "source_end": export.source_end(), "audio": audio_grid.is_some(),
            "frame_options": wire, "sampling": sampling_json(export.planned()),
            "audio_frames": audio_grid.map(|grid| grid.frames()),
            "audio_block_frames": MAX_AUDIO_MIX_BLOCK_FRAMES,
            "status": self.export_status(&export)?,
        }).to_string();
        self.finite_export = Some(export);
        Ok(reply)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = finiteExportActive))]
    pub fn finite_export_active(&self) -> bool {
        self.finite_export.is_some()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = finiteExportStatusJson))]
    pub fn finite_export_status_json(&self) -> Result<String, String> {
        self.validate_export_host()?;
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        Ok(self.export_status(export)?.to_string())
    }

    /// Shared Rust selects and evaluates the next sample on the existing reader.
    /// The host then drains its ordinary publication and awaits actual display.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = observeFiniteExportJson))]
    pub fn observe_finite_export_json(&mut self) -> Result<String, String> {
        self.validate_export_host()?;
        let export = self
            .finite_export
            .as_mut()
            .ok_or("no finite export is active")?;
        export
            .advance(&mut self.playback.reader)
            .map_err(|e| e.to_string())?;
        self.finite_export_status_json()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = acknowledgeFiniteExportJson))]
    pub fn acknowledge_finite_export_json(
        &mut self,
        observation_json: &str,
    ) -> Result<String, String> {
        let observation = parse_observation(observation_json)?;
        self.validate_export_host()?;
        let reader = &self.playback.reader;
        let export = self
            .finite_export
            .as_mut()
            .ok_or("no finite export is active")?;
        let ExportFramePolicyStatus::SampleReady(sample) =
            export.status(reader).map_err(|e| e.to_string())?
        else {
            return Err("finite export has no sample awaiting acknowledgement".into());
        };
        if observation
            != ObservationWire::from_observation(sample.observation, reader.runtime_identity())
        {
            return Err(
                "finite export acknowledgement does not match the pending publication".into(),
            );
        }
        export
            .acknowledge_sample(reader, sample)
            .map_err(|e| e.to_string())?;
        self.finite_export_status_json()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = mixFiniteExportAudioBlock))]
    pub fn mix_finite_export_audio_block(
        &self,
        first_frame: u64,
        frames: u32,
    ) -> Result<Vec<f32>, String> {
        self.validate_export_host()?;
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        let grid = export
            .audio_grid()
            .ok_or("finite export has no authored audio")?;
        let frames = frames as usize;
        if frames == 0 || frames > MAX_AUDIO_MIX_BLOCK_FRAMES {
            return Err("finite export PCM request exceeds its bounded block profile".into());
        }
        let mut samples = vec![0.0; frames * usize::from(grid.format().channels())];
        export
            .mix_audio_block(&self.playback.reader, first_frame, &mut samples)
            .map_err(|e| e.to_string())?;
        Ok(samples)
    }

    /// Capture retains ownership through PCM delivery after video completion.
    /// The host's finally path releases it, including any failed/cancelled run.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = cancelFiniteExport))]
    pub fn cancel_finite_export(&mut self) {
        if let Some(mut export) = self.finite_export.take() {
            export.cancel();
        }
        self.clock = crate::BrowserExecutionWakeClock::default();
    }
}
