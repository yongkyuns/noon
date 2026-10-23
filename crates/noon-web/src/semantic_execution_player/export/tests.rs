use super::*;
use noon::{AudioClipOptions, AudioResource, Scene};

fn player() -> SemanticExecutionPlayer {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let samples = (0..4_000)
        .map(|frame| frame as f32 / 4_000.0)
        .collect::<Vec<_>>();
    let resource = AudioResource::from_interleaved(8_000, 1, samples, 32_000).unwrap();
    scene
        .audio(
            resource,
            AudioClipOptions {
                start_time: 0.125,
                duration: Some(1.0),
                looped: true,
                gain: 0.5,
                ..AudioClipOptions::default()
            },
        )
        .unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 1.125, 27)
            .unwrap();
    player.pause();
    player
}

fn options() -> Value {
    json!({
        "frame_rate": { "numerator": 16, "denominator": 1 },
        "start_frame": 2, "stop": { "kind": "end_time", "time": 0.21875 },
        "max_frames": 64, "max_transitions_per_sample": 64, "final_hold_seconds": 0.0,
    })
}

fn parse(json: String) -> Value {
    serde_json::from_str(&json).unwrap()
}

fn settle(player: &mut SemanticExecutionPlayer) -> Value {
    let status = parse(player.finite_export_status_json().unwrap());
    assert_eq!(status["kind"], "needs_sample");
    let time = status["target_time"].as_f64().unwrap();
    if player.time() != time {
        assert!(player
            .advance_forward_to_callback_phase_json(time)
            .unwrap()
            .is_none());
    }
    parse(player.observe_finite_export_json().unwrap())
}

#[test]
fn policy_retains_every_prefix_and_completion_until_exact_acknowledgement() {
    let mut player = player();
    let initial_publication = player.session.publication_context();
    let plan = parse(
        player
            .begin_finite_export_json(&options().to_string())
            .unwrap(),
    );
    assert_eq!(plan["source_end"], 1.125);
    assert_eq!(plan["sampling"]["frames"], 2);
    assert_eq!(plan["audio_frames"], 6_000);
    assert_eq!(player.session.publication_context(), initial_publication);
    assert!(player.finite_export_active());
    assert!(player
        .begin_finite_export_json(&options().to_string())
        .is_err());
    let mut kinds = Vec::new();
    let mut output_indices = Vec::new();
    loop {
        let sample = settle(&mut player);
        let observation = sample["observation"].clone();
        assert_eq!(parse(player.finite_export_status_json().unwrap()), sample);
        assert!(player.observe_finite_export_json().is_err());
        kinds.push(sample["sample_kind"].as_str().unwrap().to_owned());
        if let Some(index) = sample["frame"]["source_index"].as_u64() {
            output_indices.push((sample["frame"]["pts"].as_u64().unwrap(), index));
        }
        // This unit test checks only policy identity. The browser integration
        // separately proves its real renderer barrier before issuing this ack.
        let status = parse(
            player
                .acknowledge_finite_export_json(&observation.to_string())
                .unwrap(),
        );
        if status["kind"] == "complete" {
            assert_eq!(status["sampling"], plan["sampling"]);
            break;
        }
    }
    assert_eq!(
        kinds,
        ["prefix", "prefix", "output", "output", "completion"]
    );
    assert_eq!(output_indices, [(0, 2), (1, 3)]);
    assert_eq!(player.time(), 0.21875);
    player.cancel_finite_export();
    assert!(!player.finite_export_active());
    assert!(player.finite_export_status_json().is_err());
    assert!(player.mix_finite_export_audio_block(0, 1).is_err());
}

#[test]
fn pcm_uses_shared_absolute_grid_without_advancing_publication_and_silences_tail() {
    let mut player = player();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    let publication = player.session.publication_context();
    let full = player.mix_finite_export_audio_block(0, 6_000).unwrap();
    let mut partitioned = Vec::new();
    for (first, count) in [(0, 7), (7, 4_490), (4_497, 1_503)] {
        partitioned.extend(player.mix_finite_export_audio_block(first, count).unwrap());
    }
    assert_eq!(full, partitioned);
    // The crop begins exactly at the clip onset; six output samples correspond
    // to one 8 kHz source sample. Both stereo channels receive the mono value.
    assert_eq!(full[0], 0.0);
    assert!((full[6 * 2] - 0.5 / 4_000.0).abs() < 1e-7);
    assert!((full[600 * 2] - 0.0125).abs() < 1e-7);
    assert!(full[4_500 * 2..].iter().all(|&value| value == 0.0));
    assert_eq!(player.time(), 0.0);
    assert_eq!(player.session.publication_context(), publication);
    for (first, frames) in [(0, 0), (0, 16_385), (6_000, 1), (u64::MAX, 1)] {
        assert!(player.mix_finite_export_audio_block(first, frames).is_err());
    }
}

