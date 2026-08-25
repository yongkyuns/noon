use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, Property, SceneDefinition, Style, TrackTiming,
    Transform2D, Vec2,
};
use noon_ir::{decode_scene, encode_scene};

#[test]
fn detached_transform_snapshots_round_trip_without_scene_identity() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let to = ObjectSnapshot {
        geometry: GeometryRef::circle(1.0),
        transform: Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: 0.4,
            scale: Vec2::new(1.5, 0.75),
        },
        style: Style {
            fill: Some(Color::rgb(0.2, 0.5, 0.9)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.2,
            stroke_width_mode: noon_core::StrokeWidthMode::ScaleWithObject,
            opacity: 0.6,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
        },
    };
    scene
        .animate_transform(
            object,
            from.clone(),
            to.clone(),
            TrackTiming::new(0.5, 2.0, Easing::EaseInOutCubic),
        )
        .unwrap();

    let json = encode_scene(&scene).unwrap();
    assert!(json.contains("\"property\":\"transform\""));
    assert!(json.contains("\"object\":{\"from\":"));
    let decoded = decode_scene(&json).unwrap();
    assert_eq!(decoded.objects(), scene.objects());
    assert_eq!(decoded.tracks(), scene.tracks());
    assert_eq!(decoded.tracks()[0].property, Property::Transform);

    let encoded_value = serde_json::to_value(&decoded.tracks()[0].values).unwrap();
    let snapshots = encoded_value.get("object").unwrap();
    assert!(snapshots.get("from").unwrap().get("id").is_none());
    assert!(snapshots.get("to").unwrap().get("id").is_none());
}
