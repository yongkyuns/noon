use super::{
    tests::{candidate, key},
    *,
};

fn grouped(scene: &Scene, generation: u64, x: f64, reverse: bool) -> SourceCandidate {
    let mut next = candidate(scene, generation, [("left", x), ("right", 4.0)]);
    let members = if reverse {
        [key("right"), key("left")]
    } else {
        [key("left"), key("right")]
    };
    next.declare_family(SourceFamilyDeclaration::new(key("group"), members))
        .unwrap();
    next.set_root_members([key("group")]);
    next
}

#[test]
fn grouped_reorder_preserves_family_object_and_runtime_identities() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let initial = grouped(&scene, 1, 0.0, false);
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let group = first.node_for(&key("group")).unwrap();
    let left = first.node_for(&key("left")).unwrap();
    let right = first.node_for(&key("right")).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let runtime = scene.owned_execution().runtime_identity();
    let edited = grouped(&scene, 2, 0.0, true);
    let second = reconciler.reconcile(&mut scene, &edited).unwrap();
    assert_eq!(first.nodes(), second.nodes());
    assert_eq!(scene.owned_execution().runtime_identity(), runtime);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(group)
            .unwrap(),
        vec![right, left]
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![group]
    );
    let revision = scene.revision();
    let same = grouped(&scene, 3, 0.0, true);
    assert_eq!(
        reconciler
            .reconcile(&mut scene, &same)
            .unwrap()
            .mutation_impacts(),
        0
    );
    assert_eq!(scene.revision(), revision);
}

#[test]
fn nested_leaf_edit_only_mutates_that_leaf() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let initial = grouped(&scene, 1, 0.0, false);
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    scene.owned_execution_mut().take_frame_changes();
    let runtime = scene.owned_execution().runtime_identity();
    let edited = grouped(&scene, 2, 2.0, false);
    let second = reconciler.reconcile(&mut scene, &edited).unwrap();
    assert_eq!(second.mutation_impacts(), 1);
    assert_eq!(first.nodes(), second.nodes());
    assert_eq!(scene.owned_execution().runtime_identity(), runtime);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(second.node_for(&key("left")).unwrap())
            .unwrap()
            .transform
            .translation
            .x,
        2.0
    );
}

#[test]
fn reversing_nested_family_relationship_is_one_valid_transaction() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let build = |scene: &Scene, generation, outer: &'static str, inner: &'static str| {
        let mut next = candidate(scene, generation, [("leaf", 0.0)]);
        next.declare_family(SourceFamilyDeclaration::new(key(inner), [key("leaf")]))
            .unwrap();
        next.declare_family(SourceFamilyDeclaration::new(key(outer), [key(inner)]))
            .unwrap();
        next.set_root_members([key(outer)]);
        next
    };
    let initial = build(&scene, 1, "a", "b");
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let edited = build(&scene, 2, "b", "a");
    let second = reconciler.reconcile(&mut scene, &edited).unwrap();
    for name in ["leaf", "a", "b"] {
        assert_eq!(first.node_for(&key(name)), second.node_for(&key(name)));
    }
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_family_members_checked(second.node_for(&key("b")).unwrap())
            .unwrap(),
        vec![second.node_for(&key("a")).unwrap()]
    );
    assert_eq!(
        store
            .semantic_family_members_checked(second.node_for(&key("a")).unwrap())
            .unwrap(),
        vec![second.node_for(&key("leaf")).unwrap()]
    );
}

