use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, MutationTransaction, ObjectId, Property, SceneDefinition, ScenePatch,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
};

#[test]
fn hundred_thousand_object_transaction_preflight_clones_no_compiled_scene() {
    let mut scene = SceneDefinition::new();
    for _ in 0..100_000 {
        scene.add(GeometryRef::circle(1.0));
    }
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let transaction =
        MutationTransaction::from_mutations([ScenePatch::RemoveObject(ObjectId::new(10))]);
    let stats = compiled
        .preflight_transaction(&transaction)
        .expect("removal preflights");
    assert_eq!(stats.objects_indexed, 100_000);
    assert_eq!(stats.mutations_preflighted, 1);
    assert_eq!(stats.staged_compiled_scene_clones, 0);
    assert_eq!(compiled.object_index(ObjectId::new(10)), Some(10));
}

#[test]
fn transaction_preflight_rejects_late_invalid_track_without_mutation() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let before = compiled.clone();
    let transaction = MutationTransaction::from_mutations([
        ScenePatch::AddTrack(TrackDefinition {
            id: TrackId::new(10),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }),
        ScenePatch::AddTrack(TrackDefinition {
            id: TrackId::new(11),
            object: ObjectId::new(999),
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }),
    ]);
    assert!(compiled.preflight_transaction(&transaction).is_err());
    assert_eq!(compiled, before);
}
