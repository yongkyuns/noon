//! Finite media export over the player's existing execution owner.
//!
//! JavaScript coordinates the real worker/render boundary and pixel readback.
//! Shared Rust policy owns every sample, acknowledgement and PCM coordinate.

use super::{PublicationContext, RuntimeIdentity, SemanticExecutionPlayer};
use noon::integration::{
    AudioMixFormat, ExportAudioGrid, ExportEndReason, ExportFrameOptions, ExportFramePolicy,
    ExportFramePolicyStatus, ExportFrameSummary, ExportSample, ExportSampleKind, ExportStop,
    FrameRate, SampleObservation, MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const MAX_SAFE_JS_INTEGER: u64 = (1_u64 << 53) - 1;

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
                ExportFramePolicyStatus::SampleReady(sample) => sample_json(sample),
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameRateWire {
    numerator: u32,
    denominator: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StopWire {
    SourceEnd {},
    FrameCount { frames: u64 },
    EndTime { time: f64 },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameOptionsWire {
    frame_rate: FrameRateWire,
    start_frame: u64,
    stop: StopWire,
    max_frames: u64,
    max_transitions_per_sample: u32,
    final_hold_seconds: f64,
}

impl FrameOptionsWire {
    fn options(self) -> Result<ExportFrameOptions, String> {
        if self.max_frames > MAX_SAFE_JS_INTEGER
            || self.start_frame > MAX_SAFE_JS_INTEGER
            || matches!(self.stop, StopWire::FrameCount { frames } if frames > MAX_SAFE_JS_INTEGER)
        {
            return Err("browser export frame indices exceed exact transport integers".into());
        }
        Ok(ExportFrameOptions {
            frame_rate: FrameRate::new(self.frame_rate.numerator, self.frame_rate.denominator)
                .map_err(|error| error.to_string())?,
            start_frame: self.start_frame,
            stop: match self.stop {
                StopWire::SourceEnd {} => ExportStop::SourceEnd,
                StopWire::FrameCount { frames } => ExportStop::FrameCount(frames),
                StopWire::EndTime { time } => ExportStop::EndTime(time),
            },
            max_frames: self.max_frames,
            max_transitions_per_sample: self.max_transitions_per_sample,
            final_hold_seconds: self.final_hold_seconds,
        })
    }
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct PublicationWire {
    scene_revision: String,
    execution_revision: String,
    frame_epoch: String,
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ObservationWire {
    requested_time: f64,
    published_time: f64,
    publication: PublicationWire,
}

impl From<SampleObservation> for ObservationWire {
    fn from(observation: SampleObservation) -> Self {
        Self {
            requested_time: observation.requested_time,
            published_time: observation.published_time,
            publication: PublicationWire {
                scene_revision: observation.publication.scene_revision().get().to_string(),
                execution_revision: observation
                    .publication
                    .execution_revision()
                    .get()
                    .to_string(),
                frame_epoch: observation.publication.frame_epoch().get().to_string(),
            },
        }
    }
}

fn observation_json(observation: SampleObservation) -> Value {
    let publication = observation.publication;
    json!({
        "requested_time": observation.requested_time,
        "published_time": observation.published_time,
        "publication": {
            "scene_revision": publication.scene_revision().get().to_string(),
            "execution_revision": publication.execution_revision().get().to_string(),
            "frame_epoch": publication.frame_epoch().get().to_string(),
        },
    })
}

fn sample_json(sample: ExportSample) -> Value {
    json!({
        "kind": "sample",
        "sample_kind": match sample.kind {
            ExportSampleKind::Prefix => "prefix", ExportSampleKind::Output => "output",
            ExportSampleKind::Completion => "completion",
        },
        "observation": observation_json(sample.observation),
        "frame": sample.frame.map(|frame| json!({
            "pts": frame.pts, "source_index": frame.source_sample.index(), "held": frame.held,
        })),
    })
}

fn sampling_json(sampling: ExportFrameSummary) -> Value {
    json!({
        "frames": sampling.frames,
        "frame_rate": { "numerator": sampling.frame_rate.numerator(),
            "denominator": sampling.frame_rate.denominator() },
        "start_time": sampling.start_time, "end_time": sampling.end_time,
        "source_end": sampling.source_end, "scheduled_duration": sampling.scheduled_duration,
        "reason": match sampling.reason {
            ExportEndReason::SourceEnd => "source_end", ExportEndReason::RequestedStop => "requested_stop",
        },
    })
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
        if options_json.len() > 65_536 {
            return Err("finite export options exceed their byte budget".into());
        }
        let mut wire: FrameOptionsWire = serde_json::from_str(options_json)
            .map_err(|error| format!("invalid finite export options: {error}"))?;
        let options = wire.options()?;
        wire.frame_rate = FrameRateWire {
            numerator: options.frame_rate.numerator(),
            denominator: options.frame_rate.denominator(),
        };
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
            "source_end": source_end, "audio": audio_grid.is_some(), "frame_options": wire,
            "sampling": sampling_json(planned), "audio_frames": audio_grid.map(|grid| grid.frames()),
            "audio_block_frames": MAX_AUDIO_MIX_BLOCK_FRAMES,
            "status": export.status()?,
        }).to_string();
        self.pending_scene_control = None;
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
        if observation_json.len() > 4_096 {
            return Err("finite export acknowledgement exceeds its byte budget".into());
        }
        let observation: ObservationWire = serde_json::from_str(observation_json)
            .map_err(|error| format!("invalid finite export acknowledgement: {error}"))?;
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
        if observation != ObservationWire::from(sample.observation)
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
