use noon::integration::CallbackAdvance;
use noon::Scene;
use noon_runtime::TimelineWakeState;

#[test]
fn decimal_wait_completion_requires_the_exact_accumulated_endpoint() {
    let mut scene = Scene::new();
    let object = scene.circle(1.0).unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();

    // Isolate the timing of the foreground fixture: a two-second segment
    // followed by two 0.2-second waits. No renderer or language wrapper is needed.
    for duration in [2.0, 0.2] {
        let segment = session.wait_segment(duration).unwrap();
        session
            .advance_segment_to_callback_barrier(segment, segment.end_time())
            .unwrap();
        assert!(session.segment_state(segment).is_complete());
    }
    let segment = session.wait_segment(0.2).unwrap();
    let endpoint = segment.end_time();
    assert_eq!(endpoint, 2.0 + 0.2 + 0.2);
    assert_eq!(endpoint.to_bits(), 2.4_f64.to_bits() + 1);

    // A sample one ULP before the endpoint must not finish the continuation or
    // silently rewrite its timestamp. This is an interior sample, not a hang.
    assert!(matches!(
        session
            .advance_segment_to_callback_barrier(segment, 2.4)
            .unwrap(),
        CallbackAdvance::Ready(frame) if frame.time == 2.4
    ));
    assert!(!session.segment_state(segment).is_complete());
    assert_eq!(
        session.segment_state(segment).timeline(),
        TimelineWakeState::Deadline(endpoint)
    );

    assert!(matches!(
        session
            .advance_segment_to_callback_barrier(segment, endpoint)
            .unwrap(),
        CallbackAdvance::Ready(frame) if frame.time == endpoint
    ));
    assert!(session.segment_state(segment).is_complete());
    assert_eq!(session.frame().time.to_bits(), endpoint.to_bits());
    assert_eq!(
        session.segment_state(segment).timeline(),
        TimelineWakeState::Quiescent
    );
}
