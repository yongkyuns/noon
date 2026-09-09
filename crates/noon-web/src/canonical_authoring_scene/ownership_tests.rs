use super::*;
use noon_core::Vec2;

fn scene_with_circle() -> (CanonicalAuthoringScene, noon::Mobject) {
    let mut scene = CanonicalAuthoringScene::default();
    let circle = scene.scene.circle(1.0).unwrap();
    scene.bind_mobject(ObjectId::new(0), &circle).unwrap();
    (scene, circle)
}

#[test]
fn ownership_observations_and_continuation_follow_one_runtime() {
    let (mut scene, circle) = scene_with_circle();
    assert_eq!(scene.live_execution_ownership(), "none");
    scene.live_player(1.0).unwrap();
    assert_eq!(scene.live_execution_ownership(), "active");
    let mut player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    player.initial_delta_json().unwrap();
    assert_eq!(scene.live_execution_ownership(), "transferred");
    assert!(scene.player_ownership.local().is_none());
    scene.return_execution_player(player).unwrap();
    assert_eq!(scene.live_execution_ownership(), "returned");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );

    scene.begin_ordinary_wait(0.25).unwrap();
    let mut player = scene.resume_execution_player().unwrap();
    assert_eq!(player.ownership_identity(), identity);
    player.live_advance_segment_to(0.25).unwrap();
    player.live_complete_segment().unwrap();
    assert!(player.live_complete_segment().is_err());
    player.drain_delta_json().unwrap();
    scene.return_execution_player(player).unwrap();
    assert!(scene.resume_execution_player().is_err());
    let player = scene.active_live_player().unwrap();
    assert_eq!(player.ownership_identity(), identity);
    assert_eq!(player.time(), 0.25);
    // Return/resume did not reset the initial-snapshot/encoder state.
    player.live_set_translation(&circle, 1.0, 0.0).unwrap();
    let delta: serde_json::Value =
        serde_json::from_str(&player.drain_delta_json().unwrap().unwrap()).unwrap();
    assert_eq!(delta["session"], 41);
    assert!(delta["sequence"].as_u64().unwrap() > 0);
    assert_ne!(delta["snapshot"], true);
}

#[test]
fn transferred_context_cannot_drive_query_publish_or_take_again() {
    let (mut scene, circle) = scene_with_circle();
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    let revision = scene.scene.store().borrow().scene_revision();
    assert!(scene.mobject_layout(&circle).is_err());
    assert!(scene.live_contains_mobject(&circle).is_err());
    assert!(scene.active_live_player().is_err());
    assert!(scene.begin_ordinary_wait(0.25).is_err());
    assert!(scene.take_execution_player(1.0, 42).is_err());
    assert!(scene.prepare_execution_run().is_err());
    assert!(scene
        .add_updater(&circle, HostCallbackId::new(3), 0.0, None)
        .is_err());
    assert_eq!(scene.scene.store().borrow().scene_revision(), revision);
    assert_eq!(scene.live_execution_ownership(), "transferred");
    scene.return_execution_player(player).unwrap();
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(scene.mobject_layout(&circle).unwrap(), (0.0, 0.0, 2.0, 2.0));
}

#[test]
fn foreign_store_return_does_not_replace_either_contexts_lease() {
    let (mut scene, _) = scene_with_circle();
    let (mut foreign, _) = scene_with_circle();
    let expected = scene.take_execution_player(1.0, 41).unwrap();
    let other = foreign.take_execution_player(1.0, 41).unwrap();
    let identity = expected.ownership_identity();
    let other_identity = other.ownership_identity();
    let rejection = scene.return_execution_player(other).unwrap_err();
    assert_eq!(rejection.reason, PlayerReturnError::ForeignScene);
    let other = *rejection.player;
    let rejection = foreign.return_execution_player(expected).unwrap_err();
    assert_eq!(rejection.reason, PlayerReturnError::ForeignScene);
    let expected = *rejection.player;
    assert!(scene.player_ownership.is_transferred());
    assert!(foreign.player_ownership.is_transferred());
    // Even a consuming failed return restores both rightful players to the caller.
    scene.return_execution_player(expected).unwrap();
    foreign.return_execution_player(other).unwrap();
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(
        foreign.active_live_player().unwrap().ownership_identity(),
        other_identity
    );
}

