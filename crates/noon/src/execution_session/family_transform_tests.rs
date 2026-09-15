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

fn matching_triangle() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -0.5))
        .line_to(Vec2::new(-0.25, 1.0))
        .close()
}

fn matching_kite() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(1.5, 0.0))
        .line_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(-0.5, 0.0))
        .close()
}

fn matching_path_object(store: &mut SemanticStore, path: VectorPath, x: f64) -> SemanticNodeId {
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

#[test]
fn matching_family_quarter_phases_agree_with_direct_seek_and_keep_cleanup_stable() {
    fn assert_position(actual: impl Into<f64>, expected: f64) {
        assert!((actual.into() - expected).abs() < 1e-5);
    }

    for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut observations = Vec::new();
        for dense in [false, true] {
            // Rebuild independently: the direct route must not inherit a
            // forward run's evaluated state or its completion transaction.
            let mut store = SemanticStore::new();
            let before = object(&mut store, -4.0);
            let source_triangle = matching_path_object(&mut store, matching_triangle(), -2.0);
            let source_kite = matching_path_object(&mut store, matching_kite(), 2.0);
            let source = family(&mut store, &[source_triangle, source_kite]);
            let after = object(&mut store, 4.0);
            let target_kite = matching_path_object(&mut store, matching_kite(), -4.0);
            let target_triangle = matching_path_object(&mut store, matching_triangle(), 4.0);
            let target = family(&mut store, &[target_kite, target_triangle]);
            let root = family(&mut store, &[before, source, after]);
            let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
            let source_ids = [source_triangle, source_kite]
                .map(|node| session.execution_index.execution_object_id(node).unwrap());
            let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
                source,
                target_state: target,
                options: AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            };
            let segment = session
                .declare_and_activate_composition(
                    &mut store,
                    root,
                    &request,
                    AnimationOptions::new(),
                )
                .unwrap();
            let sample_time = alpha * segment.end_time();
            if dense {
                for step in 1..=16 {
                    session
                        .advance_segment_to(segment, sample_time * f64::from(step) / 16.0)
                        .unwrap();
                }
            } else {
                session.seek(sample_time).unwrap();
            }
            let positions = |session: &ExecutionSession| {
                source_ids.map(|id| {
                    let index = session.runtime.frame_index_for_object(id).unwrap();
                    session.frame().objects[index].transform.translation.x
                })
            };
            let sample = positions(&session);
            assert_position(sample[0], -2.0 + 6.0 * alpha);
            assert_position(sample[1], 2.0 - 6.0 * alpha);
            assert_eq!(
                store.semantic_family_members_checked(root).unwrap(),
                &[before, after, source]
            );
            session.take_renderer_publication();
            session.advance_segment_to(segment, sample_time).unwrap();
            assert_eq!(positions(&session), sample);
            observations.push(sample);

            session
                .advance_segment_to(segment, segment.end_time())
                .unwrap();
            session.complete_segment(&mut store, segment).unwrap();
            assert_eq!(
                store.semantic_family_members_checked(root).unwrap(),
                &[before, after, target]
            );
            assert_eq!(
                store.semantic_family_members_checked(target).unwrap(),
                &[target_kite, target_triangle]
            );
            for id in source_ids {
                assert!(session.runtime.frame_index_for_object(id).is_none());
            }
            let expected_order = [before, after, target_kite, target_triangle]
                .map(|node| session.execution_index.execution_object_id(node).unwrap());
            let painter_ids = |session: &ExecutionSession| {
                session
                    .painter_order()
                    .iter()
                    .map(|&index| session.frame().objects[index as usize].id)
                    .collect::<Vec<_>>()
            };
            assert_eq!(painter_ids(&session), expected_order);
            session.take_renderer_publication();
            session.advance_to(segment.end_time() + 0.25).unwrap();
            assert_eq!(painter_ids(&session), expected_order);
            assert_eq!(
                store.semantic_family_members_checked(root).unwrap(),
                &[before, after, target]
            );
        }
        assert_eq!(observations[0], observations[1], "phase {alpha}");
    }
}

