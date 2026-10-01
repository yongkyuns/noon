use super::*;
use crate::{RetainedExecutionFrameMirror, TransportObjectContent};
use noon_core::{
    AnimationOptions, GeometryResourceLookup, HostCallbackId, NativeInputModifiers,
    NativePointerId, NativePointerInput, NativePointerInputKind, NativePointerPosition,
    RateFunction, SemanticClickIndicate,
    SemanticMutationTransaction, SemanticMutationTransactionError, SemanticObjectProperty,
    SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, TrackTiming,
};

#[test]
fn worker_tick_timestamps_drive_actual_transient_click_indicate_samples() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut target_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.8 });
    target_state.transform.translation = SemanticVec3::new(0.0, 0.0, 0.0);
    let target = store.insert_semantic_object(target_state);
    store.add_semantic_family_member(root, target).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_click_indicate(
        target,
        Some(SemanticClickIndicate::new(1.2, noon_core::YELLOW, 0.4)),
    );
    transaction.apply(&mut store).unwrap();

    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.enable_pointer_fill_selection(5.0).unwrap();
    session.disable_pointer_fill_selection().unwrap();
    let pointer = NativePointerId {
        source: 17,
        pointer: 0,
    };
    let token = session.configure_native_pointer_input(pointer, 1).unwrap();
    let position =
        NativePointerPosition::new(noon_core::Vec2::ZERO, noon_core::Vec2::new(300.0, 150.0))
            .unwrap();
    for (sequence, kind) in [
        (
            1,
            NativePointerInputKind::Press {
                position,
                button: 0,
            },
        ),
        (
            2,
            NativePointerInputKind::Release {
                position,
                button: 0,
            },
        ),
    ] {
        session
            .submit_native_pointer_input(
                &token,
                NativePointerInput::new(
                    sequence,
                    token.pointer(),
                    token.context(),
                    NativeInputModifiers::default(),
                    kind,
                ),
            )
            .unwrap();
    }
    assert!(
        session.interactions_active(),
        "click must start the declared transient"
    );
    let store = std::rc::Rc::new(std::cell::RefCell::new(store));
    let mut player =
        SemanticExecutionPlayer::from_live_session(session, store, root, 1.0, 64).unwrap();

    for timestamp_ms in [1_000.0, 1_200.0, 1_400.0] {
        player.tick_callback_phase_json(timestamp_ms).unwrap();
        let scale = player.session.frame().objects[0].transform.scale.x;
        match timestamp_ms {
            1_000.0 => assert!((scale - 1.0).abs() < 1e-5),
            1_200.0 => assert!(
                scale > 1.05,
                "midpoint timestamp must show the transient effect"
            ),
            1_400.0 => assert!(
                (scale - 1.0).abs() < 1e-5,
                "endpoint timestamp must restore the source"
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn callback_json_regions_preserve_native_host_order_and_publish_once() {
    let mut scene = noon::Scene::new();
    let object = scene.circle(1.0).unwrap();
    scene.add(&object).unwrap();
    {
        let mut store = scene.integration_store().borrow_mut();
        let translation = store
            .insert_semantic_input_signal(SemanticVec3::new(4.0, 1.0, 0.0))
            .unwrap();
        let width = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let mut track = SemanticMutationTransaction::new();
        track.add_scalar_signal_track(
            width,
            1.0,
            2.0,
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );
        track.apply(&mut store).unwrap();
        store
            .bind_semantic_signal(
                translation,
                object.node_id(),
                SemanticObjectProperty::Translation,
            )
            .unwrap();
        let mut first = SemanticMutationTransaction::new();
        first.add_updater(object.node_id(), HostCallbackId::new(7), 1.0, None);
        first.apply(&mut store).unwrap();
        store
            .bind_semantic_signal(width, object.node_id(), SemanticObjectProperty::StrokeWidth)
            .unwrap();
        let mut second = SemanticMutationTransaction::new();
        second.add_updater(object.node_id(), HostCallbackId::new(8), 1.0, None);
        second.apply(&mut store).unwrap();
    }
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        64,
    )
    .unwrap();
    let publication = player.session.publication_context();
    let first_json = player.advance_to_callback_phase(1.0).unwrap().unwrap();
    let first: serde_json::Value = serde_json::from_str(&first_json).unwrap();
    assert_eq!(first["region"], 0);
    assert_eq!(first["invocations"][0]["callback_id"], "7");
    assert_eq!(first["objects"][0]["transform"]["translation"]["x"], 4.0);
    assert_eq!(first["objects"][0]["style"]["stroke_width"], 1.0);
    let first_batch = serde_json::json!({
        "token": first["token"], "region": first["region"],
        "writes": [
            {"kind":"translation", "object":first["objects"][0]["node"], "translation":{"x":4.0,"y":9.0}},
            {"kind":"stroke_width", "object":first["objects"][0]["node"], "stroke_width":3.0}
        ]
    }).to_string();
    let premature_content = serde_json::json!({
        "token": first["token"], "region": first["region"], "writes": [],
        "content": {"object": first["objects"][0]["node"],
            "geometry": {"kind": "circle", "radius": 2.0}}
    });
    assert!(player
        .commit_callback_phase_json(&premature_content.to_string())
        .is_err());
    assert_eq!(player.session.publication_context(), publication);
    let second_json = player
        .commit_callback_phase_json(&first_batch)
        .unwrap()
        .unwrap();
    let second: serde_json::Value = serde_json::from_str(&second_json).unwrap();
    assert_eq!(second["region"], 1);
    assert_eq!(second["invocations"][0]["callback_id"], "8");
    assert_eq!(second["objects"][0]["transform"]["translation"]["y"], 9.0);
    assert_eq!(second["objects"][0]["style"]["stroke_width"], 2.0);
    assert_eq!(player.session.publication_context(), publication);
    assert!(player.commit_callback_phase_json(&first_batch).is_err());
    let second_batch = serde_json::json!({
        "token": second["token"], "region": second["region"],
        "writes": [{"kind":"opacity", "object":second["objects"][0]["node"], "opacity":0.7}]
    })
    .to_string();
    assert!(player
        .commit_callback_phase_json(&second_batch)
        .unwrap()
        .is_none());
    assert_eq!(
        player.session.frame().objects[0].transform.translation,
        noon_core::Vec2::new(4.0, 9.0)
    );
    assert_eq!(player.session.frame().objects[0].style.stroke_width, 2.0);
    assert_eq!(player.session.frame().objects[0].style.opacity, 0.7);
    assert_eq!(
        player.session.publication_context().frame_epoch(),
        publication.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn terminal_callback_content_replaces_one_effective_row_across_frames() {
    let mut scene = noon::Scene::new();
    let source = scene.circle(1.0).unwrap();
    let unrelated = scene.circle(0.5).unwrap();
    scene.add(&source).unwrap();
    scene.add(&unrelated).unwrap();
    for _ in 2..600 {
        let static_circle = scene.circle(0.5).unwrap();
        scene.add(&static_circle).unwrap();
    }
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(source.node_id(), HostCallbackId::new(9), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        77,
    )
    .unwrap();
    assert_eq!(player.session.frame().objects.len(), 600);
    player.session.take_frame_changes();
    let before = player.session.publication_context();
    let first: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    assert_eq!(first["objects"].as_array().unwrap().len(), 1);
    let content_batch = |phase: &serde_json::Value, radius: f32| {
        serde_json::json!({
            "token": phase["token"], "region": phase["region"],
            "writes": [{"kind": "translation", "object": phase["objects"][0]["node"],
                "translation": {"x": 3.0, "y": 0.0}}],
            "content": {"object": phase["objects"][0]["node"],
                "geometry": {"kind": "circle", "radius": radius}}
        })
        .to_string()
    };
    let malformed = serde_json::json!({
        "token": first["token"], "region": first["region"], "writes": [],
        "content": {"object": first["objects"][0]["node"],
            "geometry": {"kind": "circle", "radius": "invalid"}}
    });
    assert!(player
        .commit_callback_phase_json(&malformed.to_string())
        .is_err());
    assert_eq!(player.session.publication_context(), before);
    assert!(player.session.pending_callback_token().is_some());
    assert!(player
        .commit_callback_phase_json(&content_batch(&first, 2.0))
        .unwrap()
        .is_none());
    let lease = player
        .session
        .effective_content_lease(source.node_id())
        .unwrap();
    assert_eq!(
        player.session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        player.session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        player.session.frame().objects[0].content,
        ObjectContentRef::Geometry(GeometryRef::circle(2.0))
    );
    assert_eq!(
        player.session.frame().objects[1].content,
        ObjectContentRef::Geometry(GeometryRef::circle(0.5))
    );
    assert_eq!(player.session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
    assert!(player.session.last_spatial_update_stats().leaves_upserted <= 1);

    for step in 1..=32 {
        let time = f64::from(step) / 32.0;
        let radius = 2.0 + (step as f32) / 32.0;
        let next: serde_json::Value =
            serde_json::from_str(&player.advance_to_callback_phase(time).unwrap().unwrap())
                .unwrap();
        assert_eq!(next["objects"].as_array().unwrap().len(), 1);
        assert!(player
            .commit_callback_phase_json(&content_batch(&next, radius))
            .unwrap()
            .is_none());
        assert_eq!(
            player.session.effective_content_lease(source.node_id()),
            Some(lease)
        );
        assert_eq!(player.session.take_frame_changes().object_indices(), &[0]);
        assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
        assert!(player.session.last_spatial_update_stats().leaves_upserted <= 1);
    }
    assert_eq!(
        player.session.frame().objects[0].content,
        ObjectContentRef::Geometry(GeometryRef::circle(3.0))
    );
    assert_eq!(player.session.frame().time, 1.0);
}

#[test]
fn callback_path_replacement_uses_one_lease_and_retires_old_resource() {
    let mut scene = noon::Scene::new();
    let source = scene.circle(1.0).unwrap();
    scene.add(&source).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(source.node_id(), HostCallbackId::new(9), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        80,
    )
    .unwrap();
    let make_batch = |phase: &serde_json::Value, x: f32| {
        serde_json::json!({
            "token": phase["token"], "region": phase["region"], "writes": [],
            "content": {"object": phase["objects"][0]["node"], "path": {
                "points": [[0.0, 0.0], [x, 0.0], [0.0, 1.0]], "closed": true
            }}
        })
        .to_string()
    };
    let first: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let invalid = serde_json::json!({
        "token": first["token"], "region": first["region"], "writes": [],
        "content": {"object": first["objects"][0]["node"], "path": {
            "points": [[0.0, 0.0]], "closed": true
        }}
    });
    assert!(player
        .commit_callback_phase_json(&invalid.to_string())
        .is_err());
    assert!(player.session.pending_callback_token().is_some());
    let missing_producer = serde_json::json!({
        "token": first["token"], "region": first["region"], "writes": [],
        "content": {"object": first["objects"][0]["node"]}
    });
    assert!(player
        .commit_callback_phase_json(&missing_producer.to_string())
        .is_err());
    assert!(player.session.pending_callback_token().is_some());
    player
        .commit_callback_phase_json(&make_batch(&first, 2.0))
        .unwrap();
    let lease = player
        .session
        .effective_content_lease(source.node_id())
        .unwrap();
    let ObjectContentRef::Geometry(GeometryRef::External(first_id)) =
        player.session.frame().objects[0].content
    else {
        panic!("expected external path")
    };
    let first_handle = player
        .session
        .geometry_resources()
        .current_handle(first_id)
        .unwrap();
    assert!(player
        .session
        .geometry_resources()
        .get(first_handle)
        .is_some());

    let second: serde_json::Value =
        serde_json::from_str(&player.advance_to_callback_phase(0.5).unwrap().unwrap()).unwrap();
    assert!(player
        .commit_callback_phase_json(&make_batch(&first, 4.0))
        .is_err());
    player
        .commit_callback_phase_json(&make_batch(&second, 3.0))
        .unwrap();
    assert_eq!(
        player.session.effective_content_lease(source.node_id()),
        Some(lease)
    );
    assert!(player
        .session
        .geometry_resources()
        .get(first_handle)
        .is_none());
    assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
    assert!(player.session.last_spatial_update_stats().leaves_upserted <= 1);
}

#[test]
fn callback_content_does_not_adopt_another_producers_lease() {
    let mut scene = noon::Scene::new();
    let source = scene.circle(1.0).unwrap();
    scene.add(&source).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(source.node_id(), HostCallbackId::new(9), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let replacement = session
        .prepare_effective_content_replacement(
            source.node_id(),
            ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            None,
            None,
        )
        .unwrap();
    let foreign = session
        .commit_effective_content_replacement(replacement)
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        78,
    )
    .unwrap();
    let before = player.session.publication_context();
    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let batch = serde_json::json!({
        "token": phase["token"], "region": phase["region"], "writes": [],
        "content": {"object": phase["objects"][0]["node"],
            "geometry": {"kind": "circle", "radius": 3.0}}
    });
    assert!(player
        .commit_callback_phase_json(&batch.to_string())
        .is_err());
    assert_eq!(player.session.publication_context(), before);
    assert_eq!(
        player.session.effective_content_lease(source.node_id()),
        Some(foreign)
    );
    assert_eq!(
        player.session.frame().objects[0].content,
        ObjectContentRef::Geometry(GeometryRef::circle(2.0))
    );
}

struct NumericRuleBackend;

impl noon::LatexBackend for NumericRuleBackend {
    fn identity(&self) -> &str {
        "semantic-execution-player-numeric-rule-fixture"
    }

    fn format(&self) -> noon::LatexFormat {
        noon::LatexFormat::Preloaded
    }

    fn font(&mut self, _: &str) -> Result<noon::DviFontResource, String> {
        Err("font-free fixture".into())
    }

    fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
        let mut dvi = vec![247, 2];
        for value in [25_400_000u32, 473_628_672, 1000] {
            dvi.extend(value.to_be_bytes());
        }
        dvi.push(0);
        dvi.push(139);
        dvi.extend([0; 44]);
        dvi.push(132);
        dvi.extend(655_360i32.to_be_bytes());
        dvi.extend(327_680i32.to_be_bytes());
        dvi.push(140);
        dvi.push(248);
        dvi.extend([0; 28]);
        dvi.push(249);
        dvi.extend([0; 4]);
        dvi.push(2);
        dvi.extend([223; 4]);
        Ok(dvi)
    }
}

fn callback_batch_with_y_and_opacity(phase: &serde_json::Value) -> String {
    let row = &phase["objects"][0];
    let mut translation = row["transform"]["translation"].clone();
    translation["y"] = serde_json::json!(1.0);
    serde_json::json!({
        "token": phase["token"].clone(),
        "writes": [
            {
                "kind": "translation",
                "object": row["node"].clone(),
                "translation": translation,
            },
            {
                "kind": "opacity",
                "object": row["node"].clone(),
                "opacity": 0.5,
            },
        ],
    })
    .to_string()
}

#[test]
fn live_advance_projection_preserves_clock_frame_and_retry() {
    let mut scene = noon::Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        41,
    )
    .unwrap();
    player.live_wait(0.25).unwrap();
    player.delta(true).unwrap().unwrap();
    let frame = player.debug_frame_json();
    let publication = player.session.publication_context();
    let resources = player.resource_bundle_bytes();
    let authored = object.state().unwrap();
    let clock = player.clock.clone();
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = player.live_advance_segment_to(time).unwrap_err();
        assert_eq!(error.category, "invalid_input");
        assert_eq!(error.code, "advance.evaluation");
        let cause = error.cause.as_ref().unwrap();
        assert_eq!(cause.code, "evaluation.invalid_time");
        assert_eq!(
            cause.message,
            noon_runtime::EvaluationError::InvalidTime(time).to_string()
        );
        assert!(cause.cause.is_none());
        let error = player.live_evaluate(time).unwrap_err();
        assert_eq!(error.category, "invalid_input");
        assert_eq!(error.code, "clock.invalid_scene_time");
        assert_eq!(player.clock, clock);
        assert_eq!(player.debug_frame_json(), frame);
        assert_eq!(player.session.publication_context(), publication);
        assert_eq!(player.resource_bundle_bytes(), resources);
        assert_eq!(object.state().unwrap(), authored);
        assert!(player.delta(false).unwrap().is_none());
    }
    for (time, code) in [
        (-0.25, "clock.invalid_scene_time"),
        (2.0, "clock.time_outside_loop"),
    ] {
        let error = player.live_evaluate(time).unwrap_err();
        assert_eq!(error.category, "invalid_input");
        assert_eq!(error.code, code);
        assert_eq!(player.clock, clock);
        assert_eq!(player.debug_frame_json(), frame);
        assert_eq!(player.session.publication_context(), publication);
        assert!(player.delta(false).unwrap().is_none());
    }
    // Segment advancement clamps, deterministic evaluation can seek backward.
    player.live_advance_segment_to(-1.0).unwrap();
    assert_eq!(player.time(), 0.0);
    player.live_advance_segment_to(0.125).unwrap();
    player.live_advance_segment_to(0.0625).unwrap();
    assert_eq!(player.time(), 0.125);
    player.live_evaluate(0.0625).unwrap();
    assert_eq!(player.time(), 0.0625);
    player.live_advance_segment_to(9.0).unwrap();
    assert_eq!(player.time(), 0.25);
    player.live_complete_segment().unwrap();
    assert_eq!(object.state().unwrap(), authored);
    assert_eq!(player.resource_bundle_bytes(), resources);
}

#[test]
fn live_advance_projection_preserves_callback_guard_and_recovery() {
    let mut scene = noon::Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(object.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        41,
    )
    .unwrap();
    player.live_wait(0.25).unwrap();
    player.delta(true).unwrap().unwrap();
    let resources = player.resource_bundle_bytes();
    let error = player.live_evaluate(0.125).unwrap_err();
    assert_eq!(error.category, "unsupported_operation");
    assert_eq!(error.code, "evaluation.callback_barrier");
    let phase = player.initial_callback_phase_json().unwrap().unwrap();
    let frame = player.debug_frame_json();
    let clock = player.clock.clone();
    let pending = player.pending_callback_phase;
    let publication = player.session.publication_context();
    let error = player.live_evaluate(0.125).unwrap_err();
    assert_eq!(error.category, "pending_work");
    assert_eq!(error.code, "evaluation.callback_pending");
    // Clock admission still precedes the runtime callback guard.
    assert_eq!(
        player.live_evaluate(f64::NAN).unwrap_err().code,
        "clock.invalid_scene_time"
    );
    assert_eq!(player.pending_callback_phase, pending);
    assert_eq!(player.clock, clock);
    assert_eq!(player.debug_frame_json(), frame);
    assert_eq!(player.session.publication_context(), publication);
    assert_eq!(player.resource_bundle_bytes(), resources);
    assert!(player.delta(false).unwrap().is_none());
    let acknowledge = |player: &mut SemanticExecutionPlayer, phase: &str| {
        let phase: serde_json::Value = serde_json::from_str(phase).unwrap();
        player
            .commit_callback_phase_json(
                &serde_json::json!({
                    "token": phase["token"], "writes": [],
                })
                .to_string(),
            )
            .unwrap();
    };
    acknowledge(&mut player, &phase);
    let drive = player.live_drive_segment_to_authored_time(0.25).unwrap();
    acknowledge(&mut player, drive.callback_phase_json.as_ref().unwrap());
    assert!(player
        .live_drive_segment_to_authored_time(0.25)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();
    assert_eq!(player.time(), 0.25);
    assert_eq!(player.resource_bundle_bytes(), resources);
}

#[test]
fn live_transform_projection_preserves_atomic_rejection_and_local_retry() {
    type Edit =
        fn(&mut SemanticExecutionPlayer, &noon::Mobject, f64) -> Result<(), AuthoringFailure>;
    let edits: [(Edit, SemanticObjectProperty); 4] = [
        (
            |player, object, value| player.live_set_translation(object, value, -1.0),
            SemanticObjectProperty::Translation,
        ),
        (
            |player, object, value| player.live_shift(object, value, -1.0),
            SemanticObjectProperty::Translation,
        ),
        (
            |player, object, value| player.live_set_scale(object, value, 0.5),
            SemanticObjectProperty::Scale,
        ),
        (
            |player, object, value| player.live_set_rotation(object, value),
            SemanticObjectProperty::RotationZ,
        ),
    ];
    for (edit, property) in edits {
        let mut scene = noon::Scene::new();
        let object = scene.circle(0.5).unwrap();
        scene.add(&object).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            1.0,
            41,
        )
        .unwrap();
        player.delta(true).unwrap().unwrap();
        let authored = object.state().unwrap();
        let publication = player.session.publication_context();
        let frame = player.debug_frame_json();
        let resources = player.resource_bundle_bytes();
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let error = edit(&mut player, &object, value).unwrap_err();
            assert_eq!(error.category, "invalid_input");
            assert_eq!(error.code, "live.publication");
            let cause = error.cause.as_ref().unwrap();
            assert_eq!(cause.category, "invalid_input");
            assert_eq!(cause.code, "publication.semantic");
            let leaf = cause.cause.as_ref().unwrap();
            assert_eq!(leaf.code, "transaction.non_finite_property_value");
            assert!(leaf.cause.is_none());
            let expected = SemanticMutationTransactionError::NonFinitePropertyValue {
                index: 0,
                object: object.node_id(),
                property,
            };
            assert_eq!(leaf.message, expected.to_string());
            assert_eq!(object.state().unwrap(), authored);
            assert_eq!(player.session.publication_context(), publication);
            assert_eq!(player.debug_frame_json(), frame);
            assert_eq!(player.resource_bundle_bytes(), resources);
            assert!(player.delta(false).unwrap().is_none());
        }
        edit(&mut player, &object, 2.0).unwrap();
        let delta = player.delta(false).unwrap().unwrap();
        assert!(!delta.retained.snapshot);
        assert_eq!(delta.retained.objects.len(), 1);
        assert!(delta.retained.removed_slots.is_empty());
        assert!(player.delta(false).unwrap().is_none());
        assert_eq!(player.resource_bundle_bytes(), resources);
        assert_ne!(object.state().unwrap(), authored);
        assert_ne!(player.session.publication_context(), publication);
    }
}

#[test]
fn live_content_and_observation_errors_keep_atomicity_and_local_retry() {
    let mut scene = noon::Scene::new();
    let target = scene.circle(0.5).unwrap();
    let source = scene.circle(0.75).unwrap();
    let mut other = noon::Scene::new();
    let foreign = other.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        41,
    )
    .unwrap();
    player.delta(true).unwrap().unwrap();
    let authored = target.state().unwrap();
    let original_source = source.state().unwrap();
    let publication = player.session.publication_context();
    let frame = player.debug_frame_json();
    let resources = player.resource_bundle_bytes();
    for (invalid_target, invalid_source) in [(&foreign, &source), (&target, &foreign)] {
        let error = player
            .live_replace_content(invalid_target, invalid_source)
            .unwrap_err();
        assert_eq!(error.category, "foreign_handle");
        assert_eq!(error.code, "live.foreign_store");
        assert_eq!(target.state().unwrap(), authored);
        assert_eq!(source.state().unwrap(), original_source);
        assert_eq!(player.session.publication_context(), publication);
        assert_eq!(player.debug_frame_json(), frame);
        assert_eq!(player.resource_bundle_bytes(), resources);
        assert!(player.delta(false).unwrap().is_none());
    }
    // A valid detached semantic object is not an effective execution row.
    let error = player.live_effective(&source).unwrap_err();
    assert_eq!(error.category, "stale_handle");
    assert_eq!(error.code, "live.publication");
    let cause = error.cause.as_ref().unwrap();
    assert_eq!(cause.code, "publication.unknown_object");
    assert!(cause.cause.is_none());
    assert_eq!(player.session.publication_context(), publication);
    assert_eq!(player.debug_frame_json(), frame);
    assert_eq!(player.resource_bundle_bytes(), resources);
    assert!(player.delta(false).unwrap().is_none());

    player.live_replace_content(&target, &source).unwrap();
    let after = target.state().unwrap();
    assert_eq!(after.content, original_source.content);
    assert_ne!(after.content, authored.content);
    assert_eq!(after.transform, authored.transform);
    assert_eq!(after.style, authored.style);
    assert_eq!(source.state().unwrap(), original_source);
    player.live_effective(&target).unwrap();
    let delta = player.delta(false).unwrap().unwrap();
    assert!(!delta.retained.snapshot);
    assert_eq!(delta.retained.objects.len(), 1);
    assert!(delta.retained.removed_slots.is_empty());
    assert!(player.delta(false).unwrap().is_none());
    assert_eq!(player.resource_bundle_bytes(), resources);
}

