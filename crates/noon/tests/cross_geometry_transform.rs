use noon::prelude::*;
use noon::{Property, TrackValues};

#[test]
fn rust_facade_keeps_cross_kind_transform_semantic() {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::new(1.0).color(BLUE));

    scene
        .play(Transform::new(circle, Square::new(2.0).color(PURPLE)))
        .run_time(1.5)
        .unwrap();

    let tracks = scene.definition().tracks();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].property, Property::Transform);
    let TrackValues::Object { from, to } = &tracks[0].values else {
        panic!("Transform must lower to object snapshots");
    };
    assert!(matches!(from.geometry, GeometryRef::Circle { .. }));
    assert!(matches!(to.geometry, GeometryRef::Rectangle { .. }));
    assert_eq!(from.style.fill, Some(BLUE));
    assert_eq!(to.style.fill, Some(PURPLE));
}
