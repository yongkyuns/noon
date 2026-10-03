use std::sync::Arc;

use noon_compile::{
    lower_semantic_execution_root_with_animation_root, ExecutionPatch, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, CompositionTimeMap, CompositionTimeMapStep, GeometryResource,
    GeometryResourceLookup, MeshResource, Property, RateFunction, SemanticAnimationCompositionKind,
    SemanticMutationTransaction, SemanticObjectRole, SemanticObjectState,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticProjection3D,
    SemanticRotation3D, SemanticStore, SemanticTransform, SemanticVec3, SemanticWorldTransform3D,
    StoredGeometry, TrackId, TrackTiming,
};
use noon_runtime::SceneInstance;

fn pose(x: f64, z_angle: f64) -> SemanticWorldTransform3D {
    SemanticWorldTransform3D::new(
        SemanticVec3::new(x, 0.0, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), z_angle).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap()
}

fn mesh() -> MeshResource {
    MeshResource::new(
        vec![
            SemanticVec3::new(0.0, 0.0, 0.0),
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .unwrap()
}

#[test]
fn mesh_world_track_is_f64_seek_stable_and_keeps_exact_resource_snapshot() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let handle = store.insert_geometry_mesh(mesh());
    let authored = SemanticWorldTransform3D::new(
        SemanticVec3::new(1.0e100, -3.0, 2.0),
        SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform = SemanticTransform {
        translation: authored.translation,
        scale: authored.scale,
        orientation: noon_core::SemanticOrientation::Spatial(authored.rotation),
    };
    let object = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, object).unwrap();

    let from = pose(-1.0e100, 0.0);
    let to = pose(1.0e100, std::f64::consts::PI);
    let time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
        0.25,
        0.5,
        RateFunction::Linear,
    )]);
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        object,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform { from, to },
        TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map,
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let animation_root = committed.resolve(animation_root).unwrap();
    let lowered = lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
        animation_root,
    )
    .unwrap();
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let row = |instance: &SceneInstance| instance.frame().objects[0].world_transform().unwrap();

    let initial = row(&forward);
    assert_eq!(initial.translation.x, authored.translation.x);
    assert_eq!(
        store
            .node(object)
            .unwrap()
            .semantic_object_state()
            .unwrap()
            .transform
            .world_transform(),
        Some(authored)
    );
    assert_eq!(forward.frame().objects[0].geometry(), None);
    assert!(forward.frame().objects[0].content.geometry().is_some());

    let source_resource = match store.geometry_resources().get(handle).unwrap() {
        GeometryResource::Mesh(resource) => Arc::clone(resource),
        _ => panic!("mesh handle resolved to non-mesh resource"),
    };
    let retained_resource = match forward.geometry_resources().get(handle).unwrap() {
        GeometryResource::Mesh(resource) => Arc::clone(resource),
        _ => panic!("compiled handle resolved to non-mesh resource"),
    };
    assert_eq!(
        Arc::as_ptr(&source_resource),
        Arc::as_ptr(&retained_resource)
    );
    assert_eq!(
        forward.geometry_resources().current_handle(handle.id),
        Some(handle)
    );

    // The map is inactive before local alpha .25, begins exactly at .25, and
    // finishes at .75. Values at either side of the mapped boundaries are tested
    // through both the incremental evaluator and a direct seek.
    let samples = [0.0, 0.5, 0.500_001, 1.0, 1.499_999, 1.5, 2.0];
    for time in samples {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame(), seek.frame(), "time {time}");
    }
    let fine_time = 0.5 + 2.0_f64.powi(-40);
    let mut fine_forward = SceneInstance::from_semantic_execution(
        lower_semantic_execution_root_with_animation_root(
            &store,
            root,
            &mut SemanticExecutionIndex::new(),
            animation_root,
        )
        .unwrap(),
    );
    let mut fine_seek = fine_forward.clone();
    fine_forward.advance_to(fine_time).unwrap();
    fine_seek.seek(fine_time).unwrap();
    assert_eq!(fine_forward.frame(), fine_seek.frame());
    let progress = 2.0_f64.powi(-40);
    let expected_x = (1.0 - progress) * from.translation.x + progress * to.translation.x;
    let sampled_x = fine_seek.frame().objects[0]
        .world_transform()
        .unwrap()
        .translation
        .x;
    assert_eq!(sampled_x, expected_x);
    assert_ne!(sampled_x, from.translation.x);

    assert_eq!(row(&forward).translation.x, to.translation.x);
    assert_eq!(row(&forward).rotation, to.rotation);
    seek.seek(0.25).unwrap();
    assert_eq!(row(&seek), initial);
    seek.seek(1.0).unwrap();
    assert!((row(&seek).translation.x).abs() < 1.0e85);
    seek.seek(0.0).unwrap();
    assert_eq!(row(&seek), initial);

    // Removing the producer's semantic node retires its source resource while
    // the already-lowered runtime publication keeps its immutable exact version.
    store.remove_node(object).unwrap();
    assert!(store.geometry_resources().get(handle).is_none());
    assert!(
        matches!(forward.geometry_resources().get(handle), Some(GeometryResource::Mesh(resource)) if Arc::ptr_eq(resource, &retained_resource))
    );
}

