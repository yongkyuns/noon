use super::*;
use crate::{
    Color, EffectUpdate, Pixels, SemanticMutationImpact, SemanticMutationTransaction,
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
fn effect(store: &SemanticStore, id: SemanticNodeId) -> Glow {
    let effect = store
        .semantic_effect_state(id)
        .unwrap()
        .definition()
        .as_glow()
        .unwrap();
    effect
}

#[test]
fn visibility_key_is_distinct_and_pending_named_filters_precede_it() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let owner = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let visibility = tx.create_visibility_effect(owner, NoiseDissolve::default());
    let named_visibility = tx.create_effect(owner, "visibility", Glow::default());
    let named_dissolve = tx.create_effect(owner, "dissolve", Blur::default());
    let prepared = tx.prepare(&mut store).unwrap();
    let planned = prepared.animation_effect_snapshot(owner).unwrap();
    assert_eq!(
        planned.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
        vec![
            "visibility".into(),
            "dissolve".into(),
            SemanticEffectName::Visibility,
        ]
    );
    assert_eq!(prepared.store().len(), 0);
    let result = prepared.commit();
    let owner = result.resolve(owner).unwrap();
    let visibility = result.resolve(visibility).unwrap();
    let named_visibility = result.resolve(named_visibility).unwrap();
    let named_dissolve = result.resolve(named_dissolve).unwrap();
    assert_eq!(
        store.node(owner).unwrap().effect_ids(),
        &[named_visibility, named_dissolve, visibility]
    );
    assert_eq!(store.animation_effect_snapshot(owner).unwrap(), planned);
    assert_eq!(
        store.effect_by_name(owner, "visibility").unwrap(),
        Some(named_visibility)
    );
    assert_eq!(
        store.effect_by_name(owner, "dissolve").unwrap(),
        Some(named_dissolve)
    );
    assert_eq!(store.effect_by_name(owner, "").unwrap(), None);
    assert_eq!(store.visibility_effect(owner).unwrap(), Some(visibility));
    assert_eq!(
        store.semantic_effect_state(visibility).unwrap().name(),
        None
    );
}

#[test]
fn named_edits_follow_authored_order_and_cannot_bypass_or_replace_visibility() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let first = attach(&mut store, owner, "first");
    let mut tx = SemanticMutationTransaction::new();
    let visibility =
        tx.create_visibility_effect(owner, NoiseDissolve::default().with_progress(1.0).unwrap());
    let visibility = tx.apply(&mut store).unwrap().resolve(visibility).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let second = tx.create_effect(owner, "visibility", Glow::default());
    let third = tx.create_effect(owner, "third", Shadow::default());
    let prepared = tx.prepare(&mut store).unwrap();
    let planned = prepared.animation_effect_snapshot(owner).unwrap();
    assert_eq!(
        planned.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
        vec![
            "first".into(),
            "visibility".into(),
            "third".into(),
            SemanticEffectName::Visibility,
        ]
    );
    let result = prepared.commit();
    assert_eq!(
        store.node(owner).unwrap().effect_ids(),
        &[
            first,
            result.resolve(second).unwrap(),
            result.resolve(third).unwrap(),
            visibility,
        ]
    );
    assert_eq!(store.animation_effect_snapshot(owner).unwrap(), planned);
    for definition in [Glow::default().into(), NoiseDissolve::default().into()] {
        let before = store.animation_effect_snapshot(owner).unwrap();
        let count = store.len();
        let mut bad = SemanticMutationTransaction::new();
        bad.create_effect(owner, "valid-prefix", Blur::default());
        bad.create_node(SemanticNodeCreation::Effect {
            owner: owner.into(),
            name: SemanticEffectName::Visibility,
            definition,
            copy_provenance: None,
        });
        assert!(bad.apply(&mut store).is_err());
        assert_eq!(store.len(), count);
        assert_eq!(store.animation_effect_snapshot(owner).unwrap(), before);
        assert!(store
            .effect_by_name(owner, "valid-prefix")
            .unwrap()
            .is_none());
    }
    let plain = object(&mut store);
    let mut bad = SemanticMutationTransaction::new();
    bad.create_node(SemanticNodeCreation::Effect {
        owner: plain.into(),
        name: SemanticEffectName::Visibility,
        definition: Glow::default().into(),
        copy_provenance: None,
    });
    assert!(bad.apply(&mut store).is_err());
    assert!(store.visibility_effect(plain).unwrap().is_none());
}

