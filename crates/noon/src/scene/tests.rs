use super::*;
use noon_core::{AnimationOptions, RateFunction};

#[test]
fn ordinary_transform_preflight_is_read_only_and_shares_affine_payload_validation() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut affine_target = circle.target_editor().unwrap();
    affine_target.set_translation(2.0, 0.0).unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &affine_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    for rate_func in [None, Some(RateFunction::Smooth)] {
        let mut smooth = options;
        smooth.rate_func = rate_func;
        assert!(scene
            .can_ordinary_transform_to(&circle, &affine_target, smooth)
            .unwrap());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    let mut style_target = circle.target_editor().unwrap();
    style_target.set_fill_opacity(0.5).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );

    style_target.set_stroke_cap("round").unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(!scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );

    let foreign = Scene::new().circle(1.0).unwrap();
    assert!(scene
        .can_ordinary_transform_to(&circle, &foreign, options)
        .is_err());
    scene
        .integration_store()
        .borrow_mut()
        .remove_node(style_target.node_id())
        .unwrap();
    assert!(scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .is_err());
}

#[test]
fn membership_preserves_identity_isolates_roots_and_rejects_foreign_stores() {
    let store = Rc::new(RefCell::new(SemanticStore::new()));
    let mut first = Scene::with_integration_store(Rc::clone(&store));
    let mut second = Scene::with_integration_store(Rc::clone(&store));
    let object = first.circle(1.0).unwrap();
    let id = object.node_id();
    first.add(&object).unwrap();
    first.add(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    second.add(&object).unwrap();
    first.remove(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    first.add(&object).unwrap();
    assert_eq!(object.node_id(), id);
    let mut foreign = Scene::new();
    assert!(foreign.add(&object).is_err());
    assert!(foreign
        .execution_session()
        .unwrap()
        .frame()
        .objects
        .is_empty());
}

#[test]
fn batch_membership_uses_authoritative_root_order_and_one_revision() {
    let mut scene = Scene::new();
    let first = scene.circle(1.0).unwrap();
    let second = scene.square(1.0).unwrap();
    let replacement = scene.rectangle(2.0, 1.0).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    scene
        .add_many(&[
            MobjectTarget::Object(&first),
            MobjectTarget::Object(&second),
        ])
        .unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision().get(),
        before.get() + 1
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        &[first.node_id(), second.node_id()]
    );

    scene
        .replace(
            MobjectTarget::Object(&first),
            MobjectTarget::Object(&replacement),
        )
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        &[replacement.node_id(), second.node_id()]
    );
    scene.clear().unwrap();
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());
}

#[test]
fn scene_owned_membership_publishes_running_edits_and_rejects_stale_state_atomically() {
    let mut scene = Scene::new();
    let first = scene.circle(1.0).unwrap();
    let candidate = scene.rectangle(2.0, 1.0).unwrap();
    let mut detached_editor = scene.square(1.0).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);

    let before = scene.revision();
    scene.add(&first).unwrap();
    assert_eq!(scene.revision().get(), before.get() + 1);
    assert!(scene
        .owned_execution()
        .execution_object_id(first.node_id())
        .is_some());
    assert_eq!(
        scene
            .owned_execution()
            .publication_context()
            .scene_revision(),
        scene.revision()
    );

    scene.remove(&first).unwrap();
    assert!(!scene
        .owned_execution()
        .semantic_object_is_reachable(first.node_id()));
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());

    detached_editor.set_translation(1.0, 0.0).unwrap();
    let stale_revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let members = scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .to_vec();
    let error = scene.add(&candidate).unwrap_err();
    assert!(matches!(
        error,
        crate::AuthoringError::ExecutionPublication(
            crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
        )
    ));
    assert_eq!(scene.revision(), stale_revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        members.as_slice()
    );
    assert!(scene
        .owned_execution()
        .execution_object_id(candidate.node_id())
        .is_none());
}

