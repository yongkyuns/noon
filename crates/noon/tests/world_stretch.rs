use noon::{
    AnimationOptions, LayoutAnchor,
    LayoutDimension::{Height, Width},
    ManimRotationPivot as Pivot, Scene,
};

fn near(a: (f64, f64), b: (f64, f64)) {
    assert!(
        (a.0 - b.0).abs() < 2e-6 && (a.1 - b.1).abs() < 2e-6,
        "{a:?} != {b:?}"
    );
}
fn resources(scene: &Scene) -> usize {
    scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len()
}

#[test]
fn rotated_stretch_transforms_world_controls_and_only_replaces_affected_content() {
    let scene = Scene::new();
    let mut shape = scene.square(2.).unwrap();
    shape.rotate(0.37).unwrap();
    shape.shift(1., -2.).unwrap();
    let before = shape.path_query().unwrap();
    let id = shape.node_id();
    let style = shape.state().unwrap().style;
    let unrelated = scene.circle(1.).unwrap();
    let untouched = unrelated.state().unwrap();
    let count = resources(&scene);
    LayoutAnchor::from(&shape)
        .stretch(-2., Width, Pivot::Point(3., 1.))
        .unwrap();
    let after = shape.path_query().unwrap();
    for curve in 0..before.curve_count() {
        for (a, b) in before
            .curve_points(curve)
            .unwrap()
            .into_iter()
            .zip(after.curve_points(curve).unwrap())
        {
            near(b, (3. + (a.0 - 3.) * -2., a.1));
        }
    }
    assert_eq!(shape.node_id(), id);
    assert_eq!(shape.state().unwrap().style, style);
    assert_eq!(unrelated.state().unwrap(), untouched);
    assert_eq!(resources(&scene), count + 1);
    let content = shape.state().unwrap().content;
    LayoutAnchor::from(&shape)
        .stretch(0.5, Height, Pivot::Center)
        .unwrap();
    assert_eq!(shape.state().unwrap().content, content);
    assert_eq!(resources(&scene), count + 1);
}

#[test]
fn aliased_family_stretch_is_one_atomic_publication_and_keeps_shared_pivot() {
    let mut scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    a.rotate(0.4).unwrap();
    a.shift(-1., 0.).unwrap();
    let mut b = scene.square(1.).unwrap();
    b.shift(1., 0.).unwrap();
    let family = scene
        .family(&[(&a).into(), (&b).into(), (&a).into()])
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let b_content = b.state().unwrap().content;
    let revision = scene.revision();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .stretch(&(&family).into(), 2., Width, Pivot::Point(0., 0.))
        .unwrap();
    near(a.center().unwrap(), (-2., 0.));
    near(b.center().unwrap(), (2., 0.));
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(b.state().unwrap().content, b_content);
    assert_eq!(session.frame().objects.len(), 2);
    assert_eq!(session.last_patch_stats().full_seeks, 0);
}

#[test]
fn invalid_late_family_member_rolls_back_resources_and_affine_edits() {
    let scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    a.rotate(0.4).unwrap();
    let mut b = scene
        .path(
            noon::VectorPath::new()
                .move_to(noon::Vec2::ZERO)
                .line_to(noon::Vec2::new(1., 1.)),
            noon_core::SemanticStyle {
                stroke: Some(noon_core::SemanticPaint::Solid(noon_core::Color::WHITE)),
                ..Default::default()
            },
        )
        .unwrap();
    b.rotate(0.4).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let before = [a.state().unwrap(), b.state().unwrap()];
    let revision = scene.revision();
    let count = resources(&scene);
    assert!(LayoutAnchor::from(&family)
        .stretch(2., Width, Pivot::Center)
        .is_err());
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(resources(&scene), count);
}

#[test]
fn stretched_target_animates_and_reconciles_through_shared_transform() {
    let mut scene = Scene::new();
    let mut object = scene.square(1.).unwrap();
    object.rotate(0.37).unwrap();
    scene.add(&object).unwrap();
    let target = object.target_editor().unwrap();
    LayoutAnchor::from(&target)
        .stretch(2., Width, Pivot::Center)
        .unwrap();
    let width = target.width().unwrap();
    let animation = scene
        .declare_transform_to(&object, &target, AnimationOptions::new())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    assert!((live.effective_layout(&object).unwrap().width - width).abs() < 2e-6);
}

#[test]
fn rotated_replace_fits_both_world_dimensions_and_center() {
    let scene = Scene::new();
    let mut shape = scene.rectangle(2., 1.).unwrap();
    shape.rotate(0.37).unwrap();
    let mut target = scene.rectangle(4., 3.).unwrap();
    target.shift(2., -1.).unwrap();
    LayoutAnchor::from(&shape)
        .replace_layout(&(&target).into(), Width, true)
        .unwrap();
    near(shape.center().unwrap(), target.center().unwrap());
    near((shape.width().unwrap(), shape.height().unwrap()), (4., 3.));
}