#[test]
fn membership_deltas_omit_unchanged_rows_and_preserve_incremental_order() {
    let mut scene = noon::Scene::new();
    let anchor = scene.circle(0.5).unwrap();
    let toggled = scene.circle(1.0).unwrap();
    scene.add(&anchor).unwrap();
    scene.add(&toggled).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        1,
    )
    .unwrap();
    let mut mirror = RetainedExecutionFrameMirror::default();
    let initial = player.delta(true).unwrap().unwrap();
    assert_eq!(initial.retained.objects[1].slot.generation, 0);
    mirror.apply(initial.retained).unwrap();
    player
        .live_edit_membership(noon::SceneMembershipRequest::Remove(&[(&toggled).into()]))
        .unwrap();
    let retired = player.delta(false).unwrap().unwrap();
    assert!(!retired.retained.snapshot);
    assert!(retired.retained.objects.is_empty());
    assert_eq!(retired.retained.removed_slots.len(), 1);
    mirror.apply(retired.retained).unwrap();
    player
        .live_edit_membership(noon::SceneMembershipRequest::Add(&[(&toggled).into()]))
        .unwrap();
    assert!(player.session.execution_slot_for_frame_index(1).is_some());
    let snapshot = player.delta(false).unwrap().unwrap();
    assert!(!snapshot.retained.snapshot);
    assert_eq!(snapshot.retained.objects.len(), 1);
    assert_eq!(snapshot.retained.objects[0].order, 1);
    mirror.apply(snapshot.retained).unwrap();
    player.live_set_translation(&toggled, 2.0, -1.0).unwrap();
    let delta = player.delta(false).unwrap().unwrap();
    assert!(!delta.retained.snapshot);
    assert_eq!(delta.retained.objects.len(), 1);
    assert_eq!(delta.retained.objects[0].order, 1);
    mirror.apply(delta.retained).unwrap();
    assert_eq!(
        mirror.frame().unwrap().objects[1].transform.translation,
        noon_core::Vec2::new(2.0, -1.0)
    );
}

