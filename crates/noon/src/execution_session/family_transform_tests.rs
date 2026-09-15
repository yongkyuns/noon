use noon_core::{
    AnimationOptions, GeometryRef, RateFunction, SemanticObjectState, SemanticStore, SemanticVec3,
    StoredGeometry, Vec2, VectorPath,
};

use super::*;

fn object(store: &mut SemanticStore, x: f64) -> SemanticNodeId {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    store.insert_semantic_object(state)
}

fn path_object(store: &mut SemanticStore, x: f64) -> SemanticNodeId {
    let path = VectorPath::new()
        .move_to(Vec2::new(-0.6, -0.45))
        .line_to(Vec2::new(0.6, -0.45))
        .line_to(Vec2::new(0.0, 0.6))
        .close();
    let handle = store.insert_geometry_path(path).unwrap();
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    store.insert_semantic_object(state)
}

fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let family = store.insert_family();
    for &member in members {
        store.add_member(family, member).unwrap();
    }
    family
}

fn expansion_session() -> (ExecutionSession, ExecutionSegment) {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 12.0);
    let t2 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1, t2]);

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    (session, segment)
}

fn path_expansion_session() -> (SemanticStore, ExecutionSession, ExecutionSegment) {
    let mut store = SemanticStore::new();
    let s0 = path_object(&mut store, -2.0);
    let s1 = path_object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = path_object(&mut store, -3.0);
    let t1 = path_object(&mut store, 0.0);
    let t2 = path_object(&mut store, 3.0);
    let target = family(&mut store, &[t0, t1, t2]);

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    (store, session, segment)
}

#[test]
fn unequal_family_transform_publishes_one_identity_free_expansion_copy() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 12.0);
    let t2 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1, t2]);
    let source_members = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    let target_members = store
        .semantic_family_members_checked(target)
        .unwrap()
        .to_vec();

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();

    session.advance_segment_to(segment, 0.5).unwrap();
    {
        let publication = session.take_renderer_publication();
        assert_eq!(publication.transient_presentations().len(), 1);
        let copy = &publication.transient_presentations()[0];
        assert!(copy.state().appearance > 0.0 && copy.state().appearance < 1.0);
        assert_eq!(copy.anchor_object_index(), 0);
    }
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
    assert_eq!(
        store.semantic_family_members_checked(target).unwrap(),
        target_members
    );

    session.advance_segment_to(segment, 1.0).unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    let persisted = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    assert_eq!(persisted.len(), 3);
    assert_eq!(persisted[0], s0);
    assert_eq!(persisted[2], s1);
    let expanded = persisted[1];
    assert!(!source_members.contains(&expanded));
    assert!(!target_members.contains(&expanded));
    assert_eq!(
        store.semantic_family_members_checked(target).unwrap(),
        target_members
    );
    for (&member, expected_x) in persisted.iter().zip([10.0, 12.0, 14.0]) {
        assert_eq!(
            store
                .semantic_object_state_checked(member)
                .unwrap()
                .transform
                .translation,
            SemanticVec3::new(expected_x, 0.0, 0.0)
        );
    }

    // Completion replaces the identity-free interpolation copy with the new stable
    // semantic child in the same publication, so the endpoint never double-renders.
    let publication = session.take_renderer_publication();
    assert!(publication.transient_presentations().is_empty());
    assert_eq!(session.frame().objects.len(), 3);

    session.advance_to(2.0).unwrap();
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        persisted
    );
    assert_eq!(session.frame().objects.len(), 3);
}

#[test]
fn unequal_family_transform_scene_root_member_publishes_expansion_copy() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let scene_root = family(&mut store, &[source]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 12.0);
    let t2 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1, t2]);

    let mut session = ExecutionSession::from_semantic_root(&store, scene_root).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, scene_root, &request, AnimationOptions::new())
        .unwrap();

    session.advance_segment_to(segment, 0.5).unwrap();
    let publication = session.take_renderer_publication();
    assert_eq!(publication.transient_presentations().len(), 1);
    let copy = &publication.transient_presentations()[0];
    assert!(copy.state().appearance > 0.0 && copy.state().appearance < 1.0);
}

