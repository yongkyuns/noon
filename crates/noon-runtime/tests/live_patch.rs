use noon_compile::{CompilePatchError, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

type ComparableObjectState = (
    ObjectId,
    noon_runtime::FrameObjectState,
    bool,
    f32,
    f32,
    Option<std::sync::Arc<GeometryRef>>,
    Option<Transform2D>,
);

fn semantic_frame(instance: &SceneInstance) -> Vec<ComparableObjectState> {
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
            frame.render_transforms[index],
        ));
    }
    objects.sort_by_key(|entry| entry.0);
    objects
}

fn assert_live_matches_recompile(
    live: &mut SceneInstance,
    objects: &[CompiledObject],
    tracks: &[TrackDefinition],
    time: f64,
) {
    let compiled = CompiledScene::compile_objects(objects.to_vec(), tracks)
        .expect("execution data must compile");
    let mut expected = SceneInstance::new(compiled);
    expected.seek(time).expect("valid seek");
    live.seek(time).expect("valid seek");
    assert_eq!(live.frame().time, expected.frame().time);
    assert_eq!(semantic_frame(live), semantic_frame(&expected));
}

#[test]
fn create_add_track_and_remove_match_full_recompile() {
    let original = ObjectId::new(0);
    let mut objects = vec![CompiledObject::new(
        original,
        GeometryRef::rectangle(2.0, 1.0),
        Transform2D::IDENTITY,
        Style::default(),
    )];
    let compiled =
        CompiledScene::compile_objects(objects.clone(), &[]).expect("execution data compiles");
    let mut live = SceneInstance::new(compiled);
    let time = 1.5;
    live.seek(time).expect("valid seek");

    let created = ObjectId::new(10);
    let created_object = CompiledObject::new(
        created,
        GeometryRef::circle(0.5),
        Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::IDENTITY
        },
        Style::default(),
    );
    live.apply_execution_patch(&ExecutionPatch::CreateObject(created_object.clone()))
        .expect("live create succeeds");
    objects.push(created_object);

    let track = TrackDefinition {
        id: TrackId::new(20),
        object: created,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::new(2.0, -1.0),
            to: Vec2::new(6.0, 3.0),
        },
        timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    let add_track = ExecutionPatch::AddTrack(track.clone());
    live.apply_execution_patch(&add_track)
        .expect("live track add must succeed");
    let tracks = [track];

    assert_live_matches_recompile(&mut live, &objects, &tracks, time);

    let remove = ExecutionPatch::RemoveObject(original);
    live.apply_execution_patch(&remove)
        .expect("live remove must succeed");
    objects.retain(|object| object.id != original);

    assert_live_matches_recompile(&mut live, &objects, &tracks, time);
    assert_eq!(live.frame().objects.len(), 2);
    assert!(!live.frame().presences[0]);
    assert_eq!(live.frame().objects[1].id, created);
}

#[test]
fn rejected_patch_is_transactional() {
    let objects = vec![CompiledObject::new(
        ObjectId::new(0),
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    )];
    let compiled = CompiledScene::compile_objects(objects, &[]).expect("execution data compiles");
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).expect("valid seek");
    let before = live.frame().clone();

    let invalid = ExecutionPatch::AddTrack(TrackDefinition {
        id: TrackId::new(9),
        object: ObjectId::new(999),
        property: Property::Opacity,
        values: TrackValues::Scalar { from: 1.0, to: 0.0 },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    assert_eq!(
        live.apply_execution_patch(&invalid),
        Err(CompilePatchError::UnknownObject(ObjectId::new(999)))
    );
    assert_eq!(live.frame(), &before);
}

#[test]
fn replacing_track_preserves_unrelated_object_identity_and_time() {
    let animated = ObjectId::new(0);
    let untouched = ObjectId::new(1);
    let objects = vec![
        CompiledObject::new(
            animated,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ),
        CompiledObject::new(
            untouched,
            GeometryRef::rectangle(3.0, 2.0),
            Transform2D::IDENTITY,
            Style::default(),
        ),
    ];
    let track_id = TrackId::new(0);
    let initial_track = TrackDefinition {
        id: track_id,
        object: animated,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(4.0, 0.0),
        },
        timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    let compiled = CompiledScene::compile_objects(objects.clone(), &[initial_track])
        .expect("execution data compiles");
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).expect("valid seek");

    let replacement = TrackDefinition {
        id: track_id,
        object: animated,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(8.0, 2.0),
        },
        timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    live.apply_execution_patch(&ExecutionPatch::ReplaceTrack(replacement.clone()))
        .expect("live patch succeeds");
    let tracks = [replacement];

    assert_eq!(live.frame().time, 2.0);
    assert_eq!(live.frame().objects[1].id, untouched);
    assert_live_matches_recompile(&mut live, &objects, &tracks, 2.0);
}

