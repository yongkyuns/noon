use super::*;
use crate::{
    Color, GlowUpdate, Pixels, SemanticMutationImpact, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticNodeCreation, SemanticObjectProperty,
    SemanticObjectState, SemanticVec3, StoredGeometry,
};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let token = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.apply(store).unwrap().resolve(token).unwrap()
}
fn attach(store: &mut SemanticStore, owner: SemanticNodeId, name: &str) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let token = tx.create_effect(owner, name, Glow::default());
    tx.apply(store).unwrap().resolve(token).unwrap()
}
fn glow(store: &SemanticStore, id: SemanticNodeId) -> Glow {
    let EffectDefinition::Glow(glow) = store.semantic_effect_state(id).unwrap().definition();
    glow
}

#[test]
fn creation_shares_allocator_and_atomic_pending_owner() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let owner = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let halo = tx.create_effect(owner, "glow", Glow::default());
    assert_eq!(store.len(), 0);
    let result = tx.apply(&mut store).unwrap();
    let (owner, halo) = (
        result.resolve(owner).unwrap(),
        result.resolve(halo).unwrap(),
    );
    assert_ne!(owner, halo);
    assert_eq!(store.len(), 2);
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[halo]);
    assert_eq!(store.semantic_effect_state(halo).unwrap().owner(), owner);
    assert_eq!(store.effect_by_name(owner, "glow").unwrap(), Some(halo));
    assert_eq!(store.node(owner).unwrap().member_count(), 0);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert!(result
        .impacts()
        .contains(&SemanticMutationImpact::EffectAttachment {
            owner,
            effect: halo
        }));
}

#[test]
fn attachments_keep_insertion_order_and_are_not_painter_members() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let first = attach(&mut store, owner, "glow");
    let second = attach(&mut store, owner, "accent");
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[first, second]);
    let family = store.insert_family();
    let mut tx = SemanticMutationTransaction::new();
    tx.add_member(family, first);
    assert!(tx.apply(&mut store).is_err());
    assert!(store.add_member(family, first).is_err());
    assert!(store.attach_to_scene(first).is_err());
    assert_eq!(store.node(family).unwrap().member_count(), 0);
}

#[test]
fn duplicate_name_and_invalid_batch_do_not_consume_ids_or_revision() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let first = attach(&mut store, owner, "glow");
    let before = (
        store.len(),
        store.scene_revision(),
        store.preview_node_allocations().next(),
    );
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate.create_effect(owner, "glow", Glow::default());
    assert!(duplicate.apply(&mut store).is_err());
    let mut batch = SemanticMutationTransaction::new();
    batch.create_effect(owner, "accent", Glow::default());
    batch.create_effect(owner, "accent", Glow::default());
    assert!(batch.apply(&mut store).is_err());
    assert_eq!(
        before,
        (
            store.len(),
            store.scene_revision(),
            store.preview_node_allocations().next()
        )
    );
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[first]);
}

#[test]
fn parameter_mutation_touches_one_slot_and_preserves_other_parameters() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let other_owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let other = attach(&mut store, other_owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default().intensity(1.4));
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::EffectParameters {
            owner,
            effect: halo
        }]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(glow(&store, halo).intensity(), 1.4);
    assert_eq!(glow(&store, halo).radius(), Glow::default().radius());
    assert_eq!(glow(&store, other), Glow::default());
    let before = store.scene_revision();
    let mut empty = SemanticMutationTransaction::new();
    empty.update_effect(halo, GlowUpdate::default());
    assert!(empty.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), before);
}

#[test]
fn failed_parameter_update_rolls_back_other_ordinary_edits() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let before = (
        store.scene_revision(),
        store.semantic_object_state_checked(owner).unwrap().clone(),
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        owner,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(2.0, 0.0, 0.0),
    );
    tx.update_effect(
        halo,
        GlowUpdate::default().color(Color::RED).intensity(-1.0),
    );
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::EffectParameter { .. })
    ));
    assert_eq!(store.scene_revision(), before.0);
    assert_eq!(
        store.semantic_object_state_checked(owner).unwrap(),
        &before.1
    );
    assert_eq!(glow(&store, halo), Glow::default());
}

#[test]
fn repeated_parameter_writes_reject_without_panicking() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default().intensity(0.5));
    tx.update_effect(halo, GlowUpdate::default().radius(Pixels(10.0)));
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateTarget { .. })
    ));
    assert_eq!(glow(&store, halo), Glow::default());
}