fn animated_player() -> SemanticExecutionPlayer {
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    circle.shift(2.0, -1.0).unwrap();
    circle.scale(1.5, 0.5).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 0.4).unwrap();
    circle.set_stroke_join("miter").unwrap();
    circle.set_stroke_cap("butt").unwrap();
    scene.add(&circle).unwrap();
    let static_circle = scene.circle(0.25).unwrap();
    scene.add(&static_circle).unwrap();
    let mut target = circle.target_editor().unwrap();
    target.shift(4.0, 0.0).unwrap();
    let animation = scene
        .integration_store()
        .borrow_mut()
        .insert_semantic_transform_animation(
            circle.node_id(),
            target.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .activate_animation(
            &scene.integration_store().borrow(),
            animation,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    SemanticExecutionPlayer::from_session(session, 2.0, 42).unwrap()
}

#[test]
fn native_input_codec_reaches_one_session_and_keeps_event_occurrences_ordered() {
    let mut store = SemanticStore::new();
    let opacity = store.insert_semantic_input_signal(0.25_f64).unwrap();
    let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();
    store
        .bind_semantic_native_state_input(
            opacity,
            NativeStateSource::Control {
                name: "opacity".to_owned(),
            },
        )
        .unwrap();
    store
        .bind_semantic_native_event_input(clicks, NativeEventSource::PointerDown { button: 0 })
        .unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(opacity, object, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    store
        .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
        .unwrap();
    let session = ExecutionSession::from_semantic_store(&store).unwrap();
    let mut player = SemanticExecutionPlayer::from_session(session, 2.0, 61).unwrap();

    player
        .set_native_state_input_json(
            r#"{"source":{"kind":"control","name":"opacity"},"value":{"kind":"scalar","value":0.75}}"#,
        )
        .unwrap();
    let event = r#"{"source":{"kind":"pointer_down","button":0}}"#;
    player.emit_native_event_json(event).unwrap();
    player.emit_native_event_json(event).unwrap();

    assert_eq!(player.session.frame().objects[0].style.opacity, 0.75);
    assert_eq!(player.session.frame().objects[0].transform.rotation, 2.0);
    assert_eq!(player.next_native_event_sequence, 2);
}

#[test]
fn rejected_native_input_keeps_frame_and_player_event_sequence_unchanged() {
    let mut store = SemanticStore::new();
    let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();
    store
        .bind_semantic_native_event_input(clicks, NativeEventSource::PointerDown { button: 0 })
        .unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(object, HostCallbackId::new(9), 0.0, None);
    transaction.apply(&mut store).unwrap();
    let session = ExecutionSession::from_semantic_store(&store).unwrap();
    let mut player = SemanticExecutionPlayer::from_session(session, 2.0, 62).unwrap();
    let frame = player.session.frame().clone();

    let error = player
        .emit_native_event_json(r#"{"source":{"kind":"pointer_down","button":0}}"#)
        .unwrap_err();

    assert!(error.contains("unsupported while required callbacks are configured"));
    assert_eq!(player.session.frame(), &frame);
    assert_eq!(player.next_native_event_sequence, 0);
}

#[test]
fn live_segment_wake_drives_one_leased_session_without_a_host_timeline() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(0.4).unwrap();
    scene.add(&circle).unwrap();
    let mut target = circle.target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        63,
    )
    .unwrap();

    let endpoint = player
        .live_declare_and_activate_composition(
            &noon::AnimationCompositionRequest::TransformTo(noon::TransformToRequest::new(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )),
            noon_core::AnimationOptions::new(),
        )
        .unwrap();
    assert_eq!(endpoint, 2.0);
    assert_eq!(
        player.time(),
        0.0,
        "begin must not fast-forward the segment"
    );

    let wake = player.live_segment_wake(1_000.0).unwrap();
    assert_eq!(wake.cadence(), "animation_frame");
    assert_eq!(wake.timer_after_milliseconds(), None);
    assert!(!player
        .live_drive_segment_from_wall_time(2_000.0)
        .unwrap()
        .reached_endpoint());
    assert_eq!(
        player
            .live_effective(&circle)
            .unwrap()
            .transform
            .translation,
        Vec2::new(1.0, -0.5)
    );

    assert!(player
        .live_drive_segment_from_wall_time(4_000.0)
        .unwrap()
        .reached_endpoint());
    assert_eq!(player.time(), endpoint);
    player.live_complete_segment().unwrap();
    assert_eq!(
        player
            .live_effective(&circle)
            .unwrap()
            .transform
            .translation,
        Vec2::new(2.0, -1.0)
    );

    assert_eq!(player.live_wait(1.0).unwrap(), 3.0);
    assert_eq!(player.time(), 2.0, "beginning a wait must not advance it");
    let wait_wake = player.live_segment_wake(5_000.0).unwrap();
    assert_eq!(wait_wake.cadence(), "timer");
    assert_eq!(wait_wake.timer_after_milliseconds(), Some(1_000.0));
    assert!(player
        .live_drive_segment_from_wall_time(6_000.0)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();
    assert_eq!(player.time(), 3.0);
}

#[test]
fn external_authored_samples_are_monotonic_and_reuse_the_live_player() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(0.4).unwrap();
    scene.add(&circle).unwrap();
    let mut target = circle.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        4.0,
        67,
    )
    .unwrap();

    player
        .live_declare_and_activate_composition(
            &noon::AnimationCompositionRequest::TransformTo(noon::TransformToRequest::new(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )),
            noon_core::AnimationOptions::new(),
        )
        .unwrap();
    let midpoint = player.live_drive_segment_to_authored_time(1.25).unwrap();
    assert!(midpoint.callback_phase_json().is_none());
    assert!(!midpoint.reached_endpoint());
    assert_eq!(player.time(), 1.25);

    let frame = player.session.frame().clone();
    let error = player.live_drive_segment_to_authored_time(1.0).unwrap_err();
    // This legacy guard is not a settled R2 producer yet.
    assert_eq!(error.category, "unclassified");
    assert_eq!(error.code, "unclassified");
    assert_eq!(
        error.message,
        "external continuation sample requires time at or after 1.25, got 1"
    );
    assert_eq!(player.session.frame(), &frame);

    assert!(player
        .live_drive_segment_to_authored_time(3.0)
        .unwrap()
        .reached_endpoint());
    assert_eq!(
        player.time(),
        2.0,
        "Rust clamps the external sample at the segment boundary"
    );
    player.live_complete_segment().unwrap();
    assert!(player.live_drive_segment_to_authored_time(3.0).is_err());

    player.live_wait(1.0).unwrap();
    assert!(player
        .live_drive_segment_to_authored_time(3.0)
        .unwrap()
        .reached_endpoint());
    assert_eq!(player.time(), 3.0);
}

