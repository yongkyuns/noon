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
        &[SemanticMutationImpact::EffectParameter {
            owner,
            effect: halo,
            parameter: crate::GlowParameter::Intensity,
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
fn overlapping_parameter_writes_reject_without_panicking() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default().intensity(0.5));
    tx.update_effect(
        halo,
        GlowUpdate::default().radius(Pixels(10.0)).intensity(0.5),
    );
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateEffectParameter {
            effect, parameter: crate::GlowParameter::Intensity, ..
        }) if effect == halo
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

#[test]
fn independent_parameter_updates_compose_in_one_ordinary_transaction() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        owner,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(2.0, 0.0, 0.0),
    );
    tx.update_effect(halo, GlowUpdate::default().intensity(1.4));
    tx.update_effect(halo, GlowUpdate::default().radius(Pixels(12.0)));
    tx.apply(&mut store).unwrap();
    assert_eq!(glow(&store, halo).intensity(), 1.4);
    assert_eq!(glow(&store, halo).radius(), crate::GlowRadius::Pixels(12.0));
    assert_eq!(
        store
            .semantic_object_state_checked(owner)
            .unwrap()
            .transform
            .translation
            .x,
        2.0
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn dirty_parameter_impacts_exclude_explicit_unchanged_channels() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(
        halo,
        GlowUpdate::default().intensity(0.35).radius(Pixels(12.0)),
    );
    tx.update_effect(halo, GlowUpdate::default().color(Color::WHITE));
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::EffectParameter {
            owner,
            effect: halo,
            parameter: crate::GlowParameter::Radius,
        }]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn unchanged_explicit_write_still_conflicts_with_another_writer() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default().intensity(0.35));
    tx.update_effect(halo, GlowUpdate::default().intensity(0.35));
    assert!(matches!(tx.apply(&mut store), Err(
        SemanticMutationTransactionError::DuplicateEffectParameter {
            index: 1, effect, parameter: crate::GlowParameter::Intensity,
        }) if effect == halo));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(glow(&store, halo), Glow::default());
}

#[test]
fn empty_requests_own_no_channels_but_still_validate_identity() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default());
    tx.update_effect(halo, GlowUpdate::default());
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, GlowUpdate::default());
    tx.update_effect(halo, GlowUpdate::default().intensity(1.4));
    tx.update_effect(halo, GlowUpdate::default());
    tx.apply(&mut store).unwrap();
    assert_eq!(glow(&store, halo).intensity(), 1.4);
    store.remove_node(halo).unwrap();
    let mut stale = SemanticMutationTransaction::new();
    stale.update_effect(halo, GlowUpdate::default());
    assert!(stale.apply(&mut store).is_err());
}

#[test]
fn later_invalid_disjoint_parameter_rolls_back_the_complete_transaction() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let revision = store.scene_revision();
    let original = store.semantic_object_state_checked(owner).unwrap().clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        owner,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(2.0, 0.0, 0.0),
    );
    tx.update_effect(halo, GlowUpdate::default().intensity(1.4));
    tx.update_effect(halo, GlowUpdate::default().radius(-1.0));
    assert!(matches!(tx.apply(&mut store), Err(
        SemanticMutationTransactionError::EffectParameter {
            index: 2, effect, error: crate::GlowParameterError::InvalidRadius,
        }) if effect == halo));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_object_state_checked(owner).unwrap(),
        &original
    );
    assert_eq!(glow(&store, halo), Glow::default());
}

#[test]
fn every_disjoint_parameter_order_preserves_earlier_updates() {
    use crate::{GlowParameter, GlowRadius, GlowSource};
    let updates = [
        GlowUpdate::default().color(Color::RED),
        GlowUpdate::default().radius(Pixels(12.0)),
        GlowUpdate::default().intensity(1.4),
        GlowUpdate::default().source(GlowSource::Silhouette),
    ];
    let parameters = [
        GlowParameter::Color,
        GlowParameter::Radius,
        GlowParameter::Intensity,
        GlowParameter::Source,
    ];
    let mut cases = 0;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if order
                        .iter()
                        .enumerate()
                        .any(|(i, value)| order[..i].contains(value))
                    {
                        continue;
                    }
                    let mut store = SemanticStore::new();
                    let owner = object(&mut store);
                    let halo = attach(&mut store, owner, "glow");
                    let mut tx = SemanticMutationTransaction::new();
                    for index in order {
                        tx.update_effect(halo, updates[index]);
                    }
                    let result = tx.apply(&mut store).unwrap();
                    let expected: Vec<_> = order
                        .into_iter()
                        .map(|index| SemanticMutationImpact::EffectParameter {
                            owner,
                            effect: halo,
                            parameter: parameters[index],
                        })
                        .collect();
                    assert_eq!(result.impacts(), expected);
                    let value = glow(&store, halo);
                    assert_eq!(value.color(), Color::RED);
                    assert_eq!(value.radius(), GlowRadius::Pixels(12.0));
                    assert_eq!(value.intensity(), 1.4);
                    assert_eq!(value.source(), GlowSource::Silhouette);
                    assert_eq!(store.last_mutation_stats().slots_written, 1);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 24);
}

#[test]
fn identical_parameter_names_on_different_attachments_do_not_conflict() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let first = attach(&mut store, owner, "glow");
    let second = attach(&mut store, owner, "accent");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(first, GlowUpdate::default().intensity(1.4));
    tx.update_effect(second, GlowUpdate::default().intensity(0.1));
    tx.apply(&mut store).unwrap();
    assert_eq!(glow(&store, first).intensity(), 1.4);
    assert_eq!(glow(&store, second).intensity(), 0.1);
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[first, second]);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}
