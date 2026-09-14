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
    {
        let publication = session.take_renderer_publication();
        assert_eq!(publication.transient_presentations().len(), 1);
        assert_eq!(
            publication.transient_presentations()[0].state().appearance,
            1.0
        );
    }
    // Completion grants the endpoint one coherent publication, then queues one
    // presentation-only follow-up so the transient occurrence is erased without
    // claiming stable frame, painter-order, or spatial dirtiness.
    assert!(session.wake_state().frame_pending());
    let removal = session.take_renderer_publication();
    assert!(!removal.changes().is_all());
    assert!(removal.changes().requires_presentation_redraw());
    assert!(!removal.changes().is_structural());
    assert!(!removal.changes().has_painter_order_change());
    assert!(removal.changes().object_indices().is_empty());
    assert!(removal.changes().added_indices().is_empty());
    assert!(removal.changes().removed_indices().is_empty());
    assert!(removal.transient_presentations().is_empty());
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
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
    {
        let endpoint = forward.take_renderer_publication();
        assert_eq!(endpoint.transient_presentations().len(), 1);
        let copy = &endpoint.transient_presentations()[0];
        assert_eq!(copy.state().appearance, 1.0);
        assert!(matches!(
            copy.state().effective_render_geometry(),
            Some(GeometryRef::VectorPath(_))
        ));
    }

    assert!(forward.wake_state().frame_pending());
    let retirement = forward.take_renderer_publication();
    assert!(retirement.changes().requires_presentation_redraw());
    assert!(!retirement.changes().has_stable_changes());
    assert!(retirement.transient_presentations().is_empty());
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
    assert_eq!(session.frame().objects[hidden_index].appearance, 0.0);
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
    assert_eq!(
        store
            .semantic_object_state_checked(s1)
            .unwrap()
            .style
            .object_opacity,
        1.0
    );
}

fn matching_arc_session(
    path_arc: f64,
) -> (
    SemanticStore,
    ExecutionSession,
    ExecutionSegment,
    SemanticNodeId,
    SemanticNodeId,
) {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let source = family(&mut store, &[s0, s1]);
    let t0 = object(&mut store, 2.0);
    let t1 = object(&mut store, 0.0);
    let target = family(&mut store, &[t0, t1]);
    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(path_arc),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    (store, session, segment, s0, s1)
}

fn node_translation(session: &ExecutionSession, node: SemanticNodeId) -> Vec2 {
    let index = session
        .execution_index
        .execution_object_id(node)
        .and_then(|object| session.runtime.frame_index_for_object(object))
        .unwrap();
    session.frame().objects[index].transform.translation
}

#[test]
fn matching_family_path_arc_uses_curved_leaf_tracks_and_seek_is_equivalent() {
    let (mut store, mut forward, segment, s0, s1) =
        matching_arc_session(std::f64::consts::FRAC_PI_2);
    forward.advance_segment_to(segment, 0.5).unwrap();
    let forward_left = node_translation(&forward, s0);
    let forward_right = node_translation(&forward, s1);
    assert!((forward_left.x - 1.0).abs() < 1e-5);
    assert!((forward_right.x - 1.0).abs() < 1e-5);
    assert!(forward_left.y < -0.1);
    assert!(forward_right.y > 0.1);

    let (_direct_store, mut direct, _direct_segment, direct_s0, direct_s1) =
        matching_arc_session(std::f64::consts::FRAC_PI_2);
    direct.seek(0.5).unwrap();
    assert_eq!(node_translation(&direct, direct_s0), forward_left);
    assert_eq!(node_translation(&direct, direct_s1), forward_right);

    forward.advance_segment_to(segment, 1.0).unwrap();
    forward.complete_segment(&mut store, segment).unwrap();
    assert_eq!(node_translation(&forward, s0), Vec2::new(2.0, 0.0));
    assert_eq!(node_translation(&forward, s1), Vec2::new(0.0, 0.0));
}

#[test]
fn matching_family_negative_path_arc_reverses_curvature() {
    let (_positive_store, mut positive, positive_segment, positive_s0, positive_s1) =
        matching_arc_session(std::f64::consts::FRAC_PI_2);
    positive.advance_segment_to(positive_segment, 0.5).unwrap();
    let positive_left = node_translation(&positive, positive_s0);
    let positive_right = node_translation(&positive, positive_s1);

    let (_negative_store, mut negative, negative_segment, negative_s0, negative_s1) =
        matching_arc_session(-std::f64::consts::FRAC_PI_2);
    negative.advance_segment_to(negative_segment, 0.5).unwrap();
    let negative_left = node_translation(&negative, negative_s0);
    let negative_right = node_translation(&negative, negative_s1);

    assert!(positive_left.y < 0.0 && negative_left.y > 0.0);
    assert!(positive_right.y > 0.0 && negative_right.y < 0.0);
}

#[test]
fn significant_family_path_arc_rejects_unequal_topology_before_publication() {
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
    let before = store.scene_revision();
    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(std::f64::consts::FRAC_PI_2),
    };
    let error = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("path_arc requires matching flat leaf topology"));
    assert_eq!(store.scene_revision(), before);
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
    assert_eq!(node_translation(&session, s0), Vec2::new(0.0, 0.0));
    assert_eq!(node_translation(&session, s1), Vec2::new(2.0, 0.0));
}

#[test]
fn subthreshold_family_path_arc_keeps_existing_unequal_alignment() {
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
            .path_arc(noon_core::MANIM_STRAIGHT_PATH_ARC_THRESHOLD * 0.5),
    };
    session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
}
