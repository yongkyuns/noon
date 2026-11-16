use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};

use super::*;
use crate::narration::preparation::MAX_WAV_BYTES;
use crate::narration::NarrationCue;

fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let data_size = u32::try_from(samples.len() * 2).unwrap();
    let mut output = Vec::new();
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&(36 + data_size).to_le_bytes());
    output.extend_from_slice(b"WAVEfmt ");
    output.extend_from_slice(&16_u32.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&channels.to_le_bytes());
    output.extend_from_slice(&rate.to_le_bytes());
    output.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    output.extend_from_slice(&(channels * 2).to_le_bytes());
    output.extend_from_slice(&16_u16.to_le_bytes());
    output.extend_from_slice(b"data");
    output.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        output.extend_from_slice(&sample.to_le_bytes());
    }
    output
}

fn recording() -> Vec<u8> {
    wav(&[-32768, -16384, 0, 16384, 32767], 24_000, 1)
}

fn speech_request() -> SpeechRequest {
    SpeechRequest::from_json(r#"{"provider":"elevenlabs","voice_id":"explicit_voice","model_id":"eleven_flash_v2_5","text":"Double the input.","voice_settings":{"stability":0.5,"similarity_boost":0.75,"style":0.0,"use_speaker_boost":true,"speed":1.0}}"#).unwrap()
}

fn setup() -> (tempfile::TempDir, NarrationPreparer) {
    let directory = tempfile::tempdir().unwrap();
    let preparer = NarrationPreparer::new(directory.path().join("cache"));
    (directory, preparer)
}

fn prepare(preparer: &NarrationPreparer) -> PreparedOutput {
    preparer
        .prepare_wav(&recording(), "x squared", PreparationOptions::default())
        .unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !path.exists() {
            return;
        }
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                visit(root, &path, output);
            } else if metadata.is_file() {
                output.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

fn assert_no_staging(root: &Path) {
    for directory in [
        root.to_owned(),
        root.join("audio"),
        root.join("requests"),
        root.join("narrations"),
    ] {
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries {
                assert!(!entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".prepare-"));
            }
        }
    }
}

#[test]
fn native_cache_returns_typed_narration_and_reusable_external_manifest() {
    let (_temporary, preparer) = setup();
    let output = prepare(&preparer);
    assert_eq!(output.narration.spoken_text(), "x squared");
    assert_eq!(
        output.narration.transcript_status(),
        NarrationTranscriptStatus::SuppliedUnverified
    );
    assert_eq!(output.narration.duration_seconds(), 5.0 / 24_000.0);
    let manifest = fs::read_to_string(&output.manifest_path).unwrap();
    let pcm = fs::read(
        preparer
            .cache_dir()
            .join("audio")
            .join(output.narration.audio_id().unwrap())
            .join("audio.f32le"),
    )
    .unwrap();
    let imported = PreparedNarration::from_prepared(&manifest, &pcm).unwrap();
    assert_eq!(imported.audio_id(), output.narration.audio_id());
    assert_eq!(imported.request_id(), output.narration.request_id());
    assert_eq!(
        imported.duration_seconds(),
        output.narration.duration_seconds()
    );
    assert_no_staging(preparer.cache_dir());
}

#[test]
fn wav_cache_hit_does_not_decode_the_source_again() {
    let (_temporary, preparer) = setup();
    let first = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let (request, id) = wav_request(&recording()).unwrap();
    let second = preparer
        .prepare(
            request,
            id,
            "x squared".into(),
            PreparationOptions::default(),
            NarrationTranscriptStatus::SuppliedUnverified,
            || panic!("cache hit must not decode source"),
        )
        .unwrap();
    assert_eq!(first.manifest_path, second.manifest_path);
    assert_eq!(before, snapshot(preparer.cache_dir()));
}