#[test]
fn timeline_patch_relowers_only_affected_runtime_channel() {
    let objects: Vec<_> = (0..10_000u32)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(u64::from(index)),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let mut tracks: Vec<_> = objects
        .iter()
        .enumerate()
        .map(|(index, object)| TrackDefinition {
            id: TrackId::new(index as u64),
            object: object.id,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(1.0, 0.0),
            },
            timing: TrackTiming::new(1000.0 + index as f64, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        })
        .collect();
    let compiled = CompiledScene::compile_objects(objects.clone(), &tracks)
        .expect("large execution input compiles");
    let mut live = SceneInstance::new(compiled);
    live.seek(0.5).expect("valid seek");

    let target = objects[5_000].id;
    let track = TrackDefinition {
        id: TrackId::new(50_000),
        object: target,
        property: Property::Opacity,
        values: TrackValues::Scalar {
            from: 1.0,
            to: 0.25,
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    live.apply_execution_patch(&ExecutionPatch::AddTrack(track.clone()))
        .expect("runtime timeline patch succeeds");
    tracks.push(track);

    let stats = live.last_patch_stats();
    assert_eq!(stats.channels_relowered, 1);
    assert_eq!(stats.scheduler_events_removed, 0);
    assert_eq!(stats.scheduler_events_inserted, 2);
    assert_eq!(stats.objects_recomputed, 1);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    assert!(stats.groups_evaluated <= 2);
    assert_live_matches_recompile(&mut live, &objects, &tracks, 0.5);
}

#[test]
fn moving_a_track_between_objects_relowers_only_old_and_new_channels() {
    let first = ObjectId::new(0);
    let second = ObjectId::new(1);
    let objects: Vec<_> = [first, second]
        .into_iter()
        .map(|id| {
            CompiledObject::new(
                id,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let id = TrackId::new(0);
    let initial_track = TrackDefinition {
        id,
        object: first,
        property: Property::Opacity,
        values: TrackValues::Scalar { from: 1.0, to: 0.0 },
        timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    let compiled = CompiledScene::compile_objects(objects.clone(), &[initial_track]).unwrap();
    let mut live = SceneInstance::new(compiled);
    live.seek(2.0).unwrap();

    let replacement = TrackDefinition {
        id,
        object: second,
        property: Property::Rotation,
        values: TrackValues::Scalar { from: 0.0, to: 1.0 },
        timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    live.apply_execution_patch(&ExecutionPatch::ReplaceTrack(replacement.clone()))
        .unwrap();
    let tracks = [replacement];
    let stats = live.last_patch_stats();
    assert_eq!(stats.channels_relowered, 2);
    assert_eq!(stats.scheduler_events_removed, 2);
    assert_eq!(stats.scheduler_events_inserted, 2);
    assert_eq!(stats.objects_recomputed, 2);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_seeks, 0);
    assert_live_matches_recompile(&mut live, &objects, &tracks, 2.0);
}

#[test]
fn structural_remove_and_create_touch_only_their_stable_frame_slots() {
    let mut objects: Vec<_> = (0..100_000)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(index),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let tracks = [];
    let compiled = CompiledScene::compile_objects(objects.clone(), &tracks).unwrap();
    let mut live = SceneInstance::new(compiled);
    live.seek(0.5).unwrap();
    live.take_frame_changes();
    let untouched_id = live.frame().objects[11].id;
    let untouched_before = live.frame().objects[11].clone();

    let removed = objects[10].id;
    let remove = ExecutionPatch::RemoveObject(removed);
    live.apply_execution_patch(&remove).unwrap();
    objects.retain(|object| object.id != removed);
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
    assert_live_matches_recompile(&mut live, &objects, &tracks, 0.5);

    live.take_frame_changes();
    let created = ObjectId::new(200_000);
    let created_object = CompiledObject::new(
        created,
        GeometryRef::rectangle(2.0, 3.0),
        Transform2D::IDENTITY,
        Style::default(),
    );
    live.apply_execution_patch(&ExecutionPatch::CreateObject(created_object.clone()))
        .unwrap();
    objects.push(created_object);
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
    assert_live_matches_recompile(&mut live, &objects, &tracks, 0.5);
}

#[test]
fn remove_then_recreate_same_object_id_reuses_its_live_slot() {
    let object = ObjectId::new(0);
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        &[],
    )
    .unwrap();
    let mut live = SceneInstance::new(compiled);

    let remove = ExecutionPatch::RemoveObject(object);
    live.apply_execution_patch(&remove).unwrap();

    let recreated = CompiledObject::new(
        object,
        GeometryRef::rectangle(3.0, 2.0),
        Transform2D::IDENTITY,
        Style::default(),
    );
    live.apply_execution_patch(&ExecutionPatch::CreateObject(recreated.clone()))
        .unwrap();
    let objects = [recreated];
    let tracks = [];

    assert_eq!(live.frame().objects.len(), 1);
    assert_eq!(live.frame().objects[0].id, object);
    assert!(live.frame().presences[0]);
    assert!(live.object_slot_is_live(0));
    assert_eq!(live.last_patch_stats().object_slots_reactivated, 1);
    assert_eq!(live.last_patch_stats().object_slots_appended, 0);
    assert_live_matches_recompile(&mut live, &objects, &tracks, 0.0);
}
