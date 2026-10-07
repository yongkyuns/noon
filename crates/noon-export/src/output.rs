//! Native file sinks layered on top of frame-accurate headless capture.
//!
//! These adapters own external file/process lifetime only. They do not schedule
//! scene time, render frames, or reinterpret Noon's semantic/runtime state.

use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

use image::ImageEncoder;
use noon::integration::{ExportFrameOptions, FrameRate};
use noon::{LiveContinuation, LiveProgram, RustHostCallbackTable};

use crate::{
    capture_frames, CaptureOptions, CapturePixelFormat, CaptureRunError, CaptureSummary,
    CapturedFrame,
};

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct NativeOutputSummary {
    pub capture: CaptureSummary,
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum NativeOutputError<C, S> {
    Capture(CaptureRunError<C, S>),
    Finalize(S),
}

impl<C: fmt::Display, S: fmt::Display> fmt::Display for NativeOutputError<C, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(error) => error.fmt(f),
            Self::Finalize(error) => write!(f, "output finalization failed: {error}"),
        }
    }
}

impl<C: Error + 'static, S: Error + 'static> Error for NativeOutputError<C, S> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            Self::Finalize(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PngSequenceOptions {
    pub directory: PathBuf,
    pub prefix: String,
    pub overwrite: bool,
}

impl PngSequenceOptions {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            prefix: "frame".to_owned(),
            overwrite: false,
        }
    }
}

#[derive(Debug)]
pub enum PngSequenceError {
    InvalidConfiguration(&'static str),
    DestinationExists(PathBuf),
    FrameMismatch(&'static str),
    Io {
        action: &'static str,
        source: io::Error,
    },
    Encode(image::ImageError),
}

impl PngSequenceError {
    fn io(action: &'static str, source: io::Error) -> Self {
        Self::Io { action, source }
    }
}

impl fmt::Display for PngSequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => write!(f, "PNG sequence configuration: {message}"),
            Self::DestinationExists(path) => write!(f, "PNG sequence destination already exists: {}", path.display()),
            Self::FrameMismatch(message) => write!(f, "PNG sequence frame mismatch: {message}"),
            Self::Io { action, source } => write!(f, "{action}: {source}"),
            Self::Encode(error) => error.fmt(f),
        }
    }
}

impl Error for PngSequenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Encode(error) => Some(error),
            _ => None,
        }
    }
}

/// Capture one fresh program to a directory of numbered PNG files.
///
/// Frames remain hidden in a sibling scratch directory until capture succeeds
/// and every PNG has been closed. Existing output is left untouched on any
/// capture/encoding failure. With overwrite enabled, replacement happens only
/// during final publication and attempts rollback if the final rename fails.
pub fn capture_png_sequence<C>(
    program: &mut LiveProgram<C>,
    callbacks: &mut RustHostCallbackTable,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    png_options: PngSequenceOptions,
) -> Result<NativeOutputSummary, NativeOutputError<C::Error, PngSequenceError>>
where
    C: LiveContinuation,
{
    let width = capture_options.width;
    let height = capture_options.height;
    let mut sink = PngSequenceSink::new(png_options, width, height)
        .map_err(NativeOutputError::Finalize)?;
    let capture = capture_frames(
        program,
        callbacks,
        frame_options,
        capture_options,
        |frame| sink.write_frame(frame),
    )
    .map_err(NativeOutputError::Capture)?;
    sink.finish(capture).map_err(NativeOutputError::Finalize)
}

struct PngSequenceSink {
    options: PngSequenceOptions,
    scratch: ScratchDir,
    width: u32,
    height: u32,
    next_pts: u64,
}

impl PngSequenceSink {
    fn new(
        options: PngSequenceOptions,
        width: u32,
        height: u32,
    ) -> Result<Self, PngSequenceError> {
        validate_leaf_component(&options.prefix)
            .map_err(PngSequenceError::InvalidConfiguration)?;
        validate_destination(&options.directory, options.overwrite)
            .map_err(|error| map_publish_error(error, &options.directory))?;
        let scratch = ScratchDir::new(&options.directory, "png")
            .map_err(|error| PngSequenceError::io("create PNG scratch directory", error))?;
        if options.directory.exists() && !options.directory.is_dir() {
            return Err(PngSequenceError::InvalidConfiguration(
                "existing PNG destination is not a directory",
            ));
        }
        Ok(Self {
            options,
            scratch,
            width,
            height,
            next_pts: 0,
        })
    }