#[test]
fn visibility_updates_reuse_one_generation_and_retirement_rejects_stale_writes() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let named = attach(&mut store, owner, "dissolve");
    let mut tx = SemanticMutationTransaction::new();
    let pending = store
        .stage_visibility_mask(owner, NoiseDissolve::default(), &mut tx)
        .unwrap();
    let result = tx.apply(&mut store).unwrap();
    let crate::SemanticTransactionNodeRef::Pending(token) = pending else {
        panic!("new visibility binding")
    };
    let visibility = result.resolve(token).unwrap();
    let count = store.len();
    for seed in 0..128 {
        let progress = f64::from(seed % 2);
        let definition = NoiseDissolve::new(seed, 0.08)
            .unwrap()
            .with_progress(progress)
            .unwrap();
        let mut tx = SemanticMutationTransaction::new();
        assert_eq!(
            store
                .stage_visibility_mask(owner, definition, &mut tx)
                .unwrap()
                .existing(),
            Some(visibility)
        );
        let prepared = tx.prepare(&mut store).unwrap();
        assert_eq!(
            prepared
                .animation_effect_snapshot(owner)
                .unwrap()
                .last()
                .unwrap()
                .definition,
            definition.into()
        );
        prepared.commit();
        assert_eq!(store.len(), count);
        assert_eq!(store.visibility_effect(owner).unwrap(), Some(visibility));
        assert_eq!(
            store
                .semantic_effect_state(visibility)
                .unwrap()
                .definition(),
            definition.into()
        );
        assert_eq!(
            store.node(owner).unwrap().effect_ids(),
            &[named, visibility]
        );
    }
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(visibility);
    remove.apply(&mut store).unwrap();
    let mut add = SemanticMutationTransaction::new();
    let replacement = add.create_visibility_effect(owner, NoiseDissolve::default());
    let replacement = add.apply(&mut store).unwrap().resolve(replacement).unwrap();
    assert_eq!(replacement.slot(), visibility.slot());
    assert_ne!(replacement.generation(), visibility.generation());
    let before = store.animation_effect_snapshot(owner).unwrap();
    let mut stale = SemanticMutationTransaction::new();
    stale.update_effect(named, EffectUpdate::default().intensity(2.0));
    stale.update_effect(visibility, EffectUpdate::default().progress(1.0));
    assert!(stale.apply(&mut store).is_err());
    assert_eq!(store.animation_effect_snapshot(owner).unwrap(), before);
}

#[test]
fn visibility_copies_have_independent_identity_and_preserve_the_typed_role() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let authored = attach(&mut store, owner, "visibility");
    let definition = NoiseDissolve::new(7, 0.4)
        .unwrap()
        .with_progress(1.0)
        .unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let original = tx.create_visibility_effect(owner, definition);
    let original = tx.apply(&mut store).unwrap().resolve(original).unwrap();
    let direct = store.copy_semantic_object(owner).unwrap();
    let detached = object(&mut store);
    let mut tx = SemanticMutationTransaction::new();
    store.copy_effects_into(owner, detached, &mut tx).unwrap();
    let prepared = tx.prepare(&mut store).unwrap();
    let planned = prepared.animation_effect_snapshot(detached).unwrap();
    assert!(planned.last().unwrap().name.is_visibility());
    prepared.commit();
    assert_eq!(store.animation_effect_snapshot(detached).unwrap(), planned);
    let frozen = object(&mut store);
    let mut tx = SemanticMutationTransaction::new();
    store
        .snapshot_effects_into(detached, frozen, &mut tx)
        .unwrap();
    tx.apply(&mut store).unwrap();
    let ids =
        [owner, direct, detached, frozen].map(|id| store.visibility_effect(id).unwrap().unwrap());
    assert_eq!(ids[0], original);
    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4
    );
    for (owner, id) in [owner, direct, detached, frozen].into_iter().zip(ids) {
        assert_eq!(
            store.semantic_effect_state(id).unwrap().definition(),
            definition.into()
        );
        assert!(store.effect_by_name(owner, "visibility").unwrap().is_some());
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(ids[1], EffectUpdate::default().progress(0.0));
    tx.apply(&mut store).unwrap();
    assert_eq!(
        store.semantic_effect_state(original).unwrap().definition(),
        definition.into()
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(owner);
    tx.apply(&mut store).unwrap();
    assert!(store.node(original).is_none() && store.node(authored).is_none());
    assert!(ids[1..].iter().all(|id| store.node(*id).is_some()));
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
    tx.update_effect(halo, EffectUpdate::default().intensity(1.4));
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::EffectParameter {
            owner,
            effect: halo,
            parameter: crate::EffectParameter::Intensity,
        }]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(effect(&store, halo).intensity(), 1.4);
    assert_eq!(effect(&store, halo).radius(), Glow::default().radius());
    assert_eq!(effect(&store, other), Glow::default());
    let before = store.scene_revision();
    let mut empty = SemanticMutationTransaction::new();
    empty.update_effect(halo, EffectUpdate::default());
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
        EffectUpdate::default().color(Color::RED).intensity(-1.0),
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
    assert_eq!(effect(&store, halo), Glow::default());
}

