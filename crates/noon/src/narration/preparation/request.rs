use serde::{Deserialize, Serialize};

use super::{codec, invalid, validate_text, NarrationError, OUTPUT_FORMAT};

/// Every supported synthesis setting is explicit and part of request identity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VoiceSettings {
    pub stability: f64,
    pub similarity_boost: f64,
    pub style: f64,
    pub use_speaker_boost: bool,
    pub speed: f64,
}

/// A dictionary version is pinned, so later edits cannot silently alter speech.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PronunciationDictionary {
    pub pronunciation_dictionary_id: String,
    pub version_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum Provider {
    #[serde(rename = "elevenlabs")]
    ElevenLabs,
}

/// A typed provider request, independent of credentials and network access.
///
/// Optional pronunciation/context fields alter the request fingerprint. Caption
/// and animation cue edits belong to `PreparationOptions`, outside this request.
/// Public fields remain editable; every consumption method validates again.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpeechRequest {
    provider: Provider,
    pub voice_id: String,
    pub model_id: String,
    pub text: String,
    pub voice_settings: VoiceSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronunciation_dictionary_locators: Option<Vec<PronunciationDictionary>>,
}

impl SpeechRequest {
    pub fn elevenlabs(
        voice_id: impl Into<String>,
        model_id: impl Into<String>,
        text: impl Into<String>,
        voice_settings: VoiceSettings,
    ) -> Result<Self, NarrationError> {
        let request = Self {
            provider: Provider::ElevenLabs,
            voice_id: voice_id.into(),
            model_id: model_id.into(),
            text: text.into(),
            voice_settings,
            language_code: None,
            previous_text: None,
            next_text: None,
            pronunciation_dictionary_locators: None,
        };
        request.validate()?;
        Ok(request)
    }

    /// Strict external configuration boundary, rejecting unknown/duplicate keys.
    pub fn from_json(json: &str) -> Result<Self, NarrationError> {
        let value = codec::parse_json(json.as_bytes())?;
        // Optional inputs are omitted or specified explicitly, never JSON null.
        for field in [
            "language_code",
            "previous_text",
            "next_text",
            "pronunciation_dictionary_locators",
        ] {
            if value.get(field).is_some_and(serde_json::Value::is_null) {
                return Err(invalid(
                    "optional speech inputs must have explicit values when present",
                ));
            }
        }
        let request: Self = serde_json::from_value(value)
            .map_err(|_| invalid("speech request has missing, invalid, or unsupported fields"))?;
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), NarrationError> {
        identifier(&self.voice_id)?;
        identifier(&self.model_id)?;
        validate_text(&self.text, "text", false)?;
        let settings = &self.voice_settings;
        if [
            settings.stability,
            settings.similarity_boost,
            settings.style,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(invalid("stability, similarity_boost, and style must be finite numbers between zero and one"));
        }
        if !settings.speed.is_finite() || settings.speed <= 0.0 {
            return Err(invalid(
                "speed must be finite and positive; provider-specific limits still apply",
            ));
        }
        if let Some(code) = &self.language_code {
            if code.len() != 2 || !code.bytes().all(|byte| byte.is_ascii_lowercase()) {
                return Err(invalid(
                    "language_code must be a two-letter lowercase ISO 639-1 code",
                ));
            }
            if self.model_id == "eleven_multilingual_v2" {
                return Err(invalid(
                    "eleven_multilingual_v2 does not support language_code",
                ));
            }
        }
        for text in [&self.previous_text, &self.next_text].into_iter().flatten() {
            validate_text(text, "continuity text", true)?;
        }
        if let Some(locators) = &self.pronunciation_dictionary_locators {
            if !(1..=3).contains(&locators.len()) {
                return Err(invalid(
                    "pronunciation dictionaries require one to three versioned entries",
                ));
            }
            for locator in locators {
                identifier(&locator.pronunciation_dictionary_id)?;
                identifier(&locator.version_id)?;
            }
        }
        Ok(())
    }

    pub fn voice_id(&self) -> &str {
        &self.voice_id
    }
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Deterministic bytes only for hashing/persistence, not an engine boundary.
    pub fn normalized_json(&self) -> Result<Vec<u8>, NarrationError> {
        codec::canonical_json(&self.normalized_value()?)
    }

    pub(crate) fn normalized_value(&self) -> Result<serde_json::Value, NarrationError> {
        self.validate()?;
        let mut value = serde_json::to_value(self)
            .map_err(|_| invalid("speech request could not be encoded"))?;
        value["output_format"] = OUTPUT_FORMAT.into();
        Ok(value)
    }

    pub fn request_id(&self) -> Result<String, NarrationError> {
        Ok(codec::digest(&self.normalized_json()?))
    }

    /// Provider HTTP body; provider/voice/format are represented by the endpoint.
    pub fn provider_body_json(&self) -> Result<Vec<u8>, NarrationError> {
        self.validate()?;
        let mut value = serde_json::to_value(self)
            .map_err(|_| invalid("speech request could not be encoded"))?;
        let object = value
            .as_object_mut()
            .expect("speech request serializes as an object");
        object.remove("provider");
        object.remove("voice_id");
        codec::canonical_json(&value)
    }
}

fn identifier(value: &str) -> Result<(), NarrationError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid(
            "voice, model, and dictionary identifiers must be explicit provider identifiers",
        ));
    }
    Ok(())
}
