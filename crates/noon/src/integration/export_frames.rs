//! Bounded output-frame policy over the existing cooperative sample operation.
//!
//! This module owns output indices and acknowledgements, not scene time,
//! animation evaluation, source continuation, rendering, or encoder completion.

mod policy;
pub use policy::{ExportFramePolicy, ExportFramePolicyError, ExportFramePolicyStatus};

use std::{error::Error, fmt};

use noon_core::PublicationContext;
use noon_runtime::RendererPublication;

use super::{
    ForwardSample, ForwardSampleError, ForwardSampleStatus, FrameGridError, FrameRate, FrameSample,
    SampleObservation,
};
use crate::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, RustHostCallbackTable,
};

/// An upper bound on the export. Natural completion can end it earlier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportStop {
    SourceEnd,
    /// Maximum output frames after `start_frame`, not a safety-cap success.
    FrameCount(u64),
    /// Exclusive authored-time end; this need not lie on the frame grid.
    EndTime(f64),
}

/// Explicit bounds for one fresh source run. There are no unlimited defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportFrameOptions {
    pub frame_rate: FrameRate,
    /// Crop on the source's zero-origin frame grid. Earlier frames are evaluated
    /// and their publications consumed, but are not sent to the video sink.
    pub start_frame: u64,
    pub stop: ExportStop,
    /// Safety cap on grid samples, INCLUDING the discarded prefix and holds.
    /// One final observation at the cap may prove completion; it is not output.
    pub max_frames: u64,
    pub max_transitions_per_sample: u32,
    /// Explicit frozen terminal hold. This does not keep calling live updaters.
    pub final_hold_seconds: f64,
}

/// One output frame; `source_sample.index()` and rebased video PTS are distinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportFrame {
    pub source_sample: FrameSample,
    pub pts: u64,
    /// The requested time is in an explicit terminal hold, not a fresh evaluation.
    pub held: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportSampleKind {
    Prefix,
    Output,
    Completion,
}

/// A coherent sample which must be consumed before progressing. An output-less
/// prefix/completion observation is not permission to discard resource deltas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportSample {
    pub observation: SampleObservation,
    pub kind: ExportSampleKind,
    pub frame: Option<ExportFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportEndReason {
    SourceEnd,
    RequestedStop,
}

/// Sampling has finished, NOT the encoder/muxer. A file becomes successful only
/// after the platform sink has independently flushed and finalized it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportFrameSummary {
    pub frames: u64,
    pub frame_rate: FrameRate,
    pub start_time: f64,
    pub end_time: f64,
    pub source_end: Option<f64>,
    pub scheduled_duration: f64,
    pub reason: ExportEndReason,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportFramesStatus {
    Progress,
    /// A semantic endpoint receipt, never an additional output frame.
    PublicationPending(PublicationContext),
    SampleReady(ExportSample),
    Complete(ExportFrameSummary),
}

/// A forward-only, constant-storage export run borrowing the original owner.
///
/// A fresh source is required. Crops replay the exact zero-origin sample prefix;
/// they are not independent seeks. Non-grid crop starts are intentionally not
/// exposed: rounding one silently would change stateful callback history.
///
/// On SampleReady, prepare/retain the publication, capture only Output samples,
/// and acknowledge the sample after the sink accepts that output. Slow consumers
/// may yield between calls; polling cannot advance a pending sample. Cancellation
/// is cooperative and cannot interrupt a synchronous callback that never returns.
/// On consumer failure, cancel and discard the export rather than retry a consumed
/// publication. Neither this type nor its summary certifies a completed video.
///
/// The reusable `ExportFramePolicy` owns frame/range decisions. This adapter only
/// coordinates it with Rust's existing continuation and publication protocol.
pub struct ExportFrames<'a, C: LiveContinuation> {
    sample: ForwardSample<'a, C>,
    policy: ExportFramePolicy,
    max_transitions_per_sample: u32,
}

impl<'a, C: LiveContinuation> ExportFrames<'a, C> {
    pub fn new(
        program: &'a mut LiveProgram<C>,
        callbacks: &'a mut RustHostCallbackTable,
        options: ExportFrameOptions,
    ) -> Result<Self, ExportFramesError<C::Error>> {
        if program.status() != LiveProgramStatus::ReadyToResume
            || program.session().frame().time != 0.0
        {
            return Err(ExportFramesError::SourceAlreadyStarted);
        }
        let policy = ExportFramePolicy::new(options)?;
        let sample =
            ForwardSample::new(program, callbacks, 0.0, options.max_transitions_per_sample)
                .map_err(ExportFramesError::Sample)?;
        Ok(Self {
            sample,
            policy,
            max_transitions_per_sample: options.max_transitions_per_sample,
        })
    }

    pub fn session(&self) -> &ExecutionSession {
        self.sample.session()
    }

    /// Query visibility through the original session-owned spatial index. This
    /// does not advance the source or consume/acknowledge the current sample.
    pub fn query_viewport(
        &mut self,
        bounds: noon_core::Rect,
    ) -> crate::integration::ExecutionViewportQuery {
        self.sample.query_viewport(bounds)
    }

