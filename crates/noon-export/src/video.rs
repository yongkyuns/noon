//! Optional native H.264/MP4 output for the existing capture consumer boundary.
//!
//! Requires a trusted FFmpeg executable with libx264, rawvideo, scale and MP4.
//! Frames retain the shared rational grid; no output-rate conversion is used.
//! The v1 SDR profile treats captured UNORM RGB bytes as sRGB-coded values,
//! converts full-range RGB to limited-range BT.709 YCbCr, and retains the sRGB
//! transfer tag. It is not a transfer-curve conversion to BT.709 or HDR output.

mod destination;
mod process;

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use noon::integration::{ExportFrameOptions, FrameRate, Rgba8ReadbackLayout};
use noon::{LiveContinuation, LiveProgram, RustHostCallbackTable};

use crate::{capture_frames, CaptureCancellation, CaptureOptions, CapturePixelFormat, CaptureRunError, CaptureSummary, CapturedFrame};
use destination::Destination;
use process::EncoderProcess;

/// Fixed-profile native video options. FFmpeg is only required when this adapter
/// is used; raw capture and the shared engine have no executable dependency.
#[derive(Clone, Debug)]
pub struct VideoOptions {
    pub output: PathBuf,
    pub ffmpeg: PathBuf,
    pub overwrite: bool,
    /// libx264 quality (0 is lossless YCbCr, NOT lossless RGB after subsampling).
    pub crf: u8,
    pub preset: String,
    /// Zero lets FFmpeg choose its encoder worker count.
    pub encoder_threads: u32,
    pub max_frame_bytes: u64,
    /// Deadline for each blocking frame write; idle authoring is not timed.
    pub write_timeout: Duration,
    /// Deadline for closing the input, encoding delayed packets and muxer flush.
    pub finalize_timeout: Duration,
}

impl VideoOptions {
    pub fn mp4(output: impl Into<PathBuf>) -> Self {
        Self {
            output: output.into(), ffmpeg: "ffmpeg".into(), overwrite: false,
            crf: 18, preset: "medium".into(), encoder_threads: 0,
            max_frame_bytes: 256 * 1024 * 1024,
            write_timeout: Duration::from_secs(30), finalize_timeout: Duration::from_secs(120),
        }
    }

    fn validate(&self, width: u32, height: u32, rate: FrameRate) -> Result<usize, VideoError> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(VideoError::Configuration("H.264 yuv420p requires positive even dimensions; no implicit resize"));
        }
        if rate.numerator() > i32::MAX as u32 || rate.denominator() > i32::MAX as u32 {
            return Err(VideoError::Configuration("frame rate exceeds FFmpeg's rational/time-scale range"));
        }
        if self.crf > 51 || !["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"].contains(&self.preset.as_str()) {
            return Err(VideoError::Configuration("invalid H.264 quality or preset"));
        }
        if self.encoder_threads > i32::MAX as u32 {
            return Err(VideoError::Configuration("encoder thread count is out of range"));
        }
        for timeout in [self.write_timeout, self.finalize_timeout] {
            if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
                return Err(VideoError::Configuration("encoder deadlines must be finite and positive"));
            }
        }
        let layout = Rgba8ReadbackLayout::new(width, height, 1, self.max_frame_bytes)
            .map_err(|e| VideoError::PixelLayout(e.to_string()))?;
        Ok(layout.packed_len())
    }

    fn command(&self, path: &Path, width: u32, height: u32, rate: FrameRate) -> Command {
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-hide_banner", "-loglevel", "error", "-nostats", "-nostdin", "-y",
            "-f", "rawvideo", "-pixel_format", "rgba", "-video_size"])
            .arg(format!("{width}x{height}"))
            .arg("-framerate").arg(format!("{}/{}", rate.numerator(), rate.denominator()))
            .args(["-i", "pipe:0", "-map", "0:v:0", "-an", "-sn", "-dn", "-vf",
                "scale=in_range=full:out_range=limited:out_color_matrix=bt709:flags=accurate_rnd+full_chroma_int,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
                "-c:v", "libx264", "-preset"])
            .arg(&self.preset).arg("-crf").arg(self.crf.to_string())
            .arg("-threads").arg(self.encoder_threads.to_string())
            .args(["-fps_mode", "passthrough", "-enc_time_base"])
            .arg(format!("{}:{}", rate.denominator(), rate.numerator()))
            .arg("-video_track_timescale").arg(rate.numerator().to_string())
            .arg("-movie_timescale").arg(rate.numerator().to_string())
            .args(["-color_range", "tv", "-colorspace", "bt709", "-color_primaries", "bt709",
                "-color_trc", "iec61966-2-1", "-movflags", "+faststart", "-f", "mp4"])
            .arg(path);
        command
    }
}