#[test]
fn matching_family_transform_matches_shapes_not_positions_and_replaces_family() {
    let mut store = SemanticStore::new();
    let source_triangle = matching_path_object(&mut store, matching_triangle(), -2.0);
    let source_kite = matching_path_object(&mut store, matching_kite(), 2.0);
    let source = family(&mut store, &[source_triangle, source_kite]);
    let target_kite = matching_path_object(&mut store, matching_kite(), -4.0);
    let target_triangle = matching_path_object(&mut store, matching_triangle(), 4.0);
    let target = family(&mut store, &[target_kite, target_triangle]);
    let root = family(&mut store, &[source]);

    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();

    session.advance_segment_to(segment, 0.5).unwrap();
    let index_for = |session: &ExecutionSession, node| {
        session
            .execution_index
            .execution_object_id(node)
            .and_then(|object| session.runtime.frame_index_for_object(object))
            .unwrap()
    };
    let triangle_x = session.frame().objects[index_for(&session, source_triangle)]
        .transform
        .translation
        .x;
    let kite_x = session.frame().objects[index_for(&session, source_kite)]
        .transform
        .translation
        .x;
    assert!((triangle_x - 1.0).abs() < 1e-5);
    assert!((kite_x + 1.0).abs() < 1e-5);
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[source]
    );

    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[target]
    );
    let source_object = session
        .execution_index
        .execution_object_id(source_triangle)
        .unwrap();
    assert!(session
        .runtime
        .frame_index_for_object(source_object)
        .is_none());
    let target_object = session
        .execution_index
        .execution_object_id(target_triangle)
        .unwrap();
    let target_index = session
        .runtime
        .frame_index_for_object(target_object)
        .unwrap();
    assert!(
        (session.frame().objects[target_index]
            .transform
            .translation
            .x
            - 4.0)
            .abs()
            < 1e-5
    );
}

#[test]
fn matching_family_succession_uses_completed_prior_morph_for_activation_key() {
    let mut store = SemanticStore::new();
    let source_leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let source = family(&mut store, &[source_leaf]);
    let morph_target = matching_path_object(&mut store, matching_kite(), 0.0);
    let target_leaf = matching_path_object(&mut store, matching_kite(), 3.0);
    let target = family(&mut store, &[target_leaf]);
    let root = family(&mut store, &[source]);

    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let request = SemanticCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Sequence,
        children: vec![
            SemanticCompositionRequest::TransformTo {
                source: source_leaf,
                target_state: morph_target,
                interpolation: noon_core::SemanticTransformInterpolation::PointCorrespondence,
                complete_priority: false,
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
            SemanticCompositionRequest::MatchingFamilyTransformTo {
                source,
                target_state: target,
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
        ],
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();
    assert!((segment.end_time() - 2.0).abs() < 1e-12);

    session.advance_segment_to(segment, 1.5).unwrap();
    let publication = session.take_renderer_publication();
    assert!(publication.transient_presentations().is_empty());
    let source_object = session
        .execution_index
        .execution_object_id(source_leaf)
        .unwrap();
    let source_index = session
        .runtime
        .frame_index_for_object(source_object)
        .unwrap();
    assert!(
        (session.frame().objects[source_index]
            .transform
            .translation
            .x
            - 1.5)
            .abs()
            < 1e-5
    );

    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[target]
    );
    // Cleanup restores the matching child's activation-effective source,
    // not the authored triangle from before the earlier Morph child.
    let restored = store.semantic_object_state_checked(source_leaf).unwrap();
    let prior_endpoint = store.semantic_object_state_checked(morph_target).unwrap();
    assert_eq!(restored.content, prior_endpoint.content);
    assert_eq!(restored.transform, prior_endpoint.transform);
    assert_eq!(restored.style, prior_endpoint.style);
    let mut readd = SemanticMutationTransaction::new();
    readd.add_member(root, source);
    session
        .apply_semantic_transaction(&mut store, readd)
        .unwrap();
    session.advance_to(segment.end_time() + 0.25).unwrap();
    let source_index = session
        .runtime
        .frame_index_for_object(source_object)
        .unwrap();
    assert_eq!(
        session.frame().objects[source_index]
            .transform
            .translation
            .x,
        0.0
    );
    assert_eq!(session.frame().objects[source_index].appearance, 1.0);
}

#[test]
fn matching_family_completion_appends_target_in_same_z_painter_order() {
    let mut store = SemanticStore::new();
    let before = object(&mut store, -4.0);
    let source_leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let source = family(&mut store, &[source_leaf]);
    let after = object(&mut store, 4.0);
    let target_leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let target = family(&mut store, &[target_leaf]);
    let root = family(&mut store, &[before, source, after]);

    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let before_execution = session.execution_index.execution_object_id(before).unwrap();
    let source_execution = session
        .execution_index
        .execution_object_id(source_leaf)
        .unwrap();
    let after_execution = session.execution_index.execution_object_id(after).unwrap();
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();

    // Setup moves the existing source family behind surviving same-z roots,
    // without allocating replacement source IDs or changing any z index.
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[before, after, source]
    );
    let active_ids = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(
        active_ids,
        vec![before_execution, after_execution, source_execution]
    );
    assert!(session.frame().objects.iter().all(|row| row.z_index == 0.0));

    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();

    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[before, after, target]
    );
    assert!(session
        .runtime
        .frame_index_for_object(source_execution)
        .is_none());
    let target_execution = session
        .execution_index
        .execution_object_id(target_leaf)
        .expect("original authored target must enter the execution domain");
    let painter_ids = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(
        painter_ids,
        vec![before_execution, after_execution, target_execution]
    );

    // Retiring interpolation-only presentation state must not reorder the exact endpoint
    // on the following deterministic publication.
    session.take_renderer_publication();
    session.advance_to(segment.end_time() + 0.25).unwrap();
    let next_painter_ids = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(next_painter_ids, painter_ids);
}