#[test]
fn callback_segment_drive_pins_time_until_exact_phase_commit() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(0.4).unwrap();
    scene.add(&circle).unwrap();
    let mut target = circle.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(
        circle.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    transaction.add_updater(circle.node_id(), HostCallbackId::new(8), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        64,
    )
    .unwrap();
    player
        .live_declare_and_activate_composition(
            &noon::AnimationCompositionRequest::TransformTo(noon::TransformToRequest::new(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )),
            noon_core::AnimationOptions::new(),
        )
        .unwrap();

    assert_eq!(
        player.live_segment_wake(1_000.0).unwrap().cadence(),
        "animation_frame"
    );
    let initial = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
    assert!(!initial.reached_endpoint());
    let initial_phase: serde_json::Value =
        serde_json::from_str(&initial.callback_phase_json().unwrap()).unwrap();
    assert_eq!(initial_phase["time"], serde_json::json!(0.0));
    assert_eq!(
        initial_phase["invocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["callback_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["7", "8"]
    );
    assert_eq!(player.time(), 0.0);
    assert!(player.live_drive_segment_from_wall_time(1_000.0).is_err());

    player
        .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&initial_phase))
        .unwrap();
    let ready = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
    assert!(ready.callback_phase_json().is_none());
    assert!(!ready.reached_endpoint());
    assert_eq!(player.time(), 0.0);

    // A mid-segment sample stays pinned while its host callback is
    // outstanding. Retrying the same sample reaches exactly 0.5 rather
    // than charging callback latency into authored time.
    let midpoint = player.live_drive_segment_from_wall_time(1_500.0).unwrap();
    let midpoint_phase: serde_json::Value =
        serde_json::from_str(&midpoint.callback_phase_json().unwrap()).unwrap();
    assert_eq!(midpoint_phase["time"], serde_json::json!(0.5));
    assert_eq!(player.time(), 0.0);
    player
        .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&midpoint_phase))
        .unwrap();
    let ready = player.live_drive_segment_from_wall_time(1_500.0).unwrap();
    assert!(ready.callback_phase_json().is_none());
    assert!(!ready.reached_endpoint());
    assert_eq!(player.time(), 0.5);

    // Simulate an opaque callback host taking 7.5 seconds after the
    // midpoint commit. Reanchoring at actual completion keeps the next
    // 16 ms wake to exactly 16 ms of authored progress.
    let wake = player.reanchor_live_segment_wake(9_000.0).unwrap();
    assert_eq!(wake.cadence(), "animation_frame");
    let after_slow_callback = player.live_drive_segment_from_wall_time(9_016.0).unwrap();
    let after_slow_callback_phase: serde_json::Value =
        serde_json::from_str(&after_slow_callback.callback_phase_json().unwrap()).unwrap();
    assert!((after_slow_callback_phase["time"].as_f64().unwrap() - 0.516).abs() < 1.0e-9);
    assert_eq!(player.time(), 0.5);
    player
        .commit_callback_phase_json(&callback_batch_with_y_and_opacity(
            &after_slow_callback_phase,
        ))
        .unwrap();
    let ready = player.live_drive_segment_from_wall_time(9_016.0).unwrap();
    assert!(ready.callback_phase_json().is_none());
    assert!(!ready.reached_endpoint());
    assert!((player.time() - 0.516).abs() < 1.0e-9);
    player.reanchor_live_segment_wake(12_000.0).unwrap();

    // The endpoint follows the same phase/commit protocol before reporting
    // readiness for completion and source resumption.
    let endpoint = player.live_drive_segment_from_wall_time(12_484.0).unwrap();
    let endpoint_phase: serde_json::Value =
        serde_json::from_str(&endpoint.callback_phase_json().unwrap()).unwrap();
    assert_eq!(endpoint_phase["time"], serde_json::json!(1.0));
    assert!((player.time() - 0.516).abs() < 1.0e-9);
    player
        .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&endpoint_phase))
        .unwrap();
    let ready = player.live_drive_segment_from_wall_time(12_484.0).unwrap();
    assert!(ready.callback_phase_json().is_none());
    assert!(ready.reached_endpoint());
    assert_eq!(player.time(), 1.0);
    player.live_complete_segment().unwrap();
    assert_eq!(
        player.session.frame().objects[0].transform.translation.x,
        2.0
    );
    assert_eq!(
        player.session.frame().objects[0].transform.translation.y,
        1.0
    );
    assert_eq!(player.session.frame().objects[0].style.opacity, 0.5);
}

