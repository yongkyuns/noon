//! The fixed direct ElevenLabs HTTP boundary. No request is made on a cache hit.

use std::io::Read;
use std::time::Duration;

use super::PreparationError;
use crate::narration::preparation::{SpeechRequest, MAX_WAV_BYTES, OUTPUT_FORMAT};

const TIMEOUT: Duration = Duration::from_secs(60);

struct Request {
    url: String,
    body: Vec<u8>,
    api_key: ureq::http::HeaderValue,
}

pub(super) fn elevenlabs_wav(request: &SpeechRequest) -> Result<Vec<u8>, PreparationError> {
    // Configuration is already validated by the common preparer before this
    // function is reached. Recheck for callers within this module as well.
    request.validate()?;
    let api_key = std::env::var("ELEVENLABS_API_KEY").ok();
    let outgoing = prepare_request(request, api_key)?;
    let agent = ureq::Agent::new_with_config(provider_config());
    let mut response = agent
        .post(&outgoing.url)
        .header("xi-api-key", outgoing.api_key)
        .header("Content-Type", "application/json")
        .header("Accept", "audio/wav")
        .send(outgoing.body.as_slice())
        .map_err(|_| PreparationError::ProviderTransport)?;
    validate_status(response.status().as_u16())?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_WAV_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PreparationError::ProviderTransport)?;
    if bytes.len() > MAX_WAV_BYTES {
        return Err(PreparationError::ProviderResponse(
            "WAV exceeds the preparation limit",
        ));
    }
    Ok(bytes)
}

fn provider_config() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
}

fn prepare_request(
    request: &SpeechRequest,
    api_key: Option<String>,
) -> Result<Request, PreparationError> {
    let api_key = api_key.ok_or(PreparationError::MissingCredential)?;
    if api_key.is_empty() {
        return Err(PreparationError::MissingCredential);
    }
    if api_key.len() > 4096 || !api_key.bytes().all(|byte| (33..=126).contains(&byte)) {
        return Err(PreparationError::InvalidCredential);
    }
    let mut api_key = ureq::http::HeaderValue::from_str(&api_key)
        .map_err(|_| PreparationError::InvalidCredential)?;
    api_key.set_sensitive(true);
    Ok(Request {
        url: format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}?output_format={OUTPUT_FORMAT}",
            request.voice_id(),
        ),
        body: request.provider_body_json()?,
        api_key,
    })
}

fn validate_status(status: u16) -> Result<(), PreparationError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(PreparationError::ProviderHttp(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SpeechRequest {
        SpeechRequest::from_json(r#"{"provider":"elevenlabs","voice_id":"explicit_voice","model_id":"eleven_flash_v2_5","text":"Double the input.","voice_settings":{"stability":0.5,"similarity_boost":0.75,"style":0.0,"use_speaker_boost":true,"speed":1.0}}"#).unwrap()
    }

    #[test]
    fn provider_request_pins_endpoint_body_and_policy() {
        let request = request();
        let outgoing = prepare_request(&request, Some("test-secret".into())).unwrap();
        assert_eq!(
            outgoing.url,
            "https://api.elevenlabs.io/v1/text-to-speech/explicit_voice?output_format=wav_24000"
        );
        assert_eq!(outgoing.api_key.to_str().unwrap(), "test-secret");
        assert!(outgoing.api_key.is_sensitive());
        assert!(!format!("{:?}", outgoing.api_key).contains("test-secret"));
        let config = provider_config();
        assert_eq!(config.max_redirects(), 0);
        assert!(config.https_only());
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().global, Some(TIMEOUT));
        let body: serde_json::Value = serde_json::from_slice(&outgoing.body).unwrap();
        assert_eq!(body["model_id"], "eleven_flash_v2_5");
        assert_eq!(body["text"], "Double the input.");
        assert!(body.get("provider").is_none());
        assert!(body.get("voice_id").is_none());
        assert!(body.get("output_format").is_none());
        assert_eq!(TIMEOUT, Duration::from_secs(60));
    }

    #[test]
    fn missing_or_malformed_credentials_are_sanitized_before_http() {
        let request = request();
        assert!(matches!(
            prepare_request(&request, None),
            Err(PreparationError::MissingCredential)
        ));
        for secret in [
            "test-secret\n".to_owned(),
            "test secret".to_owned(),
            "é".to_owned(),
            "x".repeat(4097),
        ] {
            let error = prepare_request(&request, Some(secret)).err().unwrap();
            assert!(matches!(error, PreparationError::InvalidCredential));
            assert!(!error.to_string().contains("test-secret"));
        }
    }

    #[test]
    fn all_redirects_and_failures_are_rejected_without_body_details() {
        for status in [301, 302, 303, 307, 308, 400, 401, 429, 500] {
            let error = validate_status(status).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("ElevenLabs synthesis failed with HTTP {status}")
            );
        }
        assert!(validate_status(200).is_ok());
    }
}