#[test]
fn matching_family_cleanup_restores_matched_padded_and_unmatched_sources() {
    let mut store = SemanticStore::new();
    let sources = [
        matching_path_object(&mut store, matching_triangle(), -3.0),
        matching_path_object(&mut store, matching_triangle(), 0.0),
        matching_path_object(&mut store, matching_kite(), 3.0),
    ];
    let source = family(&mut store, &sources);
    let geometry = store.insert_geometry_path(matching_triangle()).unwrap();
    let mut target_state = SemanticObjectState::new(StoredGeometry::Resource(geometry));
    target_state.transform.translation = SemanticVec3::new(6.0, 0.0, 0.0);
    target_state.style.fill = Some(noon_core::SemanticPaint::Solid(noon_core::Color::RED));
    target_state.style.fill_opacity = 0.5;
    let target_leaf = store.insert_semantic_object(target_state);
    let target = family(&mut store, &[target_leaf]);
    let root = family(&mut store, &[source]);
    let before = sources.map(|node| store.semantic_object_state_checked(node).unwrap().clone());
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let execution_ids =
        sources.map(|node| session.execution_index.execution_object_id(node).unwrap());
    let initial_rows = execution_ids.map(|id| {
        let index = session.runtime.frame_index_for_object(id).unwrap();
        session.frame().objects[index].clone()
    });
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    let padded_index = session
        .runtime
        .frame_index_for_object(execution_ids[1])
        .unwrap();
    let leftover_index = session
        .runtime
        .frame_index_for_object(execution_ids[2])
        .unwrap();
    assert!(session.frame().objects[padded_index].appearance < 1.0);
    assert!(session.frame().objects[leftover_index].appearance < 1.0);
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    for (node, expected) in sources.iter().zip(&before) {
        assert_eq!(
            store.semantic_object_state_checked(*node).unwrap(),
            expected
        );
    }
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[target]
    );
    let mut readd = SemanticMutationTransaction::new();
    readd.add_member(root, source);
    session
        .apply_semantic_transaction(&mut store, readd)
        .unwrap();
    session.advance_to(segment.end_time() + 0.25).unwrap();
    for (id, expected) in execution_ids.iter().zip(initial_rows) {
        let index = session.runtime.frame_index_for_object(*id).unwrap();
        let restored = &session.frame().objects[index];
        assert!(session.frame().is_present(index));
        assert_eq!(restored.transform, expected.transform);
        assert_eq!(restored.style, expected.style);
        assert_eq!(restored.appearance, expected.appearance);
    }
}

