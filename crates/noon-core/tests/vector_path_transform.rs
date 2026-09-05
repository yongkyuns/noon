use noon_core::{PathCommand, Transform2D, Vec2, VectorPath};

#[test]
fn transform_preserves_commands_controls_and_morph_target() {
    let path = VectorPath::new()
        .move_to(Vec2::new(1.0, 0.0))
        .quadratic_to(Vec2::new(2.0, 3.0), Vec2::new(4.0, 5.0))
        .cubic_to(
            Vec2::new(6.0, 7.0),
            Vec2::new(8.0, 9.0),
            Vec2::new(10.0, 11.0),
        )
        .line_to(Vec2::new(-1.0, -2.0))
        .close();
    let transform = Transform2D {
        translation: Vec2::new(5.0, -3.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(-2.0, 0.5),
    };
    let paired = path.clone().with_morph_target(path.clone());
    let transformed = paired.transformed(transform);
    assert_eq!(
        transformed.morph_target().unwrap().commands(),
        transformed.commands()
    );
    let point = |x, y| transform.transform_point(Vec2::new(x, y));
    assert_eq!(
        transformed.commands(),
        &[
            PathCommand::MoveTo {
                to: point(1.0, 0.0)
            },
            PathCommand::QuadraticTo {
                control: point(2.0, 3.0),
                to: point(4.0, 5.0)
            },
            PathCommand::CubicTo {
                control1: point(6.0, 7.0),
                control2: point(8.0, 9.0),
                to: point(10.0, 11.0)
            },
            PathCommand::LineTo {
                to: point(-1.0, -2.0)
            },
            PathCommand::Close,
        ]
    );
    assert_eq!(
        paired.commands(),
        path.commands(),
        "transform must leave its source immutable"
    );
}
