use super::*;
use crate::{
    InstalledRetainedExecutionMirror, RetainedFamilyExecutionDeltaEnvelope,
    RetainedTransportApplyOutcome,
};
use noon::{AudioClipOptions, AudioResource, Scene};
use noon_runtime::ReplayLimits;

fn options() -> Value {
    json!({
        "frame_rate": { "numerator": 16, "denominator": 1 },
        "start_frame": 0, "stop": { "kind": "source_end" },
        "max_frames": 64, "max_transitions_per_sample": 64, "final_hold_seconds": 0.125,
    })
}

fn parse(json: String) -> Value {
    serde_json::from_str(&json).unwrap()
}

fn completed_player() -> PreparedExecutionPlayer {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    scene
        .audio(
            AudioResource::from_interleaved(
                8_000,
                1,
                (0..4_000).map(|i| i as f32 / 4_000.0).collect::<Vec<_>>(),
                16_000,
            )
            .unwrap(),
            AudioClipOptions {
                start_time: 0.125,
                duration: Some(2.0),
                looped: true,
                gain: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let first = source.wait_segment(0.5).unwrap();
    source.advance_to(0.5).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
    scene.live(&mut source).shift(&circle, 1.0, 0.0).unwrap();
    let last = source.wait_segment(0.625).unwrap();
    source.advance_to(1.125).unwrap();
    source.seal_replay().unwrap();
    let terminal = source.finish_prepared_prefix(last, &mut publisher).unwrap();
    reader
        .reserve_object_capacity(terminal.required_object_capacity())
        .unwrap();
    reader.append(terminal).unwrap();
    // All original source owners are dropped on return. Only the real reader
    // and its original immutable soundtrack closure remain available to export.
    PreparedExecutionPlayer::from_reader(reader, 97).unwrap()
}

#[test]
fn completed_history_exports_actual_revision_receipts_and_reuses_terminal_publication() {
    let mut player = completed_player();
    let identity = player.playback.reader.runtime_identity();
    let admission = player.playback.reader.publication_context();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(
        &player.resource_bundle_bytes().unwrap(),
    )
    .unwrap();
    mirror
        .apply_json(&player.initial_delta_json().unwrap())
        .unwrap();
    let plan = parse(
        player
            .begin_finite_export_json(&options().to_string())
            .unwrap(),
    );
    assert_eq!(
        plan["source"],
        source_json("completed_history", identity, admission)
    );
    assert_eq!(plan["sampling"]["frames"], 20);
    assert_eq!(player.prepared_time(), 0.0);
    let mut frames = 0;
    let mut terminal = None;
    let mut held = 0;
    let mut metadata_only = 0;
    loop {
        let sample = parse(player.observe_finite_export_json().unwrap());
        let publication = player.playback.reader.publication_context();
        assert_eq!(
            sample["observation"]["runtime_identity"],
            identity.get().to_string()
        );
        assert_eq!(
            sample["observation"]["published_time"],
            player.prepared_time()
        );
        assert_eq!(
            sample["observation"]["publication"]["execution_revision"],
            publication.execution_revision().get().to_string()
        );
        assert_eq!(
            sample["observation"]["publication"]["frame_epoch"],
            publication.frame_epoch().get().to_string()
        );
        if let Some(json) = player.drain_delta_json().unwrap() {
            let delta: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
            assert_eq!(delta.retained.publication_context, publication);
            assert_eq!(delta.retained.time, player.prepared_time());
            metadata_only += usize::from(
                delta.retained.objects.is_empty() && delta.retained.object_patches.is_empty(),
            );
            assert_ne!(
                mirror.apply_json(&json).unwrap().0,
                RetainedTransportApplyOutcome::DroppedStale
            );
        }
        assert_eq!(mirror.frame().unwrap(), player.playback.reader.frame());
        // Repeated offering is read-only; the actual renderer barrier is tested
        // at the endpoint before an external consumer acknowledges this value.
        assert_eq!(parse(player.observe_finite_export_json().unwrap()), sample);
        assert_eq!(player.playback.reader.publication_context(), publication);
        assert!(player.drain_delta_json().unwrap().is_none());
        frames += usize::from(!sample["frame"].is_null());
        held += usize::from(sample["frame"]["held"] == true);
        if player.prepared_time() == 1.125 {
            if let Some(before) = terminal {
                assert_eq!(publication, before);
            }
            terminal = Some(publication);
        }
        let next = parse(
            player
                .acknowledge_finite_export_json(&sample["observation"].to_string())
                .unwrap(),
        );
        if next["kind"] == "complete" {
            assert_eq!(next["sampling"], plan["sampling"]);
            break;
        }
    }
    assert_eq!(frames, 20);
    assert_eq!(held, 2);
    assert!(
        metadata_only > 0,
        "quiet waits still carry exact time/publication receipts"
    );
    assert!(terminal.unwrap().execution_revision().get() > admission.execution_revision().get());
    assert_eq!(player.playback.reader.runtime_identity(), identity);
    assert!(
        player.finite_export_active(),
        "PCM still borrows the completed capture owner"
    );
    // The authored clip continues beyond visual completion, but the exported
    // hold is silent according to shared completed-history audio policy.
    assert!(player
        .mix_finite_export_audio_block(54_000, 6_000)
        .unwrap()
        .iter()
        .all(|x| *x == 0.0));
    player.cancel_finite_export();
    assert!(!player.finite_export_active());
    player.seek_delta_json(0.0).unwrap();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    assert_eq!(player.playback.reader.runtime_identity(), identity);
}

#[test]
fn completed_history_mixer_preserves_absolute_crop_phase_and_partition_identity() {
    let mut player = completed_player();
    let mut options = options();
    options["start_frame"] = 2.into();
    options["stop"] = json!({ "kind": "end_time", "time": 0.21875 });
    options["final_hold_seconds"] = 0.0.into();
    let plan = parse(
        player
            .begin_finite_export_json(&options.to_string())
            .unwrap(),
    );
    assert_eq!(plan["audio_frames"], 6_000);
    let publication = player.playback.reader.publication_context();
    let samples = player.mix_finite_export_audio_block(0, 6_000).unwrap();
    let mut partitioned = Vec::new();
    for (first, count) in [(0, 7), (7, 4_490), (4_497, 1_503)] {
        partitioned.extend(player.mix_finite_export_audio_block(first, count).unwrap());
    }
    assert_eq!(samples, partitioned);
    assert_eq!(samples[0], 0.0);
    assert!((samples[12] - 0.5 / 4_000.0).abs() < 1e-7);
    assert!(samples[9_000..].iter().all(|sample| *sample == 0.0));
    assert_eq!(player.prepared_time(), 0.0);
    assert_eq!(player.playback.reader.publication_context(), publication);
    for (first, frames) in [(0, 0), (0, 16_385), (6_000, 1), (u64::MAX, 1)] {
        assert!(player.mix_finite_export_audio_block(first, frames).is_err());
    }
}

#[test]
fn completed_history_capture_locks_controls_and_rejects_forged_or_stale_receipts() {
    let mut player = completed_player();
    player.initial_delta_json().unwrap();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    let sample = parse(player.observe_finite_export_json().unwrap());
    let publication = player.playback.reader.publication_context();
    assert!(player.seek_delta_json(0.5).is_err());
    assert!(player.set_prepared_playing(true).is_err());
    assert!(player.pause_prepared_at(1_000.0).is_err());
    assert!(player.set_pointer_fill_selection(Some(4.0)).is_err());
    assert!(player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .is_err());
    assert!(player.submit_browser_pointer_input_json("{}").is_err());
    assert!(player.scroll_inspection_view_json("{}").is_err());
    assert!(player.reserve_object_capacity(100).is_err());
    assert!(!player.needs_preparation(100.0).unwrap());
    assert!(player.tick_delta_json(9_000.0).unwrap().is_none());
    assert_eq!(player.execution_wake(9_000.0).unwrap().cadence(), "idle");
    assert_eq!(player.playback.reader.publication_context(), publication);
    for field in ["runtime_identity", "published_time", "publication"] {
        let mut forged = sample["observation"].clone();
        forged[field] = if field == "published_time" {
            1.0.into()
        } else {
            "foreign".into()
        };
        assert!(player
            .acknowledge_finite_export_json(&forged.to_string())
            .is_err());
        assert_eq!(parse(player.finite_export_status_json().unwrap()), sample);
    }
    // Private fault injection represents an out-of-band consumer; actual shared
    // owner validation rejects it even if the transport fields look plausible.
    player.playback.reader.seek(0.5).unwrap();
    assert!(player.finite_export_status_json().is_err());
    assert!(player.mix_finite_export_audio_block(0, 1).is_err());
    assert!(player
        .acknowledge_finite_export_json(&sample["observation"].to_string())
        .is_err());
    player.cancel_finite_export();
    player.seek_delta_json(0.0).unwrap();
    player.set_prepared_playing(true).unwrap();
    assert!(player
        .begin_finite_export_json(&options().to_string())
        .is_err());
    player.set_prepared_playing(false).unwrap();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
}

#[test]
fn completed_history_receipt_retains_pointer_mapping_without_hover_and_releases_on_cancel() {
    use crate::worker_pointer_presentation::WorkerPointerReceipt;
    let mut player = completed_player();
    player.set_pointer_fill_selection(Some(4.0)).unwrap();
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    let first: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    let receipt =
        |delta: &RetainedFamilyExecutionDeltaEnvelope, presentation| WorkerPointerReceipt {
            session: delta.retained.session,
            sequence: delta.retained.sequence,
            presentation,
            view_revision: 1,
        };
    player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt(&first, 1)).unwrap())
        .unwrap();
    player
        .begin_finite_export_json(&options().to_string())
        .unwrap();
    let first = parse(player.observe_finite_export_json().unwrap());
    player
        .acknowledge_finite_export_json(&first["observation"].to_string())
        .unwrap();
    player.observe_finite_export_json().unwrap();
    let next: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&player.drain_delta_json().unwrap().unwrap()).unwrap();
    let publication = player.playback.reader.publication_context();
    let receipt = receipt(&next, 2);
    assert!(player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    assert_eq!(player.playback.reader.publication_context(), publication);
    assert!(player.playback.reader.hovered_pointer_target().is_none());
    player.cancel_finite_export();
    assert!(player.submit_browser_pointer_input_json(&json!({
        "input": { "kind":"press", "source_id":1, "pointer_id":7, "view_revision":1,
            "surface_x":400.0, "surface_y":200.0, "viewport_width":800.0, "viewport_height":400.0, "button":0 },
        "presentation":receipt,
    }).to_string()).unwrap(), "the current actual picture is usable immediately after cancellation");
}
