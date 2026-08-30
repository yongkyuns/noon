use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, Property, SceneDefinition, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::SlottedSceneInstance;

const STATIC_OBJECTS: usize = 255;
const MAX_LOCAL_CANDIDATES: usize = 64;

#[test]
fn animated_position_refits_only_the_moving_leaf_without_spatial_rebuild() {
    let mut scene = SceneDefinition::new();
    let moving = scene.add(GeometryRef::rectangle(0.5, 0.5));
    scene.object_mut(moving).unwrap().transform = Transform2D {
        translation: Vec2::new(8.0, 0.0),
        ..Transform2D::IDENTITY
    };
    scene
        .add_track(
            moving,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::new(8.0, 0.0),
                to: Vec2::new(-8.0, 0.0),
            },
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("moving object position track must be valid");

    for index in 0..STATIC_OBJECTS {
        let object = scene.add(GeometryRef::rectangle(0.5, 0.5));
        scene.object_mut(object).unwrap().transform = Transform2D {
            translation: Vec2::new(20.0 + index as f32 * 4.0, 0.0),
            ..Transform2D::IDENTITY
        };
    }

    let compiled = CompiledScene::compile(&scene).expect("animated sparse scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);

    let initial = live.hit_test(Vec2::ZERO);
    assert!(initial.slots().is_empty());
    assert_eq!(initial.stats().full_scan_fallbacks, 0);

    live.advance_to(1.0)
        .expect("animation midpoint must evaluate incrementally");
    let midpoint_update = live.last_spatial_update_stats();
    assert_eq!(midpoint_update.full_rebuilds, 0);
    assert_eq!(midpoint_update.leaves_upserted, 1);
    assert_eq!(midpoint_update.leaves_removed, 0);

    let midpoint = live.hit_test(Vec2::ZERO);
    assert_eq!(midpoint.slots().len(), 1);
    assert_eq!(midpoint.stats().full_scan_fallbacks, 0);
    assert!(
        midpoint.stats().candidates_tested <= MAX_LOCAL_CANDIDATES,
        "animated hit test examined {} candidates in a sparse {}-object scene",
        midpoint.stats().candidates_tested,
        STATIC_OBJECTS + 1
    );

    live.advance_to(2.0)
        .expect("animation endpoint must evaluate incrementally");
    let endpoint_update = live.last_spatial_update_stats();
    assert_eq!(endpoint_update.full_rebuilds, 0);
    assert_eq!(endpoint_update.leaves_upserted, 1);
    assert_eq!(endpoint_update.leaves_removed, 0);

    let endpoint = live.hit_test(Vec2::ZERO);
    assert!(endpoint.slots().is_empty());
    assert_eq!(endpoint.stats().full_scan_fallbacks, 0);
}
