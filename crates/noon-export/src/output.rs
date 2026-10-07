//! Native FFmpeg file sinks over the existing capture consumer boundary.
//!
//! Both profiles use a trusted installed FFmpeg executable, never a browser or
//! shell. PNG bundles are lossless diagnostics; MP4 is SDR H.264/YUV420p. No output
//! is reported successful before EOF, encoder exit and destination publication.
mod destination;
mod process;

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::{
    capture_frames, CaptureCancellation, CaptureOptions, CapturePixelFormat, CaptureRunError,
    CaptureSummary, CapturedFrame,
};
use destination::Destination;
use noon::integration::{ExportFrameOptions, ExportFrames, FrameRate};
use noon::{LiveContinuation, LiveProgram, RustHostCallbackTable};
use process::Encoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Mp4,
    /// A newly reserved bundle with committed frames/ and frames/timing.tsv.
    /// Uses FFmpeg's PNG encoder; no video codec or chroma subsampling is involved.
    PngSequence,
}

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub path: PathBuf,
    pub format: OutputFormat,
    pub ffmpeg: PathBuf,
    /// Explicit atomic file replacement. PNG bundles never replace directories.
    pub overwrite: bool,
    /// MP4 constant-rate-factor quality (0..=51); ignored for lossless PNG.
    pub crf: u8,
    /// Per input-write / EOF-finalization deadline, not a limit on user callbacks.
    pub io_timeout: Duration,
}

impl OutputOptions {
    pub fn mp4(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: OutputFormat::Mp4,
            ffmpeg: "ffmpeg".into(),
            overwrite: false,
            crf: 18,
            io_timeout: Duration::from_secs(30),
        }
    }

    pub fn png_sequence(path: impl Into<PathBuf>) -> Self {
        Self {
            format: OutputFormat::PngSequence,
            ..Self::mp4(path)
        }
    }

    fn validate(&self, rate: FrameRate, width: u32, height: u32) -> io::Result<()> {
        if width == 0
            || height == 0
            || self.crf > 51
            || self.io_timeout.is_zero()
            || self.io_timeout > Duration::from_secs(3600)
            || rate.numerator() > i32::MAX as u32
            || rate.denominator() > i32::MAX as u32
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid encoder dimensions, rate, quality or deadline",
            ));
        }
        if self.format == OutputFormat::Mp4
            && (!width.is_multiple_of(2) || !height.is_multiple_of(2))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "YUV420p needs even dimensions; use PNG for odd sizes",
            ));
        }
        if self.format == OutputFormat::PngSequence && self.overwrite {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PNG output requires a new bundle directory",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileExportSummary {
    pub capture: CaptureSummary,
    pub path: PathBuf,
    pub format: OutputFormat,
    pub encoder_diagnostics: String,
}

/// Export through the SAME shared timing, callbacks and production renderer.
///
/// MP4 preserves the integer rational grid and uses limited-range BT.709 YCbCr
/// derived from the renderer's sRGB-encoded, BT.709-primary RGB convention. The
/// sRGB transfer is preserved and tagged, not falsely retagged as BT.709 transfer.
/// PNG preserves all captured RGBA bytes. Both currently require opaque input.
///
/// A bounded native writer copies into one reusable frame buffer. Only that
/// buffer crosses threads; source/callback objects need not implement Send.
/// Cancellation/deadlines kill FFmpeg to unblock pipe writes. OS process reaping,
/// synchronous callbacks, filesystems and hostile executables are not preemptible.
///
/// The default MP4 publication cannot overwrite an existing path, including a
/// race after preflight. PNG reserves a new outer directory; frames/ appears only
/// when the complete sequence and timing manifest are ready. A crash may leave
/// an unmistakable .incomplete directory. Neither profile promises crash durability
/// of the final directory entry or atomicity against external directory mutation.
pub fn export_file<C: LiveContinuation>(
    program: &mut LiveProgram<C>,
    callbacks: &mut RustHostCallbackTable,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, FileExportError<C::Error>> {
    capture_options
        .layout()
        .map_err(|e| FileExportError::Output(io::Error::other(e)))?;
    output
        .validate(
            frame_options.frame_rate,
            capture_options.width,
            capture_options.height,
        )
        .map_err(FileExportError::Output)?;
    // Validation does not resume or mutate the source. Do not create files for an
    // invalid run, nor discover a missing codec only after executing user code.
    {
        let _validation = ExportFrames::new(program, callbacks, frame_options)
            .map_err(|e| FileExportError::Capture(CaptureRunError::Sampling(e)))?;
    }
    if capture_options.cancellation.is_cancelled() {
        return Err(FileExportError::Capture(CaptureRunError::Cancelled));
    }
    if output.format == OutputFormat::PngSequence && frame_options.max_frames > i32::MAX as u64 {
        return Err(FileExportError::Output(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PNG frame limit exceeds image2 numbering",
        )));
    }
    let cancellation = capture_options.cancellation.clone();
    let mut sink = FileSink::new(
        output,
        frame_options.frame_rate,
        capture_options.width,
        capture_options.height,
        cancellation.clone(),
    )
    .map_err(FileExportError::Output)?;
    let capture = capture_frames(
        program,
        callbacks,
        frame_options,
        capture_options,
        |frame| sink.write(frame),
    )
    .map_err(FileExportError::Capture)?;
    if cancellation.is_cancelled() {
        return Err(FileExportError::Capture(CaptureRunError::Cancelled));
    }
    let (path, format, encoder_diagnostics) = sink
        .finish(capture.sampling.frames)
        .map_err(FileExportError::Output)?;
    Ok(FileExportSummary {
        capture,
        path,
        format,
        encoder_diagnostics,
    })
}