#[test]
fn world_track_rejects_planar_only_targets_during_lowering() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_semantic_family_member(root, object).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        object,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: pose(0.0, 0.0),
            to: pose(2.0, 1.0),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let root_animation = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let root_animation = committed.resolve(root_animation).unwrap();
    assert!(lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
        root_animation,
    )
    .is_err());
}

#[test]
fn authored_spatial_transform_survives_a_reconciled_world_track() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let handle = store.insert_geometry_mesh(mesh());
    let object =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle)));
    store.add_semantic_family_member(root, object).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        object,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: pose(0.0, 0.0),
            to: pose(10.0, 0.5),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let animation_root = committed.resolve(animation_root).unwrap();
    let lowered = lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
        animation_root,
    )
    .unwrap();
    let execution_object = lowered.compiled().objects()[0].id;
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    instance.advance_to(1.0).unwrap();
    instance
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(0),
            object: execution_object,
            property: Property::WorldTransform,
            end_time: 1.0,
        })
        .unwrap();

    let authored = pose(42.0, -0.75);
    let transform = SemanticTransform {
        translation: authored.translation,
        scale: authored.scale,
        orientation: noon_core::SemanticOrientation::Spatial(authored.rotation),
    };
    instance
        .apply_execution_patch(&ExecutionPatch::SetSemanticTransform {
            object: execution_object,
            transform,
        })
        .unwrap();
    assert_eq!(
        instance.frame().objects[0].world_transform(),
        Some(authored)
    );
    instance.seek(1.25).unwrap();
    assert_eq!(
        instance.frame().objects[0].world_transform(),
        Some(authored)
    );
}

#[test]
fn camera3d_projection_and_unique_camera_identity_are_enforced() {
    let mut invalid_declaration = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    invalid_declaration.set_role(SemanticObjectRole::Camera3D);
    assert!(invalid_declaration
        .set_camera_projection(Some(SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 2.0,
            far: 1.0,
        }))
        .is_err());

    let mut store = SemanticStore::new();
    let camera = |store: &mut SemanticStore, scale: SemanticVec3| {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        state.set_role(SemanticObjectRole::Camera3D);
        state.transform.scale = scale;
        state
            .set_camera_projection(Some(SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 100.0,
            }))
            .unwrap();
        store.insert_semantic_object(state)
    };
    let root = store.insert_family();
    let camera3d = camera(&mut store, SemanticVec3::new(1.0, 1.0, 1.0));
    store.add_semantic_family_member(root, camera3d).unwrap();
    let lowered = noon_compile::lower_semantic_execution_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
    )
    .unwrap();
    assert!(lowered.camera_object().is_some());
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    let camera_row = &instance.seek(0.0).unwrap().objects[0];
    assert!(camera_row.camera_projection().is_some());
    assert_eq!(
        camera_row.world_transform().unwrap().scale,
        SemanticVec3::new(1.0, 1.0, 1.0)
    );

    let mut invalid_scale = SemanticStore::new();
    let root = invalid_scale.insert_family();
    let invalid = camera(&mut invalid_scale, SemanticVec3::new(2.0, 1.0, 1.0));
    invalid_scale
        .add_semantic_family_member(root, invalid)
        .unwrap();
    assert!(noon_compile::lower_semantic_execution_root(
        &invalid_scale,
        root,
        &mut SemanticExecutionIndex::new(),
    )
    .is_err());

    let mut duplicate = SemanticStore::new();
    let root = duplicate.insert_family();
    let camera3d = camera(&mut duplicate, SemanticVec3::new(1.0, 1.0, 1.0));
    let mut camera2d_state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: noon_core::Vec2::new(4.0, 3.0),
    });
    camera2d_state.set_role(SemanticObjectRole::Camera2D);
    let camera2d = duplicate.insert_semantic_object(camera2d_state);
    duplicate
        .add_semantic_family_member(root, camera3d)
        .unwrap();
    duplicate
        .add_semantic_family_member(root, camera2d)
        .unwrap();
    assert!(matches!(
        noon_compile::lower_semantic_execution_root(
            &duplicate,
            root,
            &mut SemanticExecutionIndex::new(),
        ),
        Err(noon_compile::SemanticExecutionLoweringError::MultipleCameraObjects { .. })
    ));
}

#[test]
fn camera3d_rejects_world_tracks_that_change_scale_atomically() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.set_role(SemanticObjectRole::Camera3D);
    state
        .set_camera_projection(Some(SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 100.0,
        }))
        .unwrap();
    let camera = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, camera).unwrap();
    let from = pose(0.0, 0.0);
    let to = SemanticWorldTransform3D::new(
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticRotation3D::IDENTITY,
        SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        camera,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform { from, to },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let animation_root = committed.resolve(animation_root).unwrap();
    let mut index = SemanticExecutionIndex::new();
    assert!(lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut index,
        animation_root
    )
    .is_err());
    assert!(index.is_empty());
}

