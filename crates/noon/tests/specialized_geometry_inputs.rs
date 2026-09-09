//! Geometry boundary regressions exercise shared handles and their typed lowering.
use noon::{
    GeometryRef, ManimGeometryOptions as Options, Mobject, PathCommand, Scene, Vec2, VectorPath,
};
use noon_core::{GeometryResource, GeometryResourceLookup};
use std::rc::Rc;

fn path(options: Options) -> (Mobject, VectorPath) {
    let mut scene = Scene::new();
    let object =
        Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), options).unwrap();
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    let geometry = match session.frame().render_geometry(0).unwrap() {
        GeometryRef::VectorPath(path) => path.clone(),
        GeometryRef::External(id) => {
            let resources = session.geometry_resources();
            let GeometryResource::VectorPath(path) = resources
                .get(resources.current_handle(*id).unwrap())
                .unwrap();
            (**path).clone()
        }
        other => panic!("expected lowered path geometry, got {other:?}"),
    };
    (object, geometry)
}

fn point(command: &PathCommand) -> Vec2 {
    match command {
        PathCommand::MoveTo { to }
        | PathCommand::LineTo { to }
        | PathCommand::QuadraticTo { to, .. }
        | PathCommand::CubicTo { to, .. } => *to,
        PathCommand::Close => panic!("close has no explicit point"),
    }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn triangle_retains_manim_orientation_and_closed_path() {
    let (_, triangle) = path(Options::triangle().unwrap());
    let commands = triangle.commands();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[3], PathCommand::Close);
    close(point(&commands[0]).x.into(), 0.0);
    close(point(&commands[0]).y.into(), 1.0);
    close(point(&commands[1]).x.into(), -3.0_f64.sqrt() / 2.0);
    close(point(&commands[1]).y.into(), -0.5);
    close(point(&commands[2]).x.into(), 3.0_f64.sqrt() / 2.0);
    close(point(&commands[2]).y.into(), -0.5);
}

#[test]
fn elbow_preserves_signed_width_and_constructor_vs_later_rotation() {
    let (_, zero) = path(Options::elbow(0.0, std::f64::consts::FRAC_PI_3).unwrap());
    assert!(zero
        .commands()
        .iter()
        .all(|command| point(command) == Vec2::ZERO));
    let (negative, _) = path(Options::elbow(-0.5, -std::f64::consts::FRAC_PI_6).unwrap());
    close(negative.center().unwrap().0, -0.46650636);
    close(negative.width().unwrap(), 0.4330127);
    let (mut rotated, _) = path(Options::elbow(2.0, 5.0 * std::f64::consts::PI / 4.0).unwrap());
    close(rotated.width().unwrap(), 2.0 * 2.0_f64.sqrt());
    close(rotated.height().unwrap(), 2.0_f64.sqrt());
    assert_eq!(rotated.state().unwrap().transform.rotation_z, 0.0);
    rotated.rotate(std::f64::consts::FRAC_PI_2).unwrap();
    close(
        rotated.state().unwrap().transform.rotation_z,
        std::f64::consts::FRAC_PI_2,
    );
}

#[test]
fn rounded_rectangle_clamps_radius_and_preserves_signed_corner_curves() {
    let (shape, positive) = path(Options::rounded_rectangle(4.0, 2.0, 0.5).unwrap());
    close(shape.width().unwrap(), 4.0);
    close(shape.height().unwrap(), 2.0);
    assert_eq!(positive.commands().len(), 9);
    assert_eq!(point(&positive.commands()[0]), Vec2::new(2.0, 0.5));
    assert_eq!(point(&positive.commands()[1]), Vec2::new(1.5, 1.0));
    let (_, negative) = path(Options::rounded_rectangle(4.0, 2.0, -0.5).unwrap());
    assert_eq!(
        point(&positive.commands()[1]),
        point(&negative.commands()[1])
    );
    let (
        PathCommand::CubicTo {
            control1: positive_control,
            ..
        },
        PathCommand::CubicTo {
            control1: negative_control,
            ..
        },
    ) = (&positive.commands()[1], &negative.commands()[1])
    else {
        panic!("expected cubic corners");
    };
    assert_ne!(positive_control, negative_control);
    let (_, clamped) = path(Options::rounded_rectangle(4.0, 2.0, 10.0).unwrap());
    assert_eq!(point(&clamped.commands()[0]), Vec2::new(2.0, 0.0));
    assert_eq!(point(&clamped.commands()[1]), Vec2::new(1.0, 1.0));
    let (_, sharp) = path(Options::rounded_rectangle(4.0, 2.0, 0.0).unwrap());
    assert_eq!(sharp.commands().len(), 5);
    assert_eq!(point(&sharp.commands()[0]), point(&sharp.commands()[4]));
}