#[test]
fn different_root_in_the_same_store_is_not_the_leased_scene() {
    let (mut scene, _) = scene_with_circle();
    let mut other_scene =
        CanonicalAuthoringScene::with_store(std::rc::Rc::clone(scene.scene.store()));
    let expected = scene.take_execution_player(1.0, 41).unwrap();
    let other = other_scene.take_execution_player(1.0, 41).unwrap();
    let rejection = scene.return_execution_player(other).unwrap_err();
    assert_eq!(rejection.reason, PlayerReturnError::ForeignScene);
    let other = *rejection.player;
    scene.return_execution_player(expected).unwrap();
    other_scene.return_execution_player(other).unwrap();
}

#[test]
fn noncanonical_player_cannot_be_installed_as_a_returned_player() {
    let (mut scene, _) = scene_with_circle();
    let candidate =
        crate::SemanticExecutionPlayer::from_session(scene.lower_execution().unwrap(), 1.0, 41)
            .unwrap();
    let expected = scene.take_execution_player(1.0, 41).unwrap();
    assert_eq!(
        scene
            .return_execution_player(candidate)
            .map_err(|rejection| rejection.reason),
        Err(PlayerReturnError::ForeignScene)
    );
    assert!(scene.player_ownership.is_transferred());
    scene.return_execution_player(expected).unwrap();
}

#[test]
fn matching_store_root_revision_and_transport_do_not_authorize_another_runtime() {
    let (mut scene, _) = scene_with_circle();
    let candidate = scene.build_live_player(1.0, 41).unwrap();
    let expected = scene.take_execution_player(1.0, 41).unwrap();
    let identity = expected.ownership_identity();
    assert_eq!(candidate.scene_revision(), expected.scene_revision());
    assert_ne!(candidate.ownership_identity(), identity);
    assert_eq!(
        scene
            .return_execution_player(candidate)
            .map_err(|rejection| rejection.reason),
        Err(PlayerReturnError::StaleLease)
    );
    assert!(scene.player_ownership.is_transferred());
    scene.return_execution_player(expected).unwrap();
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
}

#[test]
fn stale_transport_is_rejected_even_for_the_right_runtime() {
    let (mut scene, _) = scene_with_circle();
    let mut player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    player.rebind_transport(1.0, 42).unwrap();
    let rejection = scene.return_execution_player(player).unwrap_err();
    assert_eq!(rejection.reason, PlayerReturnError::StaleLease);
    let mut player = *rejection.player;
    assert!(scene.player_ownership.is_transferred());
    // A rejected consuming return leaves the same player available for correction.
    player.rebind_transport(1.0, 41).unwrap();
    assert_eq!(player.ownership_identity(), identity);
    scene.return_execution_player(player).unwrap();
}

#[test]
fn absent_and_duplicate_returns_cannot_overwrite_a_local_owner() {
    let (mut scene, _) = scene_with_circle();
    let candidate = scene.build_live_player(1.0, 41).unwrap();
    assert_eq!(
        scene
            .return_execution_player(candidate)
            .map_err(|rejection| rejection.reason),
        Err(PlayerReturnError::NotLeased)
    );
    assert!(scene.player_ownership.is_unstarted());
    scene.live_player(1.0).unwrap();
    let active_identity = scene.active_live_player().unwrap().ownership_identity();
    let candidate = scene.build_live_player(1.0, 41).unwrap();
    assert_eq!(
        scene
            .return_execution_player(candidate)
            .map_err(|rejection| rejection.reason),
        Err(PlayerReturnError::NotLeased)
    );
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        active_identity
    );
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    scene.return_execution_player(player).unwrap();
    let duplicate_attempt = scene.build_live_player(1.0, 41).unwrap();
    assert_eq!(
        scene
            .return_execution_player(duplicate_attempt)
            .map_err(|rejection| rejection.reason),
        Err(PlayerReturnError::NotLeased)
    );
    assert_eq!(scene.live_execution_ownership(), "returned");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
}

