//! Prepared speech and manual phrase metadata, independent of scene execution.
//!
//! PCM is prepared once, before execution. A prepared value and its Scene
//! placement never synthesize speech, align words, or own another clock or scene
//! representation. Optional preparation helpers live beside this authoring API;
//! Scene admission uses the normal immutable audio and semantic clip boundary.

use std::{collections::HashSet, sync::Arc};

use crate::{AudioResource, AudioResourceError, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET};

#[cfg(feature = "narration-import")]
mod codec;
#[cfg(feature = "narration-import")]
mod import;
#[cfg(feature = "narration-preparation")]
pub mod preparation;
#[cfg(test)]
mod tests;

pub(crate) const MAX_TEXT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_CUES: usize = 1000;
pub(crate) const MAX_CUE_NAME_BYTES: usize = 128;

/// How the supplied transcript relates to the prepared recording.
///
/// Neither value claims recognition, forced alignment, or listening review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NarrationTranscriptStatus {
    SuppliedUnverified,
    ProviderInputUnverified,
}

impl NarrationTranscriptStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SuppliedUnverified => "supplied-unverified",
            Self::ProviderInputUnverified => "provider-input-unverified",
        }
    }
}

/// A manually supplied interval in source-audio seconds.
///
/// Construction checks the interval itself. [`PreparedNarration::new`] also
/// checks its containment in the actual audio and its order among other cues.
#[derive(Clone, Debug, PartialEq)]
pub struct NarrationCue {
    name: String,
    start_seconds: f64,
    end_seconds: f64,
}

