use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, PathCommand, SceneDefinition, StrokeWidthMode,
    Style, TrackTiming, Transform2D, Vec2, VectorPath,
};
use noon_runtime::SceneInstance;

fn assert_vec2_close(actual: Vec2, expected: Vec2) {
    const EPSILON: f32 = 1.0e-5;
    assert!(
        (actual.x - expected.x).abs() <= EPSILON && (actual.y - expected.y).abs() <= EPSILON,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn screen_space_path_pair_keeps_endpoint_world_points_during_transform() {
    let source = VectorPath::new()
        .move_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(2.0, 0.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(0.0, 2.0));
    let style = Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.04,
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        ..Style::default()
    };

    let mut from = ObjectSnapshot::new(GeometryRef::path(source));
    from.style = style;
    from.transform = Transform2D {
        translation: Vec2::new(1.0, 2.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(2.0, 1.5),
    };
    let mut to = ObjectSnapshot::new(GeometryRef::path(target));
    to.style = style;
    to.transform = Transform2D {
        translation: Vec2::new(-1.0, 0.5),
        rotation: 0.0,
        scale: Vec2::new(1.0, 2.0),
    };

    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).expect("object").style = style;
    scene.object_mut(object).expect("object").transform = from.transform;
    scene
        .animate_transform(
            object,
            from.clone(),
            to.clone(),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("valid Transform");

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).expect("compile"));
    let frame = instance.seek(1.0).expect("seek");
    let current = frame.objects[0].transform;

    let GeometryRef::VectorPath(render_source) = frame.render_geometry(0) else {
        panic!("Transform must expose a temporary PathPair");
    };
    let render_target = render_source
        .morph_target()
        .expect("temporary PathPair must retain target");
    let PathCommand::MoveTo {
        to: source_relative,
    } = render_source.commands()[0]
    else {
        panic!("source path must start with MoveTo");
    };
    let PathCommand::MoveTo {
        to: target_relative,
    } = render_target.commands()[0]
    else {
        panic!("target path must start with MoveTo");
    };

    assert_vec2_close(
        current.transform_point(source_relative),
        from.transform.transform_point(Vec2::new(1.0, 0.0)),
    );
    assert_vec2_close(
        current.transform_point(target_relative),
        to.transform.transform_point(Vec2::new(0.0, 1.0)),
    );
}
