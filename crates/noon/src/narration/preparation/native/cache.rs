//! Bounded reads and immutable publication for a caller-owned local cache.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{PreparationError, PreparedAudio};
use crate::narration::codec;
use crate::narration::preparation::{MAX_METADATA_BYTES, MAX_PCM_BYTES};

pub(super) struct Cache {
    root: PathBuf,
}

impl Cache {
    pub(super) fn open(root: &Path) -> Result<Self, PreparationError> {
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        ensure_directory(&root)?;
        for name in ["audio", "requests", "narrations"] {
            ensure_directory(&root.join(name))?;
        }
        Ok(Self { root })
    }

    /// Check current staging/write capability before an uncached paid request.
    /// This is a preflight, not a guarantee against later disk or permission
    /// changes. Every probe is removed on success or failure.
    pub(super) fn preflight_writes(&self) -> Result<(), PreparationError> {
        self.preflight_writes_using(|file| {
            file.write_all(b"noon narration cache preflight\n")?;
            file.sync_all()
        })
    }

    pub(super) fn preflight_writes_using(
        &self,
        mut write: impl FnMut(&mut File) -> std::io::Result<()>,
    ) -> Result<(), PreparationError> {
        {
            let stage = tempfile::Builder::new()
                .prefix(".prepare-")
                .tempdir_in(self.root.join("audio"))?;
            let mut file = File::create_new(stage.path().join("probe"))?;
            write(&mut file)?;
        }
        for name in ["requests", "narrations"] {
            let mut stage = tempfile::Builder::new()
                .prefix(".prepare-")
                .tempfile_in(self.root.join(name))?;
            write(stage.as_file_mut())?;
        }
        Ok(())
    }

    pub(super) fn request_path(&self, id: &str) -> PathBuf {
        self.root.join("requests").join(format!("{id}.json"))
    }

    pub(super) fn manifest_path(&self, id: &str) -> PathBuf {
        self.root.join("narrations").join(format!("{id}.json"))
    }

    pub(super) fn load_audio(&self, id: &str) -> Result<PreparedAudio, PreparationError> {
        if !codec::is_digest(id) {
            return Err(PreparationError::InvalidCache("invalid audio identity"));
        }
        let directory = self.root.join("audio").join(id);
        require_directory(&directory)?;
        let metadata = read_bounded(&directory.join("audio.json"), MAX_METADATA_BYTES)?;
        let pcm = read_bounded(&directory.join("audio.f32le"), MAX_PCM_BYTES)?;
        let audio = PreparedAudio::from_cached(&metadata, pcm)?;
        if audio.metadata.asset_id != id {
            return Err(PreparationError::InvalidCache(
                "audio identity does not match its directory",
            ));
        }
        Ok(audio)
    }

    pub(super) fn store_audio(&self, audio: &PreparedAudio) -> Result<(), PreparationError> {
        self.store_audio_using(audio, |stage, destination| fs::rename(stage, destination))
    }

    pub(super) fn store_audio_using(
        &self,
        audio: &PreparedAudio,
        publish: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), PreparationError> {
        let id = &audio.metadata.asset_id;
        let parent = self.root.join("audio");
        let destination = parent.join(id);
        if entry_exists(&destination)? {
            self.load_audio(id)?;
            return Ok(());
        }
        let stage = tempfile::Builder::new()
            .prefix(".prepare-")
            .tempdir_in(&parent)?;
        write_new(&stage.path().join("audio.f32le"), &audio.pcm)?;
        let mut metadata = codec::canonical_json(&audio.metadata)?;
        metadata.push(b'\n');
        write_new(&stage.path().join("audio.json"), &metadata)?;
        // Accepted asset directories are nonempty and cannot be replaced by
        // rename. A concurrent winner is verified before it is reused.
        match publish(stage.path(), &destination) {
            Ok(()) => Ok(()),
            Err(error) => {
                if entry_exists(&destination)? {
                    self.load_audio(id)?;
                    Ok(())
                } else {
                    Err(error.into())
                }
            }
        }
    }
}

fn ensure_directory(path: &Path) -> Result<(), PreparationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(PreparationError::InvalidCache(
            "cache directory must be a real directory, not a symlink or file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            require_directory(path)
        }
        Err(error) => Err(error.into()),
    }
}

fn require_directory(path: &Path) -> Result<(), PreparationError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PreparationError::InvalidCache(
            "cached audio directory must not be a symlink or file",
        ));
    }
    Ok(())
}

fn entry_exists(path: &Path) -> Result<bool, PreparationError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, PreparationError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PreparationError::InvalidCache(
            "cached file or receipt must not be a symlink or non-file",
        ));
    }
    if metadata.len() > limit as u64 {
        return Err(PreparationError::InvalidCache(
            "cached file exceeds its byte limit",
        ));
    }
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(PreparationError::InvalidCache(
            "cached file exceeds its byte limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn read_optional_metadata(path: &Path) -> Result<Option<Vec<u8>>, PreparationError> {
    if entry_exists(path)? {
        read_bounded(path, MAX_METADATA_BYTES).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn validate_metadata(json: &[u8]) -> Result<(), PreparationError> {
    // Persisted files include a trailing newline within the same byte budget.
    if json.len() >= MAX_METADATA_BYTES {
        return Err(PreparationError::InvalidCache("metadata exceeds 1 MiB"));
    }
    Ok(())
}

pub(super) fn publish_metadata(path: &Path, json: &[u8]) -> Result<(), PreparationError> {
    publish_metadata_using(path, json, |stage, destination| {
        fs::hard_link(stage, destination)
    })
}

pub(super) fn publish_metadata_using(
    path: &Path,
    json: &[u8],
    publish: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), PreparationError> {
    validate_metadata(json)?;
    let mut payload = Vec::with_capacity(json.len() + 1);
    payload.extend_from_slice(json);
    payload.push(b'\n');
    let parent = path.parent().ok_or(PreparationError::InvalidCache(
        "metadata destination needs a parent",
    ))?;
    let mut stage = tempfile::Builder::new()
        .prefix(".prepare-")
        .tempfile_in(parent)?;
    stage.write_all(&payload)?;
    stage.as_file().sync_all()?;
    // A hard link publishes a complete file without replacing an occupied name.
    match publish(stage.path(), path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_bounded(path, MAX_METADATA_BYTES)? != payload {
                return Err(PreparationError::InvalidCache(
                    "existing immutable metadata is inconsistent",
                ));
            }
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), PreparationError> {
    let mut output = File::create_new(path)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    Ok(())
}