    fn write_frame(&mut self, frame: CapturedFrame<'_>) -> Result<(), PngSequenceError> {
        validate_captured_frame(
            &frame,
            self.width,
            self.height,
            self.next_pts,
            None,
        )
        .map_err(PngSequenceError::FrameMismatch)?;
        let path = self.scratch.path.join(format!(
            "{}_{:06}.png",
            self.options.prefix, self.next_pts
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| PngSequenceError::io("create PNG frame", error))?;
        let mut writer = BufWriter::new(file);
        image::codecs::png::PngEncoder::new(&mut writer)
            .write_image(
                frame.rgba,
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(PngSequenceError::Encode)?;
        writer
            .flush()
            .map_err(|error| PngSequenceError::io("flush PNG frame", error))?;
        self.next_pts += 1;
        Ok(())
    }

    fn finish(mut self, capture: CaptureSummary) -> Result<NativeOutputSummary, PngSequenceError> {
        if self.next_pts != capture.sampling.frames {
            return Err(PngSequenceError::FrameMismatch(
                "captured frame count differs from written PNG count",
            ));
        }
        publish_path(
            &self.scratch.path,
            &self.options.directory,
            self.options.overwrite,
        )
        .map_err(|error| map_publish_error(error, &self.options.directory))?;
        Ok(NativeOutputSummary {
            capture,
            path: self.options.directory.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct FfmpegMp4Options {
    pub path: PathBuf,
    pub executable: PathBuf,
    pub overwrite: bool,
    pub crf: u8,
    pub preset: String,
    /// Bound on retained stderr bytes. The pipe itself is continuously drained.
    pub max_stderr_bytes: usize,
}

impl FfmpegMp4Options {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            executable: PathBuf::from("ffmpeg"),
            overwrite: false,
            crf: 18,
            preset: "medium".to_owned(),
            max_stderr_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug)]
pub enum FfmpegMp4Error {
    InvalidConfiguration(&'static str),
    DestinationExists(PathBuf),
    FrameMismatch(&'static str),
    Spawn {
        executable: PathBuf,
        source: io::Error,
    },
    Io {
        action: &'static str,
        source: io::Error,
    },
    EncoderUnavailable,
    ProcessFailed {
        status: Option<i32>,
        stderr: String,
        stderr_truncated: bool,
    },
    StderrThread,
}

impl FfmpegMp4Error {
    fn io(action: &'static str, source: io::Error) -> Self {
        Self::Io { action, source }
    }
}

impl fmt::Display for FfmpegMp4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => write!(f, "FFmpeg MP4 configuration: {message}"),
            Self::DestinationExists(path) => write!(f, "MP4 destination already exists: {}", path.display()),
            Self::FrameMismatch(message) => write!(f, "FFmpeg input frame mismatch: {message}"),
            Self::Spawn { executable, source } => {
                write!(f, "cannot start {}: {source}", executable.display())
            }
            Self::Io { action, source } => write!(f, "{action}: {source}"),
            Self::EncoderUnavailable => {
                f.write_str("FFmpeg does not advertise the required libx264 encoder")
            }
            Self::ProcessFailed {
                status,
                stderr,
                stderr_truncated,
            } => {
                write!(f, "FFmpeg exited with status {:?}", status)?;
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                if *stderr_truncated {
                    f.write_str(" [stderr truncated]")?;
                }
                Ok(())
            }
            Self::StderrThread => f.write_str("FFmpeg stderr drain thread failed"),
        }
    }
}

impl Error for FfmpegMp4Error {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } | Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Capture and encode one H.264/yuv420p MP4 through an external FFmpeg process.
///
/// The encoder receives exactly one raw frame for each output PTS. The temporary
/// MP4 lives beside the destination and is published only after stdin closes,
/// FFmpeg exits successfully, and the muxed file is nonempty.
pub fn capture_mp4_ffmpeg<C>(
    program: &mut LiveProgram<C>,
    callbacks: &mut RustHostCallbackTable,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    ffmpeg_options: FfmpegMp4Options,
) -> Result<NativeOutputSummary, NativeOutputError<C::Error, FfmpegMp4Error>>
where
    C: LiveContinuation,
{
    let width = capture_options.width;
    let height = capture_options.height;
    let rate = frame_options.frame_rate;
    let mut sink = FfmpegMp4Sink::new(ffmpeg_options, width, height, rate)
        .map_err(NativeOutputError::Finalize)?;
    let capture = capture_frames(
        program,
        callbacks,
        frame_options,
        capture_options,
        |frame| sink.write_frame(frame),
    )
    .map_err(NativeOutputError::Capture)?;
    sink.finish(capture).map_err(NativeOutputError::Finalize)
}

struct FfmpegMp4Sink {
    options: FfmpegMp4Options,
    scratch: ScratchDir,
    temp_file: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr: Option<JoinHandle<io::Result<StderrCapture>>>,
    width: u32,
    height: u32,
    rate: FrameRate,
    next_pts: u64,
    finished: bool,
}

impl FfmpegMp4Sink {
    fn new(
        options: FfmpegMp4Options,
        width: u32,
        height: u32,
        rate: FrameRate,
    ) -> Result<Self, FfmpegMp4Error> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "H.264 yuv420p output requires positive even dimensions",
            ));
        }
        if options.crf > 51 {
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "libx264 CRF must be in 0..=51",
            ));
        }
        if options.preset.is_empty() || options.max_stderr_bytes == 0 {
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "preset and stderr bound must be nonempty",
            ));
        }
        validate_destination(&options.path, options.overwrite)
            .map_err(|error| map_ffmpeg_publish_error(error, &options.path))?;
        if options.path.exists() && !options.path.is_file() {
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "existing MP4 destination is not a file",
            ));
        }
        probe_libx264(&options.executable)?;
        let scratch = ScratchDir::new(&options.path, "mp4")
            .map_err(|error| FfmpegMp4Error::io("create MP4 scratch directory", error))?;
        let temp_file = scratch.path.join("encoded.mp4");
        let rate_arg = format!("{}/{}", rate.numerator(), rate.denominator());
        let size_arg = format!("{width}x{height}");
        let crf_arg = options.crf.to_string();
        let mut child = Command::new(&options.executable)
            .arg("-nostdin")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-n")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pixel_format")
            .arg("rgba")
            .arg("-video_size")
            .arg(size_arg)
            .arg("-framerate")
            .arg(rate_arg)
            .arg("-i")
            .arg("pipe:0")
            .arg("-map")
            .arg("0:v:0")
            .arg("-an")
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg(&options.preset)
            .arg("-crf")
            .arg(crf_arg)
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&temp_file)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| FfmpegMp4Error::Spawn {
                executable: options.executable.clone(),
                source,
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or(FfmpegMp4Error::InvalidConfiguration(
                "FFmpeg stdin was not piped",
            ))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(FfmpegMp4Error::InvalidConfiguration(
                "FFmpeg stderr was not piped",
            ))?;
        let max_stderr_bytes = options.max_stderr_bytes;
        let stderr = match thread::Builder::new()
            .name("noon-ffmpeg-stderr".to_owned())
            .spawn(move || drain_stderr(stderr, max_stderr_bytes))
        {
            Ok(handle) => handle,
            Err(error) => {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
                return Err(FfmpegMp4Error::io(
                    "spawn FFmpeg stderr drain",
                    error,
                ));
            }
        };
        Ok(Self {
            options,
            scratch,
            temp_file,
            child: Some(child),
            stdin: Some(stdin),
            stderr: Some(stderr),
            width,
            height,
            rate,
            next_pts: 0,
            finished: false,
        })
    }

    fn write_frame(&mut self, frame: CapturedFrame<'_>) -> Result<(), FfmpegMp4Error> {
        validate_captured_frame(
            &frame,
            self.width,
            self.height,
            self.next_pts,
            Some(self.rate),
        )
        .map_err(FfmpegMp4Error::FrameMismatch)?;
        self.stdin
            .as_mut()
            .ok_or(FfmpegMp4Error::FrameMismatch("FFmpeg stdin is closed"))?
            .write_all(frame.rgba)
            .map_err(|error| FfmpegMp4Error::io("write raw frame to FFmpeg", error))?;
        self.next_pts += 1;
        Ok(())
    }

    fn finish(mut self, capture: CaptureSummary) -> Result<NativeOutputSummary, FfmpegMp4Error> {
        if self.next_pts != capture.sampling.frames {
            return Err(FfmpegMp4Error::FrameMismatch(
                "captured frame count differs from FFmpeg input count",
            ));
        }
        drop(self.stdin.take());
        let status = self
            .child
            .as_mut()
            .ok_or(FfmpegMp4Error::FrameMismatch("FFmpeg process is absent"))?
            .wait()
            .map_err(|error| FfmpegMp4Error::io("wait for FFmpeg", error))?;
        self.child.take();
        let stderr = self.join_stderr()?;
        if !status.success() {
            return Err(process_failed(status, stderr));
        }
        let metadata = fs::metadata(&self.temp_file)
            .map_err(|error| FfmpegMp4Error::io("inspect finalized temporary MP4", error))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "FFmpeg reported success without a nonempty MP4",
            ));
        }
        publish_path(&self.temp_file, &self.options.path, self.options.overwrite)
            .map_err(|error| map_ffmpeg_publish_error(error, &self.options.path))?;
        self.finished = true;
        Ok(NativeOutputSummary {
            capture,
            path: self.options.path.clone(),
        })
    }

    fn join_stderr(&mut self) -> Result<StderrCapture, FfmpegMp4Error> {
        let handle = self.stderr.take().ok_or(FfmpegMp4Error::StderrThread)?;
        handle
            .join()
            .map_err(|_| FfmpegMp4Error::StderrThread)?
            .map_err(|error| FfmpegMp4Error::io("drain FFmpeg stderr", error))
    }
}

