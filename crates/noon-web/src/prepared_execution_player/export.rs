//! Worker transport over the shared completed PrefixReader export capability.
//! Source preparation is a bounded completion request, never a playback clock.

use super::PreparedExecutionPlayer;
use crate::export_wire::{
    parse_frame_options, parse_observation, sample_json, sampling_json, source_json,
    FrameOptionsWire, ObservationWire, MAX_SAFE_JS_INTEGER,
};
use noon::integration::{
    CompletedPrefixExport, ExportFramePolicy, ExportFramePolicyStatus, MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use noon::{PrefixError, PrefixReader};
use serde_json::{json, Value};

pub(super) struct HistoryExportPreparation {
    options: FrameOptionsWire,
    source_limit: f64,
}

impl HistoryExportPreparation {
    pub(super) fn needs_preparation(&self, reader: &PrefixReader) -> Result<bool, String> {
        if reader.frontier() > self.source_limit
            || reader.end_time().is_some_and(|end| end > self.source_limit)
        {
            return Err(
                "completed-history export source exceeds its max_frames preparation limit".into(),
            );
        }
        // At the exact cap the interpreter may still finish at the same time.
        // Only terminal adoption proves completion; no epsilon or guessed end.
        Ok(reader.end_time().is_none())
    }
}

impl PreparedExecutionPlayer {
    pub(super) fn ensure_export_control_available(&self) -> Result<(), String> {
        if self.finite_export_active() {
            return Err("finite export owns paused execution until capture completes".into());
        }
        Ok(())
    }

    pub(super) fn ensure_export_delivery_available(&self) -> Result<(), PrefixError> {
        if self.finite_export.is_some() {
            return Err(PrefixError::ReaderChanged);
        }
        Ok(())
    }

    fn validate_export_host(&self) -> Result<(), String> {
        if self.playback.playing()
            || self.playback.buffering()
            || self.held_frontier.is_some()
            || self.source.is_some()
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
    /// Request bounded completion of the existing canonical source while its
    /// independent reader remains paused at zero. The ordinary source driver
    /// consults needsPreparation; this does not execute or recreate Python.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = prepareHistoryExportJson))]
    pub fn prepare_history_export_json(&mut self, options_json: &str) -> Result<String, String> {
        self.ensure_export_control_available()?;
        if self.playback.playing() || self.playback.buffering() || self.held_frontier.is_some() {
            return Err(
                "history export preparation requires a fresh paused reader without active input"
                    .into(),
            );
        }
        CompletedPrefixExport::validate_preparation(&self.playback.reader)
            .map_err(|e| e.to_string())?;
        if self.source.is_none() && self.prepared_end_time().is_none() {
            return Err(
                "history export preparation requires an open canonical source or completed history"
                    .into(),
            );
        }
        let (wire, options) = parse_frame_options(options_json)?;
        let policy = ExportFramePolicy::new(options).map_err(|e| e.to_string())?;
        let preparation = HistoryExportPreparation {
            options: wire,
            source_limit: policy
                .source_preparation_limit()
                .map_err(|e| e.to_string())?,
        };
        let needs_preparation = preparation.needs_preparation(&self.playback.reader)?;
        let reply = json!({
            "kind": "preparing_history", "frame_options": wire,
            "source_preparation_limit": preparation.source_limit,
            "history_complete": !needs_preparation,
        })
        .to_string();
        self.history_export_preparation = Some(preparation);
        Ok(reply)
    }

    /// Borrow this same terminal-adopted history and its original soundtrack.
    /// Shared Rust validates the immutable owner before any sample is selected.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = beginFiniteExportJson))]
    pub fn begin_finite_export_json(&mut self, options_json: &str) -> Result<String, String> {
        if self.finite_export.is_some() {
            return Err("a finite export is already active".into());
        }
        self.validate_export_host()?;
        let (wire, options) = parse_frame_options(options_json)?;
        if let Some(preparation) = self.history_export_preparation.as_ref() {
            preparation.needs_preparation(&self.playback.reader)?;
            if preparation.options != wire {
                return Err(
                    "finite export options differ from the admitted history preparation".into(),
                );
            }
        }
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
        self.history_export_preparation = None;
        self.finite_export = Some(export);
        Ok(reply)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = finiteExportActive))]
    pub fn finite_export_active(&self) -> bool {
        self.history_export_preparation.is_some() || self.finite_export.is_some()
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
        self.history_export_preparation = None;
        if let Some(mut export) = self.finite_export.take() {
            export.cancel();
        }
        self.clock = crate::BrowserExecutionWakeClock::default();
    }
}

#[cfg(test)]
mod tests;