#[test]
fn initial_animation_root_rejects_foreign_and_stale_declaration_handles() {
    fn root(scene: &Scene) -> crate::DeclaredAnimation {
        scene
            .declare_animation(
                noon_core::SemanticAnimationIntent::Wait,
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap()
    }
    let local = Scene::new();
    let foreign = Scene::new();
    let local_root = root(&local);
    let foreign_root = root(&foreign);
    assert_eq!(
        local_root.node_id(),
        foreign_root.node_id(),
        "store-local IDs may collide"
    );
    assert!(local
        .execution_session_with_animation_root(&foreign_root)
        .is_err());
    local
        .integration_store()
        .borrow_mut()
        .remove_node(local_root.node_id())
        .unwrap();
    assert!(local
        .execution_session_with_animation_root(&local_root)
        .is_err());
}

#[test]
fn exact_transform_track_preserves_constant_endpoints_that_differ_from_base() {
    use noon_core::{
        CompositionTimeMap, SemanticAnimationIntent, SemanticObjectTrackProperty,
        SemanticObjectTrackValues, TrackTiming,
    };
    let mut scene = Scene::new();
    let object = scene.circle(1.0).unwrap();
    scene.add(&object).unwrap();
    let mut endpoint = scene.circle(2.0).unwrap();
    endpoint.set_translation(3.0, 1.0).unwrap();
    endpoint.set_object_opacity(0.25).unwrap();
    let track = scene
        .declare_animation(
            SemanticAnimationIntent::ObjectPropertyTrack {
                target: object.node_id(),
                property: SemanticObjectTrackProperty::Transform,
                values: SemanticObjectTrackValues::Object {
                    from: endpoint.node_id(),
                    to: endpoint.node_id(),
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![track.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut session = scene.execution_session_with_animation_root(&root).unwrap();
    for time in [0.0, 0.5, 1.0, 0.25] {
        session.seek(time).unwrap();
        let actual = &session.frame().objects[0];
        assert_eq!(actual.transform.translation, noon_core::Vec2::new(3.0, 1.0));
        assert_eq!(actual.style.opacity, 0.25);
        assert_eq!(
            actual.content.geometry(),
            Some(&noon_core::GeometryRef::circle(2.0))
        );
    }
    assert_eq!(
        session.frame().objects.len(),
        1,
        "endpoint identities remain detached"
    );
}

#[test]
fn effective_path_query_is_scene_owned_and_requires_running_execution() {
    let mut scene = Scene::new();
    let line = scene.line((1.0, 2.0), (4.0, 6.0)).unwrap();
    scene.add(&line).unwrap();

    let error = scene.effective_path_query(&line).unwrap_err();
    assert!(matches!(
        error,
        crate::AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable
        )
    ));

    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let query = scene.effective_path_query(&line).unwrap();
    assert_eq!(query.start().unwrap(), (1.0, 2.0));
    assert_eq!(query.end().unwrap(), (4.0, 6.0));

    let foreign_scene = Scene::new();
    let foreign = foreign_scene.line((0.0, 0.0), (1.0, 0.0)).unwrap();
    assert!(matches!(
        scene.effective_path_query(&foreign),
        Err(crate::AuthoringError::ForeignStore)
    ));
}

#[test]
fn scene_point_matching_captures_effective_target_and_blocks_pending_publication() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let target = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
    let mut endpoint = target.target_editor().unwrap();
    endpoint.shift(0.0, 2.0).unwrap();
    scene
        .add_many(&[(&source).into(), (&target).into()])
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &target,
            &endpoint,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let segment = {
        let mut live = scene.owned_live();
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();
        segment
    };
    let before = source.state().unwrap();
    assert!(scene.match_points(&source, &target).is_err());
    assert_eq!(source.state().unwrap(), before);
    {
        let mut live = scene.owned_live();
        live.advance_segment_to(segment, 2.0).unwrap();
        live.complete_segment(segment).unwrap();
    }
    scene.owned_execution_mut().seek(1.0).unwrap();
    scene.match_points(&source, &target).unwrap();
    assert!((source.center().unwrap().1 - 1.0).abs() < 1e-6);
    scene.owned_execution_mut().seek(2.0).unwrap();
    let live = scene.owned_live();
    assert!((live.effective_layout(&target).unwrap().center.1 - 2.0).abs() < 1e-6);
    assert!((live.effective_layout(&source).unwrap().center.1 - 1.0).abs() < 1e-6);
}

#[test]
fn scene_point_matching_rejects_active_render_override_before_mutating_source() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let target = scene.circle(1.0).unwrap();
    scene.add(&source).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    {
        let mut live = scene.owned_live();
        live.declare_and_activate_create(&target, AnimationOptions::new())
            .unwrap();
    }
    let before = source.state().unwrap();
    let revision = source.integration_store().borrow().scene_revision();
    assert!(scene.match_points(&source, &target).is_err());
    assert_eq!(source.state().unwrap(), before);
    assert_eq!(
        source.integration_store().borrow().scene_revision(),
        revision
    );
}

#[test]
fn effective_family_layout_is_scene_owned_and_requires_running_execution() {
    let mut scene = Scene::new();
    let mut left = scene.square(2.0).unwrap();
    let mut right = scene.square(2.0).unwrap();
    left.set_translation(-2.0, 0.0).unwrap();
    right.set_translation(2.0, 0.0).unwrap();
    let family = scene
        .family(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
        .unwrap();
    scene
        .add_many(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
        .unwrap();

    let error = scene.effective_family_layout(&family).unwrap_err();
    assert!(matches!(
        error,
        crate::AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable
        )
    ));

    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let layout = scene.effective_family_layout(&family).unwrap();
    assert_eq!(layout.center, (0.0, 0.0));
    assert_eq!((layout.width, layout.height), (6.0, 2.0));
    assert_eq!(
        layout.publication,
        scene.owned_execution().publication_context()
    );

    let foreign_scene = Scene::new();
    let foreign_object = foreign_scene.square(1.0).unwrap();
    let foreign_family = foreign_scene
        .family(&[MobjectTarget::Object(&foreign_object)])
        .unwrap();
    assert!(matches!(
        scene.effective_family_layout(&foreign_family),
        Err(crate::AuthoringError::ForeignStore)
    ));
}

#[test]
fn effective_family_layout_preserves_detached_member_authored_bounds() {
    let mut scene = Scene::new();
    let mut live = scene.square(2.0).unwrap();
    let mut detached = scene.square(2.0).unwrap();
    live.set_translation(-2.0, 0.0).unwrap();
    detached.set_translation(4.0, 0.0).unwrap();
    let family = scene
        .family(&[
            MobjectTarget::Object(&live),
            MobjectTarget::Object(&detached),
        ])
        .unwrap();
    scene.add(&live).unwrap();

    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    assert!(!scene
        .owned_execution()
        .semantic_object_is_reachable(detached.node_id()));

    let layout = scene.effective_family_layout(&family).unwrap();
    assert_eq!(layout.center, (1.0, 0.0));
    assert_eq!((layout.width, layout.height), (8.0, 2.0));
}

#[test]
fn scene_path_alignment_publishes_effective_operands_coherently() {
    let mut scene = Scene::new();
    let left = scene.line((0.0, 0.0), (3.0, 0.0)).unwrap();
    let right = scene.square(2.0).unwrap();
    scene.add(&left).unwrap();
    scene.add(&right).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    {
        let mut live = scene.owned_live();
        live.shift(&left, 1.0, 2.0).unwrap();
    }

    scene.align_points(&left, &right).unwrap();

    let left_query = scene.effective_path_query(&left).unwrap();
    let right_query = scene.effective_path_query(&right).unwrap();
    assert_eq!(left_query.curve_count(), 4);
    assert_eq!(right_query.curve_count(), 4);
    assert_eq!(left_query.start().unwrap(), (1.0, 2.0));
}

#[test]
fn scene_path_alignment_self_alias_is_noop_before_unsupported_capture() {
    let mut scene = Scene::new();
    let object = scene.circle(1.0).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    {
        let mut live = scene.owned_live();
        live.declare_and_activate_create(&object, AnimationOptions::new())
            .unwrap();
    }
    scene.owned_execution_mut().take_frame_changes();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();

    scene.align_points(&object, &object).unwrap();

    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
}

#[test]
fn scene_path_alignment_rejects_foreign_before_resource_or_frame_publication() {
    let mut scene = Scene::new();
    let object = scene.line((0.0, 0.0), (3.0, 0.0)).unwrap();
    scene.add(&object).unwrap();
    let foreign = Scene::new().square(2.0).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    scene.owned_execution_mut().take_frame_changes();
    let before = object.state().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();

    assert!(scene.align_points(&object, &foreign).is_err());

    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
}
