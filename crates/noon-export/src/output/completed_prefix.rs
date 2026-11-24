//! Native file output from one terminal-adopted history.

use std::{io, path::Path};

use noon::integration::{
    CompletedPrefixExport, ExportEndReason, ExportFrameOptions, ExportFramePolicyStatus,
};
use noon::PrefixReader;

use super::{
    audio, check_publication_cancellation, FileExportSummary, FileSink, OutputFormat,
    OutputOptions, SessionFileExportError,
};
use crate::{CaptureCancellation, CaptureOptions, CapturedFrame, SessionCapture};

/// Export a completed reader at time zero using its original history and soundtrack.
/// The file is published after all renderer receipts and encoder work complete.
pub fn export_completed_prefix_file(
    reader: &mut PrefixReader,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, SessionFileExportError> {
    use SessionFileExportError::{Capture, CompletedHistory, Output};

    let cancellation = capture_options.cancellation.clone();
    check_publication_cancellation(&cancellation).map_err(Output)?;
    capture_options
        .layout()
        .map_err(|error| Output(io::Error::other(error)))?;
    let mut export = CompletedPrefixExport::new(reader, frame_options).map_err(CompletedHistory)?;
    let mut soundtrack = export
        .audio_grid()
        .filter(|_| output.format == OutputFormat::Mp4)
        .map(|grid| {
            super::terminal_duration::validate_soundtrack_grid(
                frame_options.frame_rate,
                export.planned().frames,
                grid.frames(),
            )?;
            Ok(PrefixSoundtrack {
                reader: &*reader,
                export: &export,
                frames: grid.frames(),
            })
        })
        .transpose()
        .map_err(Output)?;
    if output.format == OutputFormat::PngSequence && frame_options.max_frames > i32::MAX as u64 {
        return Err(Output(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PNG frame limit exceeds image2 numbering",
        )));
    }
    let mut sink = FileSink::with_staged_soundtrack(
        output,
        frame_options.frame_rate,
        capture_options.width,
        capture_options.height,
        cancellation.clone(),
        soundtrack
            .as_mut()
            .map(|track| track as &mut dyn audio::StagedSoundtrack),
    )
    .map_err(Output)?;
    let mut capture = SessionCapture::new_prefix(reader, capture_options).map_err(Capture)?;
    let sampling = loop {
        check_publication_cancellation(&cancellation).map_err(Output)?;
        match export.advance(reader).map_err(CompletedHistory)? {
            ExportFramePolicyStatus::SampleReady(sample) => {
                let captured = capture
                    .capture_prefix(reader, sample.frame.is_some())
                    .map_err(Capture)?;
                super::session::check_receipt(captured.receipt, sample.observation)?;
                if let Some(frame) = sample.frame {
                    sink.write(CapturedFrame {
                        frame,
                        observation: sample.observation,
                        width: captured.width,
                        height: captured.height,
                        format: captured.format,
                        rgba: captured.rgba,
                        work: captured.receipt.work,
                    })
                    .map_err(Output)?;
                }
                export
                    .acknowledge_sample(reader, sample)
                    .map_err(CompletedHistory)?;
            }
            ExportFramePolicyStatus::Complete(sampling) => break sampling,
            ExportFramePolicyStatus::NeedsSample(_) => unreachable!("advance settles the sample"),
        }
    };
    check_publication_cancellation(&cancellation).map_err(Output)?;
    let source_duration = if frame_options.final_hold_seconds == 0.0
        && sampling.reason == ExportEndReason::SourceEnd
    {
        sampling.source_end.map(|end| end - sampling.start_time)
    } else {
        None
    };
    let (path, format, encoder_diagnostics) = sink
        .finish_with_source_end(sampling.frames, source_duration)
        .map_err(Output)?;
    Ok(FileExportSummary {
        capture: capture.summary(sampling),
        path,
        format,
        encoder_diagnostics,
    })
}

struct PrefixSoundtrack<'a> {
    reader: &'a PrefixReader,
    export: &'a CompletedPrefixExport,
    frames: u64,
}

impl audio::StagedSoundtrack for PrefixSoundtrack<'_> {
    fn frames(&self) -> u64 {
        self.frames
    }

    fn stage(&mut self, directory: &Path, cancellation: &CaptureCancellation) -> io::Result<()> {
        audio::stage_pcm(directory, self.frames, cancellation, |first, output| {
            self.export
                .mix_audio_block(self.reader, first, output)
                .map_err(io::Error::other)
        })
    }
}
