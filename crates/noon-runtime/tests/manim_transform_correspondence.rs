use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, PathCommand, SceneDefinition, StrokeWidthMode,
    Style, TrackTiming, Transform2D, Vec2, VectorPath,
};
use noon_core::{ScenePatch, TrackValues};
use noon_runtime::SceneInstance;
use std::sync::Arc;

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

    let mut identity_scene = scene.clone();
    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).expect("compile"));
    let frame = instance.seek(1.0).expect("seek");
    let current = frame.render_transform(0);
    assert_eq!(current, Transform2D::IDENTITY);
    assert_ne!(current, frame.objects[0].transform);

    let Some(GeometryRef::VectorPath(render_source)) = frame.render_geometry(0) else {
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
    let stable = instance.frame().render_geometries[0].clone().unwrap();
    let mut retained = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    for time in [0.0, 0.2, 0.7, 1.3, 2.0, 1.0] {
        let frame = instance.seek(time).unwrap();
        if time > 0.0 && time < 2.0 {
            assert!(
                Arc::ptr_eq(frame.render_geometries[0].as_ref().unwrap(), &stable),
                "progress must retain the compiled resource"
            );
        } else {
            assert!(frame.render_geometries[0].is_none());
            assert!(frame.render_transforms[0].is_none());
        }
        let retained_frame = retained.seek(time).unwrap();
        assert_eq!(frame.render_geometry(0), retained_frame.render_geometry(0));
        assert_eq!(
            frame.render_transform(0),
            retained_frame.render_transform(0)
        );
        assert_eq!(
            frame.objects[0].transform,
            retained_frame.objects[0].transform
        );
        let bounds = noon_runtime::frame_object_conservative_bounds(frame, 0).unwrap();
        let source_world = from.transform.transform_point(Vec2::new(1.0, 0.0));
        let target_world = to.transform.transform_point(Vec2::new(0.0, 1.0));
        for point in [source_world, target_world] {
            if time <= 0.0 || time >= 2.0 {
                continue;
            }
            assert!(point.x >= bounds.min.x && point.x <= bounds.max.x);
            assert!(point.y >= bounds.min.y && point.y <= bounds.max.y);
        }
    }

    // A later independent driver must move the already completed morph; it may
    // convert the stable world pair once into the previous semantic local frame.
    let delta = Vec2::new(3.0, -2.0);
    scene
        .animate_position(
            object,
            to.transform.translation,
            to.transform.translation + delta,
            TrackTiming::new(2.0, 1.0, Easing::Linear),
        )
        .unwrap();
    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut stepped = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);
    let mut retained = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    for time in [1.0, 2.0, 2.25, 2.5, 3.0] {
        let frame = stepped.advance_to(time).unwrap();
        assert_eq!(frame, direct.seek(time).unwrap());
        let retained_frame = retained.advance_to(time).unwrap();
        assert_eq!(frame.render_geometry(0), retained_frame.render_geometry(0));
        assert_eq!(
            frame.render_transform(0),
            retained_frame.render_transform(0)
        );
        if time >= 2.0 {
            assert!(frame.render_transforms[0].is_none());
            let Some(GeometryRef::VectorPath(path)) = frame.render_geometry(0) else {
                panic!("path pair");
            };
            let PathCommand::MoveTo { to: point } = path.commands()[0] else {
                panic!("endpoint");
            };
            assert_vec2_close(
                frame.render_transform(0).transform_point(point),
                to.transform.transform_point(Vec2::new(0.0, 1.0)) + delta * (time - 2.0) as f32,
            );
        }
    }
    // A concurrent nonuniform scale driver preserves the established channel
    // order: effective TRS acts on the morph pair in the interpolated local frame.
    scene
        .animate_scale(
            object,
            Vec2::new(2.0, 0.5),
            Vec2::new(4.0, 0.5),
            TrackTiming::new(0.5, 1.0, Easing::Linear),
        )
        .unwrap();
    let mut concurrent = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut retained = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let frame = concurrent.seek(1.0).unwrap();
    let retained_frame = retained.seek(1.0).unwrap();
    assert_eq!(frame.render_geometry(0), retained_frame.render_geometry(0));
    assert_eq!(
        frame.render_transform(0),
        retained_frame.render_transform(0)
    );
    assert!(frame.render_transforms[0].is_none());
    let base = Transform2D {
        translation: Vec2::new(0.0, 1.25),
        rotation: std::f32::consts::FRAC_PI_4,
        scale: Vec2::new(1.5, 1.75),
    };
    let world = from.transform.transform_point(Vec2::new(1.0, 0.0));
    let relative = (world - base.translation).rotate(-base.rotation);
    let local = Vec2::new(relative.x / base.scale.x, relative.y / base.scale.y);
    let Some(GeometryRef::VectorPath(path)) = frame.render_geometry(0) else {
        panic!("path pair");
    };
    let PathCommand::MoveTo { to: point } = path.commands()[0] else {
        panic!("endpoint");
    };
    assert_vec2_close(
        frame.render_transform(0).transform_point(point),
        frame.objects[0].transform.transform_point(local),
    );
    // Equal point content must not hide a handoff back to the registered Arc.
    // An identity TRS and a short identity scale driver make the temporary local
    // pair exactly equal to the compiled world pair while giving it another owner.
    let mut identity_track = identity_scene.tracks()[0].clone();
    let TrackValues::Object { from, to } = &mut identity_track.values else {
        panic!("transform");
    };
    from.transform = Transform2D::IDENTITY;
    to.transform = Transform2D::IDENTITY;
    identity_scene
        .apply_patch(ScenePatch::ReplaceTrack(identity_track))
        .unwrap();
    identity_scene
        .animate_scale(
            object,
            Vec2::ONE,
            Vec2::ONE,
            TrackTiming::new(0.25, 0.25, Easing::Linear),
        )
        .unwrap();
    let mut identity_native = SceneInstance::new(CompiledScene::compile(&identity_scene).unwrap());
    let mut identity_retained =
        SceneInstance::new(CompiledScene::compile(&identity_scene).unwrap());
    let native_resource = identity_native.seek(0.1).unwrap().render_geometries[0]
        .clone()
        .unwrap();
    let retained_resource = identity_retained.seek(0.1).unwrap().render_geometries[0]
        .clone()
        .unwrap();
    // Establish the overlapping interval via canonical seek ordering. The event
    // scheduler processes crossed narrow-channel events before active channels;
    // at the end crossing the active Transform therefore takes ownership again.
    for frame_arc in [
        identity_native.seek(0.3).unwrap().render_geometries[0]
            .as_ref()
            .unwrap(),
        identity_retained.seek(0.3).unwrap().render_geometries[0]
            .as_ref()
            .unwrap(),
    ] {
        assert_eq!(frame_arc.as_ref(), native_resource.as_ref());
        assert!(!Arc::ptr_eq(frame_arc, &native_resource));
        assert!(!Arc::ptr_eq(frame_arc, &retained_resource));
    }
    for time in [0.4, 0.5, 0.6] {
        let native = identity_native.advance_to(time).unwrap();
        let retained = identity_retained.advance_to(time).unwrap();
        let native_arc = native.render_geometries[0].as_ref().unwrap();
        let retained_arc = retained.render_geometries[0].as_ref().unwrap();
        assert_eq!(native_arc.as_ref(), native_resource.as_ref());
        assert_eq!(retained_arc.as_ref(), retained_resource.as_ref());
        let restored = time >= 0.5;
        assert_eq!(Arc::ptr_eq(native_arc, &native_resource), restored);
        assert_eq!(Arc::ptr_eq(retained_arc, &retained_resource), restored);
        assert_eq!(native.render_transforms[0].is_some(), restored);
        assert_eq!(retained.render_transforms[0].is_some(), restored);
    }
}