#[test]
fn shared_authoring_to_transport_preserves_style_and_emits_only_dirty_rows() {
    let mut player = animated_player();
    let mut mirror = RetainedExecutionFrameMirror::default();
    let initial: RetainedExecutionDeltaEnvelope =
        serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    assert_eq!(
        (initial.session, initial.sequence, initial.snapshot),
        (42, 0, true)
    );
    assert_eq!(initial.objects.len(), 2);
    assert_eq!(
        initial.objects[0].transform.translation,
        noon_core::Vec2::new(2.0, -1.0)
    );
    assert_eq!(
        initial.objects[0].style.stroke_join,
        noon_core::StrokeJoin::Miter
    );
    assert_eq!(
        initial.objects[0].style.stroke_cap,
        noon_core::StrokeCap::Butt
    );
    assert_eq!(initial.objects[0].style.fill.unwrap().alpha, 0.4);
    mirror.apply(initial).unwrap();
    player.tick_delta_json(0.0).unwrap();
    let halfway: RetainedExecutionDeltaEnvelope =
        serde_json::from_str(&player.tick_delta_json(500.0).unwrap().unwrap()).unwrap();
    assert!(!halfway.snapshot);
    assert_eq!(halfway.objects.len(), 1);
    assert_eq!(halfway.objects[0].transform.translation.x, 4.0);
    mirror.apply(halfway).unwrap();
    assert_eq!(
        mirror.frame().unwrap().objects[0].transform.translation.x,
        4.0
    );
    let end: RetainedExecutionDeltaEnvelope =
        serde_json::from_str(&player.tick_delta_json(1000.0).unwrap().unwrap()).unwrap();
    assert_eq!(end.objects[0].transform.translation.x, 6.0);
}

#[test]
fn callback_phase_wire_pins_runtime_publication_and_orders_effective_writes() {
    let mut scene = noon::Scene::new();
    let source = scene.circle(1.0).unwrap();
    let drift = scene.circle(0.25).unwrap();
    scene.add(&source).unwrap();
    scene.add(&drift).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(source.node_id(), HostCallbackId::new(9), 0.0, None);
    transaction.add_updater(source.node_id(), HostCallbackId::new(4), 0.0, None);
    transaction.add_updater(drift.node_id(), HostCallbackId::new(2), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        12,
    )
    .unwrap();

    let phase: serde_json::Value = serde_json::from_str(
        &player
            .initial_callback_phase_json()
            .unwrap()
            .expect("time-zero callbacks require one phase"),
    )
    .unwrap();
    assert_eq!(phase["objects"].as_array().unwrap().len(), 2);
    assert_eq!(
        phase["invocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["callback_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["9", "4", "2"]
    );
    for field in ["runtime", "sequence"] {
        assert!(phase["token"][field].is_string());
    }
    for field in ["scene_revision", "execution_revision", "frame_epoch"] {
        assert!(phase["token"]["publication"][field].is_string());
    }
    let pending_wake = player.execution_wake(1_000.0).unwrap();
    assert_eq!(pending_wake.cadence(), "idle");
    assert_eq!(pending_wake.timer_after_milliseconds(), None);

    let source_row = phase["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["node"]["slot"].as_u64() == Some(u64::from(source.node_id().slot())))
        .unwrap();
    let mut source_transform = source_row["transform"].clone();
    source_transform["translation"]["y"] = serde_json::json!(1.0);
    let mut source_style = source_row["style"].clone();
    source_style["opacity"] = serde_json::json!(0.5);
    let batch = serde_json::json!({
        "token": phase["token"].clone(),
        "writes": [
            {
                "kind": "transform",
                "object": source_row["node"].clone(),
                "transform": source_transform,
            },
            {
                "kind": "style",
                "object": source_row["node"].clone(),
                "style": source_style,
            },
        ],
    });
    player
        .commit_callback_phase_json(&batch.to_string())
        .unwrap();
    let committed_wake = player.execution_wake(9_000.0).unwrap();
    assert_eq!(committed_wake.cadence(), "animation_frame");
    assert_eq!(
        player.session.frame().objects[0].transform.translation.y,
        1.0
    );
    assert_eq!(player.session.frame().objects[0].style.opacity, 0.5);
    let publication: serde_json::Value = serde_json::from_str(
        &player
            .drain_renderer_observation_publication_json(
                &phase.to_string(),
                source.node_id().slot(),
                source.node_id().generation(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(publication["delta"]["session"], 12);
    assert_eq!(publication["delta"]["sequence"], 0);
    assert_eq!(publication["observation"]["publication"]["session"], 12);
    assert_eq!(publication["observation"]["publication"]["sequence"], 0);
    assert_eq!(
        publication["observation"]["slot"],
        publication["delta"]["objects"][0]["slot"]
    );
    assert_eq!(
        publication["observation"]["committed"]["transform"]["translation"]["y"],
        1.0
    );
    assert_eq!(publication["observation"]["committed"]["dirty"], "all");
}

#[test]
fn required_callback_membership_publishes_one_existing_handle_edit() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let removed = scene.circle(0.25).unwrap();
    scene.add(&callback_target).unwrap();
    scene.add(&removed).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before = player.session.publication_context();
    player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                [removed.clone()],
            ),
        )
        .unwrap();
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();

    assert!(player.pending_callback_phase.is_none());
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id()]
    );
    let after = player.session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn callback_provisional_geometry_stays_phase_local_until_the_shared_commit() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    scene.add(&callback_target).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before_nodes = scene.integration_store().borrow().len();
    let mut options = noon::ManimGeometryOptions::rectangle(2.0, 1.0).unwrap();
    options.set_translation(3.0, -2.0).unwrap();
    options.set_z_index(4.0).unwrap();
    let local = player
        .stage_required_callback_provisional_geometry(token, options)
        .unwrap();

    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    let state = player
        .callback_provisional_object_state(token, local)
        .unwrap();
    assert!(matches!(
        &state.content,
        noon_core::SemanticObjectContent::Geometry(noon_core::StoredGeometry::Rectangle { .. })
    ));
    assert_eq!(state.transform.translation.x, 3.0);
    assert_eq!(state.transform.translation.y, -2.0);
    assert_eq!(state.z_index(), 4.0);
    player
        .stage_required_callback_provisional_shift(token, local, 0.5, 1.5)
        .unwrap();
    player
        .stage_required_callback_provisional_fill(token, local, [0.2, 0.4, 0.8, 0.75], Some(0.6))
        .unwrap();
    assert_eq!(
        player.callback_provisional_center(token, local).unwrap(),
        (3.5, -0.5)
    );
    let styled = player
        .callback_provisional_object_state(token, local)
        .unwrap();
    assert!(matches!(
        styled.style.fill,
        Some(noon_core::SemanticPaint::Solid(color))
            if color == noon_core::Color::rgba(0.2, 0.4, 0.8, 0.75)
    ));
    assert_eq!(styled.style.fill_opacity, 0.6);
    let retained_path = player
        .stage_required_callback_provisional_geometry(
            token,
            noon::ManimGeometryOptions::path(
                noon_core::VectorPath::new()
                    .move_to(noon_core::Vec2::new(-0.5, 0.0))
                    .line_to(noon_core::Vec2::new(0.5, 0.0))
                    .line_to(noon_core::Vec2::new(0.0, 0.6)),
            )
            .unwrap(),
        )
        .unwrap();
    player
        .stage_required_callback_provisional_shift(token, retained_path, 1.5, 0.5)
        .unwrap();
    let center = player
        .callback_provisional_center(token, retained_path)
        .unwrap();
    assert!((center.0 - 1.5).abs() < 1e-6 && (center.1 - 0.8).abs() < 1e-6);
    player
        .stage_required_callback_mixed_addition(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                [],
                token,
                local,
            ),
        )
        .unwrap();
    player
        .stage_required_callback_mixed_addition(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                [],
                token,
                retained_path,
            ),
        )
        .unwrap();
    let duplicate = player
        .stage_required_callback_mixed_addition(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                [],
                token,
                local,
            ),
        )
        .unwrap_err();
    assert_eq!(duplicate.category, "invalid_input");

    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();
    assert_eq!(scene.integration_store().borrow().len(), before_nodes + 2);
    let resolved_path = player
        .take_committed_callback_provisional(token, retained_path)
        .unwrap();
    {
        let store = scene.integration_store().borrow();
        assert_eq!(store.geometry_resources().len(), 1);
        let noon_core::SemanticObjectContent::Geometry(noon_core::StoredGeometry::Resource(handle)) =
            store
                .semantic_object_state_checked(resolved_path)
                .unwrap()
                .content
        else {
            panic!("retained path must publish its resource");
        };
        assert!(store.geometry_resources().get(handle).is_some());
        assert!(player
            .session
            .effective_semantic_object(&store, resolved_path)
            .is_ok());
    }
    let resolved = player
        .take_committed_callback_provisional(token, local)
        .unwrap();
    let repeated = player
        .take_committed_callback_provisional(token, local)
        .unwrap_err();
    assert_eq!(repeated.category, "stale_publication");
    assert_eq!(repeated.code, "callback.stale_provisional");
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id(), resolved, resolved_path]
    );
    assert!(player
        .callback_provisional_object_state(token, local)
        .is_err());
}

