use noon_compile::{CompilePatchError, CompiledScene};
use noon_core::{
    CompositionTimeMap, Easing, GeometryRef, ObjectDefinition, ObjectId, Property, SceneDefinition,
    ScenePatch, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

fn semantic_frame(
    instance: &SceneInstance,
) -> Vec<(
    ObjectId,
    noon_runtime::FrameObjectState,
    bool,
    f32,
    f32,
    Option<GeometryRef>,
)> {
    let frame = instance.frame();
    let mut objects = Vec::new();
    for (index, object) in frame.objects.iter().enumerate() {
        if !instance.object_slot_is_live(index) {
            continue;
        }
        objects.push((
            object.id,
            object.clone(),
            frame.presences[index],
            frame.reveals[index],
            frame.morphs[index],
            frame.render_geometries[index].clone(),
        ));
    }
    objects.sort_by_key(|entry| entry.0);
    objects
}

fn assert_live_matches_definition(
    live: &mut SceneInstance,
    definition: &SceneDefinition,
    time: f64,
) {
    let compiled = CompiledScene::compile(definition).expect("definition must compile");
    let mut expected = SceneInstance::new(compiled);
    expected.seek(time).expect("valid seek");
    live.seek(time).expect("valid seek");
    assert_eq!(live.frame().time, expected.frame().time);
    assert_eq!(semantic_frame(live), semantic_frame(&expected));
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
        origin: None,
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
    assert_eq!(live.frame().objects.len(), 2);
    assert!(!live.frame().presences[0]);
    assert_eq!(live.frame().objects[1].id, created);
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
        origin: None,
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
        origin: None,
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

#[test]
fn timeline_patch_relowers_only_affected_runtime_channel() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(10_000);
    for index in 0..10_000u32 {
        let object = definition.add(GeometryRef::circle(1.0));
        objects.push(object);
        definition
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(1.0, 0.0),
                TrackTiming::new(1000.0 + index as f64, 1.0, Easing::Linear),
            )
            .expect("valid seed track");
    }
    let compiled = CompiledScene::compile(&definition).expect("large scene compiles");
    let mut live = SceneInstance::new(compiled);
    live.seek(0.5).expect("valid seek");

    let target = objects[5_000];
    let patch = ScenePatch::AddTrack(TrackDefinition {
        id: TrackId::new(50_000),
        object: target,
        property: Property::Opacity,
        values: TrackValues::Scalar {
            from: 1.0,
            to: 0.25,
        },
        timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
        origin: None,
        time_map: CompositionTimeMap::identity(),
    });
    live.apply_patch(&patch)
        .expect("runtime timeline patch succeeds");
    definition
        .apply_patch(patch)
        .expect("definition patch succeeds");

    let stats = live.last_patch_stats();
    assert_eq!(stats.channels_relowered, 1);
    assert_eq!(stats.scheduler_events_removed, 0);
    assert_eq!(stats.scheduler_events_inserted, 2);
    assert_eq!(stats.objects_recomputed, 1);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    assert!(stats.groups_evaluated <= 2);
    assert_live_matches_definition(&mut live, &definition, 0.5);
}

#[test]
fn moving_a_track_between_objects_relowers_only_old_and_new_channels() {
    let mut definition = SceneDefinition::new();
    let first = definition.add(GeometryRef::circle(1.0));
    let second = definition.add(GeometryRef::circle(1.0));
    let id = definition
        .animate_scalar(
            first,
            Property::Opacity,
            1.0,
            0.0,
            TrackTiming::new(0.0, 4.0, Easing::Linear),
        )
        .unwrap();
    let compiled = CompiledScene::compile(&definition).unwrap();
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).unwrap();

    let replacement = TrackDefinition {
        id,
        object: second,
        property: Property::Rotation,
        values: TrackValues::Scalar { from: 0.0, to: 1.0 },
        timing: TrackTiming::new(0.0, 4.0, Easing::Linear),
        origin: None,
        time_map: CompositionTimeMap::identity(),
    };
    live.apply_patch(&ScenePatch::ReplaceTrack(replacement.clone()))
        .unwrap();
    definition
        .apply_patch(ScenePatch::ReplaceTrack(replacement))
        .unwrap();
    let stats = live.last_patch_stats();
    assert_eq!(stats.channels_relowered, 2);
    assert_eq!(stats.scheduler_events_removed, 2);
    assert_eq!(stats.scheduler_events_inserted, 2);
    assert_eq!(stats.objects_recomputed, 2);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    assert_live_matches_definition(&mut live, &definition, 2.0);
}

