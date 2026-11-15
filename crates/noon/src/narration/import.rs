//! Explicit persisted-media import. No file access, synthesis, or scene state.

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{NarrationCue, NarrationError, NarrationTranscriptStatus, PreparedNarration};
use crate::{AudioResource, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET};

const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    audio: Audio,
    request_id: String,
    spoken_text: String,
    transcript_status: String,
    caption_text: String,
    cues: Vec<Cue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audio {
    format: String,
    encoding: String,
    sample_rate: u32,
    channels: u16,
    frames: usize,
    duration_seconds: f64,
    sha256: String,
    asset_id: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cue {
    name: String,
    start_seconds: f64,
    end_seconds: f64,
    provenance: String,
}

impl PreparedNarration {
    /// Import the v1 manifest and little-endian float PCM produced by
    /// `scripts/prepare_narration.py`.
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
        if pcm.is_empty() || pcm.len() > DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET {
            return Err(invalid("PCM must be nonempty and at most 64 MiB"));
        }
        let manifest: Manifest = serde_json::from_str(manifest)
            .map_err(|_| invalid("manifest does not match the v1 schema"))?;
        let audio = &manifest.audio;
        if manifest.format != "noon-narration-v1"
            || audio.format != "noon-narration-audio-v1"
            || audio.encoding != "f32le"
        {
            return Err(invalid("unsupported manifest or audio format"));
        }
        if !(8_000..=192_000).contains(&audio.sample_rate)
            || !matches!(audio.channels, 1 | 2)
            || audio.frames == 0
        {
            return Err(invalid("unsupported PCM rate, channels, or frame count"));
        }
        let expected_bytes = audio
            .frames
            .checked_mul(usize::from(audio.channels))
            .and_then(|samples| samples.checked_mul(4));
        if expected_bytes != Some(pcm.len()) {
            return Err(invalid(
                "PCM length does not match the complete declared frames",
            ));
        }
        let duration = audio.frames as f64 / f64::from(audio.sample_rate);
        if audio.duration_seconds != duration {
            return Err(invalid(
                "duration does not match the actual PCM frame count",
            ));
        }
        if !is_digest(&audio.sha256)
            || !is_digest(&audio.asset_id)
            || !is_digest(&manifest.request_id)
        {
            return Err(invalid(
                "audio and request identities must be lowercase SHA-256 values",
            ));
        }
        if audio.path != format!("../audio/{}/audio.f32le", audio.asset_id) {
            return Err(invalid(
                "audio path does not match the recognized asset location",
            ));
        }
        if hex(Sha256::digest(pcm).as_slice()) != audio.sha256 {
            return Err(invalid("PCM SHA-256 mismatch"));
        }
        let mut identity = Sha256::new();
        identity.update(b"noon-narration-audio-v1\0");
        identity.update(audio.sample_rate.to_le_bytes());
        identity.update(audio.channels.to_le_bytes());
        identity.update(pcm);
        if hex(identity.finalize().as_slice()) != audio.asset_id {
            return Err(invalid("audio asset identity mismatch"));
        }
        let transcript_status = match manifest.transcript_status.as_str() {
            "supplied-unverified" => NarrationTranscriptStatus::SuppliedUnverified,
            "provider-input-unverified" => NarrationTranscriptStatus::ProviderInputUnverified,
            _ => return Err(invalid("unsupported transcript provenance")),
        };
        if manifest.cues.len() > super::MAX_CUES {
            return Err(NarrationError::TooManyCues(manifest.cues.len()));
        }
        let cues = manifest
            .cues
            .into_iter()
            .map(|cue| {
                if cue.provenance != "manual" {
                    return Err(invalid("cue provenance must be manual"));
                }
                NarrationCue::manual(cue.name, cue.start_seconds, cue.end_seconds)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let samples = pcm
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect::<Vec<_>>();
        let resource = AudioResource::from_interleaved(
            audio.sample_rate,
            audio.channels,
            samples,
            DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
        )?;
        let mut narration = Self::new(resource, manifest.spoken_text, manifest.caption_text, cues)?;
        let metadata = std::sync::Arc::get_mut(&mut narration.metadata)
            .expect("new prepared narration metadata has one owner");
        metadata.transcript_status = transcript_status;
        metadata.audio_id = Some(manifest.audio.asset_id);
        metadata.request_id = Some(manifest.request_id);
        Ok(narration)
    }
}

fn invalid(reason: &'static str) -> NarrationError {
    NarrationError::InvalidManifest(reason)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    text
}

#[cfg(test)]
mod tests;
