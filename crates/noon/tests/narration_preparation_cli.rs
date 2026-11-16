#![cfg(feature = "narration-native")]

use std::fs;
use std::process::Command;

fn wav() -> Vec<u8> {
    let mut data = b"RIFF".to_vec();
    data.extend_from_slice(&40_u32.to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16_u32.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&24_000_u32.to_le_bytes());
    data.extend_from_slice(&48_000_u32.to_le_bytes());
    data.extend_from_slice(&2_u16.to_le_bytes());
    data.extend_from_slice(&16_u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&4_u32.to_le_bytes());
    data.extend_from_slice(&(-16_384_i16).to_le_bytes());
    data.extend_from_slice(&16_384_i16.to_le_bytes());
    data
}

#[test]
fn narration_cli_imports_and_reuses_manifest_without_python_or_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let recording = directory.path().join("recording.wav");
    let spoken = directory.path().join("spoken.txt");
    let cache = directory.path().join("cache");
    fs::write(&recording, wav()).unwrap();
    fs::write(&spoken, "x squared").unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_prepare_narration"))
            .args([
                "--wav",
                recording.to_str().unwrap(),
                "--spoken-text-file",
                spoken.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .env_remove("ELEVENLABS_API_KEY")
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run();
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let path = String::from_utf8(first.stdout).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(path.trim()).unwrap()).unwrap();
    assert_eq!(manifest["spoken_text"], "x squared");
    assert_eq!(manifest["transcript_status"], "supplied-unverified");
}

#[test]
fn narration_cli_rejects_duplicate_cue_fields_and_bad_source_options() {
    let directory = tempfile::tempdir().unwrap();
    let recording = directory.path().join("recording.wav");
    let spoken = directory.path().join("spoken.txt");
    let cues = directory.path().join("cues.json");
    let cache = directory.path().join("cache");
    fs::write(&recording, wav()).unwrap();
    fs::write(&spoken, "x squared").unwrap();
    fs::write(&cues, r#"[{"name":"first","name":"second","start_seconds":0,"end_seconds":0.00001,"provenance":"manual"}]"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_prepare_narration"))
        .args([
            "--wav",
            recording.to_str().unwrap(),
            "--spoken-text-file",
            spoken.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--cues",
            cues.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!cache.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_prepare_narration"))
        .args([
            "--wav",
            recording.to_str().unwrap(),
            "--request",
            "missing.json",
            "--cache-dir",
            cache.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
