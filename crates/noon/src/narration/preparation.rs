//! Shared preparation rules; ordinary typed calls require no language host.
//!
//! WAV decoding, request validation, cue validation and content identity are
//! identical on native and WASM. Optional native I/O is in the `native` module.
//! These operations never run during Scene execution or inside audio callbacks.

use serde_json::{json, Value};

use super::codec::{self, AudioMetadata};
use super::{
    validate_cues, validate_text, NarrationCue, NarrationError, NarrationTranscriptStatus,
    PreparedNarration,
};
use crate::{AudioResource, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET};

mod request;
mod wav;
pub use request::{PronunciationDictionary, SpeechRequest, VoiceSettings};

#[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
pub mod native;

pub const MAX_TEXT_BYTES: usize = super::MAX_TEXT_BYTES;
pub const MAX_PCM_BYTES: usize = DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET;
pub const MAX_WAV_BYTES: usize = MAX_PCM_BYTES + 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = codec::MAX_METADATA_BYTES;
pub const OUTPUT_FORMAT: &str = "wav_24000";

/// Text and manual cues may change without changing the recording identity.
#[derive(Clone, Debug, Default)]
pub struct PreparationOptions {
    pub caption_text: Option<String>,
    pub cues: Vec<NarrationCue>,
}

impl PreparationOptions {
    /// Reject all metadata failures knowable before audio or paid I/O exists.
    pub fn validate(&self, spoken_text: &str) -> Result<(), NarrationError> {
        validate_text(spoken_text, "spoken_text", false)?;
        if let Some(caption) = &self.caption_text {
            validate_text(caption, "caption_text", true)?;
        }
        validate_cues(&self.cues, None)?;
        codec::Manifest::validate_metadata_budget(
            spoken_text,
            self.caption_text.as_deref().unwrap_or(spoken_text),
            &self.cues,
        )
    }
}

/// Read manually timed intervals at the CLI/file boundary; timing rules remain
/// the same rules as typed [`NarrationCue`] and [`PreparedNarration`].
pub fn cues_from_json(json: &str) -> Result<Vec<NarrationCue>, NarrationError> {
    let value = codec::parse_json(json.as_bytes())?;
    let cues = serde_json::from_value(value)
        .map_err(|_| invalid("cues do not match the manual cue schema"))?;
    let cues = codec::convert_cues(cues)?;
    validate_cues(&cues, None)?;
    Ok(cues)
}

impl PreparedNarration {
    /// Decode a complete bounded PCM16 WAV into an inert typed narration.
    ///
    /// This function performs no I/O and works in native Rust or direct WASM.
    /// The transcript is supplied and unverified. Duration is measured from
    /// complete audio frames; manual cues must fit that actual duration.
    pub fn from_wav(
        wav: &[u8],
        spoken_text: impl Into<String>,
        options: PreparationOptions,
    ) -> Result<Self, NarrationError> {
        let spoken_text = spoken_text.into();
        options.validate(&spoken_text)?;
        let (_, request_id) = wav_request(wav)?;
        let audio = decode_wav(wav)?;
        finish_preparation(
            &audio,
            spoken_text,
            options,
            NarrationTranscriptStatus::SuppliedUnverified,
            request_id,
        )
    }
}

pub(crate) struct PreparedAudio {
    pub metadata: AudioMetadata,
    #[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
    pub pcm: Vec<u8>,
    pub resource: AudioResource,
}

impl PreparedAudio {
    fn from_pcm(pcm: Vec<u8>, sample_rate: u32, channels: u16) -> Result<Self, NarrationError> {
        let metadata = AudioMetadata::from_pcm(&pcm, sample_rate, channels)?;
        let resource = codec::resource_from_pcm(&pcm, sample_rate, channels)?;
        Ok(Self {
            metadata,
            #[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
            pcm,
            resource,
        })
    }

    #[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
    pub(crate) fn from_cached(metadata_json: &[u8], pcm: Vec<u8>) -> Result<Self, NarrationError> {
        let value = codec::parse_json(metadata_json)?;
        if value.get("path").is_some() {
            return Err(invalid(
                "audio cache metadata must not contain a manifest path",
            ));
        }
        let metadata: AudioMetadata = serde_json::from_value(value)
            .map_err(|_| invalid("cached audio metadata does not match the schema"))?;
        let resource = metadata.validate(&pcm, false)?;
        Ok(Self {
            metadata,
            pcm,
            resource,
        })
    }
}

pub(crate) fn decode_wav(wav: &[u8]) -> Result<PreparedAudio, NarrationError> {
    wav::decode(wav, MAX_PCM_BYTES)
}

pub(crate) fn wav_request(wav: &[u8]) -> Result<(Value, String), NarrationError> {
    if wav.len() > MAX_WAV_BYTES {
        return Err(invalid("WAV exceeds the preparation limit"));
    }
    let request = json!({ "provider": "import-wav", "source_sha256": codec::digest(wav) });
    let id = codec::digest(&codec::canonical_json(&request)?);
    Ok((request, id))
}

pub(crate) fn finish_preparation(
    audio: &PreparedAudio,
    spoken_text: String,
    options: PreparationOptions,
    status: NarrationTranscriptStatus,
    request_id: String,
) -> Result<PreparedNarration, NarrationError> {
    if !codec::is_digest(&request_id) {
        return Err(invalid("invalid preparation request identity"));
    }
    let caption_text = options.caption_text.unwrap_or_else(|| spoken_text.clone());
    let narration = PreparedNarration::new(
        audio.resource.clone(),
        spoken_text,
        caption_text,
        options.cues,
    )?;
    Ok(codec::with_provenance(
        narration,
        status,
        audio.metadata.asset_id.clone(),
        request_id,
    ))
}

#[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
pub(crate) fn manifest_bytes(
    audio: &PreparedAudio,
    narration: &PreparedNarration,
) -> Result<Vec<u8>, NarrationError> {
    codec::canonical_json(&codec::Manifest::from_narration(
        &audio.metadata,
        narration,
    )?)
}

pub(crate) fn invalid(reason: &'static str) -> NarrationError {
    NarrationError::InvalidPreparation(reason)
}

#[cfg(test)]
mod tests;
