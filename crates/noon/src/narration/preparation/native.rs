//! Blocking native I/O for shared narration preparation.
//!
//! Filesystem and provider work finishes before Scene authoring. Cache metadata is
//! an explicit persisted boundary; freshly decoded audio stays typed in memory.

mod cache;
mod provider;

use std::path::{Path, PathBuf};

use super::{
    decode_wav, finish_preparation, manifest_bytes, wav_request, PreparationOptions, PreparedAudio,
    SpeechRequest,
};
use crate::narration::{codec, NarrationError, NarrationTranscriptStatus, PreparedNarration};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A prepared Rust media value and its immutable interchange manifest.
#[derive(Debug)]
pub struct PreparedOutput {
    pub narration: PreparedNarration,
    pub manifest_path: PathBuf,
}

/// A native preparation failure. Provider errors never include credentials,
/// request bodies, or response bodies.
#[derive(Debug)]
#[non_exhaustive]
pub enum PreparationError {
    Narration(NarrationError),
    Io(std::io::Error),
    InvalidCache(&'static str),
    MissingCredential,
    InvalidCredential,
    ProviderHttp(u16),
    ProviderTransport,
    ProviderResponse(&'static str),
}

impl std::fmt::Display for PreparationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Narration(error) => error.fmt(f),
            Self::Io(error) => write!(f, "narration cache I/O failed: {error}"),
            Self::InvalidCache(reason) => write!(f, "invalid narration cache: {reason}"),
            Self::MissingCredential => f.write_str("ELEVENLABS_API_KEY is required for an uncached synthesis request"),
            Self::InvalidCredential => f.write_str("ELEVENLABS_API_KEY must contain only printable ASCII without whitespace and be at most 4096 bytes"),
            Self::ProviderHttp(status) => write!(f, "ElevenLabs synthesis failed with HTTP {status}"),
            Self::ProviderTransport => f.write_str("ElevenLabs synthesis failed before a complete response"),
            Self::ProviderResponse(reason) => write!(f, "invalid ElevenLabs response: {reason}"),
        }
    }
}

impl std::error::Error for PreparationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Narration(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NarrationError> for PreparationError {
    fn from(error: NarrationError) -> Self {
        Self::Narration(error)
    }
}

impl From<std::io::Error> for PreparationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// An immutable local narration cache with optional direct ElevenLabs synthesis.
///
/// A verified cache hit performs no credential lookup or provider request.
/// Invalid occupied entries fail visibly; they are never treated as cache misses.
/// Calls are blocking and should run during preparation, outside playback and
/// device callbacks. Concurrent misses may both synthesize; publication never
/// replaces the first accepted result.
#[derive(Clone, Debug)]
pub struct NarrationPreparer {
    cache_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    format: String,
    request: Value,
    audio_id: String,
}

impl NarrationPreparer {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Prepare an existing bounded PCM16 WAV with a supplied transcript.
    pub fn prepare_wav(
        &self,
        wav: &[u8],
        spoken_text: impl Into<String>,
        options: PreparationOptions,
    ) -> Result<PreparedOutput, PreparationError> {
        let spoken_text = spoken_text.into();
        options.validate(&spoken_text)?;
        let (request, request_id) = wav_request(wav)?;
        self.prepare(
            request,
            request_id,
            spoken_text,
            options,
            NarrationTranscriptStatus::SuppliedUnverified,
            || decode_wav(wav).map_err(PreparationError::from),
        )
    }

    /// Synthesize an explicit ElevenLabs request, or reuse its verified cache.
    /// No automatic provider retry is performed.
    pub fn prepare_speech(
        &self,
        request: &SpeechRequest,
        options: PreparationOptions,
    ) -> Result<PreparedOutput, PreparationError> {
        self.prepare_speech_using(request, options, provider::elevenlabs_wav)
    }

    fn prepare_speech_using(
        &self,
        request: &SpeechRequest,
        options: PreparationOptions,
        synthesize: impl FnOnce(&SpeechRequest) -> Result<Vec<u8>, PreparationError>,
    ) -> Result<PreparedOutput, PreparationError> {
        request.validate()?;
        options.validate(request.text())?;
        let persisted_request = request.normalized_value()?;
        let request_id = codec::digest(&codec::canonical_json(&persisted_request)?);
        self.prepare(
            persisted_request,
            request_id,
            request.text().to_owned(),
            options,
            NarrationTranscriptStatus::ProviderInputUnverified,
            || {
                let wav = synthesize(request)?;
                let audio = decode_wav(&wav)?;
                if audio.metadata.sample_rate != 24_000 {
                    return Err(PreparationError::ProviderResponse(
                        "sample rate does not match requested wav_24000",
                    ));
                }
                Ok(audio)
            },
        )
    }

    fn prepare(
        &self,
        request: Value,
        request_id: String,
        spoken_text: String,
        options: PreparationOptions,
        status: NarrationTranscriptStatus,
        source: impl FnOnce() -> Result<PreparedAudio, PreparationError>,
    ) -> Result<PreparedOutput, PreparationError> {
        // The receipt envelope is known before audio exists. Reserve the exact
        // 64-byte identity and final newline before a potentially paid request.
        let mut expected_receipt = Receipt {
            format: "noon-narration-request-v1".into(),
            request,
            audio_id: "0".repeat(64),
        };
        cache::validate_metadata(&codec::canonical_json(&expected_receipt)?)?;
        let cache = cache::Cache::open(&self.cache_dir)?;
        let receipt_path = cache.request_path(&request_id);
        let existing = cache::read_optional_metadata(&receipt_path)?;
        let (audio, cache_miss) = if let Some(bytes) = existing {
            let receipt: Receipt =
                serde_json::from_value(codec::parse_json(&bytes)?).map_err(|_| {
                    PreparationError::InvalidCache("request receipt does not match its schema")
                })?;
            if receipt.format != expected_receipt.format
                || receipt.request != expected_receipt.request
            {
                return Err(PreparationError::InvalidCache(
                    "request receipt is inconsistent",
                ));
            }
            (cache.load_audio(&receipt.audio_id)?, false)
        } else {
            cache.preflight_writes()?;
            (source()?, true)
        };
        // Duration-dependent cue validation happens before publishing anything.
        // Construction reuses the typed AudioResource rather than importing JSON.
        let narration = finish_preparation(&audio, spoken_text, options, status, request_id)?;
        // Encode and bound every persisted value before publishing the asset
        // or receipt, including duration-dependent final manifest fields.
        let manifest = manifest_bytes(&audio, &narration)?;
        cache::validate_metadata(&manifest)?;
        expected_receipt.audio_id = audio.metadata.asset_id.clone();
        let receipt = codec::canonical_json(&expected_receipt)?;
        cache::validate_metadata(&receipt)?;
        if cache_miss {
            cache.store_audio(&audio)?;
            cache::publish_metadata(&receipt_path, &receipt)?;
        }
        let manifest_path = cache.manifest_path(&codec::digest(&manifest));
        cache::publish_metadata(&manifest_path, &manifest)?;
        Ok(PreparedOutput {
            narration,
            manifest_path,
        })
    }
}

#[cfg(test)]
mod tests;
