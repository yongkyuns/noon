//! Transactional native PNG sequence output over the shared capture path.
//!
//! This module owns image files and final directory publication only. Scene time,
//! callbacks, renderer publications and pixel capture remain in the shared engine.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::ImageEncoder;
use noon::integration::ExportFrameOptions;
use noon::{LiveContinuation, LiveProgram, RustHostCallbackTable};

use crate::{
    capture_frames, CaptureOptions, CapturePixelFormat, CaptureRunError, CaptureSummary,
    CapturedFrame,
};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

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
pub struct PngExportSummary {
    pub capture: CaptureSummary,
    pub directory: PathBuf,
}

#[derive(Debug)]
pub enum PngError {
    Configuration(&'static str),
    DestinationExists(PathBuf),
    FrameContract(&'static str),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Encode(image::ImageError),
}

impl PngError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "PNG sequence configuration: {message}"),
            Self::DestinationExists(path) => {
                write!(f, "PNG sequence destination already exists: {}", path.display())
            }
            Self::FrameContract(message) => write!(f, "PNG frame contract: {message}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::Encode(error) => error.fmt(f),
        }
    }
}

impl Error for PngError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Encode(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum PngExportError<C> {
    Capture(CaptureRunError<C, PngError>),
    Output(PngError),
}

impl<C: fmt::Display> fmt::Display for PngExportError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(error) => error.fmt(f),
            Self::Output(error) => error.fmt(f),
        }
    }
}

impl<C: Error + 'static> Error for PngExportError<C> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            Self::Output(error) => Some(error),
        }
    }
}

/// Capture a fresh live program to a numbered PNG sequence.
///
/// Files remain in a private sibling staging directory until every scheduled
/// frame succeeds. Capture/encoding/cancellation errors remove staging on drop.
/// Existing output is rejected before source execution unless overwrite is
/// explicit. Overwrite moves the previous directory aside only at publication.
pub fn export_png_sequence<C: LiveContinuation>(
    program: &mut LiveProgram<C>,
    callbacks: &mut RustHostCallbackTable,
    frames: ExportFrameOptions,
    capture: CaptureOptions,
    options: PngSequenceOptions,
) -> Result<PngExportSummary, PngExportError<C::Error>> {
    let mut writer = PngSequenceWriter::new(options, capture.width, capture.height)
        .map_err(PngExportError::Output)?;
    let summary = capture_frames(program, callbacks, frames, capture, |frame| {
        writer.write_frame(&frame)
    })
    .map_err(PngExportError::Capture)?;
    writer.finish(summary).map_err(PngExportError::Output)
}

struct PngSequenceWriter {
    options: PngSequenceOptions,
    stage: StageDirectory,
    width: u32,
    height: u32,
    next_pts: u64,
}

impl PngSequenceWriter {
    fn new(options: PngSequenceOptions, width: u32, height: u32) -> Result<Self, PngError> {
        if width == 0 || height == 0 {
            return Err(PngError::Configuration(
                "PNG dimensions must be positive",
            ));
        }
        validate_prefix(&options.prefix)?;
        validate_destination(&options.directory, options.overwrite)?;
        let stage = StageDirectory::new(&options.directory)
            .map_err(|error| PngError::io("create PNG staging directory", error))?;
        Ok(Self {
            options,
            stage,
            width,
            height,
            next_pts: 0,
        })
    }

    fn write_frame(&mut self, frame: &CapturedFrame<'_>) -> Result<(), PngError> {
        self.validate_frame(frame)?;
        let path = self.stage.path.join(format!(
            "{}_{:06}.png",
            self.options.prefix, self.next_pts
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| PngError::io("create PNG frame", error))?;
        let mut writer = BufWriter::new(file);
        image::codecs::png::PngEncoder::new(&mut writer)
            .write_image(
                frame.rgba,
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(PngError::Encode)?;
        writer
            .flush()
            .map_err(|error| PngError::io("flush PNG frame", error))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| PngError::io("sync PNG frame", error))?;
        self.next_pts = self
            .next_pts
            .checked_add(1)
            .ok_or(PngError::FrameContract("PNG frame counter overflow"))?;
        Ok(())
    }

    fn validate_frame(&self, frame: &CapturedFrame<'_>) -> Result<(), PngError> {
        if frame.width != self.width || frame.height != self.height {
            return Err(PngError::FrameContract(
                "frame dimensions changed within one sequence",
            ));
        }
        if frame.format != CapturePixelFormat::RendererRgba8UnormOpaque {
            return Err(PngError::FrameContract(
                "pixel format changed within one sequence",
            ));
        }
        if frame.frame.pts != self.next_pts {
            return Err(PngError::FrameContract(
                "PNG PTS is not contiguous from zero",
            ));
        }
        let expected = usize::try_from(u64::from(self.width) * u64::from(self.height) * 4)
            .map_err(|_| PngError::FrameContract("PNG byte length is not representable"))?;
        if frame.rgba.len() != expected {
            return Err(PngError::FrameContract(
                "RGBA byte length does not match PNG dimensions",
            ));
        }
        Ok(())
    }

    fn finish(self, capture: CaptureSummary) -> Result<PngExportSummary, PngError> {
        if self.next_pts == 0 || self.next_pts != capture.sampling.frames {
            return Err(PngError::FrameContract(
                "empty sequence or capture/PNG frame-count mismatch",
            ));
        }
        publish_directory(
            &self.stage.path,
            &self.options.directory,
            self.options.overwrite,
        )?;
        Ok(PngExportSummary {
            capture,
            directory: self.options.directory.clone(),
        })
    }
}

struct StageDirectory {
    path: PathBuf,
}

impl StageDirectory {
    fn new(destination: &Path) -> io::Result<Self> {
        let parent = output_parent(destination)?;
        if !parent.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "PNG output parent directory does not exist",
            ));
        }
        let leaf = destination
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("frames"))
            .to_string_lossy();
        for _ in 0..256 {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".{leaf}.noon-png-{}-{sequence}",
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
            "cannot reserve a unique PNG staging directory",
        ))
    }
}

