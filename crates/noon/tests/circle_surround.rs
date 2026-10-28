use noon::{LayoutAnchor, Scene};

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1.0e-6, "{actual} != {expected}");
}

#[test]
fn circle_surround_preserves_stretch_ratio_and_uses_multiplicative_buffer() {
    for dimension in 0..=2 {
        for stretch in [false, true] {
            let mut scene = Scene::new();
            let mut circle = scene.circle(0.5).unwrap();
            circle.shift(-3.0, 2.0).unwrap();
            let mut target = scene.rectangle(4.0, 2.0).unwrap();
            target.shift(1.0, -1.0).unwrap();
            let paint = circle.state().unwrap().style;
            let before_target = target.state().unwrap();
            let revision = scene.revision();
            LayoutAnchor::from(&circle)
                .surround_circle(&(&target).into(), dimension, stretch, 1.2)
                .unwrap();
            near(circle.width().unwrap(), 20.0_f64.sqrt() * 1.2);
            near(
                circle.height().unwrap(),
                20.0_f64.sqrt() * 1.2 * if stretch { 0.5 } else { 1.0 },
            );
            assert_eq!(circle.center().unwrap(), (1.0, -1.0));
            assert_eq!(circle.state().unwrap().style, paint);
            assert_eq!(target.state().unwrap(), before_target);
            assert_eq!(scene.revision(), revision.checked_next().unwrap());
        }
    }
}

#[test]
fn shared_target_is_remeasured_after_replace_before_diagonal_fitting() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let mut other = scene.rectangle(2.0, 1.0).unwrap();
    other.shift(4.0, 0.0).unwrap();
    let nested = scene.family(&[(&circle).into()]).unwrap();
    let target = scene
        .family(&[(&nested).into(), (&other).into(), (&circle).into()])
        .unwrap();
    let before_other = other.state().unwrap();
    LayoutAnchor::from(&circle)
        .surround_circle(&(&target).into(), 0, false, 1.0)
        .unwrap();
    near(circle.width().unwrap(), 85.0_f64.sqrt());
    near(circle.height().unwrap(), 85.0_f64.sqrt());
    assert_eq!(circle.center().unwrap(), (1.0, 0.0));
    assert_eq!(other.state().unwrap(), before_other);
}

#[test]
fn self_target_and_signed_buffers_follow_centered_scaling() {
    for buffer in [0.0, -0.5, 1.2] {
        let mut scene = Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        circle.shift(2.0, 3.0).unwrap();
        let anchor = LayoutAnchor::from(&circle);
        anchor.surround_circle(&anchor, 0, false, buffer).unwrap();
        near(circle.width().unwrap(), 8.0_f64.sqrt() * buffer.abs());
        near(circle.height().unwrap(), 8.0_f64.sqrt() * buffer.abs());
        assert_eq!(circle.center().unwrap(), (2.0, 3.0));
    }
}

#[test]
fn collapsed_width_is_not_reinflated_by_the_width_setter() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let line = scene.line((2.0, -1.0), (2.0, 3.0)).unwrap();
    LayoutAnchor::from(&circle)
        .surround_circle(&(&line).into(), 0, true, 1.5)
        .unwrap();
    near(circle.width().unwrap(), 0.0);
    near(circle.height().unwrap(), 6.0);
    assert_eq!(circle.center().unwrap(), (2.0, 1.0));
}

#[test]
fn preexisting_ellipses_and_rotated_stretch_use_world_bounds() {
    for (rotation, stretch) in [(0.0, false), (0.3, true)] {
        let mut scene = Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        circle.scale(2.0, 1.0).unwrap();
        circle.rotate(rotation).unwrap();
        let target = scene.rectangle(4.0, 2.0).unwrap();
        LayoutAnchor::from(&circle)
            .surround_circle(&(&target).into(), 0, stretch, 1.0)
            .unwrap();
        near(circle.width().unwrap(), 20.0_f64.sqrt());
        near(circle.height().unwrap(), 20.0_f64.sqrt() * 0.5);
    }
}

#[test]
fn live_circle_surround_rejects_unfinished_source_drivers_without_writes() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let target = scene.rectangle(4.0, 2.0).unwrap();
    scene
        .add_many(&[(&circle).into(), (&target).into()])
        .unwrap();
    let mut destination = circle.target_editor().unwrap();
    destination.scale(2.0, 2.0).unwrap();
    let animation = scene
        .declare_transform_to(
            &circle,
            &destination,
            noon::AnimationOptions::new().run_time(2.0),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let before = live.effective(&circle).unwrap();
    assert!(live
        .surround_circle(&(&circle).into(), &(&target).into(), 0, false, 1.2)
        .is_err());
    assert_eq!(live.effective(&circle).unwrap(), before);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    live.surround_circle(&(&circle).into(), &(&target).into(), 0, false, 1.2)
        .unwrap();
    near(
        live.effective_layout(&circle).unwrap().width,
        20.0_f64.sqrt() * 1.2,
    );
}

#[test]
fn invalid_circle_surround_does_not_publish() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let target = scene.square(2.0).unwrap();
    let empty = scene.family(&[]).unwrap();
    let foreign = Scene::new().circle(1.0).unwrap();
    let anchor = LayoutAnchor::from(&circle);
    let before = circle.state().unwrap();
    let revision = scene.revision();
    for buffer in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(anchor
            .surround_circle(&(&target).into(), 0, false, buffer)
            .is_err());
    }
    assert!(anchor
        .surround_circle(&(&target).into(), 3, false, 1.2)
        .is_err());
    assert!(anchor
        .surround_circle(&(&empty).into(), 0, false, 1.2)
        .is_err());
    assert!(anchor
        .surround_circle(&(&foreign).into(), 0, false, 1.2)
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(circle.state().unwrap(), before);
}

#[test]
fn live_circle_surround_publishes_once_and_preserves_unrelated_objects() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let target = scene.rectangle(3.0, 4.0).unwrap();
    let other = scene.square(0.5).unwrap();
    scene
        .add_many(&[(&circle).into(), (&target).into(), (&other).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.revision();
    {
        let mut live = scene.live(&mut session);
        let before = live.effective(&other).unwrap();
        live.surround_circle(&(&circle).into(), &(&target).into(), 1, true, 0.5)
            .unwrap();
        let layout = live.effective_layout(&circle).unwrap();
        near(layout.width, 2.5);
        near(layout.height, 10.0 / 3.0);
        let after = live.effective(&other).unwrap();
        assert_eq!(after.transform, before.transform);
        assert_eq!(after.style, before.style);
        assert_eq!(after.z_index, before.z_index);
        assert_eq!(after.appearance, before.appearance);
        let before_circle = live.effective(&circle).unwrap();
        assert!(live
            .surround_circle(&(&circle).into(), &(&target).into(), 0, false, f64::NAN)
            .is_err());
        assert_eq!(live.effective(&circle).unwrap(), before_circle);
    }
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
}

#[test]
fn paired_circle_surround_builds_the_normal_native_execution_session() {
    let session = noon::example_scenes::circle_surround::session().unwrap();
    assert_eq!(session.session().frame().objects.len(), 4);
}