impl Drop for FfmpegMp4Sink {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.stderr.take() {
            let _ = handle.join();
        }
    }
}

struct StderrCapture {
    bytes: Vec<u8>,
    truncated: bool,
}

fn drain_stderr(mut stderr: impl Read, limit: usize) -> io::Result<StderrCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stderr.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let keep = remaining.min(count);
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep != count;
    }
    Ok(StderrCapture { bytes, truncated })
}

fn probe_libx264(executable: &Path) -> Result<(), FfmpegMp4Error> {
    const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
    let mut child = Command::new(executable)
        .arg("-hide_banner")
        .arg("-encoders")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| FfmpegMp4Error::Spawn {
            executable: executable.to_owned(),
            source,
        })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(FfmpegMp4Error::InvalidConfiguration(
            "FFmpeg capability stdout was not piped",
        ))?;
    let mut found = false;
    let mut total = 0_usize;
    let mut carry = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stdout
            .read(&mut buffer)
            .map_err(|error| FfmpegMp4Error::io("read FFmpeg encoder list", error))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count);
        if total > OUTPUT_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FfmpegMp4Error::InvalidConfiguration(
                "FFmpeg encoder listing exceeded the probe byte limit",
            ));
        }
        carry.extend_from_slice(&buffer[..count]);
        if carry.windows(b"libx264".len()).any(|window| window == b"libx264") {
            found = true;
        }
        if carry.len() > 32 {
            let drain = carry.len() - 32;
            carry.drain(..drain);
        }
    }
    let status = child
        .wait()
        .map_err(|error| FfmpegMp4Error::io("wait for FFmpeg encoder probe", error))?;
    if !status.success() {
        return Err(FfmpegMp4Error::ProcessFailed {
            status: status.code(),
            stderr: "FFmpeg encoder capability probe failed".to_owned(),
            stderr_truncated: false,
        });
    }
    if !found {
        return Err(FfmpegMp4Error::EncoderUnavailable);
    }
    Ok(())
}

