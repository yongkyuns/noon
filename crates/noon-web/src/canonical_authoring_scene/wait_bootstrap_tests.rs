use super::*;

fn reject_and_recover(
    operation: fn(&mut CanonicalAuthoringScene, f64) -> Result<f64, AuthoringFailure>,
    populated: bool,
) {
    for duration in [f64::NAN, -1.0, f64::NEG_INFINITY, f64::INFINITY] {
        let mut scene = CanonicalAuthoringScene::default();
        let circle = if populated {
            let object = scene.scene.circle(1.0).unwrap();
            scene.bind_mobject(ObjectId::new(0), &object).unwrap();
            Some(object)
        } else {
            None
        };
        let revision = scene.scene.revision();
        let root = scene.root_membership_keys().unwrap();
        let bindings = scene.bindings.clone();
        let identities = scene.identities.clone();
        let object_state = circle.as_ref().map(|object| object.state().unwrap());
        let geometry_count = scene
            .scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let text_count = scene
            .scene
            .integration_store()
            .borrow()
            .text_resources()
            .len();
        let error = operation(&mut scene, duration).unwrap_err();
        // Positive infinity is already rejected by presentation-clock setup.
        // Preserve that error precedence instead of adding a second validator.
        if duration != f64::INFINITY {
            assert_eq!(error.category, "invalid_input");
            assert_eq!(error.code, "live.segment");
            assert_eq!(
                error.cause.as_ref().unwrap().code,
                "segment.invalid_duration"
            );
        }
        assert!(scene.player_ownership.is_unstarted());
        assert!(scene.player_ownership.local().is_none());
        assert_eq!(scene.live_handoff_duration(), None);
        assert_eq!(scene.scene.time(), 0.0);
        assert_eq!(scene.scene.revision(), revision);
        assert_eq!(scene.root_membership_keys().unwrap(), root);
        assert_eq!(scene.bindings, bindings);
        assert_eq!(scene.identities, identities);
        assert_eq!(
            circle.as_ref().map(|object| object.state().unwrap()),
            object_state
        );
        assert_eq!(
            scene
                .scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            geometry_count
        );
        assert_eq!(
            scene
                .scene
                .integration_store()
                .borrow()
                .text_resources()
                .len(),
            text_count
        );
        // Retry on this same context, then continue on its single runtime.
        let end = scene.begin_ordinary_wait(0.5).unwrap();
        assert_eq!(end, 0.5);
        let player = scene.active_live_player().unwrap();
        let identity = player.ownership_identity();
        assert_eq!(player.time(), 0.0);
        player.live_advance_segment_to(end).unwrap();
        player.live_complete_segment().unwrap();
        assert_eq!(scene.ordinary_wait(0.25).unwrap(), 0.75);
        assert_eq!(
            scene.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(scene.active_live_player().unwrap().time(), 0.75);
        assert_eq!(scene.root_membership_keys().unwrap(), root);
    }
}

#[test]
fn invalid_first_async_wait_preserves_empty_context_and_retries() {
    reject_and_recover(CanonicalAuthoringScene::begin_ordinary_wait, false);
}

#[test]
fn invalid_first_async_wait_preserves_authored_objects_and_retries() {
    reject_and_recover(CanonicalAuthoringScene::begin_ordinary_wait, true);
}

#[test]
fn invalid_first_endpoint_wait_preserves_empty_context_and_retries() {
    reject_and_recover(CanonicalAuthoringScene::ordinary_wait, false);
}

#[test]
fn invalid_first_endpoint_wait_preserves_authored_objects_and_retries() {
    reject_and_recover(CanonicalAuthoringScene::ordinary_wait, true);
}

#[test]
fn invalid_wait_keeps_transferred_and_returned_runtime_ownership() {
    let mut scene = CanonicalAuthoringScene::default();
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    assert!(scene.begin_ordinary_wait(-1.0).is_err());
    assert!(scene.player_ownership.is_transferred());
    scene.return_execution_player(player).unwrap();
    let error = scene.begin_ordinary_wait(-1.0).unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert!(scene.player_ownership.is_returned());
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(scene.ordinary_wait(0.5).unwrap(), 0.5);
    assert_eq!(
        scene.active_live_player().unwrap().ownership_identity(),
        identity
    );
}

#[test]
fn authored_cursor_guard_and_zero_length_wait_keep_existing_semantics() {
    let mut timed = CanonicalAuthoringScene::default();
    timed.authored_wait(0.5).unwrap();
    let revision = timed.scene.revision();
    assert!(timed.begin_ordinary_wait(-1.0).is_err());
    assert!(timed.player_ownership.is_unstarted());
    assert_eq!(timed.scene.time(), 0.5);
    assert_eq!(timed.scene.revision(), revision);
    let mut empty = CanonicalAuthoringScene::default();
    assert_eq!(empty.ordinary_wait(0.0).unwrap(), 0.0);
    assert_eq!(empty.live_execution_ownership(), "active");
    assert_eq!(empty.ordinary_wait(0.25).unwrap(), 0.25);
}
