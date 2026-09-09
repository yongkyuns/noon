use super::*;
use noon::{ExecutionSessionCallbackError, ExecutionSessionCallbackReadError};
use noon_core::{HostCallbackId, SemanticMutationTransaction};
use serde_json::{json, Value};
use std::error::Error;

fn player() -> (SemanticExecutionPlayer, SemanticNodeId) {
    let mut scene = noon::Scene::new();
    let target = scene.circle(0.5).unwrap();
    let detached = scene.circle(0.2).unwrap();
    scene.add(&target).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 1.0, 41).unwrap();
    (player, detached.node_id())
}

fn phase(player: &mut SemanticExecutionPlayer) -> Value {
    serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap()
}

fn state(player: &SemanticExecutionPlayer) -> (String, Vec<u8>, Option<String>) {
    (
        player.debug_frame_json(),
        player.resource_bundle_bytes(),
        player.callback_termination_json().unwrap(),
    )
}

fn commit(player: &mut SemanticExecutionPlayer, phase: &Value) {
    player
        .commit_callback_phase_json(&json!({"token": phase["token"], "writes": []}).to_string())
        .unwrap();
}

fn finish(player: &mut SemanticExecutionPlayer, pending: Option<&Value>) {
    if let Some(phase) = pending {
        commit(player, phase);
    }
    if let Some(next) = player.advance_forward_to_callback_phase_json(0.25).unwrap() {
        commit(player, &serde_json::from_str(&next).unwrap());
    }
    let frame: Value = serde_json::from_str(&player.debug_frame_json()).unwrap();
    assert_eq!(frame["time"], 0.25);
    let delta: Value = serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    assert_eq!(delta["session"], 41);
}

#[test]
fn callback_abort_receipts_reject_foreign_replayed_and_absent_phases() {
    for interrupted in [false, true] {
        for condition in ["foreign", "replay", "no_pending"] {
            let (mut player, _) = player();
            let initial = phase(&mut player);
            player.initial_delta_json().unwrap();
            let mut pending = Some(initial.clone());
            let rejected = if condition == "foreign" {
                let (mut foreign, _) = self::player();
                phase(&mut foreign)
            } else {
                commit(&mut player, &initial);
                player.drain_delta_json().unwrap();
                pending = if condition == "replay" {
                    Some(
                        serde_json::from_str(
                            &player
                                .advance_forward_to_callback_phase_json(0.25)
                                .unwrap()
                                .unwrap(),
                        )
                        .unwrap(),
                    )
                } else {
                    None
                };
                initial
            };
            let before = state(&player);
            let error = if interrupted {
                player.interrupt_callback_phase_json(&rejected.to_string())
            } else {
                player.fail_callback_phase_json(&rejected.to_string())
            }
            .unwrap_err();
            assert_eq!(error.category, "stale_publication");
            assert_eq!(
                error.code,
                if condition == "no_pending" {
                    "callback.no_pending_phase"
                } else {
                    "callback.stale_token"
                }
            );
            assert_eq!(state(&player), before);
            assert_eq!(player.drain_delta_json().unwrap(), None);
            finish(&mut player, pending.as_ref());
        }
    }
}

#[test]
fn callback_reads_preserve_typed_rejection_and_the_pending_read_view() {
    let (mut player, detached) = player();
    let phase = phase(&mut player);
    player.initial_delta_json().unwrap();
    let before = state(&player);
    for (kind, code) in [
        ("object", "callback_read.unknown_object"),
        ("scalar_signal", "callback_read.unknown_signal"),
    ] {
        let request = json!({
            "kind": kind,
            "node": {"slot": detached.slot(), "generation": detached.generation()}
        });
        let error = player
            .required_callback_read_json(&phase["token"].to_string(), &request.to_string())
            .unwrap_err();
        assert_eq!(error.category, "stale_handle");
        assert_eq!(error.code, code);
        assert_eq!(state(&player), before);
        assert_eq!(player.drain_delta_json().unwrap(), None);
    }
    let request = json!({"kind": "object", "node": phase["objects"][0]["node"]});
    assert!(player
        .required_callback_read_json(&phase["token"].to_string(), &request.to_string())
        .is_ok());
    finish(&mut player, Some(&phase));
}

#[test]
fn callback_batch_rejection_keeps_earlier_valid_writes_unpublished() {
    let (mut player, detached) = player();
    let phase = phase(&mut player);
    player.initial_delta_json().unwrap();
    let before = state(&player);
    let mut transform = phase["objects"][0]["transform"].clone();
    transform["translation"]["x"] = json!(3.0);
    let valid =
        json!({"kind": "transform", "object": phase["objects"][0]["node"], "transform": transform});
    let invalid = json!({
        "kind": "transform",
        "object": {"slot": detached.slot(), "generation": detached.generation()},
        "transform": transform,
    });
    let error = player
        .commit_callback_phase_json(
            &json!({"token": phase["token"], "writes": [valid, invalid]}).to_string(),
        )
        .unwrap_err();
    assert_eq!(error.category, "stale_handle");
    assert_eq!(error.code, "callback.unknown_object");
    assert_eq!(state(&player), before);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    player
        .commit_callback_phase_json(
            &json!({"token": phase["token"], "writes": [valid]}).to_string(),
        )
        .unwrap();
    assert_ne!(player.debug_frame_json(), before.0);
    assert!(player.drain_delta_json().unwrap().is_some());
    finish(&mut player, None);
}

