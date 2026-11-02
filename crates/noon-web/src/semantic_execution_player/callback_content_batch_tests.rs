use super::*;
use noon_core::{GeometryResource, GeometryResourceHandle, HostCallbackId};

fn callback_scene() -> (noon::Scene, SemanticExecutionPlayer, [SemanticNodeId; 3]) {
    let mut scene = noon::Scene::new();
    let mut targets = Vec::new();
    for index in 0..128 {
        let object = scene.circle(0.5).unwrap();
        scene.add(&object).unwrap();
        if index < 3 {
            targets.push(object.node_id());
        }
    }
    let targets: [SemanticNodeId; 3] = targets.try_into().unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    for (index, target) in targets.into_iter().enumerate() {
        callbacks.add_updater(target, HostCallbackId::new(index as u64 + 7), 0.0, None);
    }
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        82,
    )
    .unwrap();
    (scene, player, targets)
}

fn phase(player: &mut SemanticExecutionPlayer, time: f64) -> serde_json::Value {
    let phase = if time == 0.0 {
        player.initial_callback_phase_json().unwrap().unwrap()
    } else {
        player.advance_to_callback_phase(time).unwrap().unwrap()
    };
    serde_json::from_str(&phase).unwrap()
}

fn node(target: SemanticNodeId) -> serde_json::Value {
    serde_json::to_value(CallbackNodeWire::from(target)).unwrap()
}

fn path(width: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(width, 0.0))
        .line_to(Vec2::new(0.0, 1.0))
        .close()
}

fn path_content(target: SemanticNodeId, width: f32) -> serde_json::Value {
    serde_json::json!({
        "object": node(target),
        "path": {"points": [[0.0, 0.0], [width, 0.0], [0.0, 1.0]], "closed": true}
    })
}

fn inline_content(target: SemanticNodeId, radius: f32) -> serde_json::Value {
    serde_json::json!({
        "object": node(target), "geometry": {"kind": "circle", "radius": radius}
    })
}

fn batch(phase: &serde_json::Value, content: Vec<serde_json::Value>) -> String {
    serde_json::json!({
        "token": phase["token"], "region": phase["region"], "writes": [], "content": content
    })
    .to_string()
}

fn geometry_handle(player: &SemanticExecutionPlayer, index: usize) -> GeometryResourceHandle {
    let ObjectContentRef::Geometry(GeometryRef::External(id)) =
        player.session.frame().objects[index].content
    else {
        panic!("callback target must hold retained geometry")
    };
    player
        .session
        .geometry_resources()
        .current_handle(id)
        .unwrap()
}