#[test]
fn speech_hit_never_requests_credentials_or_provider_and_metadata_edits_reuse_audio() {
    let (_temporary, preparer) = setup();
    let request = speech_request();
    let mut calls = 0;
    let first = preparer
        .prepare_speech_using(&request, PreparationOptions::default(), |_| {
            calls += 1;
            Ok(recording())
        })
        .unwrap();
    let pcm_path = preparer
        .cache_dir()
        .join("audio")
        .join(first.narration.audio_id().unwrap())
        .join("audio.f32le");
    let before_pcm = fs::read(&pcm_path).unwrap();
    let before_modified = fs::metadata(&pcm_path).unwrap().modified().unwrap();
    let second = preparer
        .prepare_speech_using(
            &request,
            PreparationOptions {
                caption_text: Some("x²".into()),
                cues: vec![NarrationCue::manual("term", 0.0, 4.0 / 24_000.0).unwrap()],
            },
            |_| panic!("cache hit must not enter credential/provider boundary"),
        )
        .unwrap();
    assert_eq!(calls, 1);
    assert_ne!(first.manifest_path, second.manifest_path);
    assert!(first.manifest_path.is_file());
    assert_eq!(first.narration.audio_id(), second.narration.audio_id());
    assert_eq!(first.narration.request_id(), second.narration.request_id());
    assert_eq!(
        second.narration.transcript_status(),
        NarrationTranscriptStatus::ProviderInputUnverified
    );
    assert_eq!(fs::read(&pcm_path).unwrap(), before_pcm);
    assert_eq!(
        fs::metadata(&pcm_path).unwrap().modified().unwrap(),
        before_modified
    );
    assert_eq!(second.narration.cues().len(), 1);
}

#[test]
fn changed_recording_preserves_every_previously_accepted_file() {
    let (_temporary, preparer) = setup();
    let first = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let second = preparer
        .prepare_wav(
            &wav(&[123, 456], 24_000, 1),
            "x squared",
            PreparationOptions::default(),
        )
        .unwrap();
    assert_ne!(first.narration.audio_id(), second.narration.audio_id());
    assert_ne!(first.narration.request_id(), second.narration.request_id());
    let after = snapshot(preparer.cache_dir());
    assert!(before
        .iter()
        .all(|(name, bytes)| after.get(name) == Some(bytes)));
}

#[test]
fn invalid_wav_and_out_of_duration_cues_publish_nothing() {
    let (_temporary, preparer) = setup();
    prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    for invalid in [
        b"not a WAV".to_vec(),
        recording()[..45].to_vec(),
        wav(&[], 24_000, 1),
    ] {
        assert!(preparer
            .prepare_wav(&invalid, "x squared", PreparationOptions::default())
            .is_err());
        assert_eq!(before, snapshot(preparer.cache_dir()));
    }
    let options = PreparationOptions {
        caption_text: None,
        cues: vec![NarrationCue::manual("too late", 0.0, 1.0).unwrap()],
    };
    assert!(preparer
        .prepare_wav(&wav(&[100, 200], 24_000, 1), "x squared", options)
        .is_err());
    assert_eq!(before, snapshot(preparer.cache_dir()));
}

