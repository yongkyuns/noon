//! Explicit persisted-media import. No file access, synthesis, or scene state.

use super::codec::{invalid, Manifest, MAX_METADATA_BYTES};
use super::{NarrationError, PreparedNarration};

impl PreparedNarration {
    /// Import a v1 narration manifest and little-endian float PCM.
    ///
    /// The caller owns bounded file/URL access. `audio.path` must match the
    /// recognized cache layout, but is never opened here. Complete PCM integrity,
    /// format, measured duration, and metadata are checked before returning an
    /// inert typed value. Neither request provenance nor manual cues certify
    /// the supplied transcript or phrase alignment.
    pub fn from_prepared(manifest: &str, pcm: &[u8]) -> Result<Self, NarrationError> {
        if manifest.len() > MAX_METADATA_BYTES {
            return Err(invalid("manifest exceeds 1 MiB"));
        }
        let manifest: Manifest = serde_json::from_str(manifest)
            .map_err(|_| invalid("manifest does not match the v1 schema"))?;
        manifest.into_narration(pcm)
    }
}

#[cfg(test)]
use super::codec::hex;
#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
mod tests;
