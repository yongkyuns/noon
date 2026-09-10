use noon::{ManimBecomeOptions, Scene, Vec2, VectorPath};

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
fn options() -> ManimBecomeOptions {
    ManimBecomeOptions {
        stretch: true,
        match_center: true,
        ..Default::default()
    }
}

#[test]
fn rotated_become_preserves_world_controls_identity_and_unchanged_resources() {
    let scene = Scene::new();
    let mut source = scene.rectangle(4., 1.).unwrap();
    source.shift(-2., 1.).unwrap();
    let mut target = scene.square(2.).unwrap();
    target.rotate(0.37).unwrap();
    target.shift(3., -1.).unwrap();
    let original = target.state().unwrap();
    let before = target.path_query().unwrap();
    let origin = target.center().unwrap();
    let scale = (
        source.width().unwrap() / target.width().unwrap(),
        source.height().unwrap() / target.height().unwrap(),
    );
    let center = source.center().unwrap();
    let id = source.node_id();
    let count = resources(&scene);
    source.become_handle(&target, options()).unwrap();
    let after = source.path_query().unwrap();
    for i in 0..before.curve_count() {
        for (a, b) in before
            .curve_points(i)
            .unwrap()
            .into_iter()
            .zip(after.curve_points(i).unwrap())
        {
            near(
                b,
                (
                    center.0 + (a.0 - origin.0) * scale.0,
                    center.1 + (a.1 - origin.1) * scale.1,
                ),
            );
        }
    }
    assert_eq!(source.node_id(), id);
    assert_eq!(target.state().unwrap(), original);
    assert_eq!(resources(&scene), count + 1);
    // Repeating the geometry with different paint keeps its resource and still
    // commits the target style. This exercises the mixed prepared-edit path.
    target.set_fill(1., 0., 0., 1.).unwrap();
    let content = source.state().unwrap().content;
    source.become_handle(&target, options()).unwrap();
    assert_eq!(source.state().unwrap().content, content);
    assert_eq!(source.state().unwrap().style, target.state().unwrap().style);
    assert_eq!(resources(&scene), count + 1);
}

#[test]
fn live_become_publishes_new_content_once_and_leaves_unrelated_objects_untouched() {
    let mut scene = Scene::new();
    let source = scene.rectangle(4., 1.).unwrap();
    let mut target = scene.square(2.).unwrap();
    target.rotate(0.37).unwrap();
    let unrelated = scene.circle(1.).unwrap();
    scene
        .add_many(&[(&source).into(), (&unrelated).into()])
        .unwrap();
    let untouched = unrelated.state().unwrap();
    let revision = scene.revision();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.become_mobject(&source, &target, options()).unwrap();
    let layout = live.effective_layout(&source).unwrap();
    near((layout.width, layout.height), (4., 1.));
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(unrelated.state().unwrap(), untouched);
    assert_eq!(session.frame().objects.len(), 2);
    assert_eq!(session.last_patch_stats().full_seeks, 0);
}

#[test]
fn unsupported_late_stroke_rolls_back_the_entire_family_and_resource_admission() {
    let scene = Scene::new();
    let mut source_a = scene.square(2.).unwrap();
    source_a.shift(-2., 0.).unwrap();
    let mut source_b = scene.square(2.).unwrap();
    source_b.shift(2., 0.).unwrap();
    let source = scene
        .family(&[(&source_a).into(), (&source_b).into()])
        .unwrap();
    let mut a = scene.square(1.).unwrap();
    a.rotate(0.4).unwrap();
    a.shift(-1., 0.).unwrap();
    let mut b = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.))
                .line_to(Vec2::new(1., 1.))
                .close(),
            noon_core::SemanticStyle {
                stroke: Some(noon_core::SemanticPaint::Solid(noon_core::Color::WHITE)),
                stroke_width: 0.04,
                stroke_width_mode: noon_core::StrokeWidthMode::ScaleWithObject,
                ..Default::default()
            },
        )
        .unwrap();
    b.rotate(0.4).unwrap();
    b.shift(1., 0.).unwrap();
    let target = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let before = [source_a.state().unwrap(), source_b.state().unwrap()];
    let revision = scene.revision();
    let count = resources(&scene);
    assert!(source.become_family(&target, options()).is_err());
    assert_eq!(
        [source_a.state().unwrap(), source_b.state().unwrap()],
        before
    );
    assert_eq!(scene.revision(), revision);
    assert_eq!(resources(&scene), count);
}
