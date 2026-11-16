use super::*;
use serde_json::json;

fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let bytes = samples.len() as u32 * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    wav.extend_from_slice(&(channels * 2).to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

fn speech() -> SpeechRequest {
    SpeechRequest::elevenlabs(
        "explicit_voice",
        "eleven_flash_v2_5",
        "Double the input.",
        VoiceSettings {
            stability: 0.5,
            similarity_boost: 0.75,
            style: 0.0,
            use_speaker_boost: true,
            speed: 1.0,
        },
    )
    .unwrap()
}

fn import(wav: &[u8]) -> Result<PreparedNarration, NarrationError> {
    PreparedNarration::from_wav(wav, "Two times x.", PreparationOptions::default())
}

#[test]
fn typed_wav_preparation_measures_frames_and_decodes_exact_signed_samples() {
    let narration = import(&wav(&[-32768, -16384, 0, 16384, 32767], 24000, 1)).unwrap();
    assert_eq!(
        narration.audio().samples(),
        [-1.0, -0.5, 0.0, 0.5, 32767.0 / 32768.0]
    );
    assert_eq!(narration.audio().frames(), 5);
    assert_eq!(narration.duration_seconds(), 5.0 / 24000.0);
    assert_eq!(narration.caption_text(), narration.spoken_text());
    assert_eq!(
        narration.transcript_status(),
        NarrationTranscriptStatus::SuppliedUnverified
    );

    let stereo = import(&wav(&[0, 8192, -16384, 32767, -1, 1], 11025, 2)).unwrap();
    assert_eq!(stereo.audio().frames(), 3);
    assert_eq!(stereo.audio().channels(), 2);
    assert_eq!(stereo.duration_seconds(), 3.0 / 11025.0);
    assert_eq!(
        stereo.audio_id(),
        Some("edbf4b2925cfbb146b5b8a4eabd84d1be7638e67e485dc1e2a271f178a787ab1")
    );
}

#[test]
fn recording_format_and_samples_are_content_identity_but_caption_and_cues_are_not() {
    let recording = wav(&[0, 8192, -16384, 32767], 24000, 1);
    let original = import(&recording).unwrap();
    let revised = PreparedNarration::from_wav(
        &recording,
        "Two times x.",
        PreparationOptions {
            caption_text: Some("2x".into()),
            cues: vec![NarrationCue::manual("move", 0.0, 2.0 / 24000.0).unwrap()],
        },
    )
    .unwrap();
    assert_eq!(original.audio_id(), revised.audio_id());
    assert_eq!(original.request_id(), revised.request_id());
    assert_eq!(original.audio(), revised.audio());
    assert_ne!(original.caption_text(), revised.caption_text());
    for other in [
        wav(&[0, 8192, -16384, 32767], 8000, 1),
        wav(&[0, 8192, -16384, 32767], 24000, 2),
        wav(&[1, 8192, -16384, 32767], 24000, 1),
    ] {
        let other = import(&other).unwrap();
        assert_ne!(original.audio_id(), other.audio_id());
        assert_ne!(original.request_id(), other.request_id());
    }
}

#[test]
fn malformed_wav_containers_and_unsupported_or_incomplete_pcm_are_rejected() {
    let valid = wav(&[1, -2, 3, -4], 24000, 1);
    let mut cases = vec![
        b"not a WAV".to_vec(),
        valid[..valid.len() - 1].to_vec(),
        wav(&[], 24000, 1),
        wav(&[1], 7999, 1),
        wav(&[1], 192001, 1),
        wav(&[1, 2, 3], 24000, 3),
        wav(&[1, 2, 3], 24000, 2),
    ];
    for (offset, bytes) in [
        (4, 12345_u32.to_le_bytes().to_vec()),
        (16, u32::MAX.to_le_bytes().to_vec()),
        (20, 3_u16.to_le_bytes().to_vec()),
        (28, 1_u32.to_le_bytes().to_vec()),
        (32, 1_u16.to_le_bytes().to_vec()),
        (34, 8_u16.to_le_bytes().to_vec()),
        (40, 7_u32.to_le_bytes().to_vec()),
    ] {
        let mut altered = valid.clone();
        altered[offset..offset + bytes.len()].copy_from_slice(&bytes);
        cases.push(altered);
    }
    for duplicate in [&valid[12..36], &valid[36..]] {
        let mut altered = valid.clone();
        altered.extend_from_slice(duplicate);
        let size = altered.len() as u32 - 8;
        altered[4..8].copy_from_slice(&size.to_le_bytes());
        cases.push(altered);
    }
    for altered in cases {
        assert!(import(&altered).is_err());
    }
}

#[test]
fn wav_chunk_padding_is_checked_and_decoded_budget_precedes_output_allocation() {
    let mut recording = wav(&[1, -2, 3, -4], 24000, 1);
    recording.extend_from_slice(b"JUNK\x01\0\0\0x\0");
    let size = recording.len() as u32 - 8;
    recording[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(import(&recording).is_ok());
    assert!(matches!(
        wav::decode(&recording, 12),
        Err(NarrationError::InvalidPreparation(
            "decoded PCM exceeds the preparation limit"
        ))
    ));
    recording.pop();
    let size = recording.len() as u32 - 8;
    recording[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(import(&recording).is_err());
}

#[test]
fn shared_metadata_preflight_rejects_known_failures_before_decode_and_checks_measured_bounds() {
    let cue = |name, start, end| NarrationCue::manual(name, start, end).unwrap();
    for options in [
        PreparationOptions {
            caption_text: Some("a\0b".into()),
            cues: vec![],
        },
        PreparationOptions {
            caption_text: None,
            cues: vec![cue("move", 0.0, 0.1), cue("move", 0.1, 0.2)],
        },
        PreparationOptions {
            caption_text: None,
            cues: vec![cue("later", 0.1, 0.2), cue("earlier", 0.0, 0.1)],
        },
    ] {
        assert!(options.validate("Words").is_err());
        assert!(PreparedNarration::from_wav(b"invalid", "Words", options).is_err());
    }
    let options = PreparationOptions {
        caption_text: None,
        cues: vec![cue("outside", 0.0, 1.0)],
    };
    assert!(options.validate("Words").is_ok());
    assert!(PreparedNarration::from_wav(&wav(&[1], 24000, 1), "Words", options).is_err());
    assert!(PreparationOptions::default().validate(" \n").is_err());
}

#[test]
fn metadata_preflight_bounds_combined_json_escaping_before_audio_or_provider_work() {
    let spoken = "\u{1}".repeat(MAX_TEXT_BYTES);
    let caption = "\u{2}".repeat(MAX_TEXT_BYTES);
    let cues = (0..1000)
        .map(|index| {
            NarrationCue::manual(format!("{index:04}{}", "\u{3}".repeat(124)), 0.0, 0.1).unwrap()
        })
        .collect::<Vec<_>>();
    // Every individual input is valid under the shared text/cue rules.
    validate_text(&spoken, "spoken_text", false).unwrap();
    validate_text(&caption, "caption_text", true).unwrap();
    validate_cues(&cues, None).unwrap();
    let options = PreparationOptions {
        caption_text: Some(caption),
        cues,
    };
    assert!(matches!(
        options.validate(&spoken),
        Err(NarrationError::InvalidPreparation(
            "combined narration metadata exceeds the 1 MiB preparation budget"
        ))
    ));
    // The metadata failure takes precedence even when there is no WAV to read.
    assert!(matches!(
        PreparedNarration::from_wav(b"invalid WAV", spoken, options),
        Err(NarrationError::InvalidPreparation(
            "combined narration metadata exceeds the 1 MiB preparation budget"
        ))
    ));
}

#[test]
fn manual_cue_json_uses_typed_rules_and_rejects_fabricated_provenance() {
    let base =
        json!([{"name":"move", "start_seconds":0, "end_seconds":0.2, "provenance":"manual"}]);
    let cues = cues_from_json(&base.to_string()).unwrap();
    assert_eq!(cues, [NarrationCue::manual("move", 0.0, 0.2).unwrap()]);
    for (key, value) in [
        ("provenance", json!("aligned")),
        ("start_seconds", json!(true)),
        ("start_seconds", json!(-1)),
        ("end_seconds", json!(0)),
        ("name", json!(" ")),
        ("unknown", json!(1)),
    ] {
        let mut invalid = base.clone();
        invalid[0][key] = value;
        assert!(cues_from_json(&invalid.to_string()).is_err());
    }
    assert!(cues_from_json(
        r#"[{"name":"a","name":"b","start_seconds":0,"end_seconds":1,"provenance":"manual"}]"#
    )
    .is_err());
}

#[test]
fn every_admitted_speech_input_invalidates_request_identity() {
    let base = speech();
    let mut variants = Vec::new();
    macro_rules! variant {
        ($field:ident, $value:expr) => {{
            let mut v = base.clone();
            v.$field = $value;
            variants.push(v);
        }};
    }
    variant!(text, "A changed sentence.".into());
    variant!(voice_id, "another_voice".into());
    variant!(model_id, "another_model".into());
    variant!(language_code, Some("en".into()));
    variant!(previous_text, Some("First sentence.".into()));
    variant!(next_text, Some("Next sentence.".into()));
    for (dictionary, version) in [("dictionary", "v1"), ("dictionary", "v2"), ("other", "v1")] {
        variant!(
            pronunciation_dictionary_locators,
            Some(vec![PronunciationDictionary {
                pronunciation_dictionary_id: dictionary.into(),
                version_id: version.into()
            }])
        );
    }
    for index in 0..5 {
        let mut v = base.clone();
        match index {
            0 => v.voice_settings.stability = 0.1,
            1 => v.voice_settings.similarity_boost = 0.1,
            2 => v.voice_settings.style = 0.1,
            3 => v.voice_settings.use_speaker_boost = false,
            _ => v.voice_settings.speed = 1.1,
        }
        variants.push(v);
    }
    let mut ids = std::collections::HashSet::new();
    ids.insert(base.request_id().unwrap());
    for variant in variants {
        assert!(ids.insert(variant.request_id().unwrap()));
    }
}

#[test]
fn request_normalization_is_deterministic_and_provider_body_has_only_api_fields() {
    let request = speech();
    let input = serde_json::to_string(&request).unwrap();
    let roundtrip = SpeechRequest::from_json(&input).unwrap();
    assert_eq!(
        request.request_id().unwrap(),
        roundtrip.request_id().unwrap()
    );
    let normalized = codec::parse_json(&request.normalized_json().unwrap()).unwrap();
    assert_eq!(normalized["output_format"], "wav_24000");
    assert_eq!(normalized["provider"], "elevenlabs");
    let body = codec::parse_json(&request.provider_body_json().unwrap()).unwrap();
    for field in ["provider", "voice_id", "output_format"] {
        assert!(body.get(field).is_none());
    }
    assert_eq!(body["model_id"], request.model_id);
    assert_eq!(body["text"], request.text);
    let integers = input
        .replace("\"style\":0.0", "\"style\":0")
        .replace("\"speed\":1.0", "\"speed\":1");
    assert_eq!(
        request.request_id().unwrap(),
        SpeechRequest::from_json(&integers)
            .unwrap()
            .request_id()
            .unwrap()
    );
}

#[test]
fn invalid_or_implicit_provider_inputs_are_rejected_without_io() {
    let base = serde_json::to_value(speech()).unwrap();
    for (field, value) in [
        ("provider", json!("other")),
        ("voice_id", json!("../bad")),
        ("model_id", json!("")),
        ("text", json!(" ")),
        ("voice_settings", json!({})),
        ("language_code", json!("EN")),
        ("previous_text", json!("a\0b")),
        ("next_text", json!(null)),
        ("pronunciation_dictionary_locators", json!([])),
        (
            "pronunciation_dictionary_locators",
            json!([{"pronunciation_dictionary_id":"dict"}]),
        ),
        ("endpoint", json!("https://elsewhere.invalid")),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        assert!(
            SpeechRequest::from_json(&invalid.to_string()).is_err(),
            "{field}"
        );
    }
    for field in ["provider", "voice_id", "model_id", "text", "voice_settings"] {
        let mut invalid = base.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(SpeechRequest::from_json(&invalid.to_string()).is_err());
    }
    for (field, value) in [
        ("stability", json!(-0.1)),
        ("similarity_boost", json!(1.1)),
        ("style", json!(true)),
        ("use_speaker_boost", json!(1)),
        ("speed", json!(0)),
        ("unknown", json!(0)),
    ] {
        let mut invalid = base.clone();
        invalid["voice_settings"][field] = value;
        assert!(
            SpeechRequest::from_json(&invalid.to_string()).is_err(),
            "{field}"
        );
    }
    let mut typed = speech();
    typed.voice_settings.speed = f64::NAN;
    assert!(typed.request_id().is_err());
    typed = speech();
    typed.model_id = "eleven_multilingual_v2".into();
    typed.language_code = Some("en".into());
    assert!(typed.validate().is_err());
}

#[test]
fn external_json_rejects_duplicate_fields_recursively_and_bounds_metadata() {
    for json in [
        r#"{"a":1,"a":2}"#,
        r#"{"a":{"x":1,"x":2}}"#,
        r#"[{"x":1,"x":2}]"#,
        "{}{}",
    ] {
        assert!(codec::parse_json(json.as_bytes()).is_err());
    }
    let request = serde_json::to_string(&speech())
        .unwrap()
        .replace("\"stability\":0.5", "\"stability\":0.1,\"stability\":0.5");
    assert!(SpeechRequest::from_json(&request).is_err());
    assert!(codec::parse_json(&vec![b' '; MAX_METADATA_BYTES + 1]).is_err());
}