#[test]
fn remove_readd_same_name_reuses_only_a_new_generation() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(halo);
    tx.apply(&mut store).unwrap();
    assert!(store.node(owner).unwrap().effect_ids().is_empty());
    assert!(store.node(owner).unwrap().effects.is_none());
    let replacement = attach(&mut store, owner, "glow");
    assert_eq!(replacement.slot(), halo.slot());
    assert_ne!(replacement.generation(), halo.generation());
    let mut stale = SemanticMutationTransaction::new();
    stale.update_effect(halo, GlowUpdate::default().intensity(4.0));
    assert!(stale.apply(&mut store).is_err());
    assert_eq!(glow(&store, replacement), Glow::default());
}

#[test]
fn owner_retirement_cascades_attachments_but_not_unrelated_owners() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let other = object(&mut store);
    let first = attach(&mut store, owner, "glow");
    let second = attach(&mut store, owner, "accent");
    let survivor = attach(&mut store, other, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(owner);
    tx.apply(&mut store).unwrap();
    for id in [owner, first, second] {
        assert!(store.node(id).is_none());
    }
    assert_eq!(store.node(other).unwrap().effect_ids(), &[survivor]);
    assert_eq!(store.len(), 2);
}

#[test]
fn direct_low_level_retirement_also_preserves_attachment_integrity() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    store.remove_node(halo).unwrap();
    assert!(store.node(owner).unwrap().effect_ids().is_empty());
    let halo = attach(&mut store, owner, "glow");
    store.remove_node(owner).unwrap();
    assert!(store.node(halo).is_none());
    assert_eq!(store.len(), 0);
}

#[test]
fn canceled_pending_owner_cancels_attachment_allocation() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let owner = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let halo = tx.create_effect(owner, "glow", Glow::default());
    tx.remove_node(owner);
    let before = store.scene_revision();
    let result = tx.apply(&mut store).unwrap();
    assert!(result.resolve(owner).is_none());
    assert!(result.resolve(halo).is_none());
    assert_eq!(store.len(), 0);
    assert_eq!(store.scene_revision(), before);
}

#[test]
fn foreign_pending_owner_and_family_owner_are_rejected() {
    let mut store = SemanticStore::new();
    let mut foreign = SemanticMutationTransaction::new();
    let owner = foreign.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(owner, "glow", Glow::default());
    assert!(tx.apply(&mut store).is_err());
    let family = store.insert_family();
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(family, "glow", Glow::default());
    assert!(tx.apply(&mut store).is_err());
    assert!(store.node(family).unwrap().effect_ids().is_empty());
}

#[test]
fn copy_uses_one_transaction_and_independent_attachment_identities() {
    let mut store = SemanticStore::new();
    let original = object(&mut store);
    let first = attach(&mut store, original, "glow");
    let second = attach(&mut store, original, "accent");
    let mut tx = SemanticMutationTransaction::new();
    let copied = tx.create_node(SemanticNodeCreation::object(
        store
            .semantic_object_state_checked(original)
            .unwrap()
            .clone(),
    ));
    store.copy_effects_into(original, copied, &mut tx).unwrap();
    let result = tx.apply(&mut store).unwrap();
    let copied = result.resolve(copied).unwrap();
    let copy_ids = store.node(copied).unwrap().effect_ids().to_vec();
    assert_eq!(copy_ids.len(), 2);
    assert_ne!(copy_ids[0], first);
    assert_ne!(copy_ids[1], second);
    assert_eq!(
        store.semantic_effect_state(copy_ids[1]).unwrap().name(),
        "accent"
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(copy_ids[0], GlowUpdate::default().intensity(2.0));
    tx.apply(&mut store).unwrap();
    assert_eq!(glow(&store, first).intensity(), 0.35);
    assert_eq!(glow(&store, copy_ids[0]).intensity(), 2.0);
}

#[test]
fn canceled_duplicate_creation_does_not_replace_the_existing_binding() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let original = attach(&mut store, owner, "glow");
    let before = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    let canceled = tx.create_effect(owner, "glow", Glow::default());
    tx.remove_node(canceled);
    let result = tx.apply(&mut store).unwrap();
    assert!(result.resolve(canceled).is_none());
    assert_eq!(store.scene_revision(), before);
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[original]);
}

#[test]
fn cloned_store_and_direct_object_copy_keep_the_same_ownership_invariants() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let original = attach(&mut store, owner, "glow");
    let copied = store.copy_semantic_object(owner).unwrap();
    let copied_effect = store.effect_by_name(copied, "glow").unwrap().unwrap();
    assert_ne!(copied_effect, original);
    assert_eq!(
        store.semantic_effect_state(copied_effect).unwrap().owner(),
        copied
    );
    let mut independent = store.clone();
    assert!(independent.has_effect_attachments());
    independent.remove_node(owner).unwrap();
    independent.remove_node(copied).unwrap();
    assert!(!independent.has_effect_attachments());
    assert!(store.has_effect_attachments());
    assert!(store.node(original).is_some());
}
