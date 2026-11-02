//! Finite media export over the player's existing execution owner.
//!
//! JavaScript coordinates the real worker/render boundary and pixel readback.
//! Shared Rust policy owns every sample, acknowledgement and PCM coordinate.

use super::{PublicationContext, RuntimeIdentity, SemanticExecutionPlayer};
use crate::export_wire::{
    parse_frame_options, parse_observation, sample_json, sampling_json, source_json,
    ObservationWire, MAX_SAFE_JS_INTEGER,
};
use noon::integration::{
    AudioMixFormat, ExportAudioGrid, ExportFramePolicy, ExportFramePolicyStatus,
    ExportFrameSummary, SampleObservation, MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use serde_json::{json, Value};

pub(super) struct FiniteExport {
    policy: ExportFramePolicy,
    source_end: f64,
    planned: ExportFrameSummary,
    audio_grid: Option<ExportAudioGrid>,
    runtime: RuntimeIdentity,
    publication: PublicationContext,
    last_observation: Option<SampleObservation>,
}

impl FiniteExport {
    fn validate_owner(&self, player: &SemanticExecutionPlayer) -> Result<(), String> {
        let publication = player.session.publication_context();
        if player.session.runtime_identity() != self.runtime
            || publication.scene_revision() != self.publication.scene_revision()
            || publication.execution_revision() != self.publication.execution_revision()
            || player.session.pending_callback_token().is_some()
            || player.session.replay_scope_active()
            || player.pending_callback_phase.is_some()
            || player.clock.is_playing()
            || player.session.interactions_active()
        {
            return Err("finite export lost its paused immutable execution owner".into());
        }
        Ok(())
    }

    fn status(&self) -> Result<Value, String> {
        Ok(
            match self.policy.status().map_err(|error| error.to_string())? {
                ExportFramePolicyStatus::NeedsSample(requested_time) => json!({
                    "kind": "needs_sample", "requested_time": requested_time,
                    "target_time": requested_time.min(self.source_end),
                }),
                ExportFramePolicyStatus::SampleReady(sample) => sample_json(sample, self.runtime),
                ExportFramePolicyStatus::Complete(sampling) => {
                    if sampling != self.planned {
                        return Err(
                            "completed export differs from the admitted finite range".into()
                        );
                    }
                    json!({ "kind": "complete", "sampling": sampling_json(sampling) })
                }
            },
        )
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
    /// Admit only the original fresh, paused finite execution owner. No JSON
    /// scene is loaded and no runtime, callback or frontend clock is created.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = beginFiniteExportJson))]
    pub fn begin_finite_export_json(&mut self, options_json: &str) -> Result<String, String> {
        if self.finite_export.is_some() {
            return Err("a finite export is already active".into());
        }
        if self.session.frame().time != 0.0
            || self.clock.is_playing()
            || self.session.pending_callback_token().is_some()
            || self.session.replay_scope_active()
            || self.pending_callback_phase.is_some()
            || self.session.interactions_active()
        {
            return Err("finite export requires a fresh paused immutable session without callbacks, history or active input".into());
        }
        let source_end = self
            .session
            .finite_timeline_end_time()
            .ok_or("finite export requires a complete finite immutable timeline (#2263)")?;
        let (wire, options) = parse_frame_options(options_json)?;
        let policy = ExportFramePolicy::new(options).map_err(|error| error.to_string())?;
        let planned = policy
            .finite_summary(source_end)
            .map_err(|error| error.to_string())?;
        let audio_grid = if self.session.has_audio() {
            let grid = ExportAudioGrid::new(planned, AudioMixFormat::STEREO_48_KHZ)
                .map_err(|error| error.to_string())?;
            if grid.frames() > MAX_SAFE_JS_INTEGER {
                return Err("browser export PCM count exceeds exact transport integers".into());
            }
            self.session
                .mix_audio_block(grid.scene_start(), grid.frames(), grid.format(), &mut [])
                .map_err(|error| error.to_string())?;
            Some(grid)
        } else {
            None
        };
        let export = FiniteExport {
            policy,
            source_end,
            planned,
            audio_grid,
            runtime: self.session.runtime_identity(),
            publication: self.session.publication_context(),
            last_observation: None,
        };
        let reply = json!({
            "source": source_json("immutable", export.runtime, export.publication),
            "source_end": source_end, "audio": audio_grid.is_some(), "frame_options": wire,
            "sampling": sampling_json(planned), "audio_frames": audio_grid.map(|grid| grid.frames()),
            "audio_block_frames": MAX_AUDIO_MIX_BLOCK_FRAMES,
            "status": export.status()?,
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
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        export.validate_owner(self)?;
        Ok(export.status()?.to_string())
    }

    /// Record the runtime's coherent publication after the host drives the
    /// existing callback-aware forward operation to the Rust-selected target.
    /// The host must then present/read back this exact sample before acknowledging.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = observeFiniteExportJson))]
    pub fn observe_finite_export_json(&mut self) -> Result<String, String> {
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        export.validate_owner(self)?;
        let ExportFramePolicyStatus::NeedsSample(requested_time) =
            export.policy.status().map_err(|error| error.to_string())?
        else {
            return Err("finite export has no unobserved sample request".into());
        };
        let observation = SampleObservation {
            requested_time,
            published_time: self.session.frame().time,
            publication: self.session.publication_context(),
        };
        if observation.published_time != requested_time.min(export.source_end)
            || export.last_observation.is_some_and(|previous| {
                observation.publication.frame_epoch().get()
                    < previous.publication.frame_epoch().get()
                    || (observation.published_time != previous.published_time
                        && observation.publication.frame_epoch()
                            == previous.publication.frame_epoch())
            })
        {
            return Err("runtime did not publish the exact finite export sample".into());
        }
        let export = self
            .finite_export
            .as_mut()
            .expect("validated finite export");
        export
            .policy
            .observe(observation, requested_time >= export.source_end)
            .map_err(|error| error.to_string())?;
        export.last_observation = Some(observation);
        Ok(export.status()?.to_string())
    }

    /// The explicit external acknowledgement carries the unchanged observation.
    /// It is valid only after matching renderer completion and any pixel copy;
    /// the host owns that real cross-worker barrier, not this metadata policy.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = acknowledgeFiniteExportJson))]
    pub fn acknowledge_finite_export_json(
        &mut self,
        observation_json: &str,
    ) -> Result<String, String> {
        let observation = parse_observation(observation_json)?;
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        export.validate_owner(self)?;
        let ExportFramePolicyStatus::SampleReady(sample) =
            export.policy.status().map_err(|error| error.to_string())?
        else {
            return Err("finite export has no sample awaiting acknowledgement".into());
        };
        if observation != ObservationWire::from_observation(sample.observation, export.runtime)
            || self.session.frame().time != sample.observation.published_time
            || self.session.publication_context() != sample.observation.publication
        {
            return Err(
                "finite export acknowledgement does not match the pending publication".into(),
            );
        }
        let export = self
            .finite_export
            .as_mut()
            .expect("validated finite export");
        export
            .policy
            .acknowledge_sample(sample)
            .map_err(|error| error.to_string())?;
        Ok(export.status()?.to_string())
    }

    /// Read-only shared mixer blocks. Viewer device mute, gain and wall time
    /// have no effect on exported authored PCM, and this does not advance video.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = mixFiniteExportAudioBlock))]
    pub fn mix_finite_export_audio_block(
        &self,
        first_frame: u64,
        frames: u32,
    ) -> Result<Vec<f32>, String> {
        let export = self
            .finite_export
            .as_ref()
            .ok_or("no finite export is active")?;
        export.validate_owner(self)?;
        let grid = export
            .audio_grid
            .ok_or("finite export has no authored audio")?;
        let frames = frames as usize;
        if frames == 0 || frames > MAX_AUDIO_MIX_BLOCK_FRAMES {
            return Err("finite export PCM request exceeds its bounded block profile".into());
        }
        let mixed_frames = grid
            .mixed_frames_in_block(first_frame, frames)
            .map_err(|error| error.to_string())?;
        let channels = usize::from(grid.format().channels());
        let mut samples = vec![0.0_f32; frames * channels];
        if mixed_frames > 0 {
            self.session
                .mix_audio_block(
                    grid.scene_start(),
                    first_frame,
                    grid.format(),
                    &mut samples[..mixed_frames * channels],
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(samples)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = cancelFiniteExport))]
    pub fn cancel_finite_export(&mut self) {
        if let Some(mut export) = self.finite_export.take() {
            export.policy.cancel();
        }
    }
}

#[cfg(test)]
mod tests;