#[test]
fn valid_callback_abort_is_terminal_and_repeat_uses_shared_no_pending_precedence() {
    for interrupted in [false, true] {
        let (mut player, _) = player();
        let phase = phase(&mut player).to_string();
        if interrupted {
            player.interrupt_callback_phase_json(&phase).unwrap();
        } else {
            player.fail_callback_phase_json(&phase).unwrap();
        }
        let before = state(&player);
        let error = player.fail_callback_phase_json(&phase).unwrap_err();
        assert_eq!(error.code, "callback.no_pending_phase");
        assert_eq!(error.category, "stale_publication");
        assert_eq!(state(&player), before);
        assert!(player.advance_forward_to_callback_phase_json(0.25).is_err());
        assert_eq!(state(&player), before);
    }
}

#[test]
fn callback_projection_preserves_nested_causes_and_token_identity_diagnostics() {
    let node = SemanticNodeId::new(17, 2);
    let failure = AuthoringFailure::from(ExecutionSessionCallbackError::Read(
        ExecutionSessionCallbackReadError::UnknownObject(node),
    ));
    assert_eq!(failure.category, "stale_handle");
    assert_eq!(failure.code, "callback.read");
    assert_eq!(
        failure.cause.as_ref().unwrap().code,
        "callback_read.unknown_object"
    );
    assert!(failure.source().is_some());

    let (mut first, _) = player();
    let (mut second, _) = player();
    let expected =
        SemanticExecutionPlayer::phase_token_from_json(&phase(&mut first).to_string()).unwrap();
    let actual =
        SemanticExecutionPlayer::phase_token_from_json(&phase(&mut second).to_string()).unwrap();
    let original = ExecutionSessionCallbackError::StaleToken { expected, actual };
    let message = format!("{original}; expected {expected:?}, actual {actual:?}");
    let failure = AuthoringFailure::from(original);
    assert_eq!(failure.message, message);
    assert_ne!(expected.runtime(), actual.runtime());
    assert_eq!(expected.sequence(), actual.sequence());
}

#[test]
fn callback_decoder_and_local_guards_remain_explicitly_unclassified() {
    let (mut player, _) = player();
    let phase = phase(&mut player);
    let before = state(&player);
    for error in [
        player
            .commit_callback_phase_json("{broken json")
            .unwrap_err(),
        player.fail_callback_phase_json("{broken json").unwrap_err(),
        player
            .interrupt_callback_phase_json("{broken json")
            .unwrap_err(),
        player.required_callback_read_json("{}", "{}").unwrap_err(),
    ] {
        assert_eq!(error.category, "unclassified");
        assert_eq!(error.code, "unclassified");
        assert!(!error.message.is_empty());
    }
    assert_eq!(state(&player), before);
    finish(&mut player, Some(&phase));
}

#[test]
fn family_read_rejection_preserves_nested_cause_and_the_same_pending_phase() {
    let mut scene = noon::Scene::new();
    let target = scene.circle(0.5).unwrap();
    let detached = scene.circle(0.2).unwrap();
    let mixed = scene
        .family(&[(&target).into(), (&detached).into()])
        .unwrap();
    let live = scene.family(&[(&target).into()]).unwrap();
    scene.add(&target).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        41,
    )
    .unwrap();
    let phase = phase(&mut player);
    player.initial_delta_json().unwrap();
    let before = state(&player);
    let revision = scene.revision();
    let request = |node: SemanticNodeId| {
        json!({"kind": "family", "node": {
            "slot": node.slot(), "generation": node.generation(),
        }})
        .to_string()
    };
    let error = player
        .required_callback_read_json(&phase["token"].to_string(), &request(mixed.node_id()))
        .unwrap_err();
    assert_eq!(error.category, "stale_handle");
    assert_eq!(error.code, "callback.family.read");
    let callback = error.cause.as_ref().unwrap();
    assert_eq!(callback.code, "callback.read");
    let read = callback.cause.as_ref().unwrap();
    assert_eq!(read.code, "callback_read.unknown_object");
    assert!(read.cause.is_none());
    assert!(error.source().unwrap().source().is_some());
    assert_eq!(state(&player), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    let value: Value = serde_json::from_str(
        &player
            .required_callback_read_json(&phase["token"].to_string(), &request(live.node_id()))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["kind"], "family");
    assert_eq!(value["objects"], phase["objects"]);
    assert_eq!(state(&player), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    finish(&mut player, Some(&phase));
}
