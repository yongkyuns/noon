//! Native file output for one complete immutable execution plan.

use std::{error::Error, fmt, io};

use noon::integration::{
    ExportEndReason, ExportFrameOptions, ExportFramePolicy, ExportFramePolicyError,
    ExportFramePolicyStatus, SampleObservation,
};
use noon::{EvaluationError, ExecutionSession};

use super::{audio::Soundtrack, check_publication_cancellation, FileExportSummary, FileSink};
use super::{OutputFormat, OutputOptions};
use crate::{CaptureOptions, CaptureReceipt, CapturedFrame, SessionCapture, SessionCaptureError};

/// Export a complete finite scene through the shared frame policy and renderer.
///
/// The session must be at time zero, without required callbacks, live segment
/// completion or retained source history. Its existing scheduler supplies the
/// finite endpoint. Crops consume the zero-origin visual prefix through
/// [`ExportFramePolicy`]; audio samples retain the absolute scene crop phase.
///
/// MP4 includes authored audio as 48 kHz stereo AAC when the scene has clips.
/// The shared Rust mixer applies their trims, loops, gains and fades. Viewer
/// mute/volume and realtime output ramps do not change an authored soundtrack.
/// PNG remains an explicitly visual diagnostic format.
///
/// Bounded PCM blocks are staged privately before rendering, then supplied as
/// a file to the same FFmpeg process that accepts the video frames. Audio never
/// advances the visual runtime or sets the movie length. Overload, cancellation,
/// capture, encoder or finalization errors preserve an existing destination.
/// A failed run is abandoned; construct a new session before another export.
pub fn export_session_file(
    session: &mut ExecutionSession,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, SessionFileExportError> {
    check_publication_cancellation(&capture_options.cancellation)
        .map_err(SessionFileExportError::Output)?;
    if session.frame().time != 0.0 {
        return Err(SessionFileExportError::SessionNotAtStart);
    }
    if session.pending_callback_token().is_some() || session.replay_scope_active() {
        return Err(SessionFileExportError::UnsupportedSession);
    }
    let source_end = session
        .finite_timeline_end_time()
        .ok_or(SessionFileExportError::UnsupportedSession)?;
    capture_options
        .layout()
        .map_err(|error| SessionFileExportError::Output(io::Error::other(error)))?;
    output
        .validate(
            frame_options.frame_rate,
            capture_options.width,
            capture_options.height,
        )
        .map_err(SessionFileExportError::Output)?;
    if output.format == OutputFormat::PngSequence && frame_options.max_frames > i32::MAX as u64 {
        return Err(SessionFileExportError::Output(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PNG frame limit exceeds image2 numbering",
        )));
    }
    let mut policy =
        ExportFramePolicy::new(frame_options).map_err(SessionFileExportError::Sampling)?;
    let planned = policy
        .finite_summary(source_end)
        .map_err(SessionFileExportError::Sampling)?;
    let soundtrack = if output.format == OutputFormat::Mp4 && session.has_audio() {
        Some(Soundtrack::new(session, planned).map_err(SessionFileExportError::Output)?)
    } else {
        None
    };
    let cancellation = capture_options.cancellation.clone();
    let mut sink = FileSink::with_soundtrack(
        output,
        frame_options.frame_rate,
        capture_options.width,
        capture_options.height,
        cancellation.clone(),
        soundtrack,
    )
    .map_err(SessionFileExportError::Output)?;
    let mut capture =
        SessionCapture::new(session, capture_options).map_err(SessionFileExportError::Capture)?;
    let sampling = loop {
        check_publication_cancellation(&cancellation).map_err(SessionFileExportError::Output)?;
        match policy.status().map_err(SessionFileExportError::Sampling)? {
            ExportFramePolicyStatus::NeedsSample(requested_time) => {
                let time = requested_time.min(source_end);
                // A terminal hold reuses the already settled publication. It
                // cannot create another runtime epoch or extend authored audio.
                if session.frame().time != time {
                    session
                        .advance_to(time)
                        .map_err(SessionFileExportError::Evaluation)?;
                }
                policy
                    .observe(
                        SampleObservation {
                            requested_time,
                            published_time: session.frame().time,
                            publication: session.publication_context(),
                        },
                        requested_time >= source_end,
                    )
                    .map_err(SessionFileExportError::Sampling)?;
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                if let Some(frame) = sample.frame {
                    let captured = capture
                        .capture(session)
                        .map_err(SessionFileExportError::Capture)?;
                    check_receipt(captured.receipt, sample.observation)?;
                    sink.write(CapturedFrame {
                        frame,
                        observation: sample.observation,
                        width: captured.width,
                        height: captured.height,
                        format: captured.format,
                        rgba: captured.rgba,
                        work: captured.receipt.work,
                    })
                    .map_err(SessionFileExportError::Output)?;
                } else {
                    let receipt = capture
                        .render(session)
                        .map_err(SessionFileExportError::Capture)?;
                    check_receipt(receipt, sample.observation)?;
                }
                policy
                    .acknowledge_sample(sample)
                    .map_err(SessionFileExportError::Sampling)?;
            }
            ExportFramePolicyStatus::Complete(sampling) => break sampling,
        }
    };
    if sampling != planned {
        return Err(SessionFileExportError::Output(io::Error::other(
            "completed sampling differs from the admitted finite output range",
        )));
    }
    check_publication_cancellation(&cancellation).map_err(SessionFileExportError::Output)?;
    let source_duration = if frame_options.final_hold_seconds == 0.0
        && sampling.reason == ExportEndReason::SourceEnd
    {
        sampling.source_end.map(|end| end - sampling.start_time)
    } else {
        None
    };
    let (path, format, encoder_diagnostics) = sink
        .finish_with_source_end(sampling.frames, source_duration)
        .map_err(SessionFileExportError::Output)?;
    Ok(FileExportSummary {
        capture: capture.summary(sampling),
        path,
        format,
        encoder_diagnostics,
    })
}

fn check_receipt(
    receipt: CaptureReceipt,
    observation: SampleObservation,
) -> Result<(), SessionFileExportError> {
    if receipt.publication != observation.publication
        || receipt.published_time != observation.published_time
    {
        return Err(SessionFileExportError::Output(io::Error::new(
            io::ErrorKind::InvalidData,
            "capture publication differs from its admitted sample",
        )));
    }
    Ok(())
}

#[derive(Debug)]
pub enum SessionFileExportError {
    SessionNotAtStart,
    UnsupportedSession,
    Sampling(ExportFramePolicyError),
    Evaluation(EvaluationError),
    Capture(SessionCaptureError),
    Output(io::Error),
}

impl fmt::Display for SessionFileExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionNotAtStart => formatter.write_str("finite export requires a session at time zero"),
            Self::UnsupportedSession => formatter.write_str(
                "finite export requires a complete immutable timeline without callbacks, pending segments or source history (#2263)",
            ),
            Self::Sampling(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Capture(error) => error.fmt(formatter),
            Self::Output(error) => write!(formatter, "file export: {error}"),
        }
    }
}

impl Error for SessionFileExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sampling(error) => Some(error),
            Self::Evaluation(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::SessionNotAtStart | Self::UnsupportedSession => None,
        }
    }
}
