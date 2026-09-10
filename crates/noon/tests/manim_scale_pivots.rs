use noon::Scene;

#[test]
fn manim_scale_about_point_moves_center_relative_to_pivot() {
    let scene = Scene::new();
    let mut line = scene.line((1.0, 1.0), (3.0, 1.0)).unwrap();
    let before = scene.revision();

    line.manim_scale_about_point(2.0, 3.0, 0.0, 0.0).unwrap();

    assert_eq!(scene.revision(), before.checked_next().unwrap());
    let center = line.center().unwrap();
    assert!((center.0 - 4.0).abs() < 1e-9);
    assert!((center.1 - 3.0).abs() < 1e-9);
    assert!((line.width().unwrap() - 4.0).abs() < 1e-9);
}

#[test]
fn manim_scale_about_edge_keeps_the_selected_critical_point_fixed() {
    let scene = Scene::new();
    let mut line = scene.line((1.0, 1.0), (3.0, 1.0)).unwrap();
    let before = scene.revision();

    line.manim_scale_about_edge(2.0, 1.0, 1.0, 0.0).unwrap();

    assert_eq!(scene.revision(), before.checked_next().unwrap());
    let center = line.center().unwrap();
    assert!((center.0 - 1.0).abs() < 1e-9);
    assert!((center.1 - 1.0).abs() < 1e-9);
    let right = line.critical_point(1.0, 0.0).unwrap();
    assert!((right.0 - 3.0).abs() < 1e-9);
    assert!((right.1 - 1.0).abs() < 1e-9);
}

#[test]
fn live_explicit_scale_pivots_publish_once_and_preserve_edge_anchor() {
    let mut scene = Scene::new();
    let line = scene.line((1.0, -2.0), (3.0, -2.0)).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();

    let before_point = scene.revision();
    {
        let mut live = scene.live(&mut session);
        live.manim_scale_about_point(&line, 2.0, 0.5, 0.0, 0.0)
            .unwrap();
        let layout = live.effective_layout(&line).unwrap();
        assert!((layout.center.0 - 4.0).abs() < 1e-6);
        assert!((layout.center.1 + 1.0).abs() < 1e-6);
    }
    assert_eq!(scene.revision(), before_point.checked_next().unwrap());

    let before_edge = scene.revision();
    let right_before = {
        let live = scene.live(&mut session);
        let layout = live.effective_layout(&line).unwrap();
        layout.center.0 + layout.width * 0.5
    };
    {
        let mut live = scene.live(&mut session);
        live.manim_scale_about_edge(&line, 0.5, 1.0, 1.0, 0.0)
            .unwrap();
        let layout = live.effective_layout(&line).unwrap();
        let right_after = layout.center.0 + layout.width * 0.5;
        assert!((right_after - right_before).abs() < 1e-6);
    }
    assert_eq!(scene.revision(), before_edge.checked_next().unwrap());
}

#[test]
fn rejected_explicit_scale_pivot_is_atomic() {
    let mut scene = Scene::new();
    let line = scene.line((1.0, 0.0), (3.0, 0.0)).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();

    {
        let mut live = scene.live(&mut session);
        let state = live.effective(&line).unwrap();
        assert!(live
            .manim_scale_about_point(&line, 2.0, 1.0, f64::NAN, 0.0)
            .is_err());
        assert_eq!(live.effective(&line).unwrap(), state);
    }

    assert_eq!(scene.revision(), before);
}

#[test]
fn aliased_family_scale_keeps_its_edge_and_applies_world_stretch_once() {
    use noon::{LayoutAnchor, ManimRotationPivot};
    let scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    let mut b = scene.square(1.).unwrap();
    a.shift(-1., 0.).unwrap();
    b.shift(1., 0.).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    LayoutAnchor::from(&family)
        .scale(2., 2., ManimRotationPivot::Edge(1., 0.))
        .unwrap();
    assert_eq!(a.center().unwrap(), (-3.5, 0.));
    assert_eq!(b.center().unwrap(), (0.5, 0.));
    b.rotate(0.3).unwrap();
    let before = family.layout().unwrap();
    family.scale(2., 1.).unwrap();
    let after = family.layout().unwrap();
    assert!((after.width() - 2. * before.width()).abs() < 2e-6);
    assert!((after.height() - before.height()).abs() < 2e-6);
}

#[test]
fn paired_scale_example_uses_the_retained_execution_path() {
    assert_eq!(
        noon::example_scenes::scale_pivots::session()
            .unwrap()
            .frame()
            .objects
            .len(),
        4
    );
}

#[test]
fn quarter_turn_world_scaling_preserves_world_dimensions_and_live_example() {
    use noon::{LayoutAnchor, LayoutDimension, ManimRotationPivot as Pivot};
    let scene = Scene::new();
    for angle in [
        std::f64::consts::FRAC_PI_2,
        -std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    ] {
        let mut shape = scene.rectangle(2., 1.).unwrap();
        shape.rotate(angle).unwrap();
        shape.shift(3., 2.).unwrap();
        let width = shape.width().unwrap();
        let height = shape.height().unwrap();
        shape.manim_scale_about_point(2., 0.5, 0., 0.).unwrap();
        assert!((shape.width().unwrap() - width * 2.).abs() < 1e-6);
        assert!((shape.height().unwrap() - height * 0.5).abs() < 1e-6);
        assert_eq!(shape.center().unwrap(), (6., 1.));
        let right = shape.critical_point(1., 0.).unwrap();
        LayoutAnchor::from(&shape)
            .rescale_to_fit_with_pivot(3., LayoutDimension::Width, true, Pivot::Edge(1., 0.))
            .unwrap();
        assert!((shape.width().unwrap() - 3.).abs() < 1e-6);
        assert!((shape.critical_point(1., 0.).unwrap().0 - right.0).abs() < 1e-6);
    }
    noon::example_scenes::family_affine::session().unwrap();
}