impl Drop for StageDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn validate_prefix(prefix: &str) -> Result<(), PngError> {
    if prefix.is_empty()
        || prefix == "."
        || prefix == ".."
        || prefix.contains('/')
        || prefix.contains('\\')
    {
        return Err(PngError::Configuration(
            "PNG prefix must be one nonempty path component",
        ));
    }
    Ok(())
}

fn validate_destination(destination: &Path, overwrite: bool) -> Result<(), PngError> {
    if destination.file_name().is_none() {
        return Err(PngError::Configuration(
            "PNG destination must name a directory",
        ));
    }
    let parent = output_parent(destination)
        .map_err(|error| PngError::io("validate PNG output parent", error))?;
    if !parent.is_dir() {
        return Err(PngError::Configuration(
            "PNG output parent directory does not exist",
        ));
    }
    if destination.exists() {
        if !overwrite {
            return Err(PngError::DestinationExists(destination.to_owned()));
        }
        if !destination.is_dir() {
            return Err(PngError::Configuration(
                "existing PNG destination is not a directory",
            ));
        }
    }
    Ok(())
}

fn publish_directory(stage: &Path, destination: &Path, overwrite: bool) -> Result<(), PngError> {
    // Repeat validation at the publication boundary so output that appeared
    // during capture is not intentionally replaced without explicit overwrite.
    validate_destination(destination, overwrite)?;
    if !destination.exists() {
        return fs::rename(stage, destination)
            .map_err(|error| PngError::io("publish PNG sequence", error));
    }

    let backup = unique_backup(destination)?;
    fs::rename(destination, &backup)
        .map_err(|error| PngError::io("stage previous PNG sequence", error))?;
    match fs::rename(stage, destination) {
        Ok(()) => match fs::remove_dir_all(&backup) {
            Ok(()) => Ok(()),
            Err(cleanup) => {
                let restore_new = fs::rename(destination, stage);
                let restore_old = fs::rename(&backup, destination);
                match (restore_new, restore_old) {
                    (Ok(()), Ok(())) => Err(PngError::io(
                        "remove replaced PNG sequence",
                        cleanup,
                    )),
                    (new_result, old_result) => Err(PngError::io(
                        "rollback PNG sequence publication",
                        io::Error::new(
                            cleanup.kind(),
                            format!(
                                "backup cleanup failed ({cleanup}); rollback new={new_result:?}, old={old_result:?}"
                            ),
                        ),
                    )),
                }
            }
        },
        Err(error) => {
            let rollback = fs::rename(&backup, destination);
            if let Err(rollback) = rollback {
                return Err(PngError::io(
                    "rollback PNG sequence publication",
                    io::Error::new(
                        rollback.kind(),
                        format!(
                            "publish failed ({error}); rollback also failed ({rollback})"
                        ),
                    ),
                ));
            }
            Err(PngError::io("publish PNG sequence", error))
        }
    }
}

fn unique_backup(destination: &Path) -> Result<PathBuf, PngError> {
    let parent = output_parent(destination)
        .map_err(|error| PngError::io("locate PNG backup parent", error))?;
    let leaf = destination
        .file_name()
        .ok_or(PngError::Configuration(
            "PNG destination must name a directory",
        ))?
        .to_string_lossy();
    for _ in 0..256 {
        let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{leaf}.noon-png-backup-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(PngError::io(
        "reserve PNG backup path",
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot reserve a unique PNG backup path",
        ),
    ))
}

fn output_parent(destination: &Path) -> io::Result<&Path> {
    match destination.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(parent) => Ok(parent),
        None => Ok(Path::new(".")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "noon-png-{name}-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn invalid_prefixes_are_rejected_before_output() {
        let root = temp_root("prefix");
        for prefix in ["", ".", "..", "../escape", "a/b", "a\\b"] {
            let mut options = PngSequenceOptions::new(root.join("frames"));
            options.prefix = prefix.to_owned();
            assert!(matches!(
                PngSequenceWriter::new(options, 16, 16),
                Err(PngError::Configuration(_))
            ));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_publication_preserves_and_replaces_explicitly() {
        let root = temp_root("publish");
        let destination = root.join("frames");
        let stage = root.join("new");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&stage).unwrap();
        fs::write(destination.join("old"), b"old").unwrap();
        fs::write(stage.join("new"), b"new").unwrap();

        assert!(matches!(
            publish_directory(&stage, &destination, false),
            Err(PngError::DestinationExists(_))
        ));
        assert_eq!(fs::read(destination.join("old")).unwrap(), b"old");
        assert_eq!(fs::read(stage.join("new")).unwrap(), b"new");

        publish_directory(&stage, &destination, true).unwrap();
        assert!(!stage.exists());
        assert!(!destination.join("old").exists());
        assert_eq!(fs::read(destination.join("new")).unwrap(), b"new");
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".noon-png-backup-")
        }));
        fs::remove_dir_all(root).unwrap();
    }
}
