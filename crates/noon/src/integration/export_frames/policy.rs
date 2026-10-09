//! Output sampling policy shared by Rust and interpreter-owned continuations.
//!
//! This owns no scene or callback schedule. Hosts settle the requested sample
//! through the existing runtime before submitting its coherent observation.

use std::{error::Error, fmt};

use noon_core::PublicationContext;

use super::{
    ExportEndReason, ExportFrame, ExportFrameOptions, ExportFrameSummary, ExportSample,
    ExportSampleKind, ExportStop,
};
use crate::integration::{FrameGrid, FrameGridError, SampleObservation};

/// A requested sample, a consumer barrier, or completed sampling (not a file).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportFramePolicyStatus {
    NeedsSample(f64),
    SampleReady(ExportSample),
    Complete(ExportFrameSummary),
}

/// Constant-storage frame/range policy, independent of source-language control flow.
///
/// A fresh source starts on the zero-origin grid. Hosts must settle all incoming
/// segment endpoints, callbacks and same-time source edits before `observe`.
/// Off-grid endpoint publications are consumed using the runtime's existing
/// admission protocol, not offered here as additional video samples.
///
/// Acknowledge only after the publication and any output were accepted. This
/// policy checks observation identity but cannot certify GPU or encoder work.
/// On a consumer failure, cancel and abandon the run; do not replay user code.
/// Rust's `ExportFrames` uses this same policy, not a separate implementation.
pub struct ExportFramePolicy {
    options: ExportFrameOptions,
    grid: FrameGrid,
    stop_time: Option<f64>,
    requested_time: f64,
    index: u64,
    emitted: u64,
    pending: Option<(ExportSample, Option<ExportFrameSummary>)>,
    complete: Option<ExportFrameSummary>,
    last_published_time: Option<f64>,
    terminal: Option<(f64, PublicationContext)>,
    failed: bool,
}

impl ExportFramePolicy {
    pub fn new(options: ExportFrameOptions) -> Result<Self, ExportFramePolicyError> {
        if options.max_frames == 0
            || options.start_frame >= options.max_frames
            || options.max_transitions_per_sample == 0
            || !options.final_hold_seconds.is_finite()
            || options.final_hold_seconds < 0.0
        {
            return Err(ExportFramePolicyError::InvalidOptions);
        }
        let grid = FrameGrid::new(options.frame_rate, 0.0)?;
        let start_time = grid.sample(options.start_frame)?.authored_time();
        let stop_time = match options.stop {
            ExportStop::SourceEnd => None,
            ExportStop::FrameCount(count) => {
                if count == 0 {
                    return Err(ExportFramePolicyError::EmptyInterval);
                }
                let end = options
                    .start_frame
                    .checked_add(count)
                    .filter(|&end| end <= options.max_frames)
                    .ok_or(ExportFramePolicyError::FrameLimit)?;
                Some(grid.sample(end)?.authored_time())
            }
            ExportStop::EndTime(end) => {
                if !end.is_finite() || end <= start_time {
                    return Err(ExportFramePolicyError::InvalidOptions);
                }
                grid.frame_count_before(end, options.max_frames)?;
                Some(end)
            }
        };
        Ok(Self {
            options,
            grid,
            stop_time,
            requested_time: 0.0,
            index: 0,
            emitted: 0,
            pending: None,
            complete: None,
            last_published_time: None,
            terminal: None,
            failed: false,
        })
    }

    /// Repeated observation of this status never advances time or output PTS.
    pub fn status(&self) -> Result<ExportFramePolicyStatus, ExportFramePolicyError> {
        if self.failed {
            return Err(ExportFramePolicyError::Inactive);
        }
        if let Some((sample, _)) = self.pending {
            return Ok(ExportFramePolicyStatus::SampleReady(sample));
        }
        if let Some(summary) = self.complete {
            return Ok(ExportFramePolicyStatus::Complete(summary));
        }
        Ok(ExportFramePolicyStatus::NeedsSample(self.requested_time))
    }

    /// Offer a settled observation of exactly the outstanding request.
    ///
    /// `source_finished` means all source continuation work has completed, not
    /// merely that one animation ended. A completed source may report an earlier
    /// publication time; frozen holds must keep that terminal publication unchanged.
    /// This validates the observation, not the truth of arbitrary host code.
    pub fn observe(
        &mut self,
        observation: SampleObservation,
        source_finished: bool,
    ) -> Result<ExportSample, ExportFramePolicyError> {
        match self.status()? {
            ExportFramePolicyStatus::NeedsSample(_) => {}
            ExportFramePolicyStatus::SampleReady(_) => {
                return Err(ExportFramePolicyError::WrongSample);
            }
            ExportFramePolicyStatus::Complete(_) => {
                return Err(ExportFramePolicyError::Inactive);
            }
        }
        let result = self.observe_inner(observation, source_finished);
        if result.is_err() {
            self.cancel();
        }
        result
    }

