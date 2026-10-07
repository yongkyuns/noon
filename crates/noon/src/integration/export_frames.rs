//! Bounded output-frame policy over the existing cooperative sample operation.
//!
//! This module owns output indices and acknowledgements, not scene time,
//! animation evaluation, source continuation, rendering, or encoder completion.

use std::{error::Error, fmt};

use noon_core::PublicationContext;
use noon_runtime::RendererPublication;

use super::{
    ForwardSample, ForwardSampleError, ForwardSampleStatus, FrameGrid, FrameGridError, FrameRate,
    FrameSample, SampleObservation,
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
pub struct ExportFrames<'a, C: LiveContinuation> {
    sample: ForwardSample<'a, C>,
    options: ExportFrameOptions,
    grid: FrameGrid,
    stop_time: Option<f64>,
    index: u64,
    emitted: u64,
    pending: Option<(ExportSample, Option<ExportFrameSummary>)>,
    complete: Option<ExportFrameSummary>,
    failed: bool,
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
        if options.max_frames == 0
            || options.start_frame >= options.max_frames
            || options.max_transitions_per_sample == 0
            || !options.final_hold_seconds.is_finite()
            || options.final_hold_seconds < 0.0
        {
            return Err(ExportFramesError::InvalidOptions);
        }
        let grid = FrameGrid::new(options.frame_rate, 0.0).map_err(ExportFramesError::Grid)?;
        let start_time = grid
            .sample(options.start_frame)
            .map_err(ExportFramesError::Grid)?
            .authored_time();
        let stop_time = match options.stop {
            ExportStop::SourceEnd => None,
            ExportStop::FrameCount(count) => {
                if count == 0 {
                    return Err(ExportFramesError::EmptyInterval);
                }
                let end = options
                    .start_frame
                    .checked_add(count)
                    .filter(|&end| end <= options.max_frames)
                    .ok_or(ExportFramesError::FrameLimit)?;
                Some(
                    grid.sample(end)
                        .map_err(ExportFramesError::Grid)?
                        .authored_time(),
                )
            }
            ExportStop::EndTime(end) => {
                if !end.is_finite() || end <= start_time {
                    return Err(ExportFramesError::InvalidOptions);
                }
                grid.frame_count_before(end, options.max_frames)
                    .map_err(ExportFramesError::Grid)?;
                Some(end)
            }
        };
        let sample = ForwardSample::new(program, callbacks, 0.0, options.max_transitions_per_sample)
            .map_err(ExportFramesError::Sample)?;
        Ok(Self {
            sample,
            options,
            grid,
            stop_time,
            index: 0,
            emitted: 0,
            pending: None,
            complete: None,
            failed: false,
        })
    }

    pub fn session(&self) -> &ExecutionSession {
        self.sample.session()
    }

    /// At most one underlying cooperative step; no wall-clock pacing.
    pub fn advance(&mut self) -> Result<ExportFramesStatus, ExportFramesError<C::Error>> {
        if self.failed {
            return Err(ExportFramesError::Inactive);
        }
        if let Some((sample, _)) = self.pending {
            return Ok(ExportFramesStatus::SampleReady(sample));
        }
        if let Some(summary) = self.complete {
            return Ok(ExportFramesStatus::Complete(summary));
        }
        let result = match self.sample.advance() {
            Ok(ForwardSampleStatus::Progress) => Ok(ExportFramesStatus::Progress),
            Ok(ForwardSampleStatus::PublicationPending(context)) => {
                Ok(ExportFramesStatus::PublicationPending(context))
            }
            Ok(ForwardSampleStatus::Ready(observation)) => self.observe(observation, false),
            Ok(ForwardSampleStatus::SourceFinished(observation)) => {
                self.observe(observation, true)
            }
            Err(error) => Err(ExportFramesError::Sample(error)),
        };
        if result.is_err() {
            self.cancel();
        }
        result
    }

    fn observe(
        &mut self,
        observation: SampleObservation,
        finished: bool,
    ) -> Result<ExportFramesStatus, ExportFramesError<C::Error>> {
        let source_end = finished.then_some(observation.published_time);
        let source_limit = match source_end {
            Some(end) => {
                let held_end = end + self.options.final_hold_seconds;
                if !held_end.is_finite()
                    || (self.options.final_hold_seconds > 0.0 && held_end <= end)
                {
                    return Err(ExportFramesError::InvalidHold);
                }
                held_end
            }
            None => f64::INFINITY,
        };
        let requested_limit = self.stop_time.unwrap_or(f64::INFINITY);
        let end = source_limit.min(requested_limit);
        let at_end = observation.requested_time >= end;
        if !at_end && self.index >= self.options.max_frames {
            return Err(ExportFramesError::FrameLimit);
        }
        let frame = if !at_end && self.index >= self.options.start_frame {
            Some(ExportFrame {
                source_sample: self.grid.sample(self.index).map_err(ExportFramesError::Grid)?,
                pts: self.index - self.options.start_frame,
                held: finished,
            })
        } else {
            None
        };
        let sample = ExportSample {
            observation,
            kind: if at_end {
                ExportSampleKind::Completion
            } else if frame.is_some() {
                ExportSampleKind::Output
            } else {
                ExportSampleKind::Prefix
            },
            frame,
        };
        let summary = if at_end {
            Some(ExportFrameSummary {
                frames: self.emitted,
                frame_rate: self.options.frame_rate,
                start_time: self
                    .grid
                    .sample(self.options.start_frame)
                    .map_err(ExportFramesError::Grid)?
                    .authored_time(),
                end_time: end,
                source_end,
                scheduled_duration: self
                    .grid
                    .sample(self.emitted)
                    .map_err(ExportFramesError::Grid)?
                    .authored_time(),
                reason: if source_limit <= requested_limit {
                    ExportEndReason::SourceEnd
                } else {
                    ExportEndReason::RequestedStop
                },
            })
        } else {
            None
        };
        self.pending = Some((sample, summary));
        Ok(ExportFramesStatus::SampleReady(sample))
    }

    pub fn take_renderer_publication(
        &mut self,
    ) -> Result<RendererPublication<'_>, ExportFramesError<C::Error>> {
        if self.failed || self.complete.is_some() {
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
        if self.failed || self.complete.is_some() {
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
        if self.failed || self.complete.is_some() {
            return Err(ExportFramesError::Inactive);
        }
        let Some((sample, summary)) = self.pending else {
            return Err(ExportFramesError::WrongSample);
        };
        if sample != expected {
            return Err(ExportFramesError::WrongSample);
        }
        if !self.sample.observation_consumed() {
            return Err(ExportFramesError::Sample(
                ForwardSampleError::PublicationNotConsumed,
            ));
        }
        if let Some(summary) = summary {
            if summary.frames == 0 {
                self.cancel();
                return Err(ExportFramesError::EmptyInterval);
            }
            self.pending = None;
            self.complete = Some(summary);
            return Ok(());
        }
        let result = self.next_sample(sample.frame.is_some());
        if result.is_err() {
            self.cancel();
        }
        result
    }

    fn next_sample(&mut self, output: bool) -> Result<(), ExportFramesError<C::Error>> {
        let next = self
            .index
            .checked_add(1)
            .ok_or(ExportFramesError::FrameLimit)?;
        let grid_time = self
            .grid
            .sample(next)
            .map_err(ExportFramesError::Grid)?
            .authored_time();
        // Drain an off-grid requested end exactly, never advance callbacks to the
        // next grid point beyond it. Source completion is handled by ForwardSample.
        let requested = self.stop_time.map_or(grid_time, |end| grid_time.min(end));
        self.sample
            .restart(requested, self.options.max_transitions_per_sample)
            .map_err(ExportFramesError::Sample)?;
        if output {
            self.emitted += 1; // bounded by index < max_frames before offer
        }
        self.index = next;
        self.pending = None;
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.failed = true;
        self.sample.cancel();
    }
}

#[derive(Debug)]
pub enum ExportFramesError<E> {
    SourceAlreadyStarted,
    InvalidOptions,
    InvalidHold,
    EmptyInterval,
    FrameLimit,
    WrongSample,
    Inactive,
    Grid(FrameGridError),
    Sample(ForwardSampleError<E>),
}

impl<E: fmt::Display> fmt::Display for ExportFramesError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceAlreadyStarted => f.write_str("export requires a fresh source at time zero"),
            Self::InvalidOptions => f.write_str("invalid export bounds or hold configuration"),
            Self::InvalidHold => f.write_str("terminal hold has no finite distinct end"),
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