#[test]
fn callback_provisional_add_preserves_interleaved_source_order() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let existing = scene.circle(0.25).unwrap();
    scene.add(&callback_target).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let first = player
        .stage_required_callback_provisional_geometry(
            token,
            noon::ManimGeometryOptions::circle(0.5).unwrap(),
        )
        .unwrap();
    let last = player
        .stage_required_callback_provisional_geometry(
            token,
            noon::ManimGeometryOptions::circle(0.75).unwrap(),
        )
        .unwrap();
    use crate::canonical_authoring_scene::CallbackMembershipTestMember::{Existing, Provisional};
    player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_ordered(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                [
                    Provisional {
                        token,
                        local: first,
                    },
                    Existing(existing.clone()),
                    Provisional { token, local: last },
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        player.callback_membership_root_keys(token).unwrap(),
        vec![
            format!(
                "{}:{}",
                callback_target.node_id().slot(),
                callback_target.node_id().generation()
            ),
            callback_provisional_key(first),
            format!(
                "{}:{}",
                existing.node_id().slot(),
                existing.node_id().generation()
            ),
            callback_provisional_key(last),
        ]
    );
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();
    let first = player
        .take_committed_callback_provisional(token, first)
        .unwrap();
    let last = player
        .take_committed_callback_provisional(token, last)
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id(), first, existing.node_id(), last]
    );
}

#[test]
fn unadmitted_callback_provisional_is_canceled_before_the_final_publication() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    scene.add(&callback_target).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before_nodes = scene.integration_store().borrow().len();
    let local = player
        .stage_required_callback_provisional_geometry(
            token,
            noon::ManimGeometryOptions::circle(0.5).unwrap(),
        )
        .unwrap();
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert!(player
        .take_committed_callback_provisional(token, local)
        .is_err());
}

#[test]
fn rejected_callback_membership_keeps_the_pending_phase_retryable() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let member = scene.circle(0.25).unwrap();
    scene.add(&callback_target).unwrap();
    scene.add(&member).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    player.initial_callback_phase_json().unwrap().unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before = player.session.publication_context();
    let error = player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                [member.clone(), member.clone()],
            ),
        )
        .unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert_eq!(error.code, "membership.duplicate_target");
    assert_eq!(player.pending_callback_phase.unwrap().0, token);
    assert_eq!(player.session.publication_context(), before);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id(), member.node_id()]
    );
}

#[test]
fn callback_membership_collector_orders_multiple_edits_with_one_effective_publication() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let first = scene.circle(0.25).unwrap();
    let second = scene.circle(0.5).unwrap();
    for member in [&callback_target, &first, &second] {
        scene.add(member).unwrap();
    }
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before = player.session.publication_context();
    for batch in [
        crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
            crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
            [first.clone()],
        ),
        crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
            crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
            [first.clone()],
        ),
    ] {
        player
            .stage_required_callback_membership(token, &batch)
            .unwrap();
    }
    assert_eq!(
        player.callback_membership_root_keys(token).unwrap(),
        vec![
            format!(
                "{}:{}",
                callback_target.node_id().slot(),
                callback_target.node_id().generation()
            ),
            format!(
                "{}:{}",
                second.node_id().slot(),
                second.node_id().generation()
            ),
            format!(
                "{}:{}",
                first.node_id().slot(),
                first.node_id().generation()
            ),
        ]
    );
    let target_row = phase["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["node"]["slot"].as_u64() == Some(u64::from(callback_target.node_id().slot()))
        })
        .unwrap();
    let mut transform = target_row["transform"].clone();
    transform["translation"]["x"] = serde_json::json!(2.0);
    player
        .commit_callback_phase_json(
            &serde_json::json!({
                "token": phase["token"].clone(),
                "writes": [{
                    "kind": "transform",
                    "object": target_row["node"].clone(),
                    "transform": transform,
                }],
            })
            .to_string(),
        )
        .unwrap();

    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id(), second.node_id(), first.node_id()]
    );
    assert_eq!(
        player.session.frame().objects[0].transform.translation.x,
        2.0
    );
    let after = player.session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn caught_callback_membership_error_retains_earlier_staged_edits() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let member = scene.circle(0.25).unwrap();
    scene.add(&callback_target).unwrap();
    scene.add(&member).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();
    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;

    player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                [member.clone()],
            ),
        )
        .unwrap();
    let error = player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                [callback_target.clone(), callback_target.clone()],
            ),
        )
        .unwrap_err();
    assert_eq!(error.code, "membership.duplicate_target");
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id()]
    );
}

#[test]
fn rejected_final_callback_membership_commit_is_terminal_and_keeps_both_states_unchanged() {
    let mut scene = noon::Scene::new();
    let callback_target = scene.circle(1.0).unwrap();
    let removed = scene.circle(0.25).unwrap();
    scene.add(&callback_target).unwrap();
    scene.add(&removed).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(
        callback_target.node_id(),
        noon_core::HostCallbackId::new(7),
        0.0,
        None,
    );
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        31,
    )
    .unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let token = player.pending_callback_phase.unwrap().0;
    let before_context = player.session.publication_context();
    let before_frame = player.session.frame().clone();
    player
        .stage_required_callback_membership(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                [removed.clone()],
            ),
        )
        .unwrap();
    let retained_path = player
        .stage_required_callback_provisional_geometry(
            token,
            noon::ManimGeometryOptions::path(
                noon_core::VectorPath::new()
                    .move_to(noon_core::Vec2::ZERO)
                    .line_to(noon_core::Vec2::ONE),
            )
            .unwrap(),
        )
        .unwrap();
    player
        .stage_required_callback_mixed_addition(
            token,
            &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                [],
                token,
                retained_path,
            ),
        )
        .unwrap();

    let target_row = phase["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["node"]["slot"].as_u64() == Some(u64::from(callback_target.node_id().slot()))
        })
        .unwrap();
    let mut unknown = target_row["node"].clone();
    unknown["slot"] = serde_json::json!(u32::MAX);
    let error = player
        .commit_callback_phase_json(
            &serde_json::json!({
                "token": phase["token"].clone(),
                "writes": [{
                    "kind": "transform",
                    "object": unknown,
                    "transform": target_row["transform"].clone(),
                }],
            })
            .to_string(),
        )
        .unwrap_err();
    assert_eq!(error.category, "stale_handle");
    assert_eq!(error.code, "callback.unknown_object");
    assert!(player.pending_callback_phase.is_none());
    assert!(player.callback_membership_transaction.is_none());
    assert_eq!(player.session.publication_context(), before_context);
    assert_eq!(player.session.frame(), &before_frame);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        0
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![callback_target.node_id(), removed.node_id()]
    );
    assert!(player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .is_err());
}

