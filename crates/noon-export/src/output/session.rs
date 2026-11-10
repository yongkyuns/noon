//! Native file output for one finite immutable plan or sealed source history.

use std::{error::Error, fmt, io};

use noon::integration::{
    ExportEndReason, ExportFrameOptions, ExportFramePolicy, ExportFramePolicyError,
    ExportFramePolicyStatus, SampleObservation,
};
use noon::{EvaluationError, ExecutionSession, ExecutionSessionInputError, SceneExecution};

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
    export_source(
        FiniteSource::Timeline(session),
        source_end,
        frame_options,
        capture_options,
        output,
        &mut || Ok(()),
    )
}

/// Export the existing native viewer owner without executing its source again.
///
/// Complete immutable plans use the same path as [`export_session_file`]. A
/// retained source must already be sealed and satisfy the engine's viewer
/// admission (no unrecorded input or callbacks). Its canonical history supplies
/// the endpoint and every sample through the existing typed seek operation.
/// Audio in retained history remains unsupported and is never silently omitted.
/// The shared frame policy consumes the zero-origin prefix before a crop.
///
/// A fresh renderer is used for each export. On failure, discard the owner;
/// neither consumed renderer publications nor partial files are retried.
pub fn export_scene_file(
    scene: &mut SceneExecution,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, SessionFileExportError> {
    export_scene_file_with_interrupt(scene, frame_options, capture_options, output, || Ok(()))
}

/// The scene exporter with a cooperative host interrupt check.
///
/// The check runs on the owner thread between samples and immediately before
/// destination publication, including after encoder finalization. Returning an
/// error abandons the run and preserves an existing destination. This hook must
/// not mutate the scene or supply time; it is for language-host signals. GPU
/// calls and encoder waits retain their configured finite deadlines.
pub fn export_scene_file_with_interrupt(
    scene: &mut SceneExecution,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
    mut interrupt: impl FnMut() -> io::Result<()>,
) -> Result<FileExportSummary, SessionFileExportError> {
    interrupt().map_err(SessionFileExportError::Output)?;
    check_publication_cancellation(&capture_options.cancellation)
        .map_err(SessionFileExportError::Output)?;
    let source_end = if scene.session().replay_scope_active() {
        if !scene.session().replay_is_sealed() {
            return Err(SessionFileExportError::UnsupportedHistory);
        }
        scene
            .session_mut()
            .seal_replay_for_viewer()
            .map_err(|_| SessionFileExportError::UnsupportedHistory)?;
        if scene.session().has_audio() {
            return Err(SessionFileExportError::HistoryAudio);
        }
        scene
            .session()
            .replay_end_time()
            .ok_or(SessionFileExportError::UnsupportedHistory)?
    } else {
        scene
            .session()
            .finite_playback_end_time()
            .ok_or(SessionFileExportError::UnsupportedSession)?
    };
    export_source(
        FiniteSource::Scene(scene),
        source_end,
        frame_options,
        capture_options,
        output,
        &mut interrupt,
    )
}

/// Only host ownership differs. Sampling, rendering, encoding and publication
/// remain a single implementation for native Rust and optional language hosts.
enum FiniteSource<'a> {
    Timeline(&'a mut ExecutionSession),
    Scene(&'a mut SceneExecution),
}

impl FiniteSource<'_> {
    fn session(&self) -> &ExecutionSession {
        match self {
            Self::Timeline(session) => session,
            Self::Scene(scene) => scene.session(),
        }
    }

    fn session_mut(&mut self) -> &mut ExecutionSession {
        match self {
            Self::Timeline(session) => session,
            Self::Scene(scene) => scene.session_mut(),
        }
    }

    fn sample(&mut self, time: f64) -> Result<(), SessionFileExportError> {
        match self {
            Self::Timeline(session) => session
                .advance_to(time)
                .map(|_| ())
                .map_err(SessionFileExportError::Evaluation),
            Self::Scene(scene) => scene
                .seek(time)
                .map(|_| ())
                .map_err(SessionFileExportError::Playback),
        }
    }
}

fn export_source(
    mut source: FiniteSource<'_>,
    source_end: f64,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
    interrupt: &mut impl FnMut() -> io::Result<()>,
) -> Result<FileExportSummary, SessionFileExportError> {
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
    let soundtrack = if output.format == OutputFormat::Mp4 && source.session().has_audio() {
        Some(Soundtrack::new(source.session(), planned).map_err(SessionFileExportError::Output)?)
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
    interrupt().map_err(SessionFileExportError::Output)?;
    // A viewer can remain at authored time zero while an input-triggered effect
    // is active. Paired seek retires that effect and its temporary attachments
    // before the first authored sample, including a static scene's frozen hold.
    if matches!(&source, FiniteSource::Scene(_)) || source.session().frame().time != 0.0 {
        source.sample(0.0)?;
    }
    let mut capture = SessionCapture::new(source.session(), capture_options)
        .map_err(SessionFileExportError::Capture)?;
    let sampling = loop {
        interrupt().map_err(SessionFileExportError::Output)?;
        check_publication_cancellation(&cancellation).map_err(SessionFileExportError::Output)?;
        match policy.status().map_err(SessionFileExportError::Sampling)? {
            ExportFramePolicyStatus::NeedsSample(requested_time) => {
                let time = requested_time.min(source_end);
                // A terminal hold reuses the already settled publication. It
                // cannot create another runtime epoch or extend authored audio.
                if source.session().frame().time != time {
                    source.sample(time)?;
                }
                policy
                    .observe(
                        SampleObservation {
                            requested_time,
                            published_time: source.session().frame().time,
                            publication: source.session().publication_context(),
                        },
                        requested_time >= source_end,
                    )
                    .map_err(SessionFileExportError::Sampling)?;
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                if let Some(frame) = sample.frame {
                    let captured = capture
                        .capture(source.session_mut())
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
                        .render(source.session_mut())
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
        .finish_with_source_end_and_interrupt(sampling.frames, source_duration, interrupt)
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
    UnsupportedHistory,
    HistoryAudio,
    Sampling(ExportFramePolicyError),
    Evaluation(EvaluationError),
    Playback(ExecutionSessionInputError),
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
            Self::UnsupportedHistory => formatter.write_str(
                "source export requires completed sealed deterministic history without unrecorded callbacks or input",
            ),
            Self::HistoryAudio => formatter.write_str(
                "audio export from retained source history is not supported; use a finite immutable session (#2263)",
            ),
            Self::Sampling(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Playback(error) => error.fmt(formatter),
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
            Self::Playback(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::SessionNotAtStart
            | Self::UnsupportedSession
            | Self::UnsupportedHistory
            | Self::HistoryAudio => None,
        }
    }
}