#[test]
fn source_end_and_terminal_holds_preserve_frozen_publication() {
    let mut player = player();
    let mut options = options();
    options["start_frame"] = 0.into();
    options["stop"] = json!({ "kind": "source_end" });
    options["final_hold_seconds"] = 0.125.into();
    let plan = parse(
        player
            .begin_finite_export_json(&options.to_string())
            .unwrap(),
    );
    assert_eq!(plan["sampling"]["frames"], 20);
    let mut terminal = None;
    let mut held = 0;
    loop {
        let sample = settle(&mut player);
        if sample["observation"]["published_time"] == 1.125 {
            let publication = sample["observation"]["publication"].clone();
            if let Some(expected) = terminal.as_ref() {
                assert_eq!(&publication, expected);
            } else {
                terminal = Some(publication);
            }
        }
        held += usize::from(sample["frame"]["held"] == true);
        let status = parse(
            player
                .acknowledge_finite_export_json(&sample["observation"].to_string())
                .unwrap(),
        );
        if status["kind"] == "complete" {
            break;
        }
    }
    assert_eq!(held, 2);
    assert_eq!(player.time(), 1.125);
}

#[test]
fn quiet_wait_publishes_current_receipt_without_dirty_rows_and_holds_reuse_it() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    scene.wait(0.125).unwrap();
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, vec![0.0; 1_000], 4_000).unwrap(),
            AudioClipOptions::default(),
        )
        .unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 0.125, 27)
            .unwrap();
    player.pause();
    let initial = player.delta(true).unwrap().unwrap();
    let mut presented = initial.retained.publication_context;
    let mut sequence = initial.retained.sequence;
    let mut options = options();
    options["start_frame"] = 0.into();
    options["stop"] = json!({ "kind": "source_end" });
    options["final_hold_seconds"] = 0.125.into();
    player
        .begin_finite_export_json(&options.to_string())
        .unwrap();
    let mut quiet_publications = 0;
    let mut reused_hold = 0;
    loop {
        let previous_time = player.time();
        let sample = settle(&mut player);
        let publication = player.session.publication_context();
        if previous_time != player.time() {
            // Runtime time and epoch advance through a wait without changing
            // objects. Export still needs a renderer receipt for this exact
            // publication before the host can acknowledge the sample.
            assert!(publication.frame_epoch().get() > presented.frame_epoch().get());
            let delta = player.delta(false).unwrap().unwrap();
            assert_eq!(delta.retained.publication_context, publication);
            assert_eq!(delta.retained.sequence, sequence + 1);
            assert_eq!(delta.retained.time, player.time());
            assert!(!delta.retained.snapshot);
            assert!(delta.retained.objects.is_empty());
            assert!(delta.retained.object_patches.is_empty());
            assert!(delta.retained.removed_slots.is_empty());
            assert!(delta.family_states.is_empty());
            assert!(delta.resource_additions.is_none());
            presented = publication;
            sequence = delta.retained.sequence;
            quiet_publications += 1;
        } else {
            assert_eq!(publication, presented);
            reused_hold += usize::from(sample["frame"]["held"] == true);
        }
        let status = parse(
            player
                .acknowledge_finite_export_json(&sample["observation"].to_string())
                .unwrap(),
        );
        if status["kind"] == "complete" {
            break;
        }
    }
    assert_eq!(quiet_publications, 2);
    assert_eq!(reused_hold, 1);
    assert_eq!(player.time(), 0.125);
}

#[test]
fn wrong_or_stale_acknowledgement_cannot_release_a_pending_publication() {
    let mut player = player();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    assert!(player.acknowledge_finite_export_json("{}").is_err());
    let sample = settle(&mut player);
    let mut wrong = sample["observation"].clone();
    wrong["publication"]["frame_epoch"] = "0001".into();
    assert!(player
        .acknowledge_finite_export_json(&wrong.to_string())
        .is_err());
    assert_eq!(parse(player.finite_export_status_json().unwrap()), sample);
    player.session.advance_to(0.01).unwrap();
    assert!(player
        .acknowledge_finite_export_json(&sample["observation"].to_string())
        .is_err());
    player.cancel_finite_export();
    assert!(player.observe_finite_export_json().is_err());
}

#[test]
fn fresh_paused_and_finite_admission_precedes_policy_or_pcm_allocation() {
    let mut playing = player();
    playing.resume();
    assert!(playing
        .begin_finite_export_json(&options().to_string())
        .is_err());
    assert!(!playing.finite_export_active());
    let mut advanced = player();
    advanced.session.advance_to(0.1).unwrap();
    assert!(advanced
        .begin_finite_export_json(&options().to_string())
        .is_err());
    let mut invalid = player();
    let mut invalid_options = options();
    invalid_options["max_frames"] = (1_u64 << 53).into();
    assert!(invalid
        .begin_finite_export_json(&invalid_options.to_string())
        .is_err());
    invalid_options = options();
    invalid_options["unexpected"] = true.into();
    assert!(invalid
        .begin_finite_export_json(&invalid_options.to_string())
        .is_err());
    invalid_options = options();
    invalid_options["stop"] = json!({ "kind": "source_end", "unexpected": true });
    assert!(invalid
        .begin_finite_export_json(&invalid_options.to_string())
        .is_err());
    assert!(!invalid.finite_export_active());
    invalid
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    invalid.resume();
    assert!(invalid.finite_export_status_json().is_err());
    assert!(invalid.mix_finite_export_audio_block(0, 1).is_err());
}
