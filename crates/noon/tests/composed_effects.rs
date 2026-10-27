//! Public Scene/LiveSession publication of composed scopes, without a transport.
use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    ExecutionSession, MobjectFamily, Scene,
};
use noon_core::{ComposedSource, SemanticMutationTransaction, SemanticNodeId};

fn row(
    session: &ExecutionSession,
    node: SemanticNodeId,
) -> Option<&noon_runtime::FrameObjectState> {
    let object = session.execution_object_id(node)?;
    session
        .frame()
        .objects
        .iter()
        .enumerate()
        .find_map(|(index, row)| {
            (row.id == object && session.frame().is_present(index)).then_some(row)
        })
}
fn plan(session: &ExecutionSession, node: SemanticNodeId) -> &ComposedSource {
    row(session, node)
        .unwrap()
        .content
        .composed_source()
        .unwrap()
}
fn attach(
    scene: &mut Scene,
    session: &mut ExecutionSession,
    owner: &MobjectFamily,
) -> SemanticNodeId {
    scene
        .live(session)
        .add_effect(
            owner,
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    owner.get_effect("soften").unwrap().node_id()
}

#[test]
fn live_group_attachment_nested_containment_detach_and_reattach_share_execution_identity() {
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.square(0.6).unwrap();
    let c = scene.circle(0.3).unwrap();
    let outside = scene.square(0.3).unwrap();
    let inner = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let outer = scene.family(&[(&inner).into(), (&c).into()]).unwrap();
    scene
        .add_many(&[(&outer).into(), (&outside).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let original_leaf = session.execution_object_id(a.node_id()).unwrap();
    assert!(session.execution_object_id(outer.node_id()).is_none());
    let outer_effect = attach(&mut scene, &mut session, &outer);
    let outer_object = session.execution_object_id(outer.node_id()).unwrap();
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
    let inner_effect = attach(&mut scene, &mut session, &inner);
    assert_eq!(
        plan(&session, inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    assert_eq!(plan(&session, outer.node_id()).parent_scope(), None);
    assert_eq!(
        session.execution_object_id(a.node_id()),
        Some(original_leaf)
    );
    session.take_renderer_publication();
    let mut update = SemanticMutationTransaction::new();
    update.update_effect(inner_effect, EffectUpdate::default().radius(Pixels(3.0)));
    scene.live(&mut session).apply(update).unwrap();
    assert_eq!(
        session
            .last_structural_publication_stats()
            .preparation
            .object_states_lowered,
        0
    );
    assert_eq!(
        session
            .take_renderer_publication()
            .changes()
            .object_indices()
            .len(),
        1
    );
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(outer_effect);
    scene.live(&mut session).apply(remove).unwrap();
    assert!(row(&session, outer.node_id()).is_none());
    assert_eq!(plan(&session, inner.node_id()).parent_scope(), None);
    assert!(row(&session, a.node_id()).is_some());
    let new_effect = attach(&mut scene, &mut session, &outer);
    assert_ne!(new_effect, outer_effect);
    assert_eq!(
        session.execution_object_id(outer.node_id()),
        Some(outer_object)
    );
    assert_eq!(
        plan(&session, inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    let root = scene.root();
    let mut detach = SemanticMutationTransaction::new();
    detach.remove_member(root, outer.node_id());
    scene.live(&mut session).apply(detach).unwrap();
    for node in [
        a.node_id(),
        b.node_id(),
        c.node_id(),
        inner.node_id(),
        outer.node_id(),
    ] {
        assert!(
            row(&session, node).is_none(),
            "detaching a scope must retire its complete execution subtree"
        );
    }
    assert!(row(&session, outside.node_id()).is_some());
    let mut reattach = SemanticMutationTransaction::new();
    reattach.add_member(root, outer.node_id());
    scene.live(&mut session).apply(reattach).unwrap();
    assert_eq!(
        session.execution_object_id(outer.node_id()),
        Some(outer_object)
    );
    assert_eq!(
        plan(&session, inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    assert_eq!(
        session.execution_object_id(a.node_id()),
        Some(original_leaf)
    );
}

#[test]
fn live_membership_relowers_only_affected_scope_plans_and_rejects_aliases_atomically() {
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.circle(0.4).unwrap();
    let c = scene.circle(0.4).unwrap();
    let added = scene.square(0.4).unwrap();
    let inner = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let outer = scene.family(&[(&inner).into(), (&c).into()]).unwrap();
    scene.add_many(&[(&outer).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    attach(&mut scene, &mut session, &outer);
    attach(&mut scene, &mut session, &inner);
    scene
        .live(&mut session)
        .add_family_members(&inner, &[(&added).into()])
        .unwrap();
    let ids = |nodes: &[SemanticNodeId]| {
        nodes
            .iter()
            .map(|node| session.execution_object_id(*node).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        plan(&session, inner.node_id()).members(),
        ids(&[a.node_id(), b.node_id(), added.node_id()])
    );
    assert_eq!(
        plan(&session, outer.node_id()).members(),
        ids(&[a.node_id(), b.node_id(), added.node_id(), c.node_id()])
    );
    let frame = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    let mut alias = SemanticMutationTransaction::new();
    alias.add_member(scene.root(), a.node_id());
    assert!(scene.live(&mut session).apply(alias).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame(), &frame);
    scene
        .live(&mut session)
        .remove_family_members(&inner, &[(&a).into()])
        .unwrap();
    assert!(row(&session, a.node_id()).is_none());
    assert_eq!(plan(&session, inner.node_id()).members().len(), 2);
    assert_eq!(plan(&session, outer.node_id()).members().len(), 3);
}

#[test]
fn first_group_attachment_rejects_external_interleaving_before_semantic_commit() {
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.circle(0.4).unwrap();
    let c = scene.circle(0.4).unwrap();
    let family = scene.family(&[(&a).into(), (&c).into()]).unwrap();
    scene.add_many(&[(&family).into(), (&b).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut priorities = SemanticMutationTransaction::new();
    priorities
        .set_z_index(b.node_id(), 1.0)
        .set_z_index(c.node_id(), 2.0);
    scene.live(&mut session).apply(priorities).unwrap();
    let frame = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(family.node_id(), "soften", Blur::default());
    let error = scene.live(&mut session).apply(tx).unwrap_err();
    assert!(
        matches!(
            error,
            noon::LiveSessionError::Publication(noon::ExecutionSessionPublicationError::Runtime(
                noon_runtime::AuthoredPublicationError::Compile(
                    noon_compile::CompilePatchError::InvalidComposedSource(
                        noon_compile::CompiledComposedSourceError {
                            kind: noon_compile::CompiledComposedSourceErrorKind::NonContiguousInput,
                            ..
                        }
                    )
                )
            ))
        ),
        "the full proposed capture graph must reject the actual interleaving"
    );
    assert_eq!(scene.revision(), revision);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame(), &frame);
    assert!(scene
        .integration_store()
        .borrow()
        .node(family.node_id())
        .unwrap()
        .effect_ids()
        .is_empty());
    assert!(session.execution_object_id(family.node_id()).is_none());
}

#[test]
fn initial_named_family_effects_use_one_controller_and_exact_handle_removal() {
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.square(0.4).unwrap();
    let mut family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    family.add_effect(Blur::default(), "first").unwrap();
    scene
        .add_effect(&family, Blur::default(), "second")
        .unwrap();
    let first = family.get_effect("first").unwrap();
    let second = scene.get_effect(&family, "second").unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    assert!(row(&session, a.node_id()).unwrap().effects.is_none());
    assert!(row(&session, b.node_id()).unwrap().effects.is_none());
    assert_eq!(
        row(&session, family.node_id())
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[first.node_id(), second.node_id()]
    );
    scene
        .live(&mut session)
        .set_effect(
            &family,
            &second,
            EffectUpdate::default().radius(Pixels(3.0)),
        )
        .unwrap_err();
    // Units cannot change through a value write; an equivalent-unit update is
    // still local and leaves both independently named generations in order.
    scene
        .live(&mut session)
        .set_effect(&family, &second, EffectUpdate::default().radius(0.3))
        .unwrap();
    scene
        .live(&mut session)
        .remove_effect(&family, &first)
        .unwrap();
    let frame = session.frame().clone();
    let context = session.publication_context();
    assert!(scene
        .live(&mut session)
        .set_effect(&family, &first, EffectUpdate::default().radius(0.2))
        .is_err());
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), context);
    scene
        .live(&mut session)
        .remove_effect(&family, &second)
        .unwrap();
    assert!(row(&session, family.node_id()).is_none());
    assert!(row(&session, a.node_id()).is_some());
    assert!(row(&session, b.node_id()).is_some());
}

#[test]
fn provisional_nested_scopes_bind_one_final_identity_and_parent_graph() {
    use noon_core::{SemanticNodeCreation, SemanticObjectState, StoredGeometry};
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut tx = SemanticMutationTransaction::new();
    // Allocate outer before inner and put both definitions before their inputs.
    // Final semantic containment, never allocation order, owns the pass graph.
    let outer = tx.create_node(SemanticNodeCreation::family());
    let inner = tx.create_node(SemanticNodeCreation::family());
    let a = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.4 },
    )));
    let b = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.3 },
    )));
    tx.create_effect(outer, "soften", Blur::default());
    let inner_effect = tx.create_effect(inner, "soften", Blur::default());
    tx.add_member(inner, a)
        .add_member(inner, b)
        .add_member(outer, inner)
        .add_member(scene.root(), outer);
    let committed = scene.live(&mut session).apply(tx).unwrap();
    let outer = committed.resolve(outer).unwrap();
    let inner = committed.resolve(inner).unwrap();
    let a = committed.resolve(a).unwrap();
    let b = committed.resolve(b).unwrap();
    assert_eq!(
        plan(&session, inner).members(),
        plan(&session, outer).members()
    );
    assert_eq!(
        plan(&session, inner).members(),
        &[
            session.execution_object_id(a).unwrap(),
            session.execution_object_id(b).unwrap()
        ]
    );
    assert_eq!(
        plan(&session, inner).parent_scope(),
        session.execution_object_id(outer)
    );
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        4
    );
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(committed.resolve(inner_effect).unwrap());
    scene.live(&mut session).apply(remove).unwrap();
    assert!(row(&session, inner).is_none());
    assert_eq!(plan(&session, outer).members().len(), 2);
    assert!(row(&session, a).is_some() && row(&session, b).is_some());
}