#[test]
fn malformed_family_candidates_preserve_prior_publication_and_generation() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let initial = grouped(&scene, 1, 0.0, false);
    reconciler.reconcile(&mut scene, &initial).unwrap();
    let revision = scene.revision();
    for members in [
        vec![key("group")],
        vec![key("missing")],
        vec![key("left"), key("left")],
    ] {
        let mut invalid = candidate(&scene, 2, [("left", 99.0), ("right", 4.0)]);
        invalid
            .declare_family(SourceFamilyDeclaration::new(key("group"), members))
            .unwrap();
        invalid.set_root_members([key("group")]);
        assert!(matches!(
            reconciler.reconcile(&mut scene, &invalid),
            Err(SourceReconciliationError::InvalidCandidate(_))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            reconciler.accepted_generation(),
            Some(SourceGeneration::new(1))
        );
    }
    let mut invalid = candidate(&scene, 2, [("left", 0.0)]);
    assert!(matches!(
        invalid.declare_family(SourceFamilyDeclaration::new(key("left"), [])),
        Err(SourceCandidateError::DuplicateSourceIdentity(_))
    ));
}

#[test]
fn deleting_one_family_keeps_a_shared_leaf_in_its_surviving_family() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let mut initial = candidate(&scene, 1, [("leaf", 0.0)]);
    for name in ["a", "b"] {
        initial
            .declare_family(SourceFamilyDeclaration::new(key(name), [key("leaf")]))
            .unwrap();
    }
    initial.set_root_members([key("a"), key("b")]);
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let mut edited = candidate(&scene, 2, [("leaf", 0.0)]);
    edited
        .declare_family(SourceFamilyDeclaration::new(key("b"), [key("leaf")]))
        .unwrap();
    edited.set_root_members([key("b")]);
    let second = reconciler.reconcile(&mut scene, &edited).unwrap();
    assert_eq!(first.node_for(&key("leaf")), second.node_for(&key("leaf")));
    let store = scene.integration_store().borrow();
    assert!(store.node(first.node_for(&key("a")).unwrap()).is_none());
    assert_eq!(
        store
            .node(second.node_for(&key("leaf")).unwrap())
            .unwrap()
            .parents(),
        &[second.node_for(&key("b")).unwrap()]
    );
}

#[test]
fn a_node_shared_outside_the_scope_is_never_deleted_or_modified() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let initial = candidate(&scene, 1, [("leaf", 0.0)]);
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let node = first.node_for(&key("leaf")).unwrap();
    let external = {
        let mut store = scene.integration_store().borrow_mut();
        let family = store.insert_family();
        store.add_member(family, node).unwrap();
        family
    };
    let revision = scene.revision();
    let empty = candidate(&scene, 2, []);
    assert!(
        matches!(reconciler.reconcile(&mut scene, &empty), Err(SourceReconciliationError::SharedOutsideScope { node: actual }) if actual == node)
    );
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(external)
            .unwrap(),
        vec![node]
    );
}

#[test]
fn moving_one_key_in_a_large_family_emits_only_one_reorder() {
    let mut scene = Scene::new();
    let mut reconciler = SourceReconciler::new();
    let build = |scene: &Scene, generation: u64, rotate: bool| {
        let mut next = SourceCandidate::new(scene, SourceGeneration::new(generation));
        let mut roots = Vec::new();
        for i in 0..1000 {
            let source = key(&format!("item-{i}"));
            roots.push(source.clone());
            next.declare(SourceObjectDeclaration::new(
                source,
                SemanticObjectState::new(noon_core::StoredGeometry::Circle { radius: 1.0 }),
            ))
            .unwrap();
        }
        if rotate {
            roots.rotate_right(1);
        }
        next.set_root_members(roots);
        next
    };
    let initial = build(&scene, 1, false);
    let first = reconciler.reconcile(&mut scene, &initial).unwrap();
    let edited = build(&scene, 2, true);
    let (transaction, changed) = topology::stage_candidate(&scene, &edited).unwrap();
    assert!(changed);
    assert_eq!(transaction.mutations().len(), 1);
    assert!(matches!(
        transaction.mutations()[0],
        noon_core::SemanticMutation::ReorderMember { .. }
    ));
    let result = reconciler.reconcile(&mut scene, &edited).unwrap();
    assert_eq!(first.nodes(), result.nodes());
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(scene.root())
        .unwrap();
    assert_eq!(members[0], result.node_for(&key("item-999")).unwrap());
    assert_eq!(members[1], result.node_for(&key("item-0")).unwrap());
}
