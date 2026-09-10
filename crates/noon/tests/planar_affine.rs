use noon::{LayoutAnchor, ManimRotationPivot as Pivot, Scene, SemanticVec3};
use std::f64::consts::FRAC_PI_2;

fn close(a: (f64, f64), b: (f64, f64)) {
    assert!(
        (a.0 - b.0).abs() < 2e-6 && (a.1 - b.1).abs() < 2e-6,
        "{a:?} != {b:?}"
    );
}

#[test]
fn reflections_preserve_retained_geometry_and_transform_world_endpoints_exactly() {
    for axis in [
        SemanticVec3::new(1., 0., 0.),
        SemanticVec3::new(0., 1., 0.),
        SemanticVec3::new(1., 1., 0.),
        SemanticVec3::new(0., 0., 1.),
    ] {
        let scene = Scene::new();
        let mut line = scene.line((1., 2.), (3., 4.)).unwrap();
        line.scale(2., 0.5).unwrap();
        line.rotate(0.37).unwrap();
        line.shift(0.5, -0.2).unwrap();
        let before = line.manim_line_endpoints().unwrap();
        let content = line.state().unwrap().content;
        let revision = scene.revision();
        line.flip(axis, Pivot::Point(2., 3.)).unwrap();
        assert_eq!(scene.revision(), revision.checked_next().unwrap());
        assert_eq!(line.state().unwrap().content, content);
        let expected = |(x, y)| {
            if axis.z != 0. {
                (4. - x, 6. - y)
            } else if axis.x == 0. {
                (4. - x, y)
            } else if axis.y == 0. {
                (x, 6. - y)
            } else {
                (y - 1., x + 1.)
            }
        };
        let after = line.manim_line_endpoints().unwrap();
        close(after.start, expected(before.start));
        close(after.end, expected(before.end));
        line.flip(axis, Pivot::Point(2., 3.)).unwrap();
        let restored = line.manim_line_endpoints().unwrap();
        close(restored.start, before.start);
        close(restored.end, before.end);
    }
}

#[test]
fn family_flip_visits_aliases_once_and_invalid_or_foreign_edits_are_atomic() {
    let mut scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    a.shift(-2., 1.).unwrap();
    let mut b = scene.circle(0.5).unwrap();
    b.shift(1., -1.).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    let unrelated = scene.circle(0.2).unwrap();
    scene
        .add_many(&[(&family).into(), (&unrelated).into()])
        .unwrap();
    let untouched = unrelated.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let anchor = LayoutAnchor::from(&family);
    let result = live
        .flip_layout(&anchor, SemanticVec3::new(0., 1., 0.), Pivot::Center)
        .unwrap();
    // Two unique leaves each change their affine fields, never a duplicated alias write.
    assert!(result.impacts().len() <= 6);
    close(live.effective_layout(&a).unwrap().center, (1., 1.));
    close(live.effective_layout(&b).unwrap().center, (-2., -1.));
    assert_eq!(unrelated.state().unwrap(), untouched);
    let before = [a.state().unwrap(), b.state().unwrap()];
    for axis in [
        SemanticVec3::ZERO,
        SemanticVec3::new(1., 0., 1.),
        SemanticVec3::new(f64::NAN, 1., 0.),
    ] {
        assert!(live.flip_layout(&anchor, axis, Pivot::Center).is_err());
        assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
    }
    let foreign = Scene::new().square(1.).unwrap();
    assert!(live
        .rotate_layout(&LayoutAnchor::from(&foreign), 1., Pivot::Center)
        .is_err());
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
}

#[test]
fn live_offset_line_rotation_resolves_center_edge_and_origin_in_shared_rust() {
    let mut scene = Scene::new();
    let line = scene.line((1., 0.), (3., 0.)).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let anchor = LayoutAnchor::from(&line);
    live.rotate(&line, FRAC_PI_2).unwrap();
    let endpoints = live.effective_line_endpoints(&line).unwrap();
    close(endpoints.start, (2., -1.));
    close(endpoints.end, (2., 1.));
    live.rotate_layout(&anchor, FRAC_PI_2, Pivot::Point(0., 0.))
        .unwrap();
    let endpoints = live.effective_line_endpoints(&line).unwrap();
    close(endpoints.start, (1., 2.));
    close(endpoints.end, (-1., 2.));
    live.rotate_layout(&anchor, FRAC_PI_2, Pivot::Edge(1., 0.))
        .unwrap();
    let endpoints = live.effective_line_endpoints(&line).unwrap();
    close(endpoints.start, (1., 2.));
    close(endpoints.end, (1., 0.));
}

#[test]
fn paired_affine_example_reaches_the_normal_retained_session() {
    let session = noon::example_scenes::planar_affine::session().unwrap();
    assert_eq!(session.frame().objects.len(), 4);
}

#[test]
fn late_reflection_overflow_rejects_the_whole_family_before_commit() {
    let scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    a.shift(-3.0e38, 0.).unwrap();
    let b = scene.square(1.).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let before = [a.state().unwrap(), b.state().unwrap()];
    let revision = scene.revision();
    assert!(family
        .flip(SemanticVec3::new(0., 1., 0.), Pivot::Point(-3.0e38, 0.))
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
}