#[test]
fn callback_sparse_read_accepts_the_raw_pending_token_and_rejects_a_foreign_one() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        13,
    )
    .unwrap();

    let phase: serde_json::Value = serde_json::from_str(
        &player
            .initial_callback_phase_json()
            .unwrap()
            .expect("time-zero callbacks require one phase"),
    )
    .unwrap();
    let raw_token = phase["token"].to_string();
    let object_request = serde_json::json!({
        "kind": "object",
        "node": phase["objects"][0]["node"].clone(),
    })
    .to_string();

    let response: serde_json::Value = serde_json::from_str(
        &player
            .required_callback_read_json(&raw_token, &object_request)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["kind"], "object");
    assert_eq!(response["object"]["node"], phase["objects"][0]["node"]);
    assert!(player.pending_callback_phase.is_some());

    let mut foreign_token = phase["token"].clone();
    foreign_token["sequence"] = serde_json::json!("999");
    assert!(player
        .required_callback_read_json(&foreign_token.to_string(), &object_request)
        .is_err());
    assert!(player.pending_callback_phase.is_some());
}

#[test]
fn interrupted_callback_phase_stays_terminal_after_player_recovery() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        13,
    )
    .unwrap();
    let phase = player.initial_callback_phase_json().unwrap().unwrap();
    assert_eq!(player.playback_time_at(50_000.0).unwrap(), player.time());
    player.interrupt_callback_phase_json(&phase).unwrap();
    assert_eq!(player.playback_time_at(60_000.0).unwrap(), player.time());
    let termination: serde_json::Value =
        serde_json::from_str(&player.callback_termination_json().unwrap().unwrap()).unwrap();
    assert_eq!(termination["kind"], "interrupted");
    assert!(player.tick_callback_phase_json(16.0).is_err());
}

#[test]
fn forward_callback_control_uses_authored_time_without_a_browser_timestamp() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        14,
    )
    .unwrap();

    let initial: serde_json::Value = serde_json::from_str(
        &player
            .initial_callback_phase_json()
            .unwrap()
            .expect("time-zero callback phase"),
    )
    .unwrap();
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": initial["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();

    let phase: serde_json::Value = serde_json::from_str(
        &player
            .advance_forward_to_callback_phase_json(1.0)
            .unwrap()
            .expect("active callback requires one authored-time phase"),
    )
    .unwrap();
    assert_eq!(phase["time"], serde_json::json!(1.0));
    player
        .commit_callback_phase_json(
            &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
        )
        .unwrap();
    assert_eq!(player.time(), 1.0);
    assert!(player.advance_forward_to_callback_phase_json(0.5).is_err());
}

#[test]
fn invalid_controls_leave_the_clock_frame_and_delta_sequence_unchanged() {
    let mut player = animated_player();
    player.initial_delta_json().unwrap();
    assert!(player.seek_delta_json(f64::NAN).is_err());
    assert!(player.tick_delta_json(f64::INFINITY).is_err());
    assert_eq!(player.time(), 0.0);
    let delta: RetainedExecutionDeltaEnvelope =
        serde_json::from_str(&player.seek_delta_json(0.5).unwrap().unwrap()).unwrap();
    assert_eq!(delta.sequence, 1);
    assert_eq!(delta.objects[0].transform.translation.x, 4.0);
}

#[test]
fn unchanged_static_ticks_do_not_retransmit_geometry() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 1).unwrap();
    player.initial_delta_json().unwrap();
    assert!(player.tick_delta_json(0.0).unwrap().is_none());
    assert!(player.tick_delta_json(500.0).unwrap().is_none());
    assert_eq!(player.time(), 0.5);
}

#[test]
fn generic_wake_settles_a_playing_static_session() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 1).unwrap();
    player.initial_delta_json().unwrap();

    let wake = player.execution_wake(8_000.0).unwrap();
    assert!(player.is_playing());
    assert_eq!(wake.cadence(), "idle");
    assert_eq!(wake.timer_after_milliseconds(), None);
    assert!(!wake.present_now());
    assert!(!player.session.has_replay_timeline_work());
}

#[test]
fn generic_wake_uses_runtime_activity_then_the_real_loop_boundary() {
    let mut player = animated_player();
    player.initial_delta_json().unwrap();
    assert!(player.session.has_replay_timeline_work());

    let active = player.execution_wake(10_000.0).unwrap();
    assert_eq!(active.cadence(), "animation_frame");
    assert!(player.tick_callback_phase_json(11_000.0).unwrap().is_none());
    player.drain_delta_json().unwrap();

    let settled = player.execution_wake(11_250.0).unwrap();
    assert_eq!(settled.cadence(), "timer");
    assert_eq!(settled.timer_after_milliseconds(), Some(750.0));

    let overdue = player.execution_wake(12_100.0).unwrap();
    assert_eq!(overdue.cadence(), "timer");
    assert_eq!(overdue.timer_after_milliseconds(), Some(0.0));
    assert!(player.tick_callback_phase_json(12_100.0).unwrap().is_none());
    let replaying = player.execution_wake(12_100.0).unwrap();
    assert_eq!(replaying.cadence(), "animation_frame");
}

#[test]
fn wait_observations_advance_without_runtime_frames_or_publications() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(0.4).unwrap();
    scene.add(&circle).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        3.0,
        71,
    )
    .unwrap();
    player.initial_delta_json().unwrap();
    player.live_wait(2.0).unwrap();
    assert_eq!(player.playback_time_at(900.0).unwrap(), 0.0);
    let wake = player.live_segment_wake(1_000.0).unwrap();
    assert_eq!(wake.cadence(), "timer");
    let frame = player.session.frame().clone();
    let clock = player.clock.clone();
    for (wall, elapsed) in [
        (1_250.0, 0.25),
        (1_500.0, 0.5),
        (2_500.0, 1.5),
        (4_000.0, 2.0),
    ] {
        assert_eq!(player.playback_time_at(wall).unwrap(), elapsed);
        assert_eq!(player.session.frame(), &frame);
        assert_eq!(player.clock, clock);
        assert!(player.drain_delta_json().unwrap().is_none());
    }
    assert!(player.playback_time_at(f64::NAN).is_err());
    assert!(player
        .live_drive_segment_from_wall_time(3_000.0)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();
    assert_eq!(player.time(), 2.0);
    player.live_wait(1.0).unwrap();
    player.live_segment_wake(8_000.0).unwrap();
    assert_eq!(player.playback_time_at(8_500.0).unwrap(), 2.5);
    assert_eq!(player.playback_time_at(10_000.0).unwrap(), 3.0);
    player.live_drive_segment_to_authored_time(3.0).unwrap();
    player.live_complete_segment().unwrap();
    player.drain_delta_json().unwrap();
    // Pure waits still have a replay clock, even with zero animation tracks.
    assert!(!player.session.has_replay_timeline_work());
    player.seek_delta_json(0.0).unwrap();
    player.resume();
    assert_eq!(player.execution_wake(20_000.0).unwrap().cadence(), "timer");
    assert_eq!(player.playback_time_at(20_500.0).unwrap(), 0.5);
    assert_eq!(player.time(), 0.0);
}

#[test]
fn replay_wait_observation_is_loop_bounded_and_does_not_advance_active_animation() {
    let mut player = animated_player();
    player.initial_delta_json().unwrap();
    player.execution_wake(10_000.0).unwrap();
    assert_eq!(player.playback_time_at(10_500.0).unwrap(), 0.0);
    player.tick_callback_phase_json(11_000.0).unwrap();
    player.drain_delta_json().unwrap();
    let frame = player.session.frame().clone();
    let clock = player.clock.clone();
    assert_eq!(player.playback_time_at(11_250.0).unwrap(), 1.25);
    assert_eq!(player.playback_time_at(11_750.0).unwrap(), 1.75);
    assert_eq!(player.playback_time_at(12_500.0).unwrap(), 2.0);
    assert_eq!(player.session.frame(), &frame);
    assert_eq!(player.clock, clock);
    assert!(player.drain_delta_json().unwrap().is_none());
    player.pause();
    assert_eq!(player.playback_time_at(20_000.0).unwrap(), 1.0);
    player.resume();
    player.execution_wake(30_000.0).unwrap();
    assert_eq!(player.playback_time_at(30_250.0).unwrap(), 1.25);
}

#[test]
fn generic_wake_suppresses_timeline_cadence_while_paused() {
    let mut player = animated_player();
    player.pause();
    let paused = player.execution_wake(1_000.0).unwrap();
    assert_eq!(paused.cadence(), "idle");

    player.resume();
    let resumed = player.execution_wake(9_000.0).unwrap();
    assert_eq!(resumed.cadence(), "animation_frame");
    assert!(player.tick_callback_phase_json(9_250.0).unwrap().is_none());
    assert_eq!(player.time(), 0.25);
}

#[test]
fn opaque_callback_history_is_explicitly_non_looping() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction.remove_updater(circle.node_id(), HostCallbackId::new(1), 0.5);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 7).unwrap();

    assert_eq!(player.clock.loop_duration(), None);
    assert!(!player.session.has_replay_timeline_work());
    let before = player.clock.clone();
    assert!(player.set_loop_duration(2.0).is_err());
    assert_eq!(player.clock, before);
}

#[test]
fn text_created_after_empty_wait_publishes_resources_once_on_admission() {
    assert_late_text_resource_admission(|player| player.live_create_text(noon::Text::new("LATE")));
    assert_late_text_resource_admission(|player| {
        player.live_create_typst(noon::Typst::new("LATE"))
    });
    assert_late_text_resource_admission(|player| {
        player.live_create_math_typst(noon::MathTypst::new("x^2"))
    });
}