#[test]
fn corrupt_pcm_audio_metadata_receipt_and_manifest_fail_without_replacement() {
    for target in [
        "pcm",
        "audio_metadata",
        "receipt",
        "manifest",
        "duplicate_receipt",
    ] {
        let (_temporary, preparer) = setup();
        let output = prepare(&preparer);
        let audio_directory = preparer
            .cache_dir()
            .join("audio")
            .join(output.narration.audio_id().unwrap());
        let receipt_path = preparer
            .cache_dir()
            .join("requests")
            .join(format!("{}.json", output.narration.request_id().unwrap()));
        match target {
            "pcm" => fs::write(audio_directory.join("audio.f32le"), vec![0_u8; 20]).unwrap(),
            "audio_metadata" => {
                let path = audio_directory.join("audio.json");
                let mut metadata: Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                metadata["duration_seconds"] = 1000.into();
                fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
            }
            "receipt" => fs::write(receipt_path, b"{}").unwrap(),
            "manifest" => fs::write(&output.manifest_path, b"{}").unwrap(),
            "duplicate_receipt" => fs::write(
                receipt_path,
                b"{\"format\":\"first\",\"format\":\"second\"}",
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot(preparer.cache_dir());
        assert!(
            preparer
                .prepare_wav(&recording(), "x squared", PreparationOptions::default())
                .is_err(),
            "{target}"
        );
        assert_eq!(before, snapshot(preparer.cache_dir()), "{target}");
        assert_no_staging(preparer.cache_dir());
    }
}

#[test]
fn failed_audio_publication_preserves_accepted_assets_and_cleans_stage() {
    let (_temporary, preparer) = setup();
    prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let cache = cache::Cache::open(preparer.cache_dir()).unwrap();
    let audio = decode_wav(&wav(&[123, 456], 24_000, 1)).unwrap();
    let result = cache.store_audio_using(&audio, |stage, _| {
        assert!(stage.join("audio.f32le").is_file());
        assert!(stage.join("audio.json").is_file());
        Err(std::io::Error::other("injected publication failure"))
    });
    assert!(result.is_err());
    assert_eq!(before, snapshot(preparer.cache_dir()));
    assert_no_staging(preparer.cache_dir());
}

#[test]
fn failed_manifest_publication_preserves_accepted_revision_and_cleans_stage() {
    let (_temporary, preparer) = setup();
    let accepted = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let target = preparer.cache_dir().join("narrations").join("new.json");
    let result = cache::publish_metadata_using(&target, b"{}", |stage, _| {
        assert_eq!(fs::read(stage).unwrap(), b"{}\n");
        Err(std::io::Error::other("injected publication failure"))
    });
    assert!(result.is_err());
    assert!(accepted.manifest_path.is_file());
    assert_eq!(before, snapshot(preparer.cache_dir()));
    assert_no_staging(preparer.cache_dir());
}

#[test]
fn invalid_configuration_and_cache_destinations_fail_before_provider_boundary() {
    for name in ["root", "audio", "requests", "narrations"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache");
        let occupied = if name == "root" {
            path.clone()
        } else {
            fs::create_dir(&path).unwrap();
            path.join(name)
        };
        fs::write(&occupied, b"preserve").unwrap();
        let preparer = NarrationPreparer::new(path);
        assert!(preparer
            .prepare_speech_using(
                &speech_request(),
                PreparationOptions::default(),
                |_| panic!("invalid destination must precede credentials/provider")
            )
            .is_err());
        assert_eq!(fs::read(occupied).unwrap(), b"preserve");
    }
    let (_temporary, preparer) = setup();
    let mut request = speech_request();
    request.voice_id = "../invalid".into();
    assert!(preparer
        .prepare_speech_using(&request, PreparationOptions::default(), |_| panic!(
            "invalid request must precede provider"
        ))
        .is_err());
    assert!(preparer
        .prepare_speech_using(
            &speech_request(),
            PreparationOptions {
                caption_text: Some("\0".into()),
                cues: Vec::new()
            },
            |_| panic!("invalid captions must precede provider")
        )
        .is_err());
    assert!(snapshot(preparer.cache_dir()).is_empty());
}

#[test]
fn provider_failure_does_not_retry_or_publish() {
    let (_temporary, preparer) = setup();
    prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let mut calls = 0;
    let error = preparer
        .prepare_speech_using(&speech_request(), PreparationOptions::default(), |_| {
            calls += 1;
            Err(PreparationError::ProviderHttp(401))
        })
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "ElevenLabs synthesis failed with HTTP 401"
    );
    assert_eq!(calls, 1);
    assert_eq!(before, snapshot(preparer.cache_dir()));
}

#[test]
fn malformed_oversized_and_wrong_rate_provider_responses_are_not_cached() {
    let (_temporary, preparer) = setup();
    for payload in [
        b"not a WAV".to_vec(),
        vec![0; MAX_WAV_BYTES + 1],
        wav(&[1, 2], 8_000, 1),
    ] {
        assert!(preparer
            .prepare_speech_using(&speech_request(), PreparationOptions::default(), |_| Ok(
                payload
            ))
            .is_err());
        assert!(snapshot(preparer.cache_dir()).is_empty());
    }
}

#[test]
fn concurrent_identical_publication_reuses_one_complete_asset() {
    let (_temporary, preparer) = setup();
    let barrier = Arc::new(Barrier::new(4));
    let threads = (0..4)
        .map(|_| {
            let preparer = preparer.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                prepare(&preparer).manifest_path
            })
        })
        .collect::<Vec<_>>();
    let paths = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert!(paths.iter().all(|path| path == &paths[0]));
    assert_eq!(
        fs::read_dir(preparer.cache_dir().join("audio"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(preparer.cache_dir().join("requests"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(preparer.cache_dir().join("narrations"))
            .unwrap()
            .count(),
        1
    );
    assert_no_staging(preparer.cache_dir());
}

#[cfg(unix)]
#[test]
fn dangling_receipt_and_symlink_cache_entries_are_rejected_before_provider() {
    use std::os::unix::fs::symlink;
    let (temporary, preparer) = setup();
    let request = speech_request();
    let output = preparer
        .prepare_speech_using(&request, PreparationOptions::default(), |_| Ok(recording()))
        .unwrap();
    let receipt = preparer
        .cache_dir()
        .join("requests")
        .join(format!("{}.json", output.narration.request_id().unwrap()));
    fs::remove_file(&receipt).unwrap();
    symlink(temporary.path().join("missing.json"), &receipt).unwrap();
    let error = preparer
        .prepare_speech_using(&request, PreparationOptions::default(), |_| {
            panic!("dangling receipt is occupied, never a miss")
        })
        .unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert!(receipt.is_symlink());
    assert!(output.manifest_path.is_file());
    for name in ["audio", "requests", "narrations"] {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        fs::create_dir(&cache).unwrap();
        symlink(directory.path().join("missing"), cache.join(name)).unwrap();
        let preparer = NarrationPreparer::new(cache);
        assert!(preparer
            .prepare_speech_using(&request, PreparationOptions::default(), |_| panic!(
                "symlink directory must precede provider"
            ))
            .is_err());
    }
}

#[test]
fn escaped_aggregate_metadata_fails_before_provider_and_preserves_accepted_cache() {
    let (_temporary, preparer) = setup();
    let accepted = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let mut request = speech_request();
    request.text = "\u{1}".repeat(crate::narration::preparation::MAX_TEXT_BYTES);
    let options = PreparationOptions {
        caption_text: Some(request.text.clone()),
        cues: (0..1000)
            .map(|index| {
                NarrationCue::manual(
                    format!("{index:04}{}", "\u{1}".repeat(124)),
                    0.0,
                    4.0 / 24_000.0,
                )
                .unwrap()
            })
            .collect(),
    };
    assert!(preparer
        .prepare_speech_using(&request, options, |_| {
            panic!("known oversized metadata must fail before credentials or synthesis")
        })
        .is_err());
    assert!(accepted.manifest_path.is_file());
    assert_eq!(before, snapshot(preparer.cache_dir()));
    assert_no_staging(preparer.cache_dir());
}

#[test]
fn oversized_receipt_envelope_fails_before_provider_and_preserves_accepted_cache() {
    use crate::narration::preparation::{MAX_METADATA_BYTES, MAX_TEXT_BYTES};
    let (_temporary, preparer) = setup();
    let accepted = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let mut request = speech_request();
    request.previous_text = Some("\u{1}".repeat(MAX_TEXT_BYTES));
    request.next_text = Some("\u{1}".repeat(MAX_TEXT_BYTES));
    request.text = "x".into();
    let target_size = MAX_METADATA_BYTES - 1;
    let padding = target_size - request.normalized_json().unwrap().len();
    request.text.push_str(&"\u{1}".repeat(padding / 6));
    request.text.push_str(&"x".repeat(padding % 6));
    assert!(request.text.len() <= MAX_TEXT_BYTES);
    assert_eq!(request.normalized_json().unwrap().len(), target_size);
    PreparationOptions::default()
        .validate(request.text())
        .unwrap();
    assert!(preparer
        .prepare_speech_using(&request, PreparationOptions::default(), |_| {
            panic!("oversized receipt envelope must fail before credentials or synthesis")
        })
        .is_err());
    assert!(accepted.manifest_path.is_file());
    assert_eq!(before, snapshot(preparer.cache_dir()));
    assert_no_staging(preparer.cache_dir());
}

#[test]
fn failed_cache_write_preflight_cleans_staging_and_preserves_accepted_cache() {
    let (_temporary, preparer) = setup();
    let accepted = prepare(&preparer);
    let before = snapshot(preparer.cache_dir());
    let cache = cache::Cache::open(preparer.cache_dir()).unwrap();
    for failure_at in 1..=3 {
        let mut writes = 0;
        let error = cache
            .preflight_writes_using(|file| {
                use std::io::Write;
                writes += 1;
                if writes == failure_at {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "injected unwritable cache",
                    ))
                } else {
                    file.write_all(b"probe")?;
                    file.sync_all()
                }
            })
            .unwrap_err();
        assert!(matches!(error, PreparationError::Io(_)));
        assert_eq!(writes, failure_at);
        assert!(accepted.manifest_path.is_file());
        assert_eq!(before, snapshot(preparer.cache_dir()));
        assert_no_staging(preparer.cache_dir());
    }
}