/// A successfully finalized and published file. This does not claim that an
/// independent decoder was run for every export; qualification does that in CI.
#[derive(Clone, Debug)]
pub struct VideoSummary {
    pub output: PathBuf,
    pub bytes: u64,
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
}

#[derive(Debug)]
pub struct VideoExportSummary {
    pub capture: CaptureSummary,
    pub video: VideoSummary,
}

/// Ordered frame consumer. Dropping it without successful `finish` kills the
/// encoder and removes its private staging output, never an existing destination.
/// Source/frame objects are not moved onto its process-supervision threads.
pub struct Mp4Encoder {
    process: Option<EncoderProcess>,
    destination: Destination,
    options: VideoOptions,
    cancellation: CaptureCancellation,
    width: u32,
    height: u32,
    rate: FrameRate,
    frame_bytes: usize,
    frames: u64,
    last_source_index: Option<u64>,
    failed: bool,
}

impl Mp4Encoder {
    pub fn new(options: VideoOptions, width: u32, height: u32, rate: FrameRate, cancellation: CaptureCancellation) -> Result<Self, VideoError> {
        let frame_bytes = options.validate(width, height, rate)?;
        if cancellation.is_cancelled() { return Err(VideoError::Cancelled); }
        let destination = Destination::new(&options.output, options.overwrite)?;
        // Exercise the real codec/filter/muxer before running arbitrary source
        // callbacks. A help/encoder-list command can misleadingly exit zero.
        let probe_path = destination.stage.join("probe.mp4");
        let mut probe = EncoderProcess::start(&mut options.command(&probe_path, 16, 16, rate), cancellation.clone())?;
        let mut black = [0_u8; 16 * 16 * 4];
        for pixel in black.chunks_exact_mut(4) { pixel[3] = 255; }
        probe.write_frame(&black, options.write_timeout)?;
        probe.finish(options.finalize_timeout)?;
        if std::fs::metadata(&probe_path)?.len() == 0 {
            return Err(VideoError::Configuration("encoder preflight produced no file"));
        }
        std::fs::remove_file(probe_path)?;
        let process = EncoderProcess::start(
            &mut options.command(&destination.temporary_video(), width, height, rate), cancellation.clone())?;
        Ok(Self { process: Some(process), destination, options, cancellation, width, height, rate,
            frame_bytes, frames: 0, last_source_index: None, failed: false })
    }

    fn validate_frame(&self, frame: &CapturedFrame<'_>) -> Result<(), VideoError> {
        if self.failed { return Err(VideoError::Inactive); }
        if self.cancellation.is_cancelled() { return Err(VideoError::Cancelled); }
        if frame.width != self.width || frame.height != self.height || frame.rgba.len() != self.frame_bytes
            || frame.format != CapturePixelFormat::RendererRgba8UnormOpaque {
            return Err(VideoError::FrameContract("frame dimensions, format or byte length changed"));
        }
        if frame.frame.pts != self.frames || frame.frame.source_sample.time_base() != self.rate.time_base() {
            return Err(VideoError::FrameContract("frame index or rational time base changed"));
        }
        if self.last_source_index.is_some_and(|index| index.checked_add(1) != Some(frame.frame.source_sample.index())) {
            return Err(VideoError::FrameContract("source frames were skipped, duplicated or reordered"));
        }
        let requested = frame.observation.requested_time;
        let published = frame.observation.published_time;
        if requested != frame.frame.source_sample.authored_time() || !published.is_finite()
            || published < 0.0 || published > requested || (!frame.frame.held && published != requested) {
            return Err(VideoError::FrameContract("frame observation does not match its requested authored time"));
        }
        if frame.rgba.chunks_exact(4).any(|pixel| pixel[3] != 255) {
            return Err(VideoError::FrameContract("opaque video cannot silently discard non-opaque alpha"));
        }
        Ok(())
    }