    /// At most one underlying cooperative step; no wall-clock pacing.
    pub fn advance(&mut self) -> Result<ExportFramesStatus, ExportFramesError<C::Error>> {
        match self.policy.status()? {
            ExportFramePolicyStatus::NeedsSample(_) => {}
            ExportFramePolicyStatus::SampleReady(sample) => {
                return Ok(ExportFramesStatus::SampleReady(sample));
            }
            ExportFramePolicyStatus::Complete(summary) => {
                return Ok(ExportFramesStatus::Complete(summary));
            }
        }
        let result = match self.sample.advance() {
            Ok(ForwardSampleStatus::Progress) => Ok(ExportFramesStatus::Progress),
            Ok(ForwardSampleStatus::PublicationPending(context)) => {
                Ok(ExportFramesStatus::PublicationPending(context))
            }
            Ok(ForwardSampleStatus::Ready(observation)) => self
                .policy
                .observe(observation, false)
                .map(ExportFramesStatus::SampleReady)
                .map_err(ExportFramesError::from),
            Ok(ForwardSampleStatus::SourceFinished(observation)) => self
                .policy
                .observe(observation, true)
                .map(ExportFramesStatus::SampleReady)
                .map_err(ExportFramesError::from),
            Err(error) => Err(ExportFramesError::Sample(error)),
        };
        if result.is_err() {
            self.cancel();
        }
        result
    }

    pub fn take_renderer_publication(
        &mut self,
    ) -> Result<RendererPublication<'_>, ExportFramesError<C::Error>> {
        if matches!(self.policy.status()?, ExportFramePolicyStatus::Complete(_)) {
            return Err(ExportFramesError::Inactive);
        }
        self.sample
            .take_renderer_publication()
            .map_err(ExportFramesError::Sample)
    }

    pub fn admit_endpoint(
        &mut self,
        context: PublicationContext,
    ) -> Result<(), ExportFramesError<C::Error>> {
        if matches!(self.policy.status()?, ExportFramePolicyStatus::Complete(_)) {
            return Err(ExportFramesError::Inactive);
        }
        self.sample
            .admit_endpoint(context)
            .map_err(ExportFramesError::Sample)
    }

    /// Acknowledge exactly the offered sample, after consuming its publication
    /// and accepting any output. This is not an encoder flush or a GPU fence.
    pub fn acknowledge_sample(
        &mut self,
        expected: ExportSample,
    ) -> Result<(), ExportFramesError<C::Error>> {
        let sample = match self.policy.status()? {
            ExportFramePolicyStatus::SampleReady(sample) => sample,
            ExportFramePolicyStatus::Complete(_) => return Err(ExportFramesError::Inactive),
            ExportFramePolicyStatus::NeedsSample(_) => return Err(ExportFramesError::WrongSample),
        };
        if sample != expected {
            return Err(ExportFramesError::WrongSample);
        }
        if !self.sample.observation_consumed() {
            return Err(ExportFramesError::Sample(
                ForwardSampleError::PublicationNotConsumed,
            ));
        }
        let result = (|| {
            self.policy.acknowledge_sample(sample)?;
            if let ExportFramePolicyStatus::NeedsSample(requested) = self.policy.status()? {
                self.sample
                    .restart(requested, self.max_transitions_per_sample)
                    .map_err(ExportFramesError::Sample)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.cancel();
        }
        result
    }

    pub fn cancel(&mut self) {
        self.policy.cancel();
        self.sample.cancel();
    }
}

#[derive(Debug)]
pub enum ExportFramesError<E> {
    SourceAlreadyStarted,
    InvalidOptions,
    InvalidHold,
    InvalidObservation,
    EmptyInterval,
    FrameLimit,
    WrongSample,
    Inactive,
    Grid(FrameGridError),
    Sample(ForwardSampleError<E>),
}

impl<E> From<ExportFramePolicyError> for ExportFramesError<E> {
    fn from(error: ExportFramePolicyError) -> Self {
        match error {
            ExportFramePolicyError::InvalidOptions => Self::InvalidOptions,
            ExportFramePolicyError::InvalidHold => Self::InvalidHold,
            ExportFramePolicyError::InvalidObservation => Self::InvalidObservation,
            ExportFramePolicyError::EmptyInterval => Self::EmptyInterval,
            ExportFramePolicyError::FrameLimit => Self::FrameLimit,
            ExportFramePolicyError::WrongSample => Self::WrongSample,
            ExportFramePolicyError::Inactive => Self::Inactive,
            ExportFramePolicyError::Grid(error) => Self::Grid(error),
        }
    }
}

impl<E: fmt::Display> fmt::Display for ExportFramesError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceAlreadyStarted => {
                f.write_str("export requires a fresh source at time zero")
            }
            Self::InvalidOptions => f.write_str("invalid export bounds or hold configuration"),
            Self::InvalidHold => f.write_str("terminal hold has no finite distinct end"),
            Self::InvalidObservation => {
                f.write_str("observation does not settle the requested export sample")
            }
            Self::EmptyInterval => {
                f.write_str("export has no frames; author or request a positive hold")
            }
            Self::FrameLimit => f.write_str("export exceeded its frame safety cap"),
            Self::WrongSample => f.write_str("acknowledgement does not match the pending sample"),
            Self::Inactive => f.write_str("export is complete, failed, or cancelled"),
            Self::Grid(error) => error.fmt(f),
            Self::Sample(error) => error.fmt(f),
        }
    }
}

impl<E: Error + 'static> Error for ExportFramesError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Grid(error) => Some(error),
            Self::Sample(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
