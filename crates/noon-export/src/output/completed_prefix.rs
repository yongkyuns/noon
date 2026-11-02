//! Native output over one existing completed-history runtime and soundtrack.

use std::{error::Error, fmt, io};

use noon::integration::{
    CompletedPrefixExport, CompletedPrefixExportError, ExportEndReason, ExportFrameOptions,
    ExportFramePolicyStatus,
};
use noon::PrefixReader;

use super::{audio::Soundtrack, check_publication_cancellation, FileExportSummary, FileSink};
use super::{OutputFormat, OutputOptions};
use crate::{CaptureOptions, CapturedFrame, SessionCapture, SessionCaptureError};

/// Capture and encode an already terminal-adopted deterministic history.
///
/// The caller supplies the original reader at time zero. This function does not
/// prepare or rerun source code, reconstruct a scene, or create another runtime.
/// Shared [`CompletedPrefixExport`] owns frame/range decisions and actual history
/// sampling; the same native retained renderer, immutable PCM mixer, encoder and
/// atomic destination owner used by finite session output handle the result.
///
/// Crops consume the zero-origin visual prefix. PCM keeps absolute authored
/// phase, is cut off at the completed source endpoint, and is silent throughout
/// terminal picture holds. PNG is visual only. A success is reported only after
/// the file sink has drained and finalized output. Failures abandon the capture;
/// a caller may explicitly seek its retained deterministic reader back to zero
/// before starting a separate export, without rerunning the source.
pub fn export_completed_prefix_file(
    reader: &mut PrefixReader,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, CompletedPrefixFileExportError> {
    check_publication_cancellation(&capture_options.cancellation)?;
    let mut export = CompletedPrefixExport::new(reader, frame_options)?;
    capture_options.layout().map_err(io::Error::other)?;
    output.validate(
        frame_options.frame_rate,
        capture_options.width,
        capture_options.height,
    )?;
    if output.format == OutputFormat::PngSequence && frame_options.max_frames > i32::MAX as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PNG frame limit exceeds image2 numbering",
        )
        .into());
    }
    let soundtrack = if output.format == OutputFormat::Mp4 && reader.has_audio() {
        Some(Soundtrack::completed_prefix(reader, &export)?)
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
    )?;
    let mut capture = SessionCapture::new_prefix(reader, capture_options)
        .map_err(CompletedPrefixFileExportError::Capture)?;
    let sampling = loop {
        check_publication_cancellation(&cancellation)?;
        match export.advance(reader)? {
            ExportFramePolicyStatus::NeedsSample(_) => unreachable!("advance settles one sample"),
            ExportFramePolicyStatus::SampleReady(sample) => {
                if let Some(frame) = sample.frame {
                    let captured = capture
                        .capture_prefix(reader)
                        .map_err(CompletedPrefixFileExportError::Capture)?;
                    super::session::check_receipt(captured.receipt, sample.observation)?;
                    sink.write(CapturedFrame {
                        frame,
                        observation: sample.observation,
                        width: captured.width,
                        height: captured.height,
                        format: captured.format,
                        rgba: captured.rgba,
                        work: captured.receipt.work,
                    })?;
                } else {
                    let receipt = capture
                        .render_prefix(reader)
                        .map_err(CompletedPrefixFileExportError::Capture)?;
                    super::session::check_receipt(receipt, sample.observation)?;
                }
                export.acknowledge_sample(reader, sample)?;
            }
            ExportFramePolicyStatus::Complete(sampling) => break sampling,
        }
    };
    check_publication_cancellation(&cancellation)?;
    let source_duration = if frame_options.final_hold_seconds == 0.0
        && sampling.reason == ExportEndReason::SourceEnd
    {
        sampling.source_end.map(|end| end - sampling.start_time)
    } else {
        None
    };
    let (path, format, encoder_diagnostics) =
        sink.finish_with_source_end(sampling.frames, source_duration)?;
    Ok(FileExportSummary {
        capture: capture.summary(sampling),
        path,
        format,
        encoder_diagnostics,
    })
}

#[derive(Debug)]
pub enum CompletedPrefixFileExportError {
    Sampling(CompletedPrefixExportError),
    Capture(SessionCaptureError),
    Output(io::Error),
}

impl From<CompletedPrefixExportError> for CompletedPrefixFileExportError {
    fn from(error: CompletedPrefixExportError) -> Self {
        Self::Sampling(error)
    }
}
impl From<io::Error> for CompletedPrefixFileExportError {
    fn from(error: io::Error) -> Self {
        Self::Output(error)
    }
}
impl fmt::Display for CompletedPrefixFileExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sampling(error) => error.fmt(formatter),
            Self::Capture(error) => error.fmt(formatter),
            Self::Output(error) => write!(formatter, "completed-history file export: {error}"),
        }
    }
}
impl Error for CompletedPrefixFileExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sampling(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::Output(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests;
