use noon_web::FrontendMobjectHandle;

#[test]
fn sub_f32_authoring_shifts_accumulate_in_semantic_space() {
    let mut handle = FrontendMobjectHandle::manim_square(1.0).expect("valid square");
    handle
        .set_translation(2.0, -1.0)
        .expect("valid initial translation");

    // At x=2, one f32 ULP is much larger than this authored edit. Applying each
    // edit to the compact snapshot would round it away permanently. The shared
    // semantic transform must accumulate the f64 edits first and only then lower
    // the current value for the runtime-facing snapshot.
    const DELTA: f64 = 1.0e-8;
    const EDITS: usize = 100;
    for _ in 0..EDITS {
        handle.shift(DELTA, -DELTA).expect("finite semantic shift");
    }

    let expected_x = 2.0 + DELTA * EDITS as f64;
    let expected_y = -1.0 - DELTA * EDITS as f64;
    let center = handle.center();
    assert!(
        (center.0 - expected_x).abs() < 1.0e-12,
        "semantic x must retain accumulated sub-f32 edits: {center:?}"
    );
    assert!(
        (center.1 - expected_y).abs() < 1.0e-12,
        "semantic y must retain accumulated sub-f32 edits: {center:?}"
    );

    let wire = handle.wire_translation();
    assert_eq!(wire.0, f64::from(expected_x as f32));
    assert_eq!(wire.1, f64::from(expected_y as f32));
}

#[test]
fn failed_semantic_lowering_does_not_partially_mutate_authoring_state() {
    let mut handle = FrontendMobjectHandle::manim_square(1.0).expect("valid square");
    handle
        .set_translation(3.25, -2.5)
        .expect("valid initial translation");
    let before_center = handle.center();
    let before_wire = handle.wire_translation();

    let too_large = f64::from(f32::MAX) * 2.0;
    assert!(
        handle.shift(too_large, 0.0).is_err(),
        "out-of-range semantic coordinates must not lower into runtime state"
    );

    assert_eq!(handle.center(), before_center);
    assert_eq!(handle.wire_translation(), before_wire);

    assert!(
        handle.shift(f64::NAN, 0.0).is_err(),
        "non-finite semantic edits must be rejected"
    );
    assert_eq!(handle.center(), before_center);
    assert_eq!(handle.wire_translation(), before_wire);
}