fn process_failed(status: ExitStatus, stderr: StderrCapture) -> FfmpegMp4Error {
    FfmpegMp4Error::ProcessFailed {
        status: status.code(),
        stderr: String::from_utf8_lossy(&stderr.bytes).trim().to_owned(),
        stderr_truncated: stderr.truncated,
    }
}

fn validate_captured_frame(
    frame: &CapturedFrame<'_>,
    width: u32,
    height: u32,
    expected_pts: u64,
    rate: Option<FrameRate>,
) -> Result<(), &'static str> {
    if frame.width != width || frame.height != height {
        return Err("dimensions changed within one output");
    }
    if frame.format != CapturePixelFormat::RendererRgba8UnormOpaque {
        return Err("pixel format changed within one output");
    }
    if frame.frame.pts != expected_pts {
        return Err("PTS is not contiguous from zero");
    }
    if let Some(rate) = rate {
        if frame.frame.source_sample.time_base() != rate.time_base() {
            return Err("frame time base differs from encoder input rate");
        }
    }
    let expected_len = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| "frame byte length is not representable")?;
    if frame.rgba.len() != expected_len {
        return Err("RGBA byte length does not match dimensions");
    }
    Ok(())
}

struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new(destination: &Path, tag: &str) -> io::Result<Self> {
        let parent = destination_parent(destination)?;
        if !parent.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "output parent directory does not exist",
            ));
        }
        let leaf = destination
            .file_name()
            .unwrap_or_else(|| OsStr::new("output"))
            .to_string_lossy();
        for _ in 0..256 {
            let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".{leaf}.noon-{tag}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot reserve a unique output scratch directory",
        ))
    }

}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn validate_destination(destination: &Path, overwrite: bool) -> Result<(), PublishError> {
    if destination.file_name().is_none() {
        return Err(PublishError::InvalidDestination);
    }
    let parent = destination_parent(destination).map_err(PublishError::Io)?;
    if !parent.is_dir() {
        return Err(PublishError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "output parent directory does not exist",
        )));
    }
    if destination.exists() && !overwrite {
        return Err(PublishError::Exists);
    }
    Ok(())
}

