//! Matching activation must use ordinary foreground-aware membership publication.
use super::*;
use noon_core::{
    plan_semantic_scene_membership, SemanticObjectState, SemanticSceneMembershipRequest,
    StoredGeometry, Vec2, VectorPath,
};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}
fn path(store: &mut SemanticStore, triangle: bool) -> SemanticNodeId {
    let mut points = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0));
    if !triangle {
        points = points.line_to(Vec2::new(-1.0, 1.0));
    }
    let handle = store.insert_geometry_path(points.close()).unwrap();
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle)))
}
fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let root = store.insert_family();
    for &member in members {
        store.add_member(root, member).unwrap();
    }
    root
}
fn assert_matching_foreground_order(source_is_foreground: bool) {
    let mut store = SemanticStore::new();
    let back = object(&mut store);
    let leaf = path(&mut store, true);
    let source = family(&mut store, &[leaf]);
    let front = object(&mut store);
    let target_leaf = path(&mut store, true);
    let leftover = path(&mut store, false);
    let target = family(&mut store, &[target_leaf, leftover]);
    let later = object(&mut store);
    let root = family(&mut store, &[back, source, front]);
    let foreground = if source_is_foreground {
        vec![source, front]
    } else {
        vec![front]
    };
    plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&foreground),
    )
    .unwrap()
    .apply(&mut store)
    .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let back_id = session.execution_index.execution_object_id(back).unwrap();
    let leaf_id = session.execution_index.execution_object_id(leaf).unwrap();
    let front_id = session.execution_index.execution_object_id(front).unwrap();
    let revision = store.scene_revision();
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
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    let activated = store.scene_revision();
    for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
        session.advance_segment_to(segment, time).unwrap();
        let ids = session
            .painter_order()
            .iter()
            .map(|&i| session.frame().objects[i as usize].id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [back_id, leaf_id, front_id],
            "matching activation/interpolation must preserve the foreground tail"
        );
        assert_eq!(store.node(root).unwrap().members(), [back, source, front]);
        assert_eq!(store.node(root).unwrap().foreground_members(), foreground);
        assert_eq!(store.scene_revision(), activated);
        assert!(!store.node(target).unwrap().is_scene_owned());
        let publication = session.take_renderer_publication();
        assert_eq!(publication.transient_presentations().len(), 1);
        assert!(
            (f64::from(publication.transient_presentations()[0].state().appearance) - time).abs()
                < 1e-6
        );
    }
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(store.node(root).unwrap().members(), [back, target, front]);
    assert_eq!(store.node(root).unwrap().foreground_members(), [front]);
    assert!(session
        .take_renderer_publication()
        .transient_presentations()
        .is_empty());
    let add =
        plan_semantic_scene_membership(&store, root, SemanticSceneMembershipRequest::Add(&[later]))
            .unwrap();
    session
        .apply_semantic_transaction_at_root(&mut store, root, add)
        .unwrap();
    assert_eq!(
        store.node(root).unwrap().members(),
        [back, target, later, front]
    );
    assert!(session.runtime.frame_index_for_object(leaf_id).is_none());
}
#[test]
fn matching_inflight_ordinary_source_respects_foreground() {
    assert_matching_foreground_order(false);
}
#[test]
fn matching_inflight_foreground_source_preserves_declaration_order() {
    assert_matching_foreground_order(true);
}