impl NarrationCue {
    pub fn manual(
        name: impl Into<String>,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Result<Self, NarrationError> {
        let name = name.into();
        validate_cue_name(&name)?;
        if !start_seconds.is_finite()
            || !end_seconds.is_finite()
            || start_seconds < 0.0
            || end_seconds <= start_seconds
        {
            return Err(NarrationError::InvalidCue(
                "manual cue must have finite 0 <= start < end",
            ));
        }
        Ok(Self {
            name,
            start_seconds,
            end_seconds,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    pub const fn end_seconds(&self) -> f64 {
        self.end_seconds
    }

    pub const fn provenance(&self) -> &'static str {
        "manual"
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NarrationMetadata {
    pub spoken_text: String,
    pub caption_text: String,
    pub transcript_status: NarrationTranscriptStatus,
    pub cues: Vec<NarrationCue>,
    pub audio_id: Option<String>,
    pub request_id: Option<String>,
}

impl NarrationMetadata {
    pub fn cue(&self, name: &str) -> Result<&NarrationCue, NarrationError> {
        validate_cue_name(name)?;
        self.cues
            .iter()
            .find(|cue| cue.name == name)
            .ok_or_else(|| NarrationError::UnknownCue(name.into()))
    }
}

/// Immutable prepared speech, display text, and explicitly manual cue metadata.
///
/// This value owns no semantic identity. Cloning it shares PCM and metadata;
/// [`crate::Scene::narrate`] admits the existing resource through normal audio
/// transactions. Direct typed construction requires no JSON or language host.
#[derive(Clone, Debug)]
pub struct PreparedNarration {
    audio: AudioResource,
    pub(crate) metadata: Arc<NarrationMetadata>,
}

impl PreparedNarration {
    /// Prepare supplied PCM and an unverified transcript without any I/O.
    ///
    /// Captions may be empty or differ from pronunciation-friendly spoken text.
    /// Cues may overlap, but their starts must be ordered and names unique.
    pub fn new(
        audio: AudioResource,
        spoken_text: impl Into<String>,
        caption_text: impl Into<String>,
        cues: Vec<NarrationCue>,
    ) -> Result<Self, NarrationError> {
        let spoken_text = spoken_text.into();
        let caption_text = caption_text.into();
        validate_text(&spoken_text, "spoken_text", false)?;
        validate_text(&caption_text, "caption_text", true)?;
        if audio.pcm_bytes() > DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET {
            return Err(NarrationError::AudioResource(
                AudioResourceError::PcmByteLimitExceeded {
                    bytes: audio.pcm_bytes(),
                    limit: DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
                },
            ));
        }
        validate_cues(&cues, Some(audio.duration_seconds()))?;
        Ok(Self {
            audio,
            metadata: Arc::new(NarrationMetadata {
                spoken_text,
                caption_text,
                transcript_status: NarrationTranscriptStatus::SuppliedUnverified,
                cues,
                audio_id: None,
                request_id: None,
            }),
        })
    }

    pub fn audio(&self) -> &AudioResource {
        &self.audio
    }

    pub fn duration_seconds(&self) -> f64 {
        self.audio.duration_seconds()
    }

    pub fn spoken_text(&self) -> &str {
        &self.metadata.spoken_text
    }

    pub fn caption_text(&self) -> &str {
        &self.metadata.caption_text
    }

    pub fn transcript_status(&self) -> NarrationTranscriptStatus {
        self.metadata.transcript_status
    }

    pub fn cues(&self) -> &[NarrationCue] {
        &self.metadata.cues
    }

    pub fn cue(&self, name: &str) -> Result<&NarrationCue, NarrationError> {
        self.metadata.cue(name)
    }

    /// Content identity, available for prepared WAV or imported cached audio.
    pub fn audio_id(&self) -> Option<&str> {
        self.metadata.audio_id.as_deref()
    }

    /// Request provenance, not evidence that the transcript matches the audio.
    pub fn request_id(&self) -> Option<&str> {
        self.metadata.request_id.as_deref()
    }
}

pub(crate) fn validate_cues(
    cues: &[NarrationCue],
    duration: Option<f64>,
) -> Result<(), NarrationError> {
    if cues.len() > MAX_CUES {
        return Err(NarrationError::TooManyCues(cues.len()));
    }
    let mut names = HashSet::with_capacity(cues.len());
    let mut previous_start = 0.0;
    for cue in cues {
        if !names.insert(cue.name.as_str()) {
            return Err(NarrationError::DuplicateCue(cue.name.clone()));
        }
        if cue.start_seconds < previous_start {
            return Err(NarrationError::InvalidCue("cue starts must be ordered"));
        }
        if duration.is_some_and(|duration| cue.end_seconds > duration) {
            return Err(NarrationError::InvalidCue(
                "cue interval exceeds the actual audio duration",
            ));
        }
        previous_start = cue.start_seconds;
    }
    Ok(())
}

pub(crate) fn validate_text(
    text: &str,
    field: &'static str,
    empty: bool,
) -> Result<(), NarrationError> {
    if text.len() > MAX_TEXT_BYTES || text.contains('\0') || (!empty && text.trim().is_empty()) {
        return Err(NarrationError::InvalidText(field));
    }
    Ok(())
}

fn validate_cue_name(name: &str) -> Result<(), NarrationError> {
    if name.len() > MAX_CUE_NAME_BYTES || name.contains('\0') || name.trim().is_empty() {
        return Err(NarrationError::InvalidCue(
            "cue name must be nonblank UTF-8 without NUL, at most 128 bytes",
        ));
    }
    Ok(())
}

/// Preparation or cue-authoring failures; failed validation changes no Scene.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum NarrationError {
    InvalidText(&'static str),
    TooManyCues(usize),
    InvalidCue(&'static str),
    DuplicateCue(String),
    UnknownCue(String),
    InvalidManifest(&'static str),
    InvalidPreparation(&'static str),
    AudioResource(AudioResourceError),
    ForeignScene,
    ChangedClip,
    InvalidTimeline,
    RunningWait,
}

impl std::fmt::Display for NarrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidText(field) => write!(formatter, "{field} must be bounded UTF-8 without NUL; spoken text must be nonblank"),
            Self::TooManyCues(count) => write!(formatter, "narration has {count} cues; at most {MAX_CUES} are supported"),
            Self::InvalidCue(reason) => formatter.write_str(reason),
            Self::DuplicateCue(name) => write!(formatter, "duplicate narration cue {name:?}"),
            Self::UnknownCue(name) => write!(formatter, "unknown narration cue {name:?}"),
            Self::InvalidManifest(reason) => write!(formatter, "invalid prepared narration: {reason}"),
            Self::InvalidPreparation(reason) => write!(formatter, "narration preparation failed: {reason}"),
            Self::AudioResource(error) => error.fmt(formatter),
            Self::ForeignScene => formatter.write_str("narration belongs to another Scene"),
            Self::ChangedClip => formatter.write_str("narration clip changed after placement; its recorded cues no longer apply"),
            Self::InvalidTimeline => formatter.write_str("narration timing is not representable on this authored timeline"),
            Self::RunningWait => formatter.write_str("narration waits currently require finite Scene authoring before execution bootstrap"),
        }
    }
}

impl std::error::Error for NarrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AudioResource(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AudioResourceError> for NarrationError {
    fn from(error: AudioResourceError) -> Self {
        Self::AudioResource(error)
    }
}