fn destination_parent(destination: &Path) -> io::Result<&Path> {
    match destination.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(parent) => Ok(parent),
        None => Ok(Path::new(".")),
    }
}

fn validate_leaf_component(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err("frame prefix must be one nonempty path component");
    }
    Ok(())
}

enum PublishError {
    InvalidDestination,
    Exists,
    Io(io::Error),
}

fn map_publish_error(error: PublishError, destination: &Path) -> PngSequenceError {
    match error {
        PublishError::InvalidDestination => {
            PngSequenceError::InvalidConfiguration("destination must name a directory")
        }
        PublishError::Exists => PngSequenceError::DestinationExists(destination.to_owned()),
        PublishError::Io(error) => PngSequenceError::io("publish PNG sequence", error),
    }
}

fn map_ffmpeg_publish_error(error: PublishError, destination: &Path) -> FfmpegMp4Error {
    match error {
        PublishError::InvalidDestination => {
            FfmpegMp4Error::InvalidConfiguration("destination must name an MP4 file")
        }
        PublishError::Exists => FfmpegMp4Error::DestinationExists(destination.to_owned()),
        PublishError::Io(error) => FfmpegMp4Error::io("publish MP4", error),
    }
}

fn publish_path(source: &Path, destination: &Path, overwrite: bool) -> Result<(), PublishError> {
    validate_destination(destination, overwrite)?;
    if !destination.exists() {
        return fs::rename(source, destination).map_err(PublishError::Io);
    }
    if !overwrite {
        return Err(PublishError::Exists);
    }

    let backup = unique_backup_path(destination)?;
    fs::rename(destination, &backup).map_err(PublishError::Io)?;
    match fs::rename(source, destination) {
        Ok(()) => match remove_any(&backup) {
            Ok(()) => Ok(()),
            Err(cleanup) => {
                let restore_new = fs::rename(destination, source);
                let restore_old = fs::rename(&backup, destination);
                match (restore_new, restore_old) {
                    (Ok(()), Ok(())) => Err(PublishError::Io(cleanup)),
                    (new_result, old_result) => Err(PublishError::Io(io::Error::new(
                        cleanup.kind(),
                        format!(
                            "published output but could not remove backup ({cleanup}); rollback new={new_result:?}, old={old_result:?}"
                        ),
                    ))),
                }
            }
        }
        Err(error) => {
            let rollback = fs::rename(&backup, destination);
            if let Err(rollback) = rollback {
                return Err(PublishError::Io(io::Error::new(
                    rollback.kind(),
                    format!(
                        "publish failed ({error}); rollback also failed ({rollback})"
                    ),
                )));
            }
            Err(PublishError::Io(error))
        }
    }
}

fn unique_backup_path(destination: &Path) -> Result<PathBuf, PublishError> {
    let parent = destination_parent(destination).map_err(PublishError::Io)?;
    let leaf = destination
        .file_name()
        .ok_or(PublishError::InvalidDestination)?
        .to_string_lossy();
    for _ in 0..256 {
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{leaf}.noon-backup-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(PublishError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot reserve a unique output backup path",
    )))
}

fn remove_any(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "noon-output-{name}-{}-{}",
            std::process::id(),
            SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn publication_preserves_existing_output_until_success() {
        let root = temp_root("publish");
        let destination = root.join("scene.mp4");
        fs::write(&destination, b"old").unwrap();
        let scratch = root.join("new.mp4");
        fs::write(&scratch, b"new").unwrap();
        assert!(matches!(
            publish_path(&scratch, &destination, false),
            Err(PublishError::Exists)
        ));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read(&scratch).unwrap(), b"new");
        publish_path(&scratch, &destination, true).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert!(!scratch.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_encoder_spawn_does_not_replace_destination() {
        let root = temp_root("spawn");
        let destination = root.join("scene.mp4");
        fs::write(&destination, b"old").unwrap();
        let mut options = FfmpegMp4Options::new(&destination);
        options.overwrite = true;
        options.executable = root.join("definitely-not-ffmpeg");
        let error = FfmpegMp4Sink::new(
            options,
            64,
            64,
            FrameRate::new(30, 1).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(error, FfmpegMp4Error::Spawn { .. }));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn png_prefix_cannot_escape_the_transaction_directory() {
        let root = temp_root("prefix");
        for prefix in ["", ".", "..", "../escape", "a/b", "a\\b"] {
            let mut options = PngSequenceOptions::new(root.join("frames"));
            options.prefix = prefix.to_owned();
            assert!(matches!(
                PngSequenceSink::new(options, 64, 64),
                Err(PngSequenceError::InvalidConfiguration(_))
            ));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
