//! Scene and integration publication of composed scopes, without a transport.
mod composed_effect_animation;
use noon::integration::scene_execution;
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
fn attach(scene: &mut Scene, owner: &MobjectFamily) -> SemanticNodeId {
    scene
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
    let mut session = scene.into_execution().unwrap();
    let original_leaf = session.session().execution_object_id(a.node_id()).unwrap();
    assert!(session
        .session()
        .execution_object_id(outer.node_id())
        .is_none());
    let outer_effect = attach(session.scene_mut(), &outer);
    let outer_object = session
        .session()
        .execution_object_id(outer.node_id())
        .unwrap();
    assert_eq!(
        session
            .session()
            .last_structural_publication_stats()
            .entered_objects,
        1
    );
    let inner_effect = attach(session.scene_mut(), &inner);
    assert_eq!(
        plan(session.session(), inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    assert_eq!(
        plan(session.session(), outer.node_id()).parent_scope(),
        None
    );
    assert_eq!(
        session.session().execution_object_id(a.node_id()),
        Some(original_leaf)
    );
    session.session_mut().take_renderer_publication();
    let mut update = SemanticMutationTransaction::new();
    update.update_effect(inner_effect, EffectUpdate::default().radius(Pixels(3.0)));
    scene_execution::publish(session.scene_mut(), update).unwrap();
    assert_eq!(
        session
            .session()
            .last_structural_publication_stats()
            .preparation
            .object_states_lowered,
        0
    );
    assert_eq!(
        session
            .session_mut()
            .take_renderer_publication()
            .changes()
            .object_indices()
            .len(),
        1
    );
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(outer_effect);
    scene_execution::publish(session.scene_mut(), remove).unwrap();
    assert!(row(session.session(), outer.node_id()).is_none());
    assert_eq!(
        plan(session.session(), inner.node_id()).parent_scope(),
        None
    );
    assert!(row(session.session(), a.node_id()).is_some());
    let new_effect = attach(session.scene_mut(), &outer);
    assert_ne!(new_effect, outer_effect);
    assert_eq!(
        session.session().execution_object_id(outer.node_id()),
        Some(outer_object)
    );
    assert_eq!(
        plan(session.session(), inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    let root = session.scene().root();
    let mut detach = SemanticMutationTransaction::new();
    detach.remove_member(root, outer.node_id());
    scene_execution::publish(session.scene_mut(), detach).unwrap();
    for node in [
        a.node_id(),
        b.node_id(),
        c.node_id(),
        inner.node_id(),
        outer.node_id(),
    ] {
        assert!(
            row(session.session(), node).is_none(),
            "detaching a scope must retire its complete execution subtree"
        );
    }
    assert!(row(session.session(), outside.node_id()).is_some());
    let mut reattach = SemanticMutationTransaction::new();
    reattach.add_member(root, outer.node_id());
    scene_execution::publish(session.scene_mut(), reattach).unwrap();
    assert_eq!(
        session.session().execution_object_id(outer.node_id()),
        Some(outer_object)
    );
    assert_eq!(
        plan(session.session(), inner.node_id()).parent_scope(),
        Some(outer_object)
    );
    assert_eq!(
        session.session().execution_object_id(a.node_id()),
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
    let mut session = scene.into_execution().unwrap();
    attach(session.scene_mut(), &outer);
    attach(session.scene_mut(), &inner);
    session
        .scene_mut()
        .add_family_members(&inner, &[(&added).into()])
        .unwrap();
    let ids = |nodes: &[SemanticNodeId]| {
        nodes
            .iter()
            .map(|node| session.session().execution_object_id(*node).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        plan(session.session(), inner.node_id()).members(),
        ids(&[a.node_id(), b.node_id(), added.node_id()])
    );
    assert_eq!(
        plan(session.session(), outer.node_id()).members(),
        ids(&[a.node_id(), b.node_id(), added.node_id(), c.node_id()])
    );
    let frame = session.session().frame().clone();
    let context = session.session().publication_context();
    let revision = session.scene().revision();
    let mut alias = SemanticMutationTransaction::new();
    alias.add_member(session.scene().root(), a.node_id());
    assert!(scene_execution::publish(session.scene_mut(), alias).is_err());
    assert_eq!(session.scene().revision(), revision);
    assert_eq!(session.session().publication_context(), context);
    assert_eq!(session.session().frame(), &frame);
    session
        .scene_mut()
        .remove_family_members(&inner, &[(&a).into()])
        .unwrap();
    assert!(row(session.session(), a.node_id()).is_none());
    assert_eq!(plan(session.session(), inner.node_id()).members().len(), 2);
    assert_eq!(plan(session.session(), outer.node_id()).members().len(), 3);
}

#[test]
fn first_group_attachment_rejects_external_interleaving_before_semantic_commit() {
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.circle(0.4).unwrap();
    let c = scene.circle(0.4).unwrap();
    let family = scene.family(&[(&a).into(), (&c).into()]).unwrap();
    scene.add_many(&[(&family).into(), (&b).into()]).unwrap();
    let mut session = scene_execution::lower(&scene).unwrap();
    let mut priorities = SemanticMutationTransaction::new();
    priorities
        .set_z_index(b.node_id(), 1.0)
        .set_z_index(c.node_id(), 2.0);
    scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut session,
        priorities,
    )
    .unwrap();
    let frame = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(family.node_id(), "soften", Blur::default());
    let error =
        scene_execution::publish_at_root(scene.integration_store(), scene.root(), &mut session, tx)
            .unwrap_err();
    assert!(
        matches!(
            error,
            noon::ExecutionSessionPublicationError::Runtime(
                noon_runtime::AuthoredPublicationError::Compile(
                    noon_compile::CompilePatchError::InvalidComposedSource(
                        noon_compile::CompiledComposedSourceError {
                            kind: noon_compile::CompiledComposedSourceErrorKind::NonContiguousInput,
                            ..
                        }
                    )
                )
            )
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
    let mut session = scene.into_execution().unwrap();
    assert!(row(session.session(), a.node_id())
        .unwrap()
        .effects
        .is_none());
    assert!(row(session.session(), b.node_id())
        .unwrap()
        .effects
        .is_none());
    assert_eq!(
        row(session.session(), family.node_id())
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[first.node_id(), second.node_id()]
    );
    session
        .scene_mut()
        .set_effect(
            &family,
            &second,
            EffectUpdate::default().radius(Pixels(3.0)),
        )
        .unwrap_err();
    // Units cannot change through a value write; an equivalent-unit update is
    // still local and leaves both independently named generations in order.
    session
        .scene_mut()
        .set_effect(&family, &second, EffectUpdate::default().radius(0.3))
        .unwrap();
    session.scene_mut().remove_effect(&family, &first).unwrap();
    let frame = session.session().frame().clone();
    let context = session.session().publication_context();
    assert!(session
        .scene_mut()
        .set_effect(&family, &first, EffectUpdate::default().radius(0.2))
        .is_err());
    assert_eq!(session.session().frame(), &frame);
    assert_eq!(session.session().publication_context(), context);
    session.scene_mut().remove_effect(&family, &second).unwrap();
    assert!(row(session.session(), family.node_id()).is_none());
    assert!(row(session.session(), a.node_id()).is_some());
    assert!(row(session.session(), b.node_id()).is_some());
}

#[test]
fn provisional_nested_scopes_bind_one_final_identity_and_parent_graph() {
    use noon_core::{SemanticNodeCreation, SemanticObjectState, StoredGeometry};
    let scene = Scene::new();
    let mut session = scene_execution::lower(&scene).unwrap();
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
    let committed =
        scene_execution::publish_at_root(scene.integration_store(), scene.root(), &mut session, tx)
            .unwrap();
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
    scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut session,
        remove,
    )
    .unwrap();
    assert!(row(&session, inner).is_none());
    assert_eq!(plan(&session, outer).members().len(), 2);
    assert!(row(&session, a).is_some() && row(&session, b).is_some());
}

#[test]
fn initial_partial_scope_opacity_lowers_without_fabricated_effect_attachments() {
    for opacity in [0.0, 0.5, 1.0 - f64::EPSILON] {
        let mut scene = Scene::new();
        let a = scene.circle(0.4).unwrap();
        let family = scene.family(&[(&a).into()]).unwrap();
        let mut style = family.own_style().unwrap();
        style.object_opacity = opacity;
        let mut tx = SemanticMutationTransaction::new();
        tx.set_family_style(family.node_id(), style);
        tx.apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let session = scene_execution::lower(&scene).unwrap();
        let scope = row(&session, family.node_id()).unwrap();
        assert_eq!(scope.style.opacity, opacity as f32);
        assert!(scope.effects.is_none());
        assert_eq!(plan(&session, family.node_id()).members().len(), 1);
    }
}

#[test]
fn live_scope_opacity_reuses_membership_and_rejects_invalid_values_atomically() {
    let mut scene = Scene::new();
    let leaves = (0..512)
        .map(|_| scene.circle(0.2).unwrap())
        .collect::<Vec<_>>();
    let members = leaves.iter().map(Into::into).collect::<Vec<_>>();
    let group = scene.family(&members).unwrap();
    group.set_object_opacity(0.5).unwrap();
    scene.add_many(&[(&group).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    let identity = session
        .session()
        .execution_object_id(group.node_id())
        .unwrap();
    let original = row(session.session(), group.node_id())
        .unwrap()
        .content
        .clone();
    session.session_mut().take_renderer_publication();
    session
        .scene_mut()
        .set_family_object_opacity(&group, 0.25)
        .unwrap();
    assert!(std::ptr::eq(
        original.composed_source().unwrap(),
        plan(session.session(), group.node_id())
    ));
    assert_eq!(
        session
            .session()
            .last_structural_publication_stats()
            .preparation
            .object_states_lowered,
        0
    );
    assert_eq!(
        session
            .session_mut()
            .take_renderer_publication()
            .changes()
            .object_indices()
            .len(),
        1
    );
    let before = session.session().frame().objects.clone();
    for opacity in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(session
            .scene_mut()
            .set_family_object_opacity(&group, opacity)
            .is_err());
        assert_eq!(session.session().frame().objects, before);
        assert_eq!(group.own_style().unwrap().object_opacity, 0.25);
    }
    session
        .scene_mut()
        .set_family_object_opacity(&group, 1.0)
        .unwrap();
    assert!(row(session.session(), group.node_id()).is_none());
    session
        .scene_mut()
        .set_family_object_opacity(&group, 0.5)
        .unwrap();
    assert_eq!(
        session.session().execution_object_id(group.node_id()),
        Some(identity)
    );
    assert_eq!(
        plan(session.session(), group.node_id()).members().len(),
        512
    );
}

#[test]
fn unsupported_family_effect_units_and_matching_targets_reject_atomically() {
    use noon::{AnimationCompositionRequest, AnimationOptions};
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let mut family = scene.family(&[(&a).into()]).unwrap();
    family.add_effect(Blur::default(), "soften").unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    let target = session.scene_mut().copy_family(&family).unwrap();
    session
        .scene_mut()
        .set_effect(
            target.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(4.0)),
        )
        .unwrap();
    let before = session.session().frame().clone();
    let revision = session.scene().revision();
    let context = session.session().publication_context();
    for request in [
        AnimationCompositionRequest::FamilyMethodTargetTo {
            source: &family,
            target_state: target.root(),
            options: AnimationOptions::new(),
        },
        AnimationCompositionRequest::FamilyTransformTo {
            source: &family,
            target_state: target.root(),
            options: AnimationOptions::new(),
        },
        AnimationCompositionRequest::MatchingFamilyTransformTo {
            source: &family,
            target_state: target.root(),
            options: AnimationOptions::new(),
        },
        AnimationCompositionRequest::MatchingSourceFamilyTransformTo {
            source: &family,
            target_state: target.root(),
            options: AnimationOptions::new(),
        },
    ] {
        let result = session
            .live()
            .declare_and_activate_composition(&request, AnimationOptions::new());
        assert!(
            result.is_err(),
            "unsupported unit changes and matching profiles must reject"
        );
        assert_eq!(session.session().frame(), &before);
        assert_eq!(session.scene().revision(), revision);
        assert_eq!(session.session().publication_context(), context);
    }
}

#[test]
fn unchanged_family_effect_copy_can_animate_member_motion_without_rebinding_scope() {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let mut family = scene.family(&[(&a).into()]).unwrap();
    family.add_effect(Blur::default(), "soften").unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let original = family.get_effect("soften").unwrap();
    let mut session = scene.into_execution().unwrap();
    let owner = session
        .session()
        .execution_object_id(family.node_id())
        .unwrap();
    let target = session.scene_mut().copy_family(&family).unwrap();
    session
        .scene_mut()
        .shift_family(target.root(), 2.0, 0.0)
        .unwrap();
    let segment = session
        .live()
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &family,
                target_state: target.root(),
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    session
        .session_mut()
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(
        row(session.session(), a.node_id())
            .unwrap()
            .transform
            .translation
            .x,
        1.0
    );
    assert_eq!(
        session.session().execution_object_id(family.node_id()),
        Some(owner)
    );
    assert_eq!(
        row(session.session(), family.node_id())
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[original.node_id()]
    );
}
