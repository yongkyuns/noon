use super::*;
use serde_json::{json, Value};

// Fixed external v1 fixture for a 11025 Hz stereo PCM16 WAV containing
// [0, 8192, -16384, 32767, -1, 1]. It pins round-trip duration, little-endian
// asset identity, and independent captions without running another language.
const EXTERNAL_MANIFEST: &str = r#"{"audio":{"asset_id":"edbf4b2925cfbb146b5b8a4eabd84d1be7638e67e485dc1e2a271f178a787ab1","channels":2,"duration_seconds":0.00027210884353741496,"encoding":"f32le","format":"noon-narration-audio-v1","frames":3,"path":"../audio/edbf4b2925cfbb146b5b8a4eabd84d1be7638e67e485dc1e2a271f178a787ab1/audio.f32le","sample_rate":11025,"sha256":"4ef4e050613c0272837a4b262373ffd706eaf0bcb38a86cef7e79edd2972e396"},"caption_text":"2x","cues":[{"end_seconds":0.00018140589569160998,"name":"move","provenance":"manual","start_seconds":0}],"format":"noon-narration-v1","request_id":"1b41f97ebf41c05c8dfa789bc1a46109e82778ad850c9e3214b7c02681f9b0e1","spoken_text":"Two times x.","transcript_status":"supplied-unverified"}"#;

fn pcm() -> Vec<u8> {
    [0_i16, 8192, -16384, 32767, -1, 1]
        .into_iter()
        .flat_map(|sample| (f32::from(sample) / 32768.0).to_le_bytes())
        .collect()
}

fn fixture() -> Value {
    serde_json::from_str(EXTERNAL_MANIFEST).unwrap()
}

fn import(value: &Value, pcm: &[u8]) -> Result<PreparedNarration, NarrationError> {
    PreparedNarration::from_prepared(&serde_json::to_string(value).unwrap(), pcm)
}

fn update_identity(value: &mut Value, pcm: &[u8]) {
    let rate = value["audio"]["sample_rate"].as_u64().unwrap() as u32;
    let channels = value["audio"]["channels"].as_u64().unwrap() as u16;
    let mut identity = Sha256::new();
    identity.update(b"noon-narration-audio-v1\0");
    identity.update(rate.to_le_bytes());
    identity.update(channels.to_le_bytes());
    identity.update(pcm);
    let id = hex(identity.finalize().as_slice());
    value["audio"]["asset_id"] = json!(id);
    value["audio"]["path"] = json!(format!("../audio/{id}/audio.f32le"));
    value["audio"]["sha256"] = json!(hex(Sha256::digest(pcm).as_slice()));
}

#[test]
fn external_preparation_fixture_preserves_pcm_duration_captions_and_manual_cues() {
    let narration = PreparedNarration::from_prepared(EXTERNAL_MANIFEST, &pcm()).unwrap();
    assert_eq!(narration.audio().sample_rate(), 11025);
    assert_eq!(narration.audio().channels(), 2);
    assert_eq!(narration.audio().frames(), 3);
    assert_eq!(narration.duration_seconds(), 3.0 / 11025.0);
    assert_eq!(
        narration.audio().samples(),
        [
            0.0,
            0.25,
            -0.5,
            32767.0 / 32768.0,
            -1.0 / 32768.0,
            1.0 / 32768.0
        ]
    );
    assert_eq!(narration.spoken_text(), "Two times x.");
    assert_eq!(narration.caption_text(), "2x");
    assert_eq!(narration.cue("move").unwrap().end_seconds(), 2.0 / 11025.0);
    assert_eq!(
        narration.audio_id(),
        Some("edbf4b2925cfbb146b5b8a4eabd84d1be7638e67e485dc1e2a271f178a787ab1")
    );
    assert_eq!(
        narration.request_id(),
        Some("1b41f97ebf41c05c8dfa789bc1a46109e82778ad850c9e3214b7c02681f9b0e1")
    );
}

#[test]
fn corrupt_pcm_or_mismatched_audio_metadata_cannot_enter_a_scene() {
    let original = pcm();
    let mut corrupt = original.clone();
    corrupt[2] ^= 1;
    assert!(PreparedNarration::from_prepared(EXTERNAL_MANIFEST, &corrupt).is_err());
    assert!(PreparedNarration::from_prepared(EXTERNAL_MANIFEST, &original[..23]).is_err());
    assert!(PreparedNarration::from_prepared(EXTERNAL_MANIFEST, &[]).is_err());
    for (field, bad) in [
        ("sample_rate", json!(8000)),
        ("sample_rate", json!(11025.0)),
        ("channels", json!(1)),
        ("frames", json!(4)),
        ("frames", json!(u64::MAX)),
        ("duration_seconds", json!(0.1)),
        ("sha256", json!("0".repeat(64))),
        ("asset_id", json!("0".repeat(64))),
        ("encoding", json!("f64le")),
        ("format", json!("noon-narration-audio-v2")),
        ("path", json!("/tmp/other-audio.f32le")),
    ] {
        let mut value = fixture();
        value["audio"][field] = bad;
        assert!(import(&value, &original).is_err(), "{field}");
    }
}

#[test]
fn integrity_valid_nonfinite_pcm_is_still_rejected_by_the_shared_resource_boundary() {
    let mut samples = pcm();
    samples[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut value = fixture();
    update_identity(&mut value, &samples);
    assert!(matches!(
        import(&value, &samples),
        Err(NarrationError::AudioResource(
            crate::AudioResourceError::NonFiniteSample { index: 0 }
        ))
    ));
}

#[test]
fn strict_schema_rejects_duplicates_unknown_fields_versions_and_false_provenance() {
    for (field, bad) in [
        ("format", json!("noon-narration-v2")),
        ("request_id", json!("not-a-hash")),
        ("spoken_text", json!(" ")),
        ("caption_text", json!("a\0b")),
        ("transcript_status", json!("aligned")),
        ("new_field", json!(true)),
    ] {
        let mut value = fixture();
        value[field] = bad;
        assert!(import(&value, &pcm()).is_err(), "{field}");
    }
    for (field, bad) in [
        ("provenance", json!("forced-aligned")),
        ("start_seconds", json!(true)),
        ("end_seconds", json!(1.0)),
        ("new_field", json!(true)),
    ] {
        let mut value = fixture();
        value["cues"][0][field] = bad;
        assert!(import(&value, &pcm()).is_err(), "cue {field}");
    }
    let duplicate =
        EXTERNAL_MANIFEST.replacen("\"channels\":2", "\"channels\":1,\"channels\":2", 1);
    assert!(PreparedNarration::from_prepared(&duplicate, &pcm()).is_err());
    assert!(PreparedNarration::from_prepared(
        &(" ".repeat(MAX_METADATA_BYTES) + EXTERNAL_MANIFEST),
        &pcm()
    )
    .is_err());
}

#[test]
fn captions_and_cues_change_independently_without_changing_imported_audio_identity() {
    let original = import(&fixture(), &pcm()).unwrap();
    let mut revised = fixture();
    revised["caption_text"] = json!("Twice the input");
    revised["cues"][0]["start_seconds"] = json!(1.0 / 11025.0);
    revised["transcript_status"] = json!("provider-input-unverified");
    let revised = import(&revised, &pcm()).unwrap();
    assert_eq!(original.audio_id(), revised.audio_id());
    assert_eq!(original.audio(), revised.audio());
    assert_ne!(original.caption_text(), revised.caption_text());
    assert_eq!(
        revised.transcript_status().as_str(),
        "provider-input-unverified"
    );
}
