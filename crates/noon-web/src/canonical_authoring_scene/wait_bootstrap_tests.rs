use super::*;

fn rejected_first_wait(
    operation: fn(&mut CanonicalAuthoringScene, f64) -> Result<f64, AuthoringFailure>,
    populated: bool,
) {
    for duration in [f64::NAN, -1.0, f64::NEG_INFINITY, f64::INFINITY] {
        let mut scene = CanonicalAuthoringScene::default();
        let object = if populated {
            let object = scene.scene.circle(1.0).unwrap();
            scene.bind_mobject(ObjectId::new(0), &object).unwrap();
            Some(object)
        } else {
            None
        };
        let revision = scene.scene.revision();
        let members = scene.root_membership_keys().unwrap();
        let bindings = scene.bindings.clone();
        let identities = scene.identities.clone();
        let state = object.as_ref().map(|object| object.state().unwrap());
        let resources = {
            let store = scene.scene.integration_store().borrow();
            (
                store.geometry_resources().len(),
                store.text_resources().len(),
            )
        };
        let error = operation(&mut scene, duration).unwrap_err();
        // Positive infinity already fails during clock setup, before wait
        // admission. Keep that existing error precedence, rather than revalidate.
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
        assert_eq!(scene.root_membership_keys().unwrap(), members);
        assert_eq!(scene.bindings, bindings);
        assert_eq!(scene.identities, identities);
        assert_eq!(object.as_ref().map(|object| object.state().unwrap()), state);
        {
            let store = scene.scene.integration_store().borrow();
            assert_eq!(
                (
                    store.geometry_resources().len(),
                    store.text_resources().len()
                ),
                resources
            );
        }
        let endpoint = scene.begin_ordinary_wait(0.5).unwrap();
        assert_eq!(endpoint, 0.5);
        let player = scene.active_live_player().unwrap();
        let identity = player.ownership_identity();
        assert_eq!(player.time(), 0.0);
        player.live_advance_segment_to(endpoint).unwrap();
        player.live_complete_segment().unwrap();
        assert_eq!(scene.ordinary_wait(0.25).unwrap(), 0.75);
        assert_eq!(
            scene.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(scene.active_live_player().unwrap().time(), 0.75);
        assert_eq!(scene.root_membership_keys().unwrap(), members);
    }
}

#[test]
fn invalid_first_wait_is_atomic_and_recoverable() {
    for populated in [false, true] {
        rejected_first_wait(CanonicalAuthoringScene::begin_ordinary_wait, populated);
        rejected_first_wait(CanonicalAuthoringScene::ordinary_wait, populated);
    }
}

#[test]
fn existing_runtime_retains_transferred_and_returned_ownership() {
    let mut scene = CanonicalAuthoringScene::default();
    let player = scene.take_execution_player(1.0, 41).unwrap();
    let identity = player.ownership_identity();
    assert!(scene.begin_ordinary_wait(-1.0).is_err());
    assert_eq!(scene.live_execution_ownership(), "transferred");
    scene.return_execution_player(player).unwrap();
    assert_eq!(
        scene.begin_ordinary_wait(-1.0).unwrap_err().category,
        "invalid_input"
    );
    assert_eq!(scene.live_execution_ownership(), "returned");
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
fn authored_cursor_guard_and_zero_wait_are_unchanged() {
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
