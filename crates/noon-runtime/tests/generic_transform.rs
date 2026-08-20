use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, Property, SceneDefinition, Style, TrackTiming,
    Transform2D, Vec2, VectorPath,
};
use noon_runtime::SceneInstance;

fn stroke_style(color: Color) -> Style {
    Style {
        fill: None,
        stroke: Some(color),
        stroke_width: 0.1,
        opacity: 1.0,
    }
}

fn path_a() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
}

fn path_b() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0))
}

fn path_c() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
}

fn snapshot(path: VectorPath, transform: Transform2D, style: Style) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry: GeometryRef::path(path),
        transform,
        style,
    }
}

#[test]
fn generic_transform_has_exact_semantic_endpoints_and_detached_render_geometry() {
    let style_a = stroke_style(Color::WHITE);
    let style_b = stroke_style(Color::rgb(0.2, 0.6, 0.9));
    let transform_a = Transform2D::IDENTITY;
    let transform_b = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        rotation: 0.8,
        scale: Vec2::new(1.5, 0.5),
    };
    let from = snapshot(path_a(), transform_a, style_a);
    let to = snapshot(path_b(), transform_b, style_b);

    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().transform = from.transform;
    scene.object_mut(object).unwrap().style = from.style;
    scene
        .animate_transform(
            object,
            from.clone(),
            to.clone(),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());

    let start = instance.seek(0.0).unwrap().clone();
    assert_eq!(start.objects.len(), 1);
    assert_eq!(start.objects[0].id, object);
    assert_eq!(start.objects[0].geometry, from.geometry);
    assert_eq!(start.objects[0].transform, from.transform);
    assert_eq!(start.objects[0].style, from.style);
    assert_eq!(start.morph(0), 0.0);
    assert_ne!(start.render_geometry(0), &start.objects[0].geometry);

    let midpoint = instance.seek(1.0).unwrap().clone();
    assert_eq!(midpoint.objects[0].id, object);
    assert_eq!(midpoint.objects[0].geometry, from.geometry);
    assert_eq!(midpoint.objects[0].transform.translation, Vec2::new(2.0, -1.0));
    assert_eq!(midpoint.objects[0].transform.rotation, 0.4);
    assert_eq!(midpoint.objects[0].transform.scale, Vec2::new(1.25, 0.75));
    assert_eq!(midpoint.objects[0].style.opacity, 1.0);
    assert_eq!(midpoint.morph(0), 0.5);

    let end = instance.seek(2.0).unwrap().clone();
    assert_eq!(end.objects.len(), 1);
    assert_eq!(end.objects[0].id, object);
    assert_eq!(end.objects[0].geometry, to.geometry);
    assert_eq!(end.objects[0].transform, to.transform);
    assert_eq!(end.objects[0].style, to.style);
    assert_eq!(end.morph(0), 1.0);
    assert_ne!(end.render_geometry(0), &end.objects[0].geometry);
}

#[test]
fn direct_seek_and_forward_playback_match_for_generic_transform() {
    let style = stroke_style(Color::WHITE);
    let from = snapshot(path_a(), Transform2D::IDENTITY, style);
    let to = snapshot(
        path_b(),
        Transform2D {
            translation: Vec2::new(3.0, 2.0),
            rotation: 0.6,
            scale: Vec2::new(1.2, 0.8),
        },
        stroke_style(Color::rgb(0.8, 0.2, 0.4)),
    );
    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().style = from.style;
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::EaseInOutCubic),
        )
        .unwrap();
    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for step in 1..=13 {
        sequential.advance_to(step as f64 * 0.1).unwrap();
    }
    direct.seek(1.3).unwrap();
    assert_eq!(sequential.frame(), direct.frame());
}

#[test]
fn sequential_transforms_are_continuous_and_choose_new_pair_at_boundary() {
    let style = stroke_style(Color::WHITE);
    let a = snapshot(path_a(), Transform2D::IDENTITY, style);
    let b = snapshot(
        path_b(),
        Transform2D {
            translation: Vec2::new(1.0, 0.0),
            ..Transform2D::IDENTITY
        },
        stroke_style(Color::rgb(0.8, 0.3, 0.2)),
    );
    let c = snapshot(
        path_c(),
        Transform2D {
            translation: Vec2::new(2.0, 1.0),
            rotation: 0.5,
            ..Transform2D::IDENTITY
        },
        stroke_style(Color::rgb(0.2, 0.8, 0.3)),
    );

    let mut scene = SceneDefinition::new();
    let object = scene.add(a.geometry.clone());
    scene.object_mut(object).unwrap().style = a.style;
    scene
        .animate_transform(
            object,
            a.clone(),
            b.clone(),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_transform(
            object,
            b.clone(),
            c.clone(),
            TrackTiming::new(1.0, 1.0, Easing::Linear),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut direct = SceneInstance::new(compiled.clone());
    let boundary = direct.seek(1.0).unwrap().clone();
    assert_eq!(boundary.objects[0].geometry, b.geometry);
    assert_eq!(boundary.objects[0].transform, b.transform);
    assert_eq!(boundary.objects[0].style, b.style);
    assert_eq!(boundary.morph(0), 0.0);

    let mut sequential = SceneInstance::new(compiled);
    sequential.advance_to(0.5).unwrap();
    sequential.advance_to(1.0).unwrap();
    assert_eq!(sequential.frame(), &boundary);

    let after = direct.seek(1.5).unwrap();
    assert_eq!(after.morph(0), 0.5);
    assert_eq!(after.objects[0].id, object);
}

#[test]
fn narrow_tracks_override_corresponding_generic_transform_channels() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let mut to = from.clone();
    to.transform.translation = Vec2::new(10.0, 0.0);
    to.transform.rotation = 1.0;
    to.style.opacity = 0.2;
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_position(
            object,
            Vec2::ZERO,
            Vec2::new(20.0, 0.0),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_scalar(
            object,
            Property::Opacity,
            1.0,
            0.8,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let frame = instance.seek(1.0).unwrap();
    assert_eq!(frame.objects[0].transform.translation, Vec2::new(10.0, 0.0));
    assert_eq!(frame.objects[0].transform.rotation, 0.5);
    assert!((frame.objects[0].style.opacity - 0.9).abs() < 1e-6);
}

#[test]
fn generic_path_transform_does_not_reuse_reveal_channel() {
    let style = stroke_style(Color::WHITE);
    let from = snapshot(path_a(), Transform2D::IDENTITY, style);
    let to = snapshot(path_b(), Transform2D::IDENTITY, style);
    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().style = style;
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_reveal(
            object,
            0.0,
            1.0,
            TrackTiming::new(0.0, 4.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let frame = instance.seek(1.0).unwrap();
    assert_eq!(frame.morph(0), 0.5);
    assert_eq!(frame.reveal(0), 0.25);
}