    pub fn write_frame(&mut self, frame: &CapturedFrame<'_>) -> Result<(), VideoError> {
        let result = (|| {
            self.validate_frame(frame)?;
            let next = self.frames.checked_add(1).ok_or(VideoError::FrameContract("frame counter overflow"))?;
            self.process.as_mut().ok_or(VideoError::Inactive)?.write_frame(frame.rgba, self.options.write_timeout)?;
            self.frames = next;
            self.last_source_index = Some(frame.frame.source_sample.index());
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
            self.process.take();
        }
        result
    }

    /// Require the shared capture summary's frame count, flush the encoder, then
    /// publish the completed file. A mismatch or empty run never publishes output.
    pub fn finish(mut self, expected_frames: u64) -> Result<VideoSummary, VideoError> {
        if self.failed { return Err(VideoError::Inactive); }
        if self.cancellation.is_cancelled() { return Err(VideoError::Cancelled); }
        if self.frames == 0 || self.frames != expected_frames {
            return Err(VideoError::FrameContract("empty video or capture/encoder frame-count mismatch"));
        }
        self.process.take().ok_or(VideoError::Inactive)?.finish(self.options.finalize_timeout)?;
        if self.cancellation.is_cancelled() { return Err(VideoError::Cancelled); }
        let bytes = self.destination.publish()?;
        Ok(VideoSummary { output: self.destination.path.clone(), bytes, frames: self.frames,
            width: self.width, height: self.height, frame_rate: self.rate })
    }
}

/// Capture and encode through the same native Rust engine. The final destination
/// is published only after source/capture success AND encoder/muxer finalization.
/// The trusted FFmpeg child is managed with bounded pipe-write/finalize waits;
/// arbitrary user callbacks and OS/filesystem calls are not a sandboxed workload.
pub fn export_video<C: LiveContinuation>(
    program: &mut LiveProgram<C>, callbacks: &mut RustHostCallbackTable,
    frames: ExportFrameOptions, capture: CaptureOptions, video: VideoOptions,
) -> Result<VideoExportSummary, VideoExportError<C::Error>> {
    capture.layout().map_err(|e| VideoExportError::Capture(CaptureRunError::Capture(e)))?;
    let mut encoder = Mp4Encoder::new(video, capture.width, capture.height, frames.frame_rate, capture.cancellation.clone())
        .map_err(VideoExportError::Output)?;
    let capture = capture_frames(program, callbacks, frames, capture, |frame| encoder.write_frame(&frame))
        .map_err(VideoExportError::Capture)?;
    let video = encoder.finish(capture.sampling.frames).map_err(VideoExportError::Output)?;
    Ok(VideoExportSummary { capture, video })
}

#[derive(Debug)]
pub enum VideoError {
    Configuration(&'static str),
    FrameContract(&'static str),
    PixelLayout(String),
    Io(io::Error),
    Encoder { operation: &'static str, detail: String, stderr: String },
    Cancelled,
    Inactive,
}

impl From<io::Error> for VideoError { fn from(error: io::Error) -> Self { Self::Io(error) } }
impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "video configuration: {message}"),
            Self::FrameContract(message) => write!(f, "video frame contract: {message}"),
            Self::PixelLayout(message) => write!(f, "video pixel layout: {message}"),
            Self::Io(error) => error.fmt(f),
            Self::Encoder { operation, detail, stderr } => write!(f, "{operation}: {detail}; encoder stderr: {stderr}"),
            Self::Cancelled => f.write_str("video export was cancelled"),
            Self::Inactive => f.write_str("video encoder is failed or closed"),
        }
    }
}
impl Error for VideoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self { Self::Io(error) => Some(error), _ => None }
    }
}

#[derive(Debug)]
pub enum VideoExportError<C> {
    Capture(CaptureRunError<C, VideoError>),
    Output(VideoError),
}
impl<C: fmt::Display> fmt::Display for VideoExportError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Capture(error) => error.fmt(f), Self::Output(error) => error.fmt(f) }
    }
}
impl<C: Error + 'static> Error for VideoExportError<C> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self { Self::Capture(error) => Some(error), Self::Output(error) => Some(error) }
    }
}

#[cfg(test)]
mod tests;
