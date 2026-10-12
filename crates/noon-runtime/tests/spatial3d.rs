use std::sync::Arc;

use noon_compile::{
    lower_semantic_execution_root, lower_semantic_execution_root_with_animation_root,
    ExecutionPatch, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, CameraAngularMotion, CameraRotationAxis, CompositionTimeMap,
    CompositionTimeMapStep, GeometryResource, GeometryResourceLookup, ManimCamera3DProfile,
    MeshResource, Property, RateFunction, SemanticAnimationCompositionKind,
    SemanticMutationTransaction, SemanticObjectRole, SemanticObjectState,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticProjection3D,
    SemanticRotation3D, SemanticSpatialMaterial, SemanticStore, SemanticTransform, SemanticVec3,
    SemanticWorldTransform3D, StoredGeometry, TrackId, TrackTiming,
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
fn ambient_camera_motion_is_authored_time_driven_and_seek_equivalent() {
    let source = ManimCamera3DProfile {
        phi: 0.8,
        theta: -1.2,
        gamma: 0.3,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let endpoint = ManimCamera3DProfile {
        theta: 0.8,
        ..source
    };
    let interval = CameraAngularMotion::new(
        source,
        CameraRotationAxis::Theta,
        1.0,
        0.0,
        Some(2.0),
        0.1,
        100.0,
    )
    .unwrap();
    assert!(CameraAngularMotion::new(
        source,
        CameraRotationAxis::Theta,
        f64::MAX,
        0.0,
        Some(2.0),
        0.1,
        100.0,
    )
    .is_none());
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera.set_camera_profile(endpoint, 0.1, 100.0).unwrap();
    camera.set_camera_motions(Arc::from([interval])).unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let lowered = noon_compile::lower_semantic_execution_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
    )
    .unwrap();
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let camera_sample =
        |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;

    forward.advance_to(0.0).unwrap();
    seek.seek(0.0).unwrap();
    assert_eq!(camera_sample(&forward), source);
    assert_eq!(forward.frame(), seek.frame());

    forward.advance_to(0.5).unwrap();
    seek.seek(0.5).unwrap();
    let mid = camera_sample(&forward);
    assert_eq!(mid.theta, -0.7);
    assert_eq!(forward.frame(), seek.frame());
    assert_eq!(forward.last_timeline_scheduler_stats().active_groups, 1);

    forward.advance_to(2.0).unwrap();
    seek.seek(2.0).unwrap();
    assert_eq!(camera_sample(&forward), endpoint);
    assert_eq!(forward.frame(), seek.frame());
    forward.advance_to(2.25).unwrap();
    seek.seek(2.25).unwrap();
    assert_eq!(camera_sample(&forward), endpoint);
    assert_eq!(forward.frame(), seek.frame());
}

#[test]
fn illusion_camera_motion_is_seek_equivalent_and_zero_rate_stays_settled() {
    let source = ManimCamera3DProfile {
        phi: 75_f64.to_radians(),
        theta: 30_f64.to_radians(),
        gamma: 0.2,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let duration = std::f64::consts::FRAC_PI_2;
    let interval = CameraAngularMotion::three_d_illusion(
        source,
        2.0,
        0.0,
        Some(duration),
        0.1,
        100.0,
        None,
        None,
    )
    .unwrap();
    let endpoint = interval.sample(duration).unwrap();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera.set_camera_profile(endpoint, 0.1, 100.0).unwrap();
    camera.set_camera_motions(Arc::from([interval])).unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let lowered =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new()).unwrap();
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let sample = |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;

    for time in [
        0.0,
        0.2,
        std::f64::consts::FRAC_PI_4,
        duration,
        duration + 0.25,
    ] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
        if time < duration {
            let phase = time * 2.0;
            let current = sample(&forward);
            assert!((current.theta - (source.theta + 0.2 * phase.sin())).abs() < 1e-14);
            assert!((current.phi - (source.phi + 0.1 * phase.cos() - 0.1)).abs() < 1e-14);
        } else {
            assert_eq!(sample(&forward), endpoint);
        }
    }

    let settled =
        CameraAngularMotion::three_d_illusion(source, 0.0, 0.0, None, 0.1, 100.0, None, None)
            .unwrap();
    let mut settled_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    settled_state.set_role(SemanticObjectRole::Camera3D);
    settled_state
        .set_camera_profile(source, 0.1, 100.0)
        .unwrap();
    settled_state
        .set_camera_motions(Arc::from([settled]))
        .unwrap();
    let mut settled_store = SemanticStore::new();
    let settled_root = settled_store.insert_family();
    let settled_camera = settled_store.insert_semantic_object(settled_state);
    settled_store
        .add_semantic_family_member(settled_root, settled_camera)
        .unwrap();
    let settled_scene = lower_semantic_execution_root(
        &settled_store,
        settled_root,
        &mut SemanticExecutionIndex::new(),
    )
    .unwrap();
    let mut settled_instance = SceneInstance::from_semantic_execution(settled_scene);
    settled_instance.advance_to(0.0).unwrap();
    assert_eq!(
        settled_instance
            .last_timeline_scheduler_stats()
            .active_groups,
        0
    );
    settled_instance.advance_to(100.0).unwrap();
    assert_eq!(
        settled_instance
            .last_timeline_scheduler_stats()
            .active_groups,
        0
    );
    assert_eq!(
        settled_instance.frame().objects[0]
            .camera_profile()
            .unwrap()
            .0,
        source
    );
}