#[test]
fn failed_handoff_keeps_each_local_ownership_phase_and_runtime() {
    let (mut scene, _) = scene_with_circle();
    assert!(scene.take_execution_player(f64::NAN, 41).is_err());
    assert!(scene.player_ownership.is_unstarted());
    scene.live_player(1.0).unwrap();
    let identity = scene.active_live_player().unwrap().ownership_identity();
    assert!(scene.take_execution_player(f64::NAN, 41).is_err());
    assert_eq!(scene.live_execution_ownership(), "active");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    scene.return_execution_player(player).unwrap();
    assert!(scene.take_execution_player(f64::NAN, 42).is_err());
    assert_eq!(scene.live_execution_ownership(), "returned");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    let recovered = scene.take_execution_player(1.0, 41).unwrap();
    assert_eq!(recovered.ownership_identity(), identity);
    scene.return_execution_player(recovered).unwrap();
}

#[test]
fn publication_while_leased_does_not_invalidate_the_ownership_identity() {
    let (mut scene, circle) = scene_with_circle();
    let mut player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    let revision = player.scene_revision();
    player.live_set_translation(&circle, 3.0, -1.0).unwrap();
    assert_ne!(player.scene_revision(), revision);
    assert_eq!(player.ownership_identity(), identity);
    scene.return_execution_player(player).unwrap();
    assert_eq!(
        scene.mobject_layout(&circle).unwrap(),
        (3.0, -1.0, 2.0, 2.0)
    );
    scene.prepare_execution_run().unwrap();
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
}

#[test]
fn only_a_stale_returned_runtime_can_be_replaced_at_a_rerun_boundary() {
    let (mut scene, mut circle) = scene_with_circle();
    scene.live_player(1.0).unwrap();
    let identity = scene.active_live_player().unwrap().ownership_identity();
    circle.shift(1.0, 0.0).unwrap();
    assert!(scene.prepare_execution_run().is_err());
    assert_eq!(scene.live_execution_ownership(), "active");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );

    let (mut scene, mut circle) = scene_with_circle();
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    scene.return_execution_player(player).unwrap();
    circle.shift(3.0, -1.0).unwrap();
    assert_eq!(
        scene.mobject_layout(&circle).unwrap(),
        (3.0, -1.0, 2.0, 2.0)
    );
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    scene.prepare_execution_run().unwrap();
    assert!(scene.player_ownership.is_unstarted());
    let mut rerun = scene.take_execution_player(1.0, 41).unwrap();
    // Same transport session, but a genuinely new runtime incarnation.
    assert_ne!(rerun.ownership_identity(), identity);
    assert_eq!(
        rerun.live_effective(&circle).unwrap().transform.translation,
        Vec2::new(3.0, -1.0)
    );
    scene.return_execution_player(rerun).unwrap();
}

#[test]
fn terminal_callback_failure_can_return_but_cannot_resume_or_replace_its_runtime() {
    let (mut scene, circle) = scene_with_circle();
    scene
        .add_updater(&circle, HostCallbackId::new(7), 0.0, None)
        .unwrap();
    scene.begin_ordinary_wait(0.25).unwrap();
    let mut player = scene.take_execution_player(0.25, 41).unwrap();
    let identity = player.ownership_identity();
    player.live_segment_wake(1_000.0).unwrap();
    let drive = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
    player
        .fail_callback_phase_json(&drive.callback_phase_json().unwrap())
        .unwrap();
    assert!(player.live_complete_segment().is_err());
    scene.return_execution_player(player).unwrap();
    assert!(scene.resume_execution_player().is_err());
    assert!(scene.drain_returned_publication_json().is_err());
    assert_eq!(scene.live_execution_ownership(), "returned");
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
}

#[test]
fn session_identity_survives_moves_but_not_runtime_clones() {
    let (scene, _) = scene_with_circle();
    let session = scene.lower_execution().unwrap();
    let identity = session.runtime_identity();
    let moved = vec![session].pop().unwrap();
    assert_eq!(moved.runtime_identity(), identity);
    assert_ne!(moved.clone().runtime_identity(), identity);
}