struct FileSink {
    // Encoder drops BEFORE its destination so an aborted process cannot keep
    // writing into a directory while the file owner is cleaning it up.
    encoder: Option<Encoder>,
    manifest: Option<BufWriter<File>>,
    destination: Option<Destination>,
    options: OutputOptions,
    rate: FrameRate,
    width: u32,
    height: u32,
    bytes: usize,
    frames: u64,
    failed: bool,
    cancellation: CaptureCancellation,
}

impl FileSink {
    fn new(
        mut options: OutputOptions,
        rate: FrameRate,
        width: u32,
        height: u32,
        cancellation: CaptureCancellation,
    ) -> io::Result<Self> {
        options.validate(rate, width, height)?;
        if options.ffmpeg.components().count() > 1 && !options.ffmpeg.is_absolute() {
            options.ffmpeg = std::env::current_dir()?.join(&options.ffmpeg);
        }
        let bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| usize::try_from(v).ok())
            .filter(|&v| v <= isize::MAX as usize)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame size overflow"))?;
        let destination = Destination::new(&options)?;
        // A real one-frame probe checks this installed build's codec, muxer and
        // required options. Its image is discarded; it is never an authored frame.
        let probe = destination.work.join("probe");
        fs::create_dir(&probe)?;
        let mut command = encoder_command(&options, rate, 2, 2, &probe);
        let mut encoder = Encoder::new(&mut command, 16, cancellation.clone(), options.io_timeout)?;
        encoder.write(&[0, 0, 0, 255].repeat(4))?;
        encoder.finish()?;
        fs::remove_dir_all(probe)?;
        let directory = if options.format == OutputFormat::PngSequence {
            destination.work.join("frames")
        } else {
            destination.work.clone()
        };
        let manifest = if options.format == OutputFormat::PngSequence {
            let mut file = BufWriter::new(File::create(directory.join("timing.tsv"))?);
            writeln!(
                file,
                "# noon-rgba8-v1 fps={}/{} width={} height={}",
                rate.numerator(),
                rate.denominator(),
                width,
                height
            )?;
            writeln!(
                file,
                "pts\tsource_index\trequested_time\tpublished_time\theld"
            )?;
            Some(file)
        } else {
            None
        };
        let encoder = Encoder::new(
            &mut encoder_command(&options, rate, width, height, &directory),
            bytes,
            cancellation.clone(),
            options.io_timeout,
        )?;
        Ok(Self {
            encoder: Some(encoder),
            manifest,
            destination: Some(destination),
            options,
            rate,
            width,
            height,
            bytes,
            frames: 0,
            failed: false,
            cancellation,
        })
    }

    fn write(&mut self, frame: CapturedFrame<'_>) -> io::Result<()> {
        let result = (|| {
            if self.failed
                || frame.frame.pts != self.frames
                || frame.frame.source_sample.time_base() != self.rate.time_base()
                || frame.observation.requested_time != frame.frame.source_sample.authored_time()
                || !frame.observation.published_time.is_finite()
                || frame.observation.published_time < 0.0
                || frame.observation.published_time > frame.observation.requested_time
                || (!frame.frame.held
                    && frame.observation.published_time != frame.observation.requested_time)
                || frame.width != self.width
                || frame.height != self.height
                || frame.format != CapturePixelFormat::RendererRgba8UnormOpaque
                || frame.rgba.len() != self.bytes
                || frame.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid, out-of-order or nonopaque captured frame",
                ));
            }
            self.encoder
                .as_mut()
                .ok_or_else(|| io::Error::other("encoder already finished"))?
                .write(frame.rgba)?;
            if let Some(manifest) = self.manifest.as_mut() {
                writeln!(
                    manifest,
                    "{}\t{}\t{:.17}\t{:.17}\t{}",
                    frame.frame.pts,
                    frame.frame.source_sample.index(),
                    frame.observation.requested_time,
                    frame.observation.published_time,
                    frame.frame.held
                )?;
            }
            self.frames = self
                .frames
                .checked_add(1)
                .ok_or_else(|| io::Error::other("output frame overflow"))?;
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
            self.encoder.take();
        }
        result
    }

    fn finish(mut self, expected_frames: u64) -> io::Result<(PathBuf, OutputFormat, String)> {
        if self.failed || self.frames == 0 || self.frames != expected_frames {
            return Err(io::Error::other(
                "sampling and encoder frame counts disagree",
            ));
        }
        let diagnostics = self
            .encoder
            .take()
            .ok_or_else(|| io::Error::other("encoder is inactive"))?
            .finish()?;
        check_publication_cancellation(&self.cancellation)?;
        if let Some(mut manifest) = self.manifest.take() {
            writeln!(manifest, "# complete frames={}", self.frames)?;
            manifest.flush()?;
            manifest.get_ref().sync_all()?;
        }
        let path = self
            .destination
            .take()
            .ok_or_else(|| io::Error::other("output already published"))?
            .publish(self.frames, || {
                check_publication_cancellation(&self.cancellation)
            })?;
        Ok((path, self.options.format, diagnostics))
    }
}