#[test]
fn overlapping_parameter_writes_reject_without_panicking() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, EffectUpdate::default().intensity(0.5));
    tx.update_effect(
        halo,
        EffectUpdate::default().radius(Pixels(10.0)).intensity(0.5),
    );
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateEffectParameter {
            effect, parameter: crate::EffectParameter::Intensity, ..
        }) if effect == halo
    ));
    assert_eq!(effect(&store, halo), Glow::default());
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
    stale.update_effect(halo, EffectUpdate::default().intensity(4.0));
    assert!(stale.apply(&mut store).is_err());
    assert_eq!(effect(&store, replacement), Glow::default());
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
fn foreign_pending_owner_and_non_authoring_owner_are_rejected() {
    let mut store = SemanticStore::new();
    let mut foreign = SemanticMutationTransaction::new();
    let owner = foreign.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(owner, "glow", Glow::default());
    assert!(tx.apply(&mut store).is_err());
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(signal, "glow", Glow::default());
    assert!(tx.apply(&mut store).is_err());
    assert!(store.node(signal).unwrap().effect_ids().is_empty());
}

#[test]
fn composed_family_attachment_is_owned_once_and_preserves_child_attachments() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let child = object(&mut store);
    store.add_member(family, child).unwrap();
    let local = attach(&mut store, child, "glow");
    let composed = attach(&mut store, family, "glow");
    assert_eq!(store.node(family).unwrap().effect_ids(), &[composed]);
    assert_eq!(store.node(child).unwrap().effect_ids(), &[local]);
    assert_eq!(
        store.semantic_effect_state(composed).unwrap().owner(),
        family
    );
    let mut transaction = SemanticMutationTransaction::new();
    transaction.update_effect(composed, EffectUpdate::default().intensity(2.0));
    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.animation_effect_snapshot(family).unwrap()[0]
            .definition
            .as_glow()
            .unwrap()
            .intensity(),
        2.0
    );
    assert_eq!(effect(prepared.store(), composed), Glow::default());
    prepared.commit();
    assert_eq!(effect(&store, composed).intensity(), 2.0);
    assert_eq!(effect(&store, local), Glow::default());
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(family);
    remove.apply(&mut store).unwrap();
    assert!(store.node(composed).is_none());
    assert!(store.node(child).is_some());
    assert!(store.node(local).is_some());
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
    let prepared = tx.prepare(&mut store).unwrap();
    let pending_effects = prepared.animation_effect_snapshot(copied).unwrap();
    assert_eq!(pending_effects.len(), 2);
    for (snapshot, source) in pending_effects.iter().zip([first, second]) {
        let copy = snapshot.copy_provenance.as_ref().unwrap();
        assert_eq!(copy.source_attachment(), source);
        assert_eq!(copy.copied_definition(), &snapshot.definition);
        assert!(!copy.writes_parameter(EffectParameter::Intensity));
    }
    let result = prepared.commit();
    let copied = result.resolve(copied).unwrap();
    let copy_ids = store.node(copied).unwrap().effect_ids().to_vec();
    assert_eq!(copy_ids.len(), 2);
    assert_ne!(copy_ids[0], first);
    assert_ne!(copy_ids[1], second);
    assert_eq!(
        store.semantic_effect_state(copy_ids[1]).unwrap().name(),
        Some("accent")
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(copy_ids[0], EffectUpdate::default().intensity(2.0));
    tx.apply(&mut store).unwrap();
    assert_eq!(effect(&store, first).intensity(), 0.35);
    assert_eq!(effect(&store, copy_ids[0]).intensity(), 2.0);

    let mut tx = SemanticMutationTransaction::new();
    let frozen = tx.create_node(SemanticNodeCreation::object(
        store.semantic_object_state_checked(copied).unwrap().clone(),
    ));
    let independent = tx.create_node(SemanticNodeCreation::object(
        store.semantic_object_state_checked(copied).unwrap().clone(),
    ));
    store
        .snapshot_effects_into(copied, frozen, &mut tx)
        .unwrap();
    store
        .copy_effects_into(copied, independent, &mut tx)
        .unwrap();
    let prepared = tx.prepare(&mut store).unwrap();
    let frozen_effects = prepared.animation_effect_snapshot(frozen).unwrap();
    let frozen_intent = frozen_effects[0].copy_provenance.as_ref().unwrap();
    assert_eq!(frozen_intent.source_attachment(), first);
    assert!(frozen_intent.writes_parameter(EffectParameter::Intensity));
    let independent_effects = prepared.animation_effect_snapshot(independent).unwrap();
    let independent_intent = independent_effects[0].copy_provenance.as_ref().unwrap();
    assert_eq!(independent_intent.source_attachment(), copy_ids[0]);
    assert!(!independent_intent.writes_parameter(EffectParameter::Intensity));
    assert_eq!(
        independent_effects[0].definition,
        frozen_effects[0].definition
    );
}