#[test]
fn matching_family_rejected_activation_preserves_original_root_order() {
    let mut store = SemanticStore::new();
    let before = object(&mut store, -4.0);
    let leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let source = family(&mut store, &[leaf]);
    let after = object(&mut store, 4.0);
    let target_leaf = matching_path_object(&mut store, matching_triangle(), 1.0);
    let target = family(&mut store, &[target_leaf]);
    let root = family(&mut store, &[before, source, after]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let revision = store.scene_revision();
    let painter_order = session.painter_order().to_vec();
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new().run_time(f64::NAN),
    };
    assert!(session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[before, source, after]
    );
    assert_eq!(session.painter_order(), painter_order);
    assert!(session.pending_segment_completion.is_none());
}

#[test]
fn matching_family_target_leftover_anchors_after_reordered_source() {
    let mut store = SemanticStore::new();
    let before = object(&mut store, -4.0);
    let leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let source = family(&mut store, &[leaf]);
    let after = object(&mut store, 4.0);
    let target_leaf = matching_path_object(&mut store, matching_triangle(), 1.0);
    let leftover = matching_path_object(&mut store, matching_kite(), 2.0);
    let target = family(&mut store, &[target_leaf, leftover]);
    let root = family(&mut store, &[before, source, after]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let source_id = session.execution_index.execution_object_id(leaf).unwrap();
    let source_index = session.runtime.frame_index_for_object(source_id).unwrap();
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();
    let plan = session.derived_display_plan.as_ref().unwrap();
    assert_eq!(plan.occurrences().len(), 1);
    assert_eq!(
        plan.occurrences()[0].painter_placement,
        noon_runtime::TransientPresentationPainterPlacement::AfterStable {
            anchor_object_index: u32::try_from(source_index).unwrap(),
        }
    );
    session.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(
        session.painter_order().last().copied(),
        Some(source_index as u32)
    );
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[before, after, target]
    );
}

#[test]
fn matching_family_rejects_shared_descendants_before_reordering_or_publication() {
    // Sharing a leaf, sharing a nested family, and aliasing within the source
    // are all outside the bounded root-only matching presentation contract.
    for alias_kind in 0..3 {
        let mut store = SemanticStore::new();
        let leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
        let nested = family(&mut store, &[leaf]);
        let source = family(&mut store, &[nested]);
        let alias = family(&mut store, &[if alias_kind == 1 { nested } else { leaf }]);
        if alias_kind == 2 {
            store.add_member(source, alias).unwrap();
        }
        let target_leaf = matching_path_object(&mut store, matching_triangle(), 1.0);
        let target = family(&mut store, &[target_leaf]);
        let after = object(&mut store, 4.0);
        let root = if alias_kind == 2 {
            family(&mut store, &[source, after])
        } else {
            family(&mut store, &[alias, source, after])
        };
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let revision = store.scene_revision();
        let roots = store
            .semantic_family_members_checked(root)
            .unwrap()
            .to_vec();
        let painter_order = session.painter_order().to_vec();
        let frame = session.frame().clone();
        let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
            source,
            target_state: target,
            options: AnimationOptions::new().run_time(1.0),
        };
        let error = session
            .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
            .unwrap_err();
        assert!(
            error.to_string().contains("AliasedSourceDescendant"),
            "{error}"
        );
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.semantic_family_members_checked(root).unwrap(), roots);
        assert_eq!(session.painter_order(), painter_order);
        assert_eq!(session.frame().objects, frame.objects);
        assert_eq!(session.frame().time, frame.time);
        assert!(session.pending_segment_completion.is_none());
        assert!(session.derived_display_plan.is_none());
    }
}