#[test]
fn callback_retained_paths_and_inline_geometry_publish_atomically_and_reuse_equal_paths() {
    let (scene, mut player, targets) = callback_scene();
    let before_nodes = scene.integration_store().borrow().len();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let before = player.session.publication_context();
    let identities: Vec<_> = player
        .session
        .frame()
        .objects
        .iter()
        .map(|row| row.id)
        .collect();
    player.session.take_frame_changes();
    let initial = phase(&mut player, 0.0);
    assert_eq!(initial["invocations"].as_array().unwrap().len(), 3);
    let contents = || {
        vec![
            path_content(targets[0], 2.0),
            path_content(targets[1], 3.0),
            inline_content(targets[2], 0.75),
        ]
    };
    player
        .commit_callback_phase_json(&batch(&initial, contents()))
        .unwrap();
    let first_handle = geometry_handle(&player, 0);
    let second_handle = geometry_handle(&player, 1);
    assert_ne!(first_handle.id, second_handle.id);
    for (handle, width) in [(first_handle, 2.0), (second_handle, 3.0)] {
        assert!(matches!(
            player.session.geometry_resources().get(handle),
            Some(GeometryResource::VectorPath(value)) if value.as_ref() == &path(width)
        ));
    }
    assert_eq!(
        player.session.frame().objects[2].content,
        ObjectContentRef::Geometry(GeometryRef::circle(0.75))
    );
    assert_eq!(
        player.session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        player.session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        player.session.take_frame_changes().object_indices(),
        &[0, 1, 2]
    );
    assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
    let leases = targets.map(|target| player.session.effective_content_lease(target).unwrap());
    let after_first = player.session.publication_context();

    // A producer may return the same value every phase. Its immutable handle and
    // renderer row stay resident; a later timestamp alone does not upload it.
    let equal = phase(&mut player, 0.25);
    player
        .commit_callback_phase_json(&batch(&equal, contents()))
        .unwrap();
    assert_eq!(geometry_handle(&player, 0), first_handle);
    assert_eq!(geometry_handle(&player, 1), second_handle);
    assert!(player
        .session
        .take_frame_changes()
        .object_indices()
        .is_empty());
    assert_eq!(
        player.session.publication_context().execution_revision(),
        after_first.execution_revision()
    );
    assert_eq!(
        targets.map(|target| player.session.effective_content_lease(target).unwrap()),
        leases
    );

    let changed = phase(&mut player, 0.5);
    player
        .commit_callback_phase_json(&batch(&changed, vec![path_content(targets[0], 4.0)]))
        .unwrap();
    let replacement = geometry_handle(&player, 0);
    assert_ne!(replacement, first_handle);
    assert!(player
        .session
        .geometry_resources()
        .get(first_handle)
        .is_none());
    assert_eq!(geometry_handle(&player, 1), second_handle);
    assert!(player
        .session
        .geometry_resources()
        .get(second_handle)
        .is_some());
    assert_eq!(player.session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
    assert!(player.session.last_spatial_update_stats().leaves_upserted <= 1);

    let retired = phase(&mut player, 0.75);
    player
        .commit_callback_phase_json(&batch(
            &retired,
            vec![
                inline_content(targets[0], 0.5),
                inline_content(targets[1], 0.5),
            ],
        ))
        .unwrap();
    assert!(player
        .session
        .geometry_resources()
        .get(replacement)
        .is_none());
    assert!(player
        .session
        .geometry_resources()
        .get(second_handle)
        .is_none());
    assert_eq!(
        player.session.take_frame_changes().object_indices(),
        &[0, 1]
    );
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        before_resources
    );
    assert_eq!(
        player.session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        player
            .session
            .frame()
            .objects
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        identities
    );
}

#[test]
fn callback_mixed_geometry_late_failure_preserves_frame_resources_and_pending_phase() {
    let (scene, mut player, targets) = callback_scene();
    let initial = phase(&mut player, 0.0);
    let token = player.session.pending_callback_token();
    let before = player.session.publication_context();
    let before_frame = player.session.frame().clone();
    let before_nodes = scene.integration_store().borrow().len();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let unsupported_text = batch(
        &initial,
        vec![
            path_content(targets[0], 2.0),
            serde_json::json!({"object": node(targets[1]), "text_source": node(targets[2])}),
        ],
    );
    let error = player
        .commit_callback_phase_json(&unsupported_text)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("text replacement remains supported for one target"));
    assert_eq!(player.session.pending_callback_token(), token);
    assert_eq!(player.session.publication_context(), before);
    assert_eq!(player.session.frame(), &before_frame);
    let mut invalid_target = inline_content(targets[1], 0.75);
    invalid_target["object"]["generation"] = serde_json::json!(u32::MAX);
    let with_properties = |contents| {
        let mut payload: serde_json::Value =
            serde_json::from_str(&batch(&initial, contents)).unwrap();
        payload["writes"] = serde_json::json!([
            {"kind": "translation", "object": node(targets[0]), "translation": {"x": 9.0, "y": -2.0}}
        ]);
        payload.to_string()
    };
    for (contents, reason) in [
        (
            vec![path_content(targets[0], 2.0), invalid_target],
            "not live in this callback phase",
        ),
        (
            vec![path_content(targets[0], 2.0), path_content(targets[0], 3.0)],
            "more than once",
        ),
    ] {
        let error = player
            .commit_callback_phase_json(&with_properties(contents))
            .unwrap_err();
        assert!(error.to_string().contains(reason), "{error}");
        assert_eq!(player.session.pending_callback_token(), token);
        assert_eq!(player.session.publication_context(), before);
        assert_eq!(player.session.frame(), &before_frame);
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            before_resources
        );
    }
    let valid = vec![
        path_content(targets[0], 2.0),
        path_content(targets[1], 3.0),
        inline_content(targets[2], 0.75),
    ];
    // The terminal property region has already been validated. Its exact
    // staged writes remain pinned while content is corrected; changing those
    // writes is a different region reply and cannot silently replace it.
    assert!(player
        .commit_callback_phase_json(&batch(&initial, valid.clone()))
        .is_err());
    assert_eq!(player.session.frame(), &before_frame);
    player
        .commit_callback_phase_json(&with_properties(valid))
        .unwrap();
    assert!(player.session.pending_callback_token().is_none());
    assert_ne!(
        geometry_handle(&player, 0).id,
        geometry_handle(&player, 1).id
    );
    assert_eq!(
        player.session.frame().objects[0].transform.translation,
        Vec2::new(9.0, -2.0)
    );
}

