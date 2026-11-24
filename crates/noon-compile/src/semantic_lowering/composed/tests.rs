use super::*;
use crate::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{
    Blur, EffectUpdate, Glow, SemanticMutationTransaction, SemanticObjectState, StoredGeometry,
};

fn object(store: &mut SemanticStore, z: f64) -> SemanticNodeId {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
    state.set_z_index(z);
    store.insert_semantic_object(state)
}
fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let owner = store.insert_family();
    for &member in members {
        store.add_member(owner, member).unwrap();
    }
    owner
}
fn effect(store: &mut SemanticStore, owner: SemanticNodeId) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let token = tx.create_effect(owner, "soften", Blur::default());
    tx.apply(store).unwrap().resolve(token).unwrap()
}

#[test]
fn family_effect_has_one_non_drawable_typed_execution_row_and_actual_painter_inputs() {
    let mut store = SemanticStore::new();
    let high = object(&mut store, 3.0);
    let low = object(&mut store, -1.0);
    let owner = family(&mut store, &[high, low]);
    let attachment = effect(&mut store, owner);
    store.attach_to_scene(owner).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let output = lower_semantic_execution(&store, &mut index).unwrap();
    let compiled = output.compiled();
    assert_eq!(compiled.objects().len(), 3);
    let owner_id = super::super::semantic_execution_object_id(owner);
    let row = &compiled.objects()[compiled.object_index(owner_id).unwrap() as usize];
    assert!(!row.draw_enabled);
    assert!(row.content.geometry().is_none());
    assert!(row.content.image().is_none());
    assert!(row.content.text().is_none());
    assert_eq!(row.effects.as_ref().unwrap().attachments(), &[attachment]);
    assert_eq!(
        row.content.composed_source().unwrap().members(),
        &[
            super::super::semantic_execution_object_id(low),
            super::super::semantic_execution_object_id(high)
        ]
    );
    assert_eq!(row.content.composed_source().unwrap().parent_scope(), None);
    assert_eq!(
        compiled.objects()[*compiled.painter_order().last().unwrap() as usize].id,
        owner_id
    );
    for child in [low, high] {
        let id = super::super::semantic_execution_object_id(child);
        assert!(
            compiled.objects()[compiled.object_index(id).unwrap() as usize]
                .effects
                .is_none()
        );
    }
}

#[test]
fn interleaved_and_aliased_sources_fail_without_changing_the_execution_index() {
    for aliased in [false, true] {
        let mut store = SemanticStore::new();
        let a = object(&mut store, 0.0);
        let b = object(&mut store, 2.0);
        let outside = object(&mut store, 1.0);
        let owner = family(&mut store, &[a, b]);
        effect(&mut store, owner);
        store.attach_to_scene(owner).unwrap();
        if aliased {
            let alias = family(&mut store, &[a]);
            store.attach_to_scene(alias).unwrap();
        } else {
            store.attach_to_scene(outside).unwrap();
        }
        let mut index = SemanticExecutionIndex::new();
        let before = index.clone();
        let revision = store.scene_revision();
        let error = lower_semantic_execution(&store, &mut index).unwrap_err();
        assert!(
            matches!(error,super::super::SemanticExecutionLoweringError::Object(SemanticLoweringError::InvalidEffectIsolation{owner:bad,..}) if bad==owner)
        );
        assert_eq!(index, before);
        assert!(index.lower_scene(&store).is_err());
        assert_eq!(
            index, before,
            "direct projection admission also preserves every index component"
        );
        assert_eq!(store.scene_revision(), revision);
    }
}

#[test]
fn nested_equal_intervals_keep_semantic_containment_independent_of_allocation_order() {
    let mut store = SemanticStore::new();
    let a = object(&mut store, 0.0);
    let b = object(&mut store, 0.0);
    let inner = family(&mut store, &[a, b]);
    effect(&mut store, inner);
    let outer = family(&mut store, &[inner]);
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(
        outer,
        "glow",
        Glow::new(EffectUpdate::default().intensity(0.5)).unwrap(),
    );
    tx.apply(&mut store).unwrap();
    store.attach_to_scene(outer).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let output = lower_semantic_execution(&store, &mut index).unwrap();
    let compiled = output.compiled();
    let plan = |owner| {
        compiled.objects()[compiled
            .object_index(super::super::semantic_execution_object_id(owner))
            .unwrap() as usize]
            .content
            .composed_source()
            .unwrap()
    };
    assert_eq!(plan(inner).members(), plan(outer).members());
    assert_eq!(
        plan(inner).parent_scope(),
        Some(super::super::semantic_execution_object_id(outer))
    );
    assert_eq!(plan(outer).parent_scope(), None);
}

#[test]
fn a_scoped_root_ignores_a_detached_alias_outside_its_execution() {
    let mut store = SemanticStore::new();
    let a = object(&mut store, 0.0);
    let owner = family(&mut store, &[a]);
    effect(&mut store, owner);
    let _detached = family(&mut store, &[a]);
    let mut index = SemanticExecutionIndex::new();
    let output = crate::lower_semantic_execution_root(&store, owner, &mut index).unwrap();
    assert_eq!(output.compiled().objects().len(), 2);
}

#[test]
fn empty_and_oversized_treatments_fail_before_any_identity_is_installed() {
    for empty in [false, true] {
        let mut store = SemanticStore::new();
        let a = object(&mut store, 0.0);
        let owner = family(
            &mut store,
            if empty { &[] } else { std::slice::from_ref(&a) },
        );
        let mut tx = SemanticMutationTransaction::new();
        for i in 0..if empty {
            1
        } else {
            noon_core::MAX_EFFECT_STACK_LEN + 1
        } {
            tx.create_effect(owner, format!("layer-{i}"), Blur::default());
        }
        tx.apply(&mut store).unwrap();
        store.attach_to_scene(owner).unwrap();
        let mut index = SemanticExecutionIndex::new();
        assert!(lower_semantic_execution(&store, &mut index).is_err());
        assert!(index.is_empty());
    }
}

#[test]
fn ordinary_families_allocate_no_composed_execution_rows() {
    let mut store = SemanticStore::new();
    let a = object(&mut store, 0.0);
    let owner = family(&mut store, &[a]);
    store.attach_to_scene(owner).unwrap();
    let output = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).unwrap();
    assert_eq!(output.compiled().objects().len(), 1);
    assert!(output.compiled().objects()[0]
        .content
        .composed_source()
        .is_none());
}