#[test]
fn annular_geometry_preserves_degenerate_centers_and_opposite_winding() {
    let (_, sector) =
        path(Options::sector(2.0, std::f64::consts::FRAC_PI_2, 0.0, 9, 0.0, 0.0).unwrap());
    assert_eq!(sector.commands().len(), 20);
    assert!(sector.commands()[..9]
        .iter()
        .all(|command| point(command) == Vec2::ZERO));
    let (_, annulus) = path(Options::annulus(1.0, 2.0, 9, 0.0, 0.0).unwrap());
    let commands = annulus.commands();
    assert_eq!(commands.len(), 20);
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, PathCommand::MoveTo { .. }))
            .count(),
        2
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, PathCommand::Close))
            .count(),
        2
    );
    // The outer first segment travels upward; the inner first segment downward.
    assert!(point(&commands[1]).y > 0.0);
    assert!(point(&commands[11]).y < 0.0);
    let (_, clockwise) = path(
        Options::annular_sector(1.0, 2.0, -std::f64::consts::FRAC_PI_2, 0.0, 9, 0.0, 0.0).unwrap(),
    );
    close(point(&clockwise.commands()[8]).x.into(), 0.0);
    close(point(&clockwise.commands()[8]).y.into(), -1.0);
}

#[test]
fn dashed_line_preserves_diagonal_interpolation_and_ratio_endpoints() {
    let (_, diagonal) = path(Options::dashed_line(1.0, 2.0, 4.0, 6.0, 1.25, 0.5).unwrap());
    assert_eq!(
        diagonal.commands().iter().map(point).collect::<Vec<_>>(),
        vec![
            Vec2::new(1.0, 2.0),
            Vec2::new(1.75, 3.0),
            Vec2::new(3.25, 5.0),
            Vec2::new(4.0, 6.0)
        ]
    );
    let (_, zero) = path(Options::dashed_line(0.0, 0.0, 2.0, 0.0, 0.5, 0.0).unwrap());
    assert_eq!(zero.commands().len(), 4);
    assert!(zero
        .commands()
        .as_chunks::<2>()
        .0
        .iter()
        .all(|dash| point(&dash[0]) == point(&dash[1])));
    let (_, solid) = path(Options::dashed_line(0.0, 0.0, 2.0, 0.0, 0.5, 1.0).unwrap());
    assert_eq!(solid.commands().len(), 8);
    let (dashes, _) = solid.commands().as_chunks::<2>();
    assert!(dashes
        .windows(2)
        .all(|pair| point(&pair[0][1]) == point(&pair[1][0])));
}

#[test]
fn shared_geometry_rejects_nonfinite_and_unrepresentable_inputs() {
    assert!(Options::elbow(f64::NAN, 0.0).is_err());
    assert!(Options::elbow(0.2, f64::INFINITY).is_err());
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(Options::rounded_rectangle(value, 2.0, 0.5).is_err());
        assert!(Options::rounded_rectangle(4.0, value, 0.5).is_err());
        assert!(Options::dashed_line(0.0, 0.0, 1.0, 1.0, value, 0.5).is_err());
    }
    assert!(Options::rounded_rectangle(4.0, 2.0, f64::NAN).is_err());
    assert!(Options::annulus(1.0, 2.0, 1, 0.0, 0.0).is_err());
    assert!(Options::annular_sector(1.0, 2.0, f64::INFINITY, 0.0, 9, 0.0, 0.0).is_err());
    assert!(Options::dashed_line(f64::NAN, 0.0, 1.0, 1.0, 0.05, 0.5).is_err());
    for ratio in [-0.1, 1.1, f64::NAN] {
        assert!(Options::dashed_line(0.0, 0.0, 1.0, 1.0, 0.05, ratio).is_err());
    }
    assert!(Options::dashed_line(
        -f64::from(f32::MAX),
        0.0,
        f64::from(f32::MAX),
        0.0,
        f64::MIN_POSITIVE,
        1.0
    )
    .is_err());
}
