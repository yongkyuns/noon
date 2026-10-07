//! Output publication owns files, never scene state. Work stays on one filesystem.
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{OutputFormat, OutputOptions};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub(super) struct Destination {
    pub path: PathBuf,
    pub work: PathBuf,
    format: OutputFormat,
    overwrite: bool,
    published: bool,
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

impl Destination {
    pub fn new(options: &OutputOptions) -> io::Result<Self> {
        let filename = options.path.file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a filename"))?;
        let parent = options.path.parent().filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let path = fs::canonicalize(parent)?.join(filename);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if options.format == OutputFormat::PngSequence || !options.overwrite {
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, "output already exists"));
                }
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "overwrite needs a regular file"));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if options.format == OutputFormat::PngSequence {
            // Reserve a NEW bundle exclusively. No pre-existing directory can be
            // removed or replaced. During execution only .incomplete is visible;
            // the committed frames/ directory appears after successful flush.
            private_directory(&path)?;
            let work = path.join(".incomplete");
            let result = Self { path, work, format: options.format, overwrite: false, published: false };
            private_directory(&result.work)?;
            fs::create_dir(result.work.join("frames"))?;
            return Ok(result);
        }
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        for _ in 0..128 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let work = path.parent().unwrap().join(format!(".noon-export-{}-{nonce:x}-{id:x}", std::process::id()));
            match private_directory(&work) {
                Ok(()) => return Ok(Self { path, work, format: options.format, overwrite: options.overwrite, published: false }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not reserve temporary output"))
    }

    pub fn publish(mut self, frames: u64) -> io::Result<PathBuf> {
        if frames == 0 { return Err(io::Error::other("refusing to publish empty output")); }
        match self.format {
            OutputFormat::Mp4 => {
                let video = self.work.join("video.mp4");
                if fs::metadata(&video)?.len() == 0 { return Err(io::Error::other("encoder wrote no video")); }
                File::open(&video)?.sync_all()?;
                if self.overwrite {
                    // Explicit replacement only; no remove-then-rename gap.
                    fs::rename(&video, &self.path)?;
                } else {
                    // Same-filesystem hard linking is atomic and cannot clobber a
                    // file created after preflight. Unsupported filesystems fail.
                    fs::hard_link(&video, &self.path)?;
                }
            }
            OutputFormat::PngSequence => {
                let directory = self.work.join("frames");
                for index in 0..frames {
                    let file = File::open(directory.join(format!("frame-{index:010}.png")))?;
                    if file.metadata()?.len() == 0 { return Err(io::Error::other("empty PNG output")); }
                    file.sync_all()?;
                }
                // The outer directory is this export's exclusive reservation.
                // The manifest moves with the completed frames in ONE rename.
                fs::rename(&directory, self.path.join("frames"))?;
            }
        }
        self.published = true;
        Ok(self.path.clone())
    }
}

impl Drop for Destination {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.work);
        if !self.published && self.format == OutputFormat::PngSequence {
            let _ = fs::remove_dir(&self.path);
        }
    }
}