#[test]
fn structural_remove_and_create_touch_only_their_stable_frame_slots() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(100_000);
    for _ in 0..100_000 {
        objects.push(definition.add(GeometryRef::circle(1.0)));
    }
    let compiled = CompiledScene::compile(&definition).unwrap();
    let mut live = SceneInstance::new(compiled);
    live.seek(0.5).unwrap();
    live.take_frame_changes();
    let untouched_id = live.frame().objects[11].id;
    let untouched_before = live.frame().objects[11].clone();

    let remove = ScenePatch::RemoveObject(objects[10]);
    live.apply_patch(&remove).unwrap();
    definition.apply_patch(remove).unwrap();
    let stats = live.last_patch_stats();
    assert_eq!(stats.object_slots_retired, 1);
    assert_eq!(stats.object_slots_appended, 0);
    assert_eq!(stats.channels_relowered, 0);
    assert_eq!(stats.objects_recomputed, 0);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    let changes = live.take_frame_changes();
    assert!(!changes.is_all());
    assert_eq!(changes.object_indices(), &[10]);
    assert!(!live.frame().presences[10]);
    assert_eq!(live.frame().objects[11].id, untouched_id);
    assert_eq!(live.frame().objects[11], untouched_before);
    assert_live_matches_definition(&mut live, &definition, 0.5);

    live.take_frame_changes();
    let created = ObjectId::new(200_000);
    let create = ScenePatch::CreateObject(ObjectDefinition::new(
        created,
        GeometryRef::rectangle(2.0, 3.0),
    ));
    live.apply_patch(&create).unwrap();
    definition.apply_patch(create).unwrap();
    let stats = live.last_patch_stats();
    assert_eq!(stats.object_slots_appended, 1);
    assert_eq!(stats.object_slots_retired, 0);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    let changes = live.take_frame_changes();
    assert_eq!(changes.object_indices(), &[100_000]);
    assert_eq!(live.frame().objects[100_000].id, created);
    assert!(live.frame().presences[100_000]);
    assert_eq!(live.frame().objects[11], untouched_before);
    assert_live_matches_definition(&mut live, &definition, 0.5);
}

#[test]
fn remove_then_recreate_same_object_id_appends_a_new_live_slot() {
    let mut definition = SceneDefinition::new();
    let object = definition.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&definition).unwrap();
    let mut live = SceneInstance::new(compiled);

    let remove = ScenePatch::RemoveObject(object);
    live.apply_patch(&remove).unwrap();
    definition.apply_patch(remove).unwrap();

    let create = ScenePatch::CreateObject(ObjectDefinition::new(
        object,
        GeometryRef::rectangle(3.0, 2.0),
    ));
    live.apply_patch(&create).unwrap();
    definition.apply_patch(create).unwrap();

    assert_eq!(live.frame().objects.len(), 2);
    assert_eq!(live.frame().objects[0].id, object);
    assert!(!live.frame().presences[0]);
    assert_eq!(live.frame().objects[1].id, object);
    assert!(live.frame().presences[1]);
    assert!(live.object_slot_is_live(1));
    assert!(!live.object_slot_is_live(0));
    assert_live_matches_definition(&mut live, &definition, 0.0);
}