#[test]
fn unequal_family_transform_direct_seek_matches_forward_playback() {
    let (mut forward, forward_segment) = expansion_session();
    forward.advance_segment_to(forward_segment, 0.25).unwrap();
    assert_eq!(
        forward
            .take_renderer_publication()
            .transient_presentations()
            .len(),
        1
    );
    forward.advance_segment_to(forward_segment, 0.5).unwrap();
    let forward_frame = forward.frame().clone();
    let forward_derived = forward
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();

    let (mut direct, _direct_segment) = expansion_session();
    direct.seek(0.5).unwrap();
    let direct_frame = direct.frame().clone();
    let direct_derived = direct
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();

    assert_eq!(forward_frame, direct_frame);
    assert_eq!(forward_derived, direct_derived);
    assert_eq!(forward_derived.len(), 1);
    assert_eq!(forward_derived[0].anchor_object_index(), 0);
    assert!(
        forward_derived[0].state().appearance > 0.0 && forward_derived[0].state().appearance < 1.0
    );
}

#[test]
fn unequal_family_transform_completed_direct_seek_matches_forward_playback() {
    let make = || {
        let mut store = SemanticStore::new();
        let s0 = object(&mut store, 0.0);
        let s1 = object(&mut store, 2.0);
        let source = family(&mut store, &[s0, s1]);
        let t0 = object(&mut store, 10.0);
        let t1 = object(&mut store, 12.0);
        let t2 = object(&mut store, 14.0);
        let target = family(&mut store, &[t0, t1, t2]);
        let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
        let request = SemanticCompositionRequest::FamilyTransformTo {
            source,
            target_state: target,
            options: AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        };
        let segment = session
            .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
            .unwrap();
        (store, source, target, session, segment)
    };

    let (mut forward_store, forward_source, forward_target, mut forward, forward_segment) = make();
    forward.advance_segment_to(forward_segment, 0.25).unwrap();
    forward.take_renderer_publication();
    forward.advance_segment_to(forward_segment, 1.0).unwrap();
    forward
        .complete_segment(&mut forward_store, forward_segment)
        .unwrap();
    assert!(forward
        .take_renderer_publication()
        .transient_presentations()
        .is_empty());

    let (mut direct_store, direct_source, direct_target, mut direct, direct_segment) = make();
    direct.seek(1.0).unwrap();
    direct
        .complete_segment(&mut direct_store, direct_segment)
        .unwrap();
    assert!(direct
        .take_renderer_publication()
        .transient_presentations()
        .is_empty());

    let states = |store: &SemanticStore, family: SemanticNodeId| {
        store
            .semantic_family_members_checked(family)
            .unwrap()
            .iter()
            .map(|member| {
                store
                    .semantic_object_state_checked(*member)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        states(&forward_store, forward_source),
        states(&direct_store, direct_source)
    );
    assert_eq!(
        states(&forward_store, forward_target),
        states(&direct_store, direct_target)
    );
    assert_eq!(forward.frame(), direct.frame());
}

#[test]
fn unequal_family_path_transform_is_seek_equivalent_and_retires_locally() {
    let (mut forward_store, mut forward, forward_segment) = path_expansion_session();
    forward.advance_segment_to(forward_segment, 0.5).unwrap();
    let forward_frame = forward.frame().clone();
    let forward_transient = forward
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();
    assert_eq!(forward_transient.len(), 1);
    assert!(matches!(
        forward_transient[0].state().effective_render_geometry(),
        Some(GeometryRef::VectorPath(_))
    ));
    assert!(
        forward_transient[0].state().appearance > 0.0
            && forward_transient[0].state().appearance < 1.0
    );

    let (_direct_store, mut direct, _direct_segment) = path_expansion_session();
    direct.seek(0.5).unwrap();
    let direct_frame = direct.frame().clone();
    let direct_transient = direct
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();
    assert_eq!(forward_frame, direct_frame);
    assert_eq!(forward_transient, direct_transient);

    forward.advance_segment_to(forward_segment, 1.0).unwrap();
    forward
        .complete_segment(&mut forward_store, forward_segment)
        .unwrap();
    let endpoint = forward.take_renderer_publication();
    assert!(endpoint.transient_presentations().is_empty());
    assert_eq!(forward.frame().objects.len(), 3);
}

#[test]
fn unequal_family_contraction_retains_only_effective_padding_fade() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let s2 = object(&mut store, 4.0);
    let source = family(&mut store, &[s0, s1, s2]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1]);
    let source_members = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    let target_members = store
        .semantic_family_members_checked(target)
        .unwrap()
        .to_vec();

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    session.advance_segment_to(segment, 1.0).unwrap();
    assert!(session
        .take_renderer_publication()
        .transient_presentations()
        .is_empty());
    session.complete_segment(&mut store, segment).unwrap();

    let hidden_index = session
        .execution_index
        .execution_object_id(s1)
        .and_then(|object| session.runtime.frame_index_for_object(object))
        .unwrap();
    // Completion releases the execution-only Appearance fade back to its
    // neutral value while authored object opacity preserves the same hidden result.
    assert_eq!(session.frame().objects[hidden_index].appearance, 1.0);
    assert_eq!(session.frame().objects[hidden_index].style.opacity, 0.0);
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
    assert_eq!(
        store.semantic_family_members_checked(target).unwrap(),
        target_members
    );
    assert_eq!(
        store
            .semantic_object_state_checked(s1)
            .unwrap()
            .style
            .object_opacity,
        0.0
    );

    // Once the hidden endpoint is authored, the temporary Appearance driver is
    // released; a later authored visibility write can therefore take effect.
    let mut show = noon_core::SemanticMutationTransaction::new();
    show.set_property(
        s1,
        noon_core::SemanticObjectProperty::ObjectOpacity,
        1.0_f64,
    );
    session
        .apply_semantic_transaction(&mut store, show)
        .unwrap();
    assert_eq!(session.frame().objects[hidden_index].appearance, 1.0);
    assert_eq!(session.frame().objects[hidden_index].style.opacity, 1.0);
}