    fn observe_inner(
        &mut self,
        observation: SampleObservation,
        finished: bool,
    ) -> Result<ExportSample, ExportFramePolicyError> {
        if observation.requested_time != self.requested_time
            || !observation.published_time.is_finite()
            || observation.published_time < 0.0
            || observation.published_time > observation.requested_time
            || (!finished && observation.published_time != observation.requested_time)
            || self
                .last_published_time
                .is_some_and(|previous| observation.published_time < previous)
            || self.terminal.is_some_and(|(time, publication)| {
                !finished
                    || observation.published_time != time
                    || observation.publication != publication
            })
        {
            return Err(ExportFramePolicyError::InvalidObservation);
        }
        let source_end = finished.then_some(observation.published_time);
        let source_limit = match source_end {
            Some(end) => {
                let held_end = end + self.options.final_hold_seconds;
                if !held_end.is_finite()
                    || (self.options.final_hold_seconds > 0.0 && held_end <= end)
                {
                    return Err(ExportFramePolicyError::InvalidHold);
                }
                held_end
            }
            None => f64::INFINITY,
        };
        let requested_limit = self.stop_time.unwrap_or(f64::INFINITY);
        let end = source_limit.min(requested_limit);
        let at_end = observation.requested_time >= end;
        if !at_end && self.index >= self.options.max_frames {
            return Err(ExportFramePolicyError::FrameLimit);
        }
        let frame = if !at_end && self.index >= self.options.start_frame {
            Some(ExportFrame {
                source_sample: self.grid.sample(self.index)?,
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
                start_time: self.grid.sample(self.options.start_frame)?.authored_time(),
                end_time: end,
                source_end,
                scheduled_duration: self.grid.sample(self.emitted)?.authored_time(),
                reason: if source_limit <= requested_limit {
                    ExportEndReason::SourceEnd
                } else {
                    ExportEndReason::RequestedStop
                },
            })
        } else {
            None
        };
        self.last_published_time = Some(observation.published_time);
        if finished {
            self.terminal = Some((observation.published_time, observation.publication));
        }
        self.pending = Some((sample, summary));
        Ok(sample)
    }

    /// Release the exact offered sample after host-side publication/output work.
    /// This acknowledges output policy only, never a runtime completion barrier.
    pub fn acknowledge_sample(
        &mut self,
        expected: ExportSample,
    ) -> Result<(), ExportFramePolicyError> {
        if self.failed || self.complete.is_some() {
            return Err(ExportFramePolicyError::Inactive);
        }
        let Some((sample, summary)) = self.pending else {
            return Err(ExportFramePolicyError::WrongSample);
        };
        if sample != expected {
            return Err(ExportFramePolicyError::WrongSample);
        }
        let result = (|| {
            if let Some(summary) = summary {
                if summary.frames == 0 {
                    return Err(ExportFramePolicyError::EmptyInterval);
                }
                self.pending = None;
                self.complete = Some(summary);
                return Ok(());
            }
            let next = self
                .index
                .checked_add(1)
                .ok_or(ExportFramePolicyError::FrameLimit)?;
            let grid_time = self.grid.sample(next)?.authored_time();
            // Drain an off-grid requested end exactly, never to the next grid point.
            self.requested_time = self.stop_time.map_or(grid_time, |end| grid_time.min(end));
            if sample.frame.is_some() {
                self.emitted += 1; // index < max_frames was checked before the offer
            }
            self.index = next;
            self.pending = None;
            Ok(())
        })();
        if result.is_err() {
            self.cancel();
        }
        result
    }

    pub fn cancel(&mut self) {
        self.failed = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFramePolicyError {
    InvalidOptions,
    InvalidHold,
    InvalidObservation,
    EmptyInterval,
    FrameLimit,
    WrongSample,
    Inactive,
    Grid(FrameGridError),
}

impl From<FrameGridError> for ExportFramePolicyError {
    fn from(error: FrameGridError) -> Self {
        Self::Grid(error)
    }
}

impl fmt::Display for ExportFramePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
        }
    }
}

impl Error for ExportFramePolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Grid(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