#[test]
fn identity_world_track_preserves_reversing_rate_endpoint_for_forward_and_seek() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let handle = store.insert_geometry_mesh(mesh());
    let authored = pose(2.0, 0.25);
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform = SemanticTransform {
        translation: authored.translation,
        scale: authored.scale,
        orientation: noon_core::SemanticOrientation::Spatial(authored.rotation),
    };
    let object = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, object).unwrap();
    let from = pose(-3.0, 0.0);
    let to = pose(10.0, 0.7);
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        object,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform { from, to },
        TrackTiming::new(0.0, 1.0, RateFunction::ThereAndBack),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let animation_root = committed.resolve(animation_root).unwrap();
    let lowered = lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
        animation_root,
    )
    .unwrap();
    let authored_base = lowered.compiled().objects()[0]
        .spatial
        .as_deref()
        .unwrap()
        .world;
    assert_eq!(authored_base, authored);
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    for (time, expected) in [(0.5, to), (1.0, from), (1.5, from)] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame(), seek.frame(), "time {time}");
        assert_eq!(forward.frame().objects[0].world_transform(), Some(expected));
        assert_eq!(authored_base, authored);
        assert_eq!(
            store
                .node(object)
                .unwrap()
                .semantic_object_state()
                .unwrap()
                .transform
                .world_transform(),
            Some(authored),
        );
    }
}

#[test]
fn multiple_world_tracks_survive_seek_forward_reversal_and_reconciliation() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let handle_a = store.insert_geometry_mesh(mesh());
    let handle_b = store.insert_geometry_mesh(mesh());
    let object_a =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle_a)));
    let object_b =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle_b)));
    store.add_semantic_family_member(root, object_a).unwrap();
    store.add_semantic_family_member(root, object_b).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track_a = transaction.create_object_property_track(
        object_a,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: pose(0.0, 0.0),
            to: pose(10.0, 0.5),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let track_b = transaction.create_object_property_track(
        object_b,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: pose(20.0, 0.0),
            to: pose(40.0, 0.8),
        },
        TrackTiming::new(0.0, 2.0, RateFunction::ThereAndBack),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track_a, track_b],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let animation_root = committed.resolve(animation_root).unwrap();
    let lowered = lower_semantic_execution_root_with_animation_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
        animation_root,
    )
    .unwrap();
    let execution_ids = [
        lowered.compiled().objects()[0].id,
        lowered.compiled().objects()[1].id,
    ];
    let mut seek_then_forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut reverse_then_forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut clean_forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut full_seek = SceneInstance::from_semantic_execution(lowered.clone());

    seek_then_forward.seek(0.5).unwrap();
    seek_then_forward.advance_to(1.0).unwrap();
    clean_forward.advance_to(1.0).unwrap();
    full_seek.seek(1.0).unwrap();
    assert_eq!(seek_then_forward.frame(), clean_forward.frame());
    assert_eq!(seek_then_forward.frame(), full_seek.frame());

    reverse_then_forward.seek(1.5).unwrap();
    reverse_then_forward.seek(0.25).unwrap();
    reverse_then_forward.advance_to(0.75).unwrap();
    clean_forward.seek(0.75).unwrap();
    full_seek.seek(0.75).unwrap();
    assert_eq!(reverse_then_forward.frame(), clean_forward.frame());
    assert_eq!(reverse_then_forward.frame(), full_seek.frame());

    reverse_then_forward.seek(1.0).unwrap();
    reverse_then_forward
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(0),
            object: execution_ids[0],
            property: Property::WorldTransform,
            end_time: 1.0,
        })
        .unwrap();
    let reconciled_pose = pose(10.0, 0.5);
    reverse_then_forward
        .apply_execution_patch(&ExecutionPatch::SetSemanticTransform {
            object: execution_ids[0],
            transform: SemanticTransform {
                translation: reconciled_pose.translation,
                scale: reconciled_pose.scale,
                orientation: noon_core::SemanticOrientation::Spatial(reconciled_pose.rotation),
            },
        })
        .unwrap();
    reverse_then_forward.seek(0.5).unwrap();
    assert_ne!(
        reverse_then_forward.frame().objects[0].world_transform(),
        Some(pose(10.0, 0.5))
    );
    reverse_then_forward.seek(1.5).unwrap();
    assert_eq!(
        reverse_then_forward.frame().objects[0].world_transform(),
        Some(pose(10.0, 0.5))
    );
    assert_eq!(
        store
            .node(object_a)
            .unwrap()
            .semantic_object_state()
            .unwrap()
            .transform
            .world_transform(),
        Some(SemanticWorldTransform3D::IDENTITY),
    );
    assert_eq!(
        reverse_then_forward.frame().objects[1]
            .world_transform()
            .unwrap()
            .translation
            .x,
        30.0,
    );
}
