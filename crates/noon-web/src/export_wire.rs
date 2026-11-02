//! Host codecs shared by immutable and completed-history media export.
//! Timeline policy and observations remain typed shared Rust values.

use noon::integration::{
    ExportEndReason, ExportFrameOptions, ExportFrameSummary, ExportSample, ExportSampleKind,
    ExportStop, FrameRate, RuntimeIdentity, SampleObservation,
};
use noon_core::PublicationContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(crate) const MAX_SAFE_JS_INTEGER: u64 = (1_u64 << 53) - 1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameRateWire {
    numerator: u32,
    denominator: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StopWire {
    SourceEnd {},
    FrameCount { frames: u64 },
    EndTime { time: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrameOptionsWire {
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

pub(crate) fn parse_frame_options(
    json: &str,
) -> Result<(FrameOptionsWire, ExportFrameOptions), String> {
    if json.len() > 65_536 {
        return Err("finite export options exceed their byte budget".into());
    }
    let mut wire: FrameOptionsWire = serde_json::from_str(json)
        .map_err(|error| format!("invalid finite export options: {error}"))?;
    let options = wire.options()?;
    wire.frame_rate = FrameRateWire {
        numerator: options.frame_rate.numerator(),
        denominator: options.frame_rate.denominator(),
    };
    Ok((wire, options))
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
pub(crate) struct ObservationWire {
    runtime_identity: String,
    requested_time: f64,
    published_time: f64,
    publication: PublicationWire,
}

impl ObservationWire {
    pub(crate) fn from_observation(
        observation: SampleObservation,
        runtime: RuntimeIdentity,
    ) -> Self {
        Self {
            runtime_identity: runtime.get().to_string(),
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

pub(crate) fn parse_observation(json: &str) -> Result<ObservationWire, String> {
    if json.len() > 4_096 {
        return Err("finite export acknowledgement exceeds its byte budget".into());
    }
    serde_json::from_str(json)
        .map_err(|error| format!("invalid finite export acknowledgement: {error}"))
}

fn publication_json(publication: PublicationContext) -> Value {
    json!({
        "scene_revision": publication.scene_revision().get().to_string(),
        "execution_revision": publication.execution_revision().get().to_string(),
        "frame_epoch": publication.frame_epoch().get().to_string(),
    })
}

pub(crate) fn source_json(
    kind: &str,
    runtime: RuntimeIdentity,
    admission: PublicationContext,
) -> Value {
    json!({
        "kind": kind, "runtime_identity": runtime.get().to_string(),
        "admission": publication_json(admission),
    })
}

fn observation_json(observation: SampleObservation, runtime: RuntimeIdentity) -> Value {
    json!({
        "runtime_identity": runtime.get().to_string(),
        "requested_time": observation.requested_time,
        "published_time": observation.published_time,
        "publication": publication_json(observation.publication),
    })
}

pub(crate) fn sample_json(sample: ExportSample, runtime: RuntimeIdentity) -> Value {
    json!({
        "kind": "sample",
        "sample_kind": match sample.kind {
            ExportSampleKind::Prefix => "prefix", ExportSampleKind::Output => "output",
            ExportSampleKind::Completion => "completion",
        },
        "observation": observation_json(sample.observation, runtime),
        "frame": sample.frame.map(|frame| json!({
            "pts": frame.pts, "source_index": frame.source_sample.index(), "held": frame.held,
        })),
    })
}

pub(crate) fn sampling_json(sampling: ExportFrameSummary) -> Value {
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
