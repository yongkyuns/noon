//! One persisted audio identity and validation codec for import and preparation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{NarrationCue, NarrationError, NarrationTranscriptStatus, PreparedNarration};
use crate::{AudioResource, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET};

pub(crate) const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioMetadata {
    pub format: String,
    pub encoding: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: usize,
    pub duration_seconds: f64,
    pub sha256: String,
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl AudioMetadata {
    pub fn from_pcm(pcm: &[u8], sample_rate: u32, channels: u16) -> Result<Self, NarrationError> {
        if !(8_000..=192_000).contains(&sample_rate) || !matches!(channels, 1 | 2) {
            return Err(invalid("unsupported PCM sample rate or channel count"));
        }
        if pcm.is_empty()
            || pcm.len() > DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET
            || !pcm.len().is_multiple_of(usize::from(channels) * 4)
        {
            return Err(invalid(
                "PCM must contain complete frames, nonempty and at most 64 MiB",
            ));
        }
        let frames = pcm.len() / (usize::from(channels) * 4);
        let mut identity = Sha256::new();
        identity.update(b"noon-narration-audio-v1\0");
        identity.update(sample_rate.to_le_bytes());
        identity.update(channels.to_le_bytes());
        identity.update(pcm);
        Ok(Self {
            format: "noon-narration-audio-v1".into(),
            encoding: "f32le".into(),
            sample_rate,
            channels,
            frames,
            duration_seconds: frames as f64 / f64::from(sample_rate),
            sha256: digest(pcm),
            asset_id: hex(identity.finalize().as_slice()),
            path: None,
        })
    }

    /// Verify all derived fields before allocating decoded float samples.
    pub fn validate(&self, pcm: &[u8], with_path: bool) -> Result<AudioResource, NarrationError> {
        let mut expected = Self::from_pcm(pcm, self.sample_rate, self.channels)?;
        if with_path {
            expected.path = Some(expected.cache_pcm_path());
        }
        if self != &expected {
            return Err(invalid("cached audio failed integrity verification"));
        }
        resource_from_pcm(pcm, self.sample_rate, self.channels)
    }

    pub fn cache_pcm_path(&self) -> String {
        format!("../audio/{}/audio.f32le", self.asset_id)
    }
}

pub(crate) fn resource_from_pcm(
    pcm: &[u8],
    sample_rate: u32,
    channels: u16,
) -> Result<AudioResource, NarrationError> {
    let samples = pcm
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    Ok(AudioResource::from_interleaved(
        sample_rate,
        channels,
        samples,
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )?)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub format: String,
    pub audio: AudioMetadata,
    pub request_id: String,
    pub spoken_text: String,
    pub transcript_status: String,
    pub caption_text: String,
    pub cues: Vec<Cue>,
}

impl Manifest {
    #[cfg(feature = "narration-preparation")]
    pub fn validate_metadata_budget(
        spoken_text: &str,
        caption_text: &str,
        cues: &[NarrationCue],
    ) -> Result<(), NarrationError> {
        // Use the persisted schema itself, including JSON escaping, before a
        // recording or paid provider call exists. All unknown integral fields
        // and identities occupy their largest supported encoding. The 32-byte
        // reserve exceeds any increase from a measured finite f64 duration.
        // This deliberately leaves a small margin at the 1 MiB file boundary.
        const DURATION_RESERVE: usize = 32;
        let mut audio = AudioMetadata {
            format: "noon-narration-audio-v1".into(),
            encoding: "f32le".into(),
            sample_rate: 192_000,
            channels: 2,
            frames: DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET / 4,
            duration_seconds: 0.0,
            sha256: "0".repeat(64),
            asset_id: "0".repeat(64),
            path: None,
        };
        audio.path = Some(audio.cache_pcm_path());
        let envelope = Self {
            format: "noon-narration-v1".into(),
            audio,
            request_id: "0".repeat(64),
            spoken_text: spoken_text.into(),
            transcript_status: NarrationTranscriptStatus::ProviderInputUnverified
                .as_str()
                .into(),
            caption_text: caption_text.into(),
            cues: cues.iter().map(Cue::from).collect(),
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|_| {
            NarrationError::InvalidPreparation("narration metadata could not be encoded")
        })?;
        if bytes.len() > MAX_METADATA_BYTES - DURATION_RESERVE {
            return Err(NarrationError::InvalidPreparation(
                "combined narration metadata exceeds the 1 MiB preparation budget",
            ));
        }
        Ok(())
    }

    pub fn into_narration(self, pcm: &[u8]) -> Result<PreparedNarration, NarrationError> {
        if self.format != "noon-narration-v1" || !is_digest(&self.request_id) {
            return Err(invalid(
                "unsupported manifest format or invalid request identity",
            ));
        }
        let status = match self.transcript_status.as_str() {
            "supplied-unverified" => NarrationTranscriptStatus::SuppliedUnverified,
            "provider-input-unverified" => NarrationTranscriptStatus::ProviderInputUnverified,
            _ => return Err(invalid("unsupported transcript provenance")),
        };
        let cues = convert_cues(self.cues)?;
        let resource = self.audio.validate(pcm, true)?;
        let narration =
            PreparedNarration::new(resource, self.spoken_text, self.caption_text, cues)?;
        Ok(with_provenance(
            narration,
            status,
            self.audio.asset_id,
            self.request_id,
        ))
    }

    #[cfg(all(feature = "narration-native", not(target_arch = "wasm32")))]
    pub fn from_narration(
        audio: &AudioMetadata,
        narration: &PreparedNarration,
    ) -> Result<Self, NarrationError> {
        let request_id = narration
            .request_id()
            .ok_or_else(|| invalid("missing preparation request identity"))?;
        if narration.audio_id() != Some(audio.asset_id.as_str()) || !is_digest(request_id) {
            return Err(invalid(
                "narration does not match the prepared audio identity",
            ));
        }
        let mut audio = audio.clone();
        audio.path = Some(audio.cache_pcm_path());
        Ok(Self {
            format: "noon-narration-v1".into(),
            audio,
            request_id: request_id.into(),
            spoken_text: narration.spoken_text().into(),
            transcript_status: narration.transcript_status().as_str().into(),
            caption_text: narration.caption_text().into(),
            cues: narration.cues().iter().map(Cue::from).collect(),
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cue {
    name: String,
    start_seconds: f64,
    end_seconds: f64,
    provenance: String,
}

impl From<&NarrationCue> for Cue {
    fn from(cue: &NarrationCue) -> Self {
        Self {
            name: cue.name().into(),
            start_seconds: cue.start_seconds(),
            end_seconds: cue.end_seconds(),
            provenance: "manual".into(),
        }
    }
}

pub(crate) fn convert_cues(cues: Vec<Cue>) -> Result<Vec<NarrationCue>, NarrationError> {
    if cues.len() > super::MAX_CUES {
        return Err(NarrationError::TooManyCues(cues.len()));
    }
    cues.into_iter()
        .map(|cue| {
            if cue.provenance != "manual" {
                return Err(invalid("cue provenance must be manual"));
            }
            NarrationCue::manual(cue.name, cue.start_seconds, cue.end_seconds)
        })
        .collect()
}

pub(crate) fn with_provenance(
    mut narration: PreparedNarration,
    status: NarrationTranscriptStatus,
    audio_id: String,
    request_id: String,
) -> PreparedNarration {
    let metadata = std::sync::Arc::get_mut(&mut narration.metadata)
        .expect("new prepared narration metadata has one owner");
    metadata.transcript_status = status;
    metadata.audio_id = Some(audio_id);
    metadata.request_id = Some(request_id);
    narration
}

pub(crate) fn invalid(reason: &'static str) -> NarrationError {
    NarrationError::InvalidManifest(reason)
}

pub(crate) fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    text
}

#[cfg(feature = "narration-preparation")]
pub(crate) fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, NarrationError> {
    let mut value =
        serde_json::to_value(value).map_err(|_| invalid("metadata could not be encoded"))?;
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid("metadata could not be encoded"))?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(invalid("metadata exceeds 1 MiB"));
    }
    Ok(bytes)
}

/// External JSON boundaries reject duplicate object fields at every depth.
#[cfg(feature = "narration-preparation")]
pub(crate) fn parse_json(bytes: &[u8]) -> Result<serde_json::Value, NarrationError> {
    use serde::de::{self, MapAccess, SeqAccess, Visitor};
    use serde_json::{Map, Number, Value};
    struct UniqueValue(Value);
    impl<'de> Deserialize<'de> for UniqueValue {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct UniqueVisitor;
            impl<'de> Visitor<'de> for UniqueVisitor {
                type Value = UniqueValue;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("JSON without duplicate object fields")
                }
                fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::Bool(value)))
                }
                fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::Number(value.into())))
                }
                fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::Number(value.into())))
                }
                fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                    Number::from_f64(value)
                        .map(|n| UniqueValue(Value::Number(n)))
                        .ok_or_else(|| E::custom("nonfinite number"))
                }
                fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::String(value.into())))
                }
                fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::String(value)))
                }
                fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(UniqueValue(Value::Null))
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                    let mut values = Vec::new();
                    while let Some(value) = seq.next_element::<UniqueValue>()? {
                        values.push(value.0);
                    }
                    Ok(UniqueValue(Value::Array(values)))
                }
                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    let mut values = Map::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if values.contains_key(&key) {
                            return Err(de::Error::custom("duplicate JSON field"));
                        }
                        let value = map.next_value::<UniqueValue>()?;
                        values.insert(key, value.0);
                    }
                    Ok(UniqueValue(Value::Object(values)))
                }
            }
            deserializer.deserialize_any(UniqueVisitor)
        }
    }
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(invalid("metadata exceeds 1 MiB"));
    }
    serde_json::from_slice::<UniqueValue>(bytes)
        .map(|value| value.0)
        .map_err(|_| invalid("invalid JSON or duplicate object field"))
}