fn check_publication_cancellation(cancellation: &CaptureCancellation) -> io::Result<()> {
    if cancellation.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "export cancelled before publication",
        ))
    } else {
        Ok(())
    }
}

fn encoder_command(
    options: &OutputOptions,
    rate: FrameRate,
    width: u32,
    height: u32,
    directory: &std::path::Path,
) -> Command {
    let mut command = Command::new(&options.ffmpeg);
    command
        .current_dir(directory)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-nostats",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
        ])
        .arg(format!("{width}x{height}"))
        .arg("-framerate")
        .arg(format!("{}/{}", rate.numerator(), rate.denominator()))
        .args([
            "-i",
            "pipe:0",
            "-an",
            "-sn",
            "-dn",
            "-fps_mode",
            "passthrough",
        ]);
    match options.format {
        OutputFormat::Mp4 => {
            command.args(["-vf", "scale=in_range=full:out_range=limited:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
                "-c:v", "libx264", "-preset", "veryfast", "-crf"])
                .arg(options.crf.to_string()).args(["-bf", "0", "-enc_time_base"])
                .arg(format!("{}:{}", rate.denominator(), rate.numerator()))
                .arg("-video_track_timescale").arg(rate.numerator().to_string())
                .args(["-movflags", "+faststart", "-f", "mp4", "video.mp4"]);
        }
        OutputFormat::PngSequence => {
            command.args([
                "-c:v",
                "png",
                "-pix_fmt",
                "rgba",
                "-threads",
                "1",
                "-start_number",
                "0",
                "-f",
                "image2",
                "frame-%010d.png",
            ]);
        }
    }
    command
}

#[derive(Debug)]
pub enum FileExportError<C> {
    Capture(CaptureRunError<C, io::Error>),
    Output(io::Error),
}
impl<C: fmt::Display> fmt::Display for FileExportError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(e) => e.fmt(f),
            Self::Output(e) => write!(f, "file export: {e}"),
        }
    }
}
impl<C: Error + 'static> Error for FileExportError<C> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(e) => Some(e),
            Self::Output(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests;