#[test]
fn callback_provisional_and_direct_paths_preserve_full_presentations_without_authored_growth() {
    let (scene, mut player, targets) = callback_scene();
    let before_nodes = scene.integration_store().borrow().len();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let before = player.session.publication_context();
    player.session.take_frame_changes();
    let mut previous_handles = None;
    for step in 0..2 {
        let phase = phase(&mut player, f64::from(step) * 0.25);
        let token = player.pending_callback_phase.unwrap().0;
        let path_source = player
            .stage_required_callback_provisional_geometry(
                token,
                noon::ManimGeometryOptions::path(path(3.0)).unwrap(),
            )
            .unwrap();
        player
            .stage_required_callback_provisional_shift(token, path_source, 2.0, -1.0)
            .unwrap();
        player
            .stage_required_callback_provisional_fill(
                token,
                path_source,
                [0.2, 0.4, 0.8, 0.75],
                Some(0.6),
            )
            .unwrap();
        let analytic_source = player
            .stage_required_callback_provisional_geometry(
                token,
                noon::ManimGeometryOptions::circle(0.75).unwrap(),
            )
            .unwrap();
        let first = if step == 0 {
            path_content(targets[0], 2.0)
        } else {
            let source = player
                .stage_required_callback_provisional_geometry(
                    token,
                    noon::ManimGeometryOptions::path(path(2.0)).unwrap(),
                )
                .unwrap();
            serde_json::json!({"object": node(targets[0]), "provisional": callback_provisional_key(source)})
        };
        let contents = vec![
            first,
            serde_json::json!({"object": node(targets[1]), "provisional": callback_provisional_key(path_source)}),
            serde_json::json!({"object": node(targets[2]), "provisional": callback_provisional_key(analytic_source)}),
        ];
        let mut invalid = contents.clone();
        invalid[2]["provisional"] = serde_json::json!("stale-path-producer");
        let frame = player.session.frame().clone();
        let publication = player.session.publication_context();
        assert!(player
            .commit_callback_phase_json(&batch(&phase, invalid))
            .is_err());
        assert_eq!(player.session.frame(), &frame);
        assert_eq!(player.session.publication_context(), publication);
        assert!(player.callback_membership_transaction.is_some());
        player
            .commit_callback_phase_json(&batch(&phase, contents))
            .unwrap();
        let handles = [geometry_handle(&player, 0), geometry_handle(&player, 1)];
        if let Some(previous) = previous_handles {
            assert_eq!(handles, previous);
        }
        previous_handles = Some(handles);
        assert_eq!(
            player.session.frame().objects[1].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        let fill = player.session.frame().objects[1].style.fill.unwrap();
        assert_eq!((fill.red, fill.green, fill.blue), (0.2, 0.4, 0.8));
        assert!((fill.alpha - 0.45).abs() < 1e-6);
        assert_eq!(
            player.session.frame().objects[2].content,
            ObjectContentRef::Geometry(GeometryRef::circle(0.75))
        );
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            before_resources
        );
        assert_eq!(
            player.session.publication_context().scene_revision(),
            before.scene_revision()
        );
        assert_eq!(player.session.last_spatial_update_stats().full_rebuilds, 0);
        assert!(player
            .session
            .take_frame_changes()
            .object_indices()
            .iter()
            .all(|index| *index < 3));
        assert!(player.callback_membership_transaction.is_none());
    }
}
