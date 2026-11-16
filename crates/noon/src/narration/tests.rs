use super::*;

fn audio() -> AudioResource {
    AudioResource::from_interleaved(8_000, 1, vec![0.125; 8_000], 32_000).unwrap()
}

#[test]
fn typed_preparation_shares_pcm_and_keeps_captions_and_provenance_explicit() {
    let audio = audio();
    let pointer = audio.samples().as_ptr();
    let narration = PreparedNarration::new(
        audio,
        "Two times x.",
        "2x",
        vec![NarrationCue::manual("move", 0.2, 0.8).unwrap()],
    )
    .unwrap();
    let cloned = narration.clone();
    assert_eq!(cloned.audio().samples().as_ptr(), pointer);
    assert_eq!(narration.duration_seconds(), 1.0);
    assert_eq!(narration.spoken_text(), "Two times x.");
    assert_eq!(narration.caption_text(), "2x");
    assert_eq!(
        narration.transcript_status().as_str(),
        "supplied-unverified"
    );
    assert_eq!(narration.cue("move").unwrap().start_seconds(), 0.2);
    assert_eq!(narration.cue("move").unwrap().end_seconds(), 0.8);
    assert_eq!(narration.cue("move").unwrap().provenance(), "manual");
    assert_eq!(narration.audio_id(), None);
    assert_eq!(narration.request_id(), None);
    assert!(matches!(
        narration.cue("missing"),
        Err(NarrationError::UnknownCue(_))
    ));
}

#[test]
fn invalid_text_and_cue_intervals_fail_during_preparation() {
    for text in [
        "".into(),
        " \n".into(),
        "a\0b".into(),
        "é".repeat(MAX_TEXT_BYTES / 2 + 1),
    ] {
        assert!(PreparedNarration::new(audio(), text, "", vec![]).is_err());
    }
    assert!(PreparedNarration::new(audio(), "Words", "", vec![]).is_ok());
    assert!(PreparedNarration::new(audio(), "Words", "a\0b", vec![]).is_err());
    for name in ["".into(), " \n".into(), "a\0b".into(), "é".repeat(65)] {
        assert!(NarrationCue::manual(name, 0.0, 1.0).is_err());
    }
    for (start, end) in [
        (-0.1, 1.0),
        (0.5, 0.5),
        (0.6, 0.5),
        (0.0, f64::NAN),
        (f64::INFINITY, 2.0),
    ] {
        assert!(NarrationCue::manual("move", start, end).is_err());
    }
}

#[test]
fn cue_order_bounds_and_unique_names_are_validated_against_actual_audio() {
    let cue = |name, start, end| NarrationCue::manual(name, start, end).unwrap();
    for cues in [
        vec![cue("move", 0.0, 0.5), cue("move", 0.5, 1.0)],
        vec![cue("later", 0.5, 1.0), cue("earlier", 0.0, 0.5)],
        vec![cue("too-long", 0.0, 1.01)],
        (0..=MAX_CUES)
            .map(|index| NarrationCue::manual(index.to_string(), 0.0, 0.5).unwrap())
            .collect(),
    ] {
        assert!(PreparedNarration::new(audio(), "Words", "Words", cues).is_err());
    }
    // Overlap is valid manual metadata; it does not imply an alignment engine.
    assert!(PreparedNarration::new(
        audio(),
        "Words",
        "Words",
        vec![cue("first", 0.0, 0.8), cue("second", 0.5, 1.0)]
    )
    .is_ok());
}

#[test]
fn language_boundary_classifies_narration_errors_from_typed_causes() {
    for (cause, category, code) in [
        (
            NarrationError::ForeignScene,
            "foreign_handle",
            "narration.foreign_scene",
        ),
        (
            NarrationError::ChangedClip,
            "stale_handle",
            "narration.changed_clip",
        ),
        (
            NarrationError::RunningWait,
            "unsupported_operation",
            "narration.running_wait",
        ),
        (
            NarrationError::UnknownCue("move".into()),
            "invalid_input",
            "narration.unknown_cue",
        ),
        (
            NarrationError::InvalidManifest("test"),
            "invalid_input",
            "narration.invalid_input",
        ),
    ] {
        let failure =
            crate::binding_error::AuthoringFailure::from(crate::AuthoringError::from(cause));
        assert_eq!(failure.category, category);
        assert_eq!(failure.code, code);
    }
}
