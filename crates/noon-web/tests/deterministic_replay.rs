use noon::ExecutionSession;
use noon_web::{verify_scene_replay, ReplayVerificationError};

type SessionFactory = fn() -> Result<ExecutionSession, String>;

fn sessions() -> [SessionFactory; 2] {
    [
        noon::example_scenes::exact_property_tracks::session,
        noon::example_scenes::specialized_geometry::session,
    ]
}

fn assert_same_execution(actual: &ExecutionSession, expected: &ExecutionSession) {
    assert_eq!(actual.painter_order(), expected.painter_order());
    // Compare typed runtime data directly, including derived renderer transforms,
    // geometry, presence/reveal/morph channels and compact family animation state.
    assert_eq!(actual.frame(), expected.frame());
}

#[test]
fn typed_direct_seek_forward_playback_and_backward_scrub_have_identical_frames() {
    for build in sessions() {
        let mut direct = build().unwrap();
        let mut forward = build().unwrap();
        let mut rewind = build().unwrap();
        for target in [0.0, 0.25, 0.499, 0.5, 1.0, 1.2, 1.75, 2.0, 2.25, 3.0, 4.0] {
            direct.seek(target).unwrap();
            forward.seek(0.0).unwrap();
            for step in 0..37 {
                forward.advance_to(target * f64::from(step) / 37.0).unwrap();
            }
            forward.advance_to(target).unwrap();
            assert_same_execution(&forward, &direct);

            for time in [0.1, 1.3, 2.9, 0.7, 3.4, target] {
                rewind.advance_to(time).unwrap();
            }
            assert_same_execution(&rewind, &direct);
        }
    }
}

#[test]
fn remaining_external_replay_verifier_validates_workloads_before_decoding() {
    assert!(matches!(
        verify_scene_replay("", &[0.5], 1),
        Err(ReplayVerificationError::InvalidForwardSampleCount(1))
    ));
    assert!(matches!(
        verify_scene_replay("", &[f64::NAN], 38),
        Err(ReplayVerificationError::NonFiniteTarget { index: 0, .. })
    ));
}

#[test]
fn typed_repeated_evaluation_is_stable_at_boundaries_and_extreme_valid_times() {
    for build in sessions() {
        for time in [
            0.0,
            f64::EPSILON,
            0.25,
            0.5,
            1.0,
            1.2,
            2.0,
            2.25,
            3.0,
            1.0e-9,
            1.0e6,
        ] {
            let mut direct = build().unwrap();
            let mut repeated = build().unwrap();
            direct.seek(time).unwrap();
            for _ in 0..3 {
                repeated.advance_to(time).unwrap();
                assert_same_execution(&repeated, &direct);
            }
            repeated.take_frame_changes();
            repeated.advance_to(time).unwrap();
            assert!(repeated.take_frame_changes().is_empty());
        }
    }
}