#[test]
fn closed_ambient_camera_intervals_hold_endpoints_across_wait_gaps_and_seek() {
    let source = ManimCamera3DProfile {
        phi: 0.8,
        theta: -1.2,
        gamma: 0.3,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let first = CameraAngularMotion::new(
        source,
        CameraRotationAxis::Theta,
        0.5,
        0.0,
        Some(1.0),
        0.1,
        100.0,
    )
    .unwrap();
    let first_endpoint = first.sample(1.0).unwrap();
    let second = CameraAngularMotion::new(
        first_endpoint,
        CameraRotationAxis::Theta,
        -0.25,
        2.0,
        Some(3.0),
        0.1,
        100.0,
    )
    .unwrap();
    let second_endpoint = second.sample(3.0).unwrap();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    // Persistent authored state is the final stop value. Earlier closed intervals
    // still carry the history needed to seek through the wait between them.
    camera
        .set_camera_profile(second_endpoint, 0.1, 100.0)
        .unwrap();
    camera
        .set_camera_motions(Arc::from([first, second]))
        .unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let lowered = noon_compile::lower_semantic_execution_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
    )
    .unwrap();
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let sample = |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;

    for (time, expected) in [
        (0.0, source),
        (1.0, first_endpoint),
        (1.5, first_endpoint),
        (2.0, first_endpoint),
        (2.5, second.sample(2.5).unwrap()),
        (3.0, second_endpoint),
        (3.5, second_endpoint),
    ] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(sample(&forward), expected, "profile at t={time}");
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
    }
}