#[test]
fn matching_flat_family_transform_carries_path_arc_to_each_leaf() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, -2.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 2.0);
    let t1 = object(&mut store, -2.0);
    let target = family(&mut store, &[t0, t1]);

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(std::f64::consts::PI),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();

    session.advance_segment_to(segment, 0.5).unwrap();
    let index_for = |session: &ExecutionSession, node| {
        session
            .execution_index
            .execution_object_id(node)
            .and_then(|object| session.runtime.frame_index_for_object(object))
            .unwrap()
    };
    let left = session.frame().objects[index_for(&session, s0)]
        .transform
        .translation;
    let right = session.frame().objects[index_for(&session, s1)]
        .transform
        .translation;
    assert!(left.x.abs() < 1e-6 && (left.y + 2.0).abs() < 1e-6);
    assert!(right.x.abs() < 1e-6 && (right.y - 2.0).abs() < 1e-6);

    session.advance_segment_to(segment, 1.0).unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(s0)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::new(2.0, 0.0, 0.0)
    );
    assert_eq!(
        store
            .semantic_object_state_checked(s1)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::new(-2.0, 0.0, 0.0)
    );
}

#[test]
fn curved_unequal_family_transform_fails_before_publication() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 12.0);
    let t2 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1, t2]);
    let revision = store.scene_revision();

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(std::f64::consts::PI / 2.0),
    };
    let error = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("curved family Transform requires matching flat family topology"));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.frame().time, 0.0);
}

#[test]
fn sub_threshold_unequal_family_path_arc_keeps_straight_fallback() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 10.0);
    let t1 = object(&mut store, 12.0);
    let t2 = object(&mut store, 14.0);
    let target = family(&mut store, &[t0, t1, t2]);

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(0.009),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(session.frame().time, 0.5);
}
