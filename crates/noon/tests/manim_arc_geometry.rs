//! B2 Arc/ArcBetweenPoints use the current retained semantic geometry path.
use noon::{
    ArcAuthoringError, AuthoringError, GeometryRef, ManimGeometryOptions as Options, Mobject,
    PathCommand, Scene, Vec2, VectorPath,
};
use noon_core::{GeometryResource, GeometryResourceLookup};
use std::rc::Rc;

fn retained_path(options: Options) -> VectorPath {
    let mut scene = Scene::new();
    let object =
        Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), options).unwrap();
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    match session.frame().render_geometry(0).unwrap() {
        GeometryRef::VectorPath(path) => path.clone(),
        GeometryRef::External(id) => {
            let resources = session.geometry_resources();
            let GeometryResource::VectorPath(path) = resources
                .get(resources.current_handle(*id).unwrap())
                .unwrap();
            (**path).clone()
        }
        other => panic!("expected retained path geometry, got {other:?}"),
    }
}

fn command_point(command: &PathCommand) -> Vec2 {
    match command {
        PathCommand::MoveTo { to }
        | PathCommand::LineTo { to }
        | PathCommand::QuadraticTo { to, .. }
        | PathCommand::CubicTo { to, .. } => *to,
        PathCommand::Close => panic!("close has no explicit point"),
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

fn close64(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

fn signed_chord_side(start: Vec2, end: Vec2, point: Vec2) -> f32 {
    let chord = end - start;
    let offset = point - start;
    chord.x * offset.y - chord.y * offset.x
}

#[test]
fn arc_retains_manim_cubic_segments_center_and_signed_direction() {
    let positive =
        retained_path(Options::arc(2.0, 0.0, std::f64::consts::FRAC_PI_2, 3, 1.0, -1.0).unwrap());
    assert_eq!(positive.commands().len(), 3);
    assert!(matches!(
        positive.commands()[1],
        PathCommand::CubicTo { .. }
    ));
    let start = command_point(&positive.commands()[0]);
    let end = command_point(positive.commands().last().unwrap());
    close(start.x, 3.0);
    close(start.y, -1.0);
    close(end.x, 1.0);
    close(end.y, 1.0);

    let clockwise =
        retained_path(Options::arc(2.0, 0.0, -std::f64::consts::FRAC_PI_2, 3, 1.0, -1.0).unwrap());
    let clockwise_end = command_point(clockwise.commands().last().unwrap());
    close(clockwise_end.x, 1.0);
    close(clockwise_end.y, -3.0);
}

#[test]
fn arc_between_points_maps_endpoints_and_radius_sign_without_frontend_geometry() {
    let start = Vec2::new(1.0, 2.0);
    let end = Vec2::new(4.0, 6.0);
    let implicit = retained_path(
        Options::arc_between_points(
            start.x.into(),
            start.y.into(),
            end.x.into(),
            end.y.into(),
            std::f64::consts::FRAC_PI_2,
            None,
            3,
        )
        .unwrap(),
    );
    assert_eq!(implicit.commands().len(), 3);
    assert_eq!(command_point(&implicit.commands()[0]), start);
    let implicit_end = command_point(implicit.commands().last().unwrap());
    close(implicit_end.x, end.x);
    close(implicit_end.y, end.y);

    let positive =
        retained_path(Options::arc_between_points(1.0, 2.0, 4.0, 6.0, 0.1, Some(3.0), 3).unwrap());
    let negative =
        retained_path(Options::arc_between_points(1.0, 2.0, 4.0, 6.0, 0.1, Some(-3.0), 3).unwrap());
    let positive_mid = command_point(&positive.commands()[1]);
    let negative_mid = command_point(&negative.commands()[1]);
    let positive_side = signed_chord_side(start, end, positive_mid);
    let negative_side = signed_chord_side(start, end, negative_mid);
    assert!(positive_side.abs() > 1e-5);
    assert!(negative_side.abs() > 1e-5);
    assert!(positive_side * negative_side < 0.0);
    for path in [&positive, &negative] {
        assert_eq!(command_point(&path.commands()[0]), start);
        let actual_end = command_point(path.commands().last().unwrap());
        close(actual_end.x, end.x);
        close(actual_end.y, end.y);
    }
}

#[test]
fn arc_between_points_metadata_matches_manim_radius_and_resolved_angle() {
    let (implicit_radius, implicit_angle) = Options::arc_between_points_metadata(
        -1.0,
        0.0,
        1.0,
        0.0,
        std::f64::consts::FRAC_PI_2,
        None,
    )
    .unwrap();
    close64(implicit_radius, 2.0_f64.sqrt());
    close64(implicit_angle, std::f64::consts::FRAC_PI_2);

    let (explicit_radius, explicit_angle) =
        Options::arc_between_points_metadata(-1.0, 0.0, 1.0, 0.0, 0.1, Some(-2.0)).unwrap();
    close64(explicit_radius, 2.0);
    close64(explicit_angle, -std::f64::consts::FRAC_PI_3);

    let (zero_radius, zero_angle) =
        Options::arc_between_points_metadata(-1.0, 0.0, 1.0, 0.0, 0.0, None).unwrap();
    assert!(zero_radius.is_infinite() && zero_radius.is_sign_positive());
    assert_eq!(zero_angle, 0.0);
}

#[test]
fn zero_angle_arc_between_points_is_the_retained_straight_line_fallback() {
    let path =
        retained_path(Options::arc_between_points(-2.0, 1.0, 3.0, -4.0, 0.0, None, 9).unwrap());
    assert_eq!(
        path.commands(),
        &[
            PathCommand::MoveTo {
                to: Vec2::new(-2.0, 1.0),
            },
            PathCommand::LineTo {
                to: Vec2::new(3.0, -4.0),
            },
        ]
    );
}

#[test]
fn arc_inputs_reject_before_semantic_resource_creation() {
    assert!(matches!(
        Options::arc(1.0, 0.0, 1.0, 1, 0.0, 0.0),
        Err(AuthoringError::Arc(ArcAuthoringError::TooFewComponents(1)))
    ));
    assert!(matches!(
        Options::arc_between_points(0.0, 0.0, 4.0, 0.0, 1.0, Some(1.0), 9),
        Err(AuthoringError::Arc(
            ArcAuthoringError::RadiusTooSmall { .. }
        ))
    ));
    assert!(Options::arc(f64::NAN, 0.0, 1.0, 9, 0.0, 0.0).is_err());
    assert!(Options::arc(1.0, 0.0, f64::INFINITY, 9, 0.0, 0.0).is_err());
    assert!(Options::arc_between_points(0.0, 0.0, f64::NAN, 1.0, 1.0, None, 9).is_err());
    assert!(Options::arc_between_points(0.0, 0.0, 1.0, 1.0, 1.0, Some(f64::NAN), 9).is_err());
}

#[test]
fn paired_arc_example_uses_the_shared_execution_session() {
    let session = noon::example_scenes::arc_geometry::session().unwrap();
    assert_eq!(session.frame().objects.len(), 3);
}

#[test]
fn arc_layout_distinguishes_anchor_center_and_control_hull_dimensions() {
    let scene = Scene::new();
    let arc = scene
        .geometry(Options::arc(1.25, -0.3, 1.8, 9, -2., 0.8).unwrap())
        .unwrap();
    let bounds = arc.layout_bounds().unwrap().unwrap();
    close64(arc.center().unwrap().0, -1.332546237637556);
    close64(bounds.width(), 1.1650965988224176);
    let arc = scene
        .geometry(
            Options::arc_between_points(-0.5, -1.5, 2.5, 1., std::f64::consts::FRAC_PI_2, None, 9)
                .unwrap(),
        )
        .unwrap();
    let bounds = arc.layout_bounds().unwrap().unwrap();
    close64(arc.center().unwrap().1, -0.25);
    close64(bounds.height(), 2.516375616589823);
}

#[test]
fn arc_family_and_live_layout_share_anchor_centers_and_handle_extents() {
    let mut scene = Scene::new();
    let arc = scene
        .geometry(Options::arc(1.25, -0.3, 1.8, 9, -2., 0.8).unwrap())
        .unwrap();
    let group = scene.family(&[(&arc).into()]).unwrap();
    close64(group.layout().unwrap().center().0, -1.332546237637556);
    close64(group.layout().unwrap().width(), 1.1650965988224176);
    scene.add_many(&[(&group).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let initial = live.effective_layout(&arc).unwrap();
    let family = live.effective_family_layout(&group).unwrap();
    close64(initial.center.0, family.center.0);
    close64(initial.width, family.width);
    live.rescale_to_fit(&(&group).into(), 2., noon::LayoutDimension::Width, false)
        .unwrap();
    let after = live.effective_family_layout(&group).unwrap();
    close64(after.center.0, initial.center.0);
    close64(after.center.1, initial.center.1);
    close64(after.width, 2.);
}