#[test]
fn copied_effect_parameter_intent_is_staged_atomic_and_idempotent() {
    let mut store = SemanticStore::new();
    let original = object(&mut store);
    let original_effect = attach(&mut store, original, "glow");
    let copied = store.copy_semantic_object(original).unwrap();
    let copied_effect = store.effect_by_name(copied, "glow").unwrap().unwrap();
    let original_state = store.semantic_effect_state(copied_effect).unwrap().clone();
    let revision = store.scene_revision();
    let update = EffectUpdate::default().intensity(Glow::default().intensity());

    let mut staged = SemanticMutationTransaction::new();
    staged.update_effect(copied_effect, update.clone());
    let prepared = staged.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.proposed_scene_revision(),
        revision.checked_next().unwrap()
    );
    assert_eq!(prepared.effect_updates().count(), 0);
    assert_eq!(
        prepared
            .store()
            .semantic_effect_state(copied_effect)
            .unwrap(),
        &original_state,
        "preparing intent does not mutate authored state"
    );
    let staged_snapshot = prepared.animation_effect_snapshot(copied).unwrap();
    assert!(staged_snapshot[0]
        .copy_provenance
        .as_ref()
        .unwrap()
        .writes_parameter(EffectParameter::Intensity));
    drop(prepared);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_effect_state(copied_effect).unwrap(),
        &original_state
    );

    let mut invalid = SemanticMutationTransaction::new();
    invalid.update_effect(copied_effect, update.clone());
    invalid.update_effect(copied_effect, EffectUpdate::default().radius(-1.0));
    assert!(invalid.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_effect_state(copied_effect).unwrap(),
        &original_state
    );

    let mut accepted = SemanticMutationTransaction::new();
    accepted.update_effect(copied_effect, update.clone());
    let result = accepted.prepare(&mut store).unwrap().commit();
    assert!(
        result.impacts().is_empty(),
        "intent has no renderer parameter impact"
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(effect(&store, copied_effect), Glow::default());
    let copy = store
        .semantic_effect_state(copied_effect)
        .unwrap()
        .copy_provenance()
        .unwrap();
    assert_eq!(copy.source_attachment(), original_effect);
    assert!(copy.writes_parameter(EffectParameter::Intensity));
    assert!(!copy.writes_parameter(EffectParameter::Radius));

    let committed_revision = store.scene_revision();
    let mut repeated = SemanticMutationTransaction::new();
    repeated.update_effect(copied_effect, update.clone());
    repeated.update_effect(original_effect, update.clone());
    assert!(repeated.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), committed_revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    assert!(store
        .semantic_effect_state(original_effect)
        .unwrap()
        .copy_provenance()
        .is_none());

    store.remove_node(original).unwrap();
    assert_eq!(effect(&store, copied_effect), Glow::default());
    assert_eq!(
        store
            .semantic_effect_state(copied_effect)
            .unwrap()
            .copy_provenance(),
        Some(copy),
        "copy provenance does not retain or cascade from its weak source"
    );
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
    tx.update_effect(halo, EffectUpdate::default().intensity(1.4));
    tx.update_effect(halo, EffectUpdate::default().radius(Pixels(12.0)));
    tx.apply(&mut store).unwrap();
    assert_eq!(effect(&store, halo).intensity(), 1.4);
    assert_eq!(
        effect(&store, halo).radius(),
        crate::EffectRadius::Pixels(12.0)
    );
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
        EffectUpdate::default().intensity(0.35).radius(Pixels(12.0)),
    );
    tx.update_effect(halo, EffectUpdate::default().color(Color::WHITE));
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::EffectParameter {
            owner,
            effect: halo,
            parameter: crate::EffectParameter::Radius,
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
    tx.update_effect(halo, EffectUpdate::default().intensity(0.35));
    tx.update_effect(halo, EffectUpdate::default().intensity(0.35));
    assert!(matches!(tx.apply(&mut store), Err(
        SemanticMutationTransactionError::DuplicateEffectParameter {
            index: 1, effect, parameter: crate::EffectParameter::Intensity,
        }) if effect == halo));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(effect(&store, halo), Glow::default());
}