#[test]
fn live_variable_tracker_update_replaces_text_through_sparse_resource_delta() {
    let mut backend = NumericRuleBackend;
    let scene = noon::Scene::new();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        82,
    )
    .unwrap();
    let mut mirror =
        crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
            .unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    mirror.apply_family(initial).unwrap();

    player.live_wait(0.5).unwrap();
    player.live_drive_segment_to_authored_time(0.5).unwrap();
    player.live_complete_segment().unwrap();
    if let Some(wait_delta) = player.delta(false).unwrap() {
        mirror.apply_family(wait_delta).unwrap();
    }

    let variable = player
        .with_live_session(|live| {
            live.create_variable(
                &mut backend,
                "x",
                1.25,
                noon::DecimalFormat::default(),
                48.0,
            )
        })
        .unwrap();
    player
        .with_live_session(|live| {
            live.add_many(&[noon::MobjectTarget::Family(variable.family())])
                .map(|_| ())
        })
        .unwrap();
    let admitted = player.delta(false).unwrap().unwrap();
    let admitted_object_count = admitted.retained.objects.len();
    assert!(admitted.resource_additions.is_some());
    mirror.apply_family(admitted).unwrap();

    player.live_set_signal(variable.tracker(), 7.5).unwrap();
    let update = player.delta(false).unwrap().unwrap();
    assert!(!update.retained.snapshot);
    assert!(update.retained.objects.len() < admitted_object_count);
    assert!(update
        .retained
        .objects
        .iter()
        .any(|object| matches!(object.content, TransportObjectContent::Text { .. })));
    assert_eq!(update.resource_additions.as_ref().unwrap().text_count(), 1);

    mirror.apply_family(update).unwrap();
    assert!(mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .filter_map(|object| object.text())
        .any(|handle| {
            mirror
                .resources()
                .texts()
                .get(handle)
                .is_some_and(|resource| resource.source.as_ref() == "7.50")
        }));
}

fn assert_late_text_resource_admission(
    create: impl FnOnce(&mut SemanticExecutionPlayer) -> Result<noon::Mobject, AuthoringFailure>,
) {
    let scene = noon::Scene::new();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        81,
    )
    .unwrap();
    let mut mirror =
        crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
            .unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    assert!(initial.retained.objects.is_empty());
    assert!(initial.resource_additions.is_none());
    mirror.apply_family(initial).unwrap();

    player.live_wait(0.5).unwrap();
    player.live_drive_segment_to_authored_time(0.5).unwrap();
    player.live_complete_segment().unwrap();
    if let Some(wait_delta) = player.delta(false).unwrap() {
        assert!(wait_delta.retained.objects.is_empty());
        assert!(wait_delta.resource_additions.is_none());
        mirror.apply_family(wait_delta).unwrap();
    }
    let label = create(&mut player).unwrap();
    assert!(!player.live_contains(&label).unwrap());
    if let Some(detached_delta) = player.delta(false).unwrap() {
        assert!(detached_delta.retained.objects.is_empty());
        assert!(detached_delta.resource_additions.is_none());
        mirror.apply_family(detached_delta).unwrap();
    }

    assert_eq!(
        player
            .live_declare_and_activate_composition(
                &noon::AnimationCompositionRequest::Fade {
                    target: &label,
                    direction: noon_core::SemanticFadeDirection::In,
                    endpoint: noon::FadeEndpoint::default(),
                    options: AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                },
                noon_core::AnimationOptions::new()
            )
            .unwrap(),
        1.5,
    );
    let admitted = player.delta(false).unwrap().unwrap();
    assert!(!admitted.retained.snapshot);
    assert_eq!(admitted.retained.objects.len(), 1);
    let additions = admitted.resource_additions.as_ref().unwrap();
    assert_eq!(additions.text_count(), 1);
    assert!(additions.font_count() + additions.geometry_count() > 0);
    mirror.apply_family(admitted).unwrap();
    let installed = mirror.frame().unwrap().objects[0].text().unwrap();
    assert!(mirror.resources().texts().get(installed).is_some());
    assert!(player.delta(false).unwrap().is_none());

    player.live_drive_segment_to_authored_time(1.5).unwrap();
    player.live_complete_segment().unwrap();
    let completed = player.delta(false).unwrap().unwrap();
    assert!(completed.resource_additions.is_none());
    mirror.apply_family(completed).unwrap();
    assert_eq!(mirror.frame().unwrap().objects[0].text(), Some(installed));
    assert!(player.delta(false).unwrap().is_none());
}

#[test]
fn shared_session_text_uses_the_mixed_resource_boundary() {
    let mut scene = noon::Scene::new();
    let label = scene
        .text(noon::Text::new("Noon").with_font_size(48.0))
        .unwrap();
    scene.add(&label).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 8).unwrap();

    let bundle = RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes()).unwrap();
    assert_eq!(bundle.text_count(), 1);
    let initial: RetainedExecutionDeltaEnvelope =
        serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    assert!(matches!(
        initial.objects[0].content,
        TransportObjectContent::Text { .. }
    ));
}

#[cfg(test)]
mod callback_provisional_stage_limit_regression {
    use super::*;

    #[test]
    fn provisional_updates_are_bounded_and_rejection_keeps_the_prior_prefix() {
        let mut scene = noon::Scene::new();
        let callback_target = scene.circle(1.0).unwrap();
        scene.add(&callback_target).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(
            callback_target.node_id(),
            noon_core::HostCallbackId::new(7),
            0.0,
            None,
        );
        callbacks
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            scene.execution_session().unwrap(),
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            1.0,
            31,
        )
        .unwrap();

        let phase: serde_json::Value =
            serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
        let token = player.pending_callback_phase.unwrap().0;
        let local = player
            .stage_required_callback_provisional_geometry(
                token,
                noon::ManimGeometryOptions::circle(0.5).unwrap(),
            )
            .unwrap();
        for _ in 0..usize::from(MAX_CALLBACK_MEMBERSHIP_STAGES - 1) {
            player
                .stage_required_callback_provisional_shift(token, local, 1.0, 0.0)
                .unwrap();
        }
        let rejection = player
            .stage_required_callback_provisional_shift(token, local, 1.0, 0.0)
            .unwrap_err();
        assert!(rejection.message.contains("bounded operation limit"));
        assert_eq!(
            player.callback_provisional_center(token, local).unwrap(),
            (f64::from(MAX_CALLBACK_MEMBERSHIP_STAGES - 1), 0.0)
        );

        player
            .stage_required_callback_mixed_addition(
                token,
                &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                    crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                    [],
                    token,
                    local,
                ),
            )
            .unwrap_err();
        // The final operation is rejected too: creation plus 127 updates has
        // exhausted the collector budget without changing its retained prefix.
        player
            .commit_callback_phase_json(
                &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
            )
            .unwrap();
    }
}

#[cfg(test)]
mod callback_mixed_provisional_membership_regression {
    use super::*;

    #[test]
    fn rejected_mixed_add_keeps_prior_callback_membership_and_admits_nothing_from_the_batch() {
        let mut scene = noon::Scene::new();
        let callback_target = scene.circle(1.0).unwrap();
        let prior_member = scene.circle(0.25).unwrap();
        scene.add(&callback_target).unwrap();
        scene.add(&prior_member).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(
            callback_target.node_id(),
            noon_core::HostCallbackId::new(7),
            0.0,
            None,
        );
        callbacks
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            scene.execution_session().unwrap(),
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            1.0,
            31,
        )
        .unwrap();
        let phase: serde_json::Value =
            serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
        let token = player.pending_callback_phase.unwrap().0;

        player
            .stage_required_callback_membership(
                token,
                &crate::canonical_authoring_scene::SceneMembershipBatch::callback_existing(
                    crate::canonical_authoring_scene::SceneMembershipBatchKind::Remove,
                    [prior_member.clone()],
                ),
            )
            .unwrap();
        let local = player
            .stage_required_callback_provisional_geometry(
                token,
                noon::ManimGeometryOptions::circle(0.5).unwrap(),
            )
            .unwrap();
        let mut foreign_scene = noon::Scene::new();
        let foreign = foreign_scene.circle(0.75).unwrap();
        let error = player
            .stage_required_callback_mixed_addition(
                token,
                &crate::canonical_authoring_scene::SceneMembershipBatch::callback_with_provisional(
                    crate::canonical_authoring_scene::SceneMembershipBatchKind::Add,
                    [callback_target.clone(), foreign],
                    token,
                    local,
                ),
            )
            .unwrap_err();
        assert_eq!(error.category, "foreign_handle");
        assert_eq!(
            player.callback_membership_root_keys(token).unwrap(),
            vec![format!(
                "{}:{}",
                callback_target.node_id().slot(),
                callback_target.node_id().generation()
            )]
        );

        player
            .commit_callback_phase_json(
                &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
            )
            .unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(scene.root())
                .unwrap(),
            vec![callback_target.node_id()]
        );
        assert!(player
            .take_committed_callback_provisional(token, local)
            .is_err());
    }
}
