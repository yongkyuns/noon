//! Private sibling staging and publish-on-success for one output file.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::VideoError;

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub(super) struct Destination {
    pub path: PathBuf,
    pub stage: PathBuf,
    overwrite: bool,
}

impl Destination {
    pub fn new(path: &Path, overwrite: bool) -> Result<Self, VideoError> {
        let name = path.file_name().ok_or(VideoError::Configuration("output needs a filename"))?;
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let path = fs::canonicalize(parent)?.join(name);
        check_destination(&path, overwrite)?;
        let parent = path.parent().expect("canonical parent joined with filename");
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        for _ in 0..64 {
            let id = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let stage = parent.join(format!(".noon-video-{}-{stamp}-{id}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&stage) {
                Ok(()) => return Ok(Self { path, stage, overwrite }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(VideoError::Configuration("could not reserve a private output directory"))
    }

    pub fn temporary_video(&self) -> PathBuf {
        self.stage.join("video.mp4")
    }

    pub fn publish(&self) -> Result<u64, VideoError> {
        let temporary = self.temporary_video();
        let file = fs::OpenOptions::new().read(true).write(true).open(&temporary)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(VideoError::Configuration("encoder produced no video file"));
        }
        file.sync_all()?;
        drop(file);
        if self.overwrite {
            check_destination(&self.path, true)?;
            // Same-filesystem rename: failure leaves the previous destination intact.
            fs::rename(&temporary, &self.path)?;
        } else {
            // Unlike exists()+rename(), this also rejects a destination created
            // concurrently while encoding. No cross-filesystem/copy fallback.
            fs::hard_link(&temporary, &self.path)?;
        }
        Ok(metadata.len())
    }
}

fn check_destination(path: &Path, overwrite: bool) -> Result<(), VideoError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !overwrite || !meta.file_type().is_file() => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "output exists; overwrite must be explicit and the target a regular file",
        ).into()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

impl Drop for Destination {
    fn drop(&mut self) {
        // Cleanup is best effort after OS/filesystem failure. Never delete the
        // destination, even when publication succeeded but cleanup is denied.
        let _ = fs::remove_dir_all(&self.stage);
    }
}