#[test]
fn empty_requests_own_no_channels_but_still_validate_identity() {
    let mut store = SemanticStore::new();
    let owner = object(&mut store);
    let halo = attach(&mut store, owner, "glow");
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, EffectUpdate::default());
    tx.update_effect(halo, EffectUpdate::default());
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(halo, EffectUpdate::default());
    tx.update_effect(halo, EffectUpdate::default().intensity(1.4));
    tx.update_effect(halo, EffectUpdate::default());
    tx.apply(&mut store).unwrap();
    assert_eq!(effect(&store, halo).intensity(), 1.4);
    store.remove_node(halo).unwrap();
    let mut stale = SemanticMutationTransaction::new();
    stale.update_effect(halo, EffectUpdate::default());
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
    tx.update_effect(halo, EffectUpdate::default().intensity(1.4));
    tx.update_effect(halo, EffectUpdate::default().radius(-1.0));
    assert!(matches!(tx.apply(&mut store), Err(
        SemanticMutationTransactionError::EffectParameter {
            index: 2, effect, error: crate::EffectParameterError::InvalidRadius,
        }) if effect == halo));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_object_state_checked(owner).unwrap(),
        &original
    );
    assert_eq!(effect(&store, halo), Glow::default());
}

#[test]
fn every_disjoint_parameter_order_preserves_earlier_updates() {
    use crate::{EffectParameter, EffectRadius, GlowSource};
    let updates = [
        EffectUpdate::default().color(Color::RED),
        EffectUpdate::default().radius(Pixels(12.0)),
        EffectUpdate::default().intensity(1.4),
        EffectUpdate::default().source(GlowSource::Silhouette),
    ];
    let parameters = [
        EffectParameter::Color,
        EffectParameter::Radius,
        EffectParameter::Intensity,
        EffectParameter::Source,
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
                        tx.update_effect(halo, updates[index].clone());
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
                    let value = effect(&store, halo);
                    assert_eq!(value.color(), Color::RED);
                    assert_eq!(value.radius(), EffectRadius::Pixels(12.0));
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
    tx.update_effect(first, EffectUpdate::default().intensity(1.4));
    tx.update_effect(second, EffectUpdate::default().intensity(0.1));
    tx.apply(&mut store).unwrap();
    assert_eq!(effect(&store, first).intensity(), 1.4);
    assert_eq!(effect(&store, second).intensity(), 0.1);
    assert_eq!(store.node(owner).unwrap().effect_ids(), &[first, second]);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}