#[test]
fn replay_budget_counts_saved_and_incoming_camera_motion_intervals() {
    fn camera_state(
        first_axis: CameraRotationAxis,
        second_axis: CameraRotationAxis,
        first_illusion: bool,
    ) -> SemanticObjectState {
        let source = ManimCamera3DProfile {
            phi: 0.8,
            theta: -1.2,
            gamma: 0.3,
            focal_distance: 20.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::ZERO,
        };
        let first = if first_illusion {
            CameraAngularMotion::three_d_illusion(
                source,
                0.2,
                0.0,
                Some(1.0),
                0.1,
                100.0,
                None,
                None,
            )
            .unwrap()
        } else {
            CameraAngularMotion::new(source, first_axis, 0.2, 0.0, Some(1.0), 0.1, 100.0).unwrap()
        };
        let middle = first.sample(1.0).unwrap();
        let second =
            CameraAngularMotion::new(middle, second_axis, 0.3, 2.0, Some(3.0), 0.1, 100.0).unwrap();
        let endpoint = second.sample(3.0).unwrap();
        let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        camera.set_role(SemanticObjectRole::Camera3D);
        camera.set_camera_profile(endpoint, 0.1, 100.0).unwrap();
        camera
            .set_camera_motions(Arc::from([first, second]))
            .unwrap();
        camera
    }

    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera = store.insert_semantic_object(camera_state(
        CameraRotationAxis::Theta,
        CameraRotationAxis::Theta,
        false,
    ));
    let ordinary = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_semantic_family_member(root, camera).unwrap();
    store.add_semantic_family_member(root, ordinary).unwrap();
    let lowered =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new()).unwrap();
    let camera_id = lowered
        .compiled()
        .objects()
        .iter()
        .find(|object| {
            object
                .spatial
                .as_ref()
                .is_some_and(|spatial| spatial.camera_profile.is_some())
        })
        .unwrap()
        .id;
    let ordinary_id = lowered
        .compiled()
        .objects()
        .iter()
        .find(|object| object.spatial.is_none())
        .unwrap()
        .id;
    let incoming = camera_state(CameraRotationAxis::Phi, CameraRotationAxis::Gamma, true);
    let incoming_lowered = {
        let mut incoming_store = SemanticStore::new();
        let incoming_root = incoming_store.insert_family();
        let incoming_object = incoming_store.insert_semantic_object(incoming);
        incoming_store
            .add_semantic_family_member(incoming_root, incoming_object)
            .unwrap();
        lower_semantic_execution_root(
            &incoming_store,
            incoming_root,
            &mut SemanticExecutionIndex::new(),
        )
        .unwrap()
    };
    let incoming_base_transform = incoming_lowered.compiled().objects()[0].base_transform;
    let incoming_spatial = incoming_lowered.compiled().objects()[0]
        .spatial
        .as_deref()
        .unwrap()
        .clone();
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    instance
        .begin_replay_retention(noon_runtime::ReplayLimits {
            revisions: 8,
            payloads: 5,
        })
        .unwrap();
    // A normal object edit with no camera-motion history costs one row payload.
    instance
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: ordinary_id,
            transform: noon_core::Transform2D {
                translation: noon_core::Vec2::new(1.0, 0.0),
                ..noon_core::Transform2D::IDENTITY
            },
        })
        .unwrap();
    assert_eq!(instance.replay_stats().payloads_retained, 1);

    // This affects one camera row, but the inverse must retain two saved and
    // two incoming interval payloads (cost 5 total), exceeding the four slots
    // left after the ordinary edit.
    instance
        .apply_execution_patch(&ExecutionPatch::SetSpatialState {
            object: camera_id,
            base_transform: incoming_base_transform,
            spatial: Some(incoming_spatial),
        })
        .unwrap();
    assert_eq!(instance.replay_stats().payloads_retained, 0);
    assert_eq!(
        instance.seal_replay(),
        Err(noon_runtime::ReplayError::RetentionLimit)
    );
}

#[test]
fn delayed_ambient_motion_holds_its_source_before_the_first_interval() {
    let source = ManimCamera3DProfile {
        phi: 0.8,
        theta: -1.2,
        gamma: 0.3,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let motion = CameraAngularMotion::new(
        source,
        CameraRotationAxis::Theta,
        0.6,
        2.0,
        Some(3.0),
        0.1,
        100.0,
    )
    .unwrap();
    let endpoint = motion.sample(3.0).unwrap();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera.set_camera_profile(endpoint, 0.1, 100.0).unwrap();
    camera.set_camera_motions(Arc::from([motion])).unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let lowered = noon_compile::lower_semantic_execution_root(
        &store,
        root,
        &mut SemanticExecutionIndex::new(),
    )
    .unwrap();
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let sample = |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;

    for (time, expected) in [(0.0, source), (1.0, source), (2.0, source), (3.0, endpoint)] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(sample(&forward), expected, "profile at t={time}");
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
    }
}

