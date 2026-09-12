use noon_core::{
    AnimationOptions, RateFunction, SemanticObjectState, SemanticStore, SemanticVec3,
    StoredGeometry,
};

use super::*;

fn object(store: &mut SemanticStore, x: f64) -> SemanticNodeId {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
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
