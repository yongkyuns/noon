use noon::{
    example_scenes::moving_around, Color, GeometryRef, LiveProgramStatus, PathCommand,
    RustHostCallbackTable, Vec2,
};

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn moving_around_captures_each_completed_target_and_keeps_identity() {
    let mut program = moving_around::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    let runtime = program.session().runtime_identity();
    let mut identity = None;
    for end in 1..=4 {
        let LiveProgramStatus::Awaiting(segment) = program.resume().unwrap() else {
            panic!("each sequential play must await its own segment");
        };
        assert!(!segment.is_complete());
        assert_eq!(program.session().frame().time, f64::from(end - 1));

        if end == 1 {
            program.drive_to(&mut callbacks, 0.25).unwrap();
            // Manim's default Smooth timing is distinct from linear timing.
            near(
                program.session().frame().objects[0].transform.translation.x,
                -0.07010372,
            );
        }
        let middle = f64::from(end) - 0.5;
        program.drive_to(&mut callbacks, middle).unwrap();
        let frame = program.session().frame();
        assert_eq!(frame.time, middle);
        assert_eq!(frame.objects.len(), 1);
        let object = &frame.objects[0];
        assert_eq!(*identity.get_or_insert(object.id), object.id);
        assert_eq!(object.style.stroke, Some(Color::BLUE));
        match end {
            1 => near(object.transform.translation.x, -0.5),
            2 => {
                let fill = object.style.fill.unwrap();
                near(fill.red, (Color::BLUE.red + Color::ORANGE.red) * 0.5);
                near(fill.green, (Color::BLUE.green + Color::ORANGE.green) * 0.5);
                near(fill.blue, (Color::BLUE.blue + Color::ORANGE.blue) * 0.5);
                near(fill.alpha, 1.0);
            }
            3 => {
                near(object.transform.scale.x, 0.65);
                near(object.transform.scale.y, 0.65);
            }
            4 => {
                // .animate.rotate interpolates corresponding points. The corner
                // radius contracts at the midpoint instead of tracing a circle.
                near(frame.morph(0), 0.5);
                let Some(GeometryRef::VectorPath(path)) = frame.render_geometry(0) else {
                    panic!("animate rotation must retain point-correspondence geometry");
                };
                let target = path.morph_target().unwrap();
                for (source, target) in path.commands().iter().zip(target.commands()) {
                    let point = match (*source, *target) {
                        (
                            PathCommand::MoveTo { to: source },
                            PathCommand::MoveTo { to: target },
                        )
                        | (
                            PathCommand::LineTo { to: source },
                            PathCommand::LineTo { to: target },
                        ) => (source + target) * 0.5,
                        (PathCommand::Close, PathCommand::Close) => continue,
                        _ => panic!("square correspondence must retain its corners"),
                    };
                    let point = frame.render_transform(0).transform_point(point);
                    let radius = ((point.x + 1.0).powi(2) + point.y.powi(2)).sqrt();
                    near(radius, 0.3 * 2.0_f32.sqrt() * 0.2_f32.cos());
                }
            }
            _ => unreachable!(),
        }

        let state = program.drive_to(&mut callbacks, f64::from(end)).unwrap();
        let frame = program.session().frame();
        assert_eq!(frame.objects.len(), 1);
        let object = &frame.objects[0];
        assert_eq!(*identity.get_or_insert(object.id), object.id);
        assert_eq!(object.transform.translation, Vec2::new(-1.0, 0.0));
        if end >= 2 {
            assert_eq!(object.style.fill, Some(Color::ORANGE));
        }
        if end >= 3 {
            assert_eq!(object.transform.scale, Vec2::new(0.3, 0.3));
        }
        if end == 4 {
            assert!((object.transform.rotation - 0.4).abs() < 1e-6);
        }
        if let LiveProgramStatus::PublicationPending(expected) = state {
            let publication = program.take_renderer_publication().context();
            assert_eq!(publication, expected);
            program.admit_publication(publication).unwrap();
        }
        assert_eq!(program.session().runtime_identity(), runtime);
    }
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(program.session().frame().time, 4.0);
}