#[test]
fn delayed_ambient_motion_follows_an_earlier_finite_camera_profile_track() {
    let from = ManimCamera3DProfile {
        phi: 0.8,
        theta: -1.2,
        gamma: 0.3,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let finite_endpoint = ManimCamera3DProfile {
        zoom: 1.75,
        frame_center: SemanticVec3::new(1.0, -0.5, 0.0),
        ..from
    };
    let motion = CameraAngularMotion::new(
        finite_endpoint,
        CameraRotationAxis::Theta,
        0.6,
        2.0,
        Some(3.0),
        0.1,
        100.0,
    )
    .unwrap();
    let ambient_endpoint = motion.sample(3.0).unwrap();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_profile(ambient_endpoint, 0.1, 100.0)
        .unwrap();
    camera.set_camera_motions(Arc::from([motion])).unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        camera_id,
        SemanticObjectTrackProperty::CameraProfile,
        SemanticObjectTrackValues::CameraProfile {
            from,
            to: finite_endpoint,
            near: 0.1,
            far: 100.0,
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
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let sample = |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;

    for (time, expected) in [
        (0.0, from),
        (
            0.5,
            ManimCamera3DProfile::interpolate(from, finite_endpoint, 0.5).unwrap(),
        ),
        (1.0, finite_endpoint),
        (1.5, finite_endpoint),
        (2.0, finite_endpoint),
        (3.0, ambient_endpoint),
    ] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(sample(&forward), expected, "profile at t={time}");
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
    }
}

#[test]
fn ambient_motion_after_delayed_finite_track_preserves_track_boundaries() {
    let from = ManimCamera3DProfile {
        phi: 0.8,
        theta: -1.2,
        gamma: 0.3,
        focal_distance: 20.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let to = ManimCamera3DProfile {
        zoom: 1.75,
        frame_center: SemanticVec3::new(1.0, -0.5, 0.0),
        ..from
    };
    let motion = CameraAngularMotion::new(
        to,
        CameraRotationAxis::Theta,
        0.6,
        3.0,
        Some(4.0),
        0.1,
        100.0,
    )
    .unwrap();
    let ambient_endpoint = motion.sample(4.0).unwrap();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_profile(ambient_endpoint, 0.1, 100.0)
        .unwrap();
    camera.set_camera_motions(Arc::from([motion])).unwrap();
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        camera_id,
        SemanticObjectTrackProperty::CameraProfile,
        SemanticObjectTrackValues::CameraProfile {
            from,
            to,
            near: 0.1,
            far: 100.0,
        },
        TrackTiming::new(1.0, 1.0, RateFunction::Linear),
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
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut seek = SceneInstance::from_semantic_execution(lowered);
    let sample = |instance: &SceneInstance| instance.frame().objects[0].camera_profile().unwrap().0;
    let midpoint = ManimCamera3DProfile::interpolate(from, to, 0.5).unwrap();

    for (time, expected) in [
        (0.0, from),
        (0.5, from),
        (1.0, from),
        (1.5, midpoint),
        (2.0, to),
        (2.5, to),
        (3.0, to),
        (4.0, ambient_endpoint),
    ] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(sample(&forward), expected, "profile at t={time}");
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
    }

    // The closed interval reaches its endpoint at t=4, then releases to that
    // authored baseline. It must remain stable beyond the close boundary in
    // both incremental evaluation and direct seek.
    for time in [4.0, 4.000_001, 5.0] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(sample(&forward), ambient_endpoint, "profile at t={time}");
        assert_eq!(forward.frame(), seek.frame(), "seek parity at t={time}");
    }
    let stopped_camera = ambient_endpoint.camera(0.1, 100.0).unwrap();
    let stopped_row = &forward.frame().objects[0];
    assert_eq!(
        stopped_row.camera_projection(),
        Some(stopped_camera.projection)
    );
    assert_eq!(
        stopped_row.world_transform(),
        Some(
            SemanticWorldTransform3D::new(
                stopped_camera.position,
                stopped_camera.orientation,
                SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap()
        )
    );
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
fn point_light_world_track_publishes_effective_spatial_state_without_authored_mutation() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let handle = store.insert_geometry_mesh(mesh());

    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(SemanticProjection3D::Orthographic {
            height: 4.0,
            near: 0.1,
            far: 100.0,
        }))
        .unwrap();
    camera.transform.translation.z = 5.0;
    let camera_id = store.insert_semantic_object(camera);
    store.add_semantic_family_member(root, camera_id).unwrap();

    let mut surface = SemanticObjectState::new(StoredGeometry::Resource(handle));
    surface.set_spatial_material(SemanticSpatialMaterial::PointLit);
    let surface_id = store.insert_semantic_object(surface);
    store.add_semantic_family_member(root, surface_id).unwrap();

    let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    light.set_role(SemanticObjectRole::PointLight3D);
    light.transform.translation.z = 2.0;
    let light_id = store.insert_semantic_object(light);
    store.add_semantic_family_member(root, light_id).unwrap();

    let world_pose = |x| {
        SemanticWorldTransform3D::new(
            SemanticVec3::new(x, 0.0, 2.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap()
    };
    let authored_light_pose = world_pose(0.0);
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        light_id,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: authored_light_pose,
            to: world_pose(2.0),
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
    let mut forward = SceneInstance::from_semantic_execution(lowered.clone());
    let mut direct_seek = SceneInstance::from_semantic_execution(lowered);
    let light_row = 2;
    assert_eq!(
        forward.frame().objects[light_row].world_transform(),
        Some(authored_light_pose)
    );
    assert!(forward.frame().objects[light_row]
        .spatial
        .as_deref()
        .is_some_and(|spatial| spatial.point_light));
    forward.take_frame_changes();
    let initial_epoch = forward.publication_context().frame_epoch();

    forward.advance_to(0.5).unwrap();
    direct_seek.seek(0.5).unwrap();
    assert_eq!(forward.frame(), direct_seek.frame());
    assert_ne!(forward.publication_context().frame_epoch(), initial_epoch);
    assert_eq!(
        forward.frame().objects[light_row]
            .world_transform()
            .unwrap()
            .translation
            .x,
        1.0
    );
    assert_eq!(
        store
            .node(light_id)
            .unwrap()
            .semantic_object_state()
            .unwrap()
            .transform
            .world_transform(),
        Some(authored_light_pose)
    );
}

#[test]
fn point_light_and_point_lit_singular_pose_mutations_reject_atomically() {
    let mut store = SemanticStore::new();
    let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    light.set_role(SemanticObjectRole::PointLight3D);
    let light_id = store.insert_semantic_object(light);
    store.attach_semantic_object(light_id).unwrap();
    let original_revision = store.scene_revision();
    let original = store
        .semantic_object_state_checked(light_id)
        .unwrap()
        .clone();

    let mut invalid_light_pose = SemanticMutationTransaction::new();
    invalid_light_pose.set_object_transform(
        light_id,
        SemanticTransform {
            scale: SemanticVec3::new(1.0, 2.0, 1.0),
            ..SemanticTransform::default()
        },
    );
    assert!(invalid_light_pose.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), original_revision);
    assert_eq!(
        store.semantic_object_state_checked(light_id).unwrap(),
        &original
    );

    let mut surface = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    surface.set_spatial_material(SemanticSpatialMaterial::PointLit);
    let surface_id = store.insert_semantic_object(surface);
    store.attach_semantic_object(surface_id).unwrap();
    let before_surface = store
        .semantic_object_state_checked(surface_id)
        .unwrap()
        .clone();
    let revision_before_surface = store.scene_revision();
    let mut singular_surface_pose = SemanticMutationTransaction::new();
    singular_surface_pose.set_object_transform(
        surface_id,
        SemanticTransform {
            scale: SemanticVec3::new(1.0, 0.0, 1.0),
            ..SemanticTransform::default()
        },
    );
    assert!(singular_surface_pose.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision_before_surface);
    assert_eq!(
        store.semantic_object_state_checked(surface_id).unwrap(),
        &before_surface
    );
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
