use noon_compile::{CompilePatchError, CompiledScene};
use noon_core::{
    CompositionTimeMap, Easing, GeometryRef, ObjectDefinition, ObjectId, Property, SceneDefinition,
    ScenePatch, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

fn assert_live_matches_definition(
    live: &mut SceneInstance,
    definition: &SceneDefinition,
    time: f64,
) {
    let compiled = CompiledScene::compile(definition).expect("definition must compile");
    let mut expected = SceneInstance::new(compiled);
    expected.seek(time).expect("valid seek");
    live.seek(time).expect("valid seek");
    let live_objects = live
        .frame()
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| live.frame().is_live(*index))
        .map(|(index, object)| {
            (
                object.clone(),
                live.frame().presences[index],
                live.frame().reveals[index],
                live.frame().morphs[index],
                live.frame().render_geometries[index].clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected_objects = expected
        .frame()
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| expected.frame().is_live(*index))
        .map(|(index, object)| {
            (
                object.clone(),
                expected.frame().presences[index],
                expected.frame().reveals[index],
                expected.frame().morphs[index],
                expected.frame().render_geometries[index].clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(live.frame().time, expected.frame().time);
    assert_eq!(live_objects, expected_objects);
}

#[test]
fn create_add_track_and_remove_match_full_recompile() {
    let mut definition = SceneDefinition::new();
    let original = definition.add(GeometryRef::rectangle(2.0, 1.0));
    let compiled = CompiledScene::compile(&definition).expect("scene must compile");
    let mut live = SceneInstance::new(compiled);
    let time = 1.5;
    live.seek(time).expect("valid seek");

    let created = ObjectId::new(10);
    let mut created_definition = ObjectDefinition::new(created, GeometryRef::circle(0.5));
    created_definition.transform = Transform2D {
        translation: Vec2::new(2.0, -1.0),
        ..Transform2D::IDENTITY
    };
    let create = ScenePatch::CreateObject(created_definition);
    live.apply_patch(&create).expect("live create must succeed");
    definition
        .apply_patch(create)
        .expect("definition create must succeed");

    let track = TrackDefinition {
        id: TrackId::new(20),
        object: created,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::new(2.0, -1.0),
            to: Vec2::new(6.0, 3.0),
        },
        timing: TrackTiming::new(1.0, 2.0, Easing::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    let add_track = ScenePatch::AddTrack(track);
    live.apply_patch(&add_track)
        .expect("live track add must succeed");
    definition
        .apply_patch(add_track)
        .expect("definition track add must succeed");

    assert_live_matches_definition(&mut live, &definition, time);

    let remove = ScenePatch::RemoveObject(original);
    live.apply_patch(&remove).expect("live remove must succeed");
    definition
        .apply_patch(remove)
        .expect("definition remove must succeed");

    assert_live_matches_definition(&mut live, &definition, time);
    assert_eq!(live.frame().live_object_count(), 1);
    let created_index = live
        .frame()
        .objects
        .iter()
        .position(|object| object.live && object.id == created)
        .expect("created object stays live");
    assert_eq!(live.frame().objects[created_index].id, created);
}

#[test]
fn rejected_patch_is_transactional() {
    let mut definition = SceneDefinition::new();
    definition.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&definition).expect("scene must compile");
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).expect("valid seek");
    let before = live.frame().clone();

    let invalid = ScenePatch::AddTrack(TrackDefinition {
        id: TrackId::new(9),
        object: ObjectId::new(999),
        property: Property::Opacity,
        values: TrackValues::Scalar { from: 1.0, to: 0.0 },
        timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    assert_eq!(
        live.apply_patch(&invalid),
        Err(CompilePatchError::UnknownObject(ObjectId::new(999)))
    );
    assert_eq!(live.frame(), &before);
}

#[test]
fn replacing_track_preserves_unrelated_object_identity_and_time() {
    let mut definition = SceneDefinition::new();
    let animated = definition.add(GeometryRef::circle(1.0));
    let untouched = definition.add(GeometryRef::rectangle(3.0, 2.0));
    let track_id = definition
        .animate_position(
            animated,
            Vec2::ZERO,
            Vec2::new(4.0, 0.0),
            TrackTiming::new(0.0, 4.0, Easing::Linear),
        )
        .expect("valid track");
    let compiled = CompiledScene::compile(&definition).expect("scene must compile");
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).expect("valid seek");

    let patch = ScenePatch::ReplaceTrack(TrackDefinition {
        id: track_id,
        object: animated,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(8.0, 2.0),
        },
        timing: TrackTiming::new(0.0, 4.0, Easing::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    live.apply_patch(&patch).expect("live patch must succeed");
    definition
        .apply_patch(patch)
        .expect("definition patch must succeed");

    assert_eq!(live.frame().time, 2.0);
    assert_eq!(live.frame().objects[1].id, untouched);
    assert_live_matches_definition(&mut live, &definition, 2.0);
}
