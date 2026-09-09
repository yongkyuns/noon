use noon::{
    LayoutAnchor,
    LayoutDimension::{Height, Width},
    Scene,
};

#[test]
fn object_fit_and_dimension_match_use_shared_bounds() {
    let scene = Scene::new();
    let source = scene.rectangle(2.0, 1.0).unwrap();
    let target = scene.rectangle(6.0, 3.0).unwrap();
    let anchor = LayoutAnchor::from(&source);
    anchor.rescale_to_fit(4.0, Width, false).unwrap();
    assert_eq!(
        (source.width().unwrap(), source.height().unwrap()),
        (4.0, 2.0)
    );
    anchor
        .match_dim_size(&(&target).into(), Height, true)
        .unwrap();
    assert_eq!(
        (source.width().unwrap(), source.height().unwrap()),
        (4.0, 3.0)
    );
    anchor
        .match_dim_size(&(&target).into(), Width, false)
        .unwrap();
    assert_eq!(
        (source.width().unwrap(), source.height().unwrap()),
        (6.0, 4.5)
    );
}

#[test]
fn family_aliases_fit_once_and_invalid_values_do_not_publish() {
    let scene = Scene::new();
    let mut a = scene.square(1.0).unwrap();
    let mut b = scene.square(1.0).unwrap();
    a.shift(-1.0, 0.0).unwrap();
    b.shift(1.0, 0.0).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    let anchor = LayoutAnchor::from(&family);
    let revision = scene.revision();
    anchor.rescale_to_fit(6.0, Width, true).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!((a.width().unwrap(), b.width().unwrap()), (2.0, 2.0));
    assert_eq!(
        (a.center().unwrap(), b.center().unwrap()),
        ((-2.0, 0.0), (2.0, 0.0))
    );
    let revision = scene.revision();
    let states = [a.state().unwrap(), b.state().unwrap()];
    assert!(anchor.rescale_to_fit(f64::NAN, Height, false).is_err());
    let foreign = Scene::new().square(1.0).unwrap();
    assert!(anchor
        .match_dim_size(&(&foreign).into(), Width, false)
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], states);
}

#[test]
fn zero_extent_is_a_noop_without_a_revision() {
    let scene = Scene::new();
    let line = scene.line((0.0, 0.0), (0.0, 2.0)).unwrap();
    let empty = scene.family(&[]).unwrap();
    let revision = scene.revision();
    LayoutAnchor::from(&line)
        .rescale_to_fit(4.0, Width, false)
        .unwrap();
    LayoutAnchor::from(&empty)
        .rescale_to_fit(4.0, Height, true)
        .unwrap();
    assert_eq!(scene.revision(), revision);
    assert_eq!(line.height().unwrap(), 2.0);
}

#[test]
fn live_fit_reads_effective_target_bounds_and_rejects_active_source_drivers() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let target = scene.square(1.0).unwrap();
    scene
        .add_many(&[(&source).into(), (&target).into()])
        .unwrap();
    let mut destination = target.target_editor().unwrap();
    destination.scale(3.0, 3.0).unwrap();
    let animation = scene
        .declare_transform_to(
            &target,
            &destination,
            noon::AnimationOptions::new()
                .run_time(2.0)
                .rate_func(noon::RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let before_source = live.effective(&source).unwrap();
    let before_target = live.effective(&target).unwrap();
    assert!(live
        .match_dim_size(&(&source).into(), &(&target).into(), Width, false)
        .is_err());
    assert!(live
        .rescale_to_fit(&(&target).into(), 5.0, Width, false)
        .is_err());
    assert_eq!(live.effective(&source).unwrap(), before_source);
    assert_eq!(live.effective(&target).unwrap(), before_target);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    live.match_dim_size(&(&source).into(), &(&target).into(), Width, false)
        .unwrap();
    assert_eq!(live.effective_layout(&source).unwrap().width, 3.0);
}

#[test]
fn live_family_fitting_preserves_unrelated_state() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let other = scene.square(2.0).unwrap();
    let family = scene.family(&[(&source).into()]).unwrap();
    scene
        .add_many(&[(&family).into(), (&other).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let before = live.effective(&other).unwrap();
    live.rescale_to_fit(&(&family).into(), 3.0, Height, true)
        .unwrap();
    assert_eq!(live.effective_family_layout(&family).unwrap().height, 3.0);
    assert_eq!(live.effective(&other).unwrap().transform, before.transform);
    let foreign = Scene::new().square(1.0).unwrap();
    assert!(live
        .match_dim_size(&(&family).into(), &(&foreign).into(), Width, false)
        .is_err());
}

#[test]
fn rotated_uniform_fit_works_and_unrepresentable_stretch_rejects_atomically() {
    let scene = Scene::new();
    let mut a = scene.square(1.0).unwrap();
    a.rotate(0.3).unwrap();
    let b = scene.square(1.0).unwrap();
    let family = scene.family(&[(&b).into(), (&a).into()]).unwrap();
    let anchor = LayoutAnchor::from(&family);
    anchor.rescale_to_fit(4.0, Width, false).unwrap();
    assert!((family.layout().unwrap().width() - 4.0).abs() < 1e-6);
    let revision = scene.revision();
    let states = [a.state().unwrap(), b.state().unwrap()];
    assert!(anchor.rescale_to_fit(2.0, Width, true).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], states);
}

#[test]
fn paired_dimension_fitting_example_builds_the_normal_execution_session() {
    noon::example_scenes::dimension_fitting::session().unwrap();
}
