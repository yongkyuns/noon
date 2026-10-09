//! Static authoring -> object projection -> compiled slots -> runtime publication.
//! Full Scene scheduling remains guarded; these are the ordinary lower-level
//! typed APIs, not a test-only renderer runtime or duplicated semantic store.
use super::*;
use crate::{effects::Pixels, Color};
use noon_compile::{CompiledScene, SemanticExecutionIndex};
use noon_runtime::SceneInstance;
use std::sync::Arc;

fn lower(scene: &Scene) -> (CompiledScene, SemanticExecutionIndex) {
    let mut index = SemanticExecutionIndex::new();
    let store = scene.integration_store().borrow();
    let projection = index.lower_root(&store, scene.root()).unwrap();
    (
        CompiledScene::from_semantic_projection(&projection).unwrap(),
        index,
    )
}

#[test]
fn authored_static_attachment_reaches_coherent_runtime_without_new_identity() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4).unwrap();
    dot.disable_stroke().unwrap();
    dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&dot).unwrap();
    scene
        .set_glow(
            &dot,
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(1.4),
        )
        .unwrap();
    let handle = scene.get_effect(&dot, "glow").unwrap();
    let (compiled, index) = lower(&scene);
    let object = index.execution_object_id(dot.node_id()).unwrap();
    let row = compiled.objects().iter().find(|r| r.id == object).unwrap();
    let definition = row.glow.clone().unwrap();
    assert_eq!(definition.attachment, handle.node_id());
    assert_eq!(definition.definition.intensity(), 1.4);
    let mut runtime = SceneInstance::new(compiled);
    {
        let publication = runtime.take_renderer_publication();
        let effective = publication
            .frame()
            .objects
            .iter()
            .find(|r| r.id == object)
            .unwrap();
        assert!(Arc::ptr_eq(effective.glow.as_ref().unwrap(), &definition));
        assert!(publication.changes().is_all());
    }
    let publication = runtime.take_renderer_publication();
    assert!(publication.changes().is_empty());
    // No source query or renderer-owned authoring cache is needed per frame.
    assert!(scene.execution_session().is_err());
}

#[test]
fn static_effect_snapshot_is_independent_of_later_authored_mutation_and_readd() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4).unwrap();
    dot.disable_stroke().unwrap();
    dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&dot).unwrap();
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(0.5))
        .unwrap();
    let (first, _) = lower(&scene);
    let original = first.objects()[0].glow.clone().unwrap();
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(1.5))
        .unwrap();
    let (second, _) = lower(&scene);
    assert_eq!(original.definition.intensity(), 0.5);
    assert_eq!(
        second.objects()[0]
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        1.5
    );
    assert_eq!(
        second.objects()[0].glow.as_ref().unwrap().attachment,
        original.attachment
    );
    scene.remove_glow(&dot).unwrap();
    let (removed, _) = lower(&scene);
    assert!(removed.objects()[0].glow.is_none());
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    let (readded, _) = lower(&scene);
    assert_ne!(
        readded.objects()[0].glow.as_ref().unwrap().attachment,
        original.attachment
    );
}

#[test]
fn unsupported_stack_rejects_projection_atomically_without_index_pollution() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4).unwrap();
    dot.disable_stroke().unwrap();
    dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&dot).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    scene.add_effect(&dot, Glow::default(), "second").unwrap();
    let mut index = SemanticExecutionIndex::new();
    assert!(index
        .lower_root(&scene.integration_store().borrow(), scene.root())
        .is_err());
    assert!(index.is_empty());
    assert_eq!(
        std::mem::size_of::<Option<Arc<noon_compile::CompiledGlow>>>(),
        std::mem::size_of::<usize>()
    );
}

// Prepared semantic/compiled boundary proof. Runtime/Scene orchestration admission
// remains independently guarded; these tests do not invent an initial publication.
#[test]
fn prepared_existing_glow_updates_merge_fields_without_mutating_either_authority() {
    use noon_compile::{
        prepare_semantic_publication, ExecutionPatch, SemanticExecutionReachability,
    };
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4).unwrap();
    dot.disable_stroke().unwrap();
    dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&dot).unwrap();
    scene
        .set_glow(
            &dot,
            GlowUpdate::default().radius(Pixels(3.25)).intensity(0.4),
        )
        .unwrap();
    let handle = scene.get_effect(&dot, "glow").unwrap();
    let (mut compiled, index) = lower(&scene);
    let original = compiled.objects()[0].glow.clone().unwrap();
    let mut store = scene.integration_store().borrow_mut();
    let revision = store.scene_revision();
    let reachability = SemanticExecutionReachability::from_root(&store, scene.root()).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(handle.node_id(), GlowUpdate::default().radius(Pixels(3.25)));
    tx.update_effect(handle.node_id(), GlowUpdate::default().color(Color::BLUE));
    tx.update_effect(handle.node_id(), GlowUpdate::default().intensity(1.5));
    let semantic = tx.prepare(&mut store).unwrap();
    assert_eq!(semantic.effect_updates().count(), 1);
    let plan = prepare_semantic_publication(&semantic, &index, &reachability).unwrap();
    assert_eq!(plan.stats().object_states_lowered, 0);
    assert_eq!(plan.possible_entry_count(), 0);
    let [ExecutionPatch::SetGlow { object, glow }] = plan.value_transaction().mutations() else {
        panic!("one final existing-attachment value patch required")
    };
    assert_eq!(*object, index.execution_object_id(dot.node_id()).unwrap());
    assert_eq!(glow.attachment, handle.node_id());
    assert_eq!(glow.definition.intensity(), 1.5);
    assert_eq!(glow.definition.color(), Color::BLUE);
    assert_eq!(glow.definition.radius(), Pixels(3.25).into());
    assert_eq!(semantic.store().scene_revision(), revision);
    assert_eq!(original.definition.intensity(), 0.4);
    compiled
        .preflight_execution_transaction(plan.value_transaction())
        .unwrap();
    let transaction = plan.value_transaction().clone();
    semantic.commit();
    for patch in transaction.mutations() {
        compiled.apply_execution_patch(patch).unwrap();
    }
    assert_eq!(
        compiled.objects()[0]
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        1.5
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
}

#[test]
fn prepared_glow_noop_and_detached_updates_have_no_execution_patch() {
    use noon_compile::{prepare_semantic_publication, SemanticExecutionReachability};
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4).unwrap();
    dot.disable_stroke().unwrap();
    dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&dot).unwrap();
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(0.4))
        .unwrap();
    let detached = scene.circle(0.2).unwrap();
    scene.set_glow(&detached, GlowUpdate::default()).unwrap();
    let active = scene.get_effect(&dot, "glow").unwrap().node_id();
    let other = scene.get_effect(&detached, "glow").unwrap().node_id();
    let (_, index) = lower(&scene);
    let mut store = scene.integration_store().borrow_mut();
    let reachability = SemanticExecutionReachability::from_root(&store, scene.root()).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(active, GlowUpdate::default().intensity(0.4));
    tx.update_effect(other, GlowUpdate::default().intensity(1.2));
    let semantic = tx.prepare(&mut store).unwrap();
    assert_eq!(semantic.effect_updates().count(), 1);
    let plan = prepare_semantic_publication(&semantic, &index, &reachability).unwrap();
    assert!(plan.value_transaction().is_empty());
    drop(semantic);
    assert_eq!(
        store.semantic_effect_state(other).unwrap().definition(),
        EffectDefinition::Glow(Glow::default())
    );
}

#[test]
fn prepared_existing_effect_enrollment_preserves_the_staged_glow_column() {
    use noon_compile::{prepare_semantic_publication, SemanticExecutionReachability};
    let mut scene = Scene::new();
    let mut detached = scene.circle(0.4).unwrap();
    detached.disable_stroke().unwrap();
    detached.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.set_glow(&detached, GlowUpdate::default()).unwrap();
    let (_, index) = lower(&scene);
    let mut store = scene.integration_store().borrow_mut();
    let revision = store.scene_revision();
    let reachability = SemanticExecutionReachability::from_root(&store, scene.root()).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.add_member(scene.root(), detached.node_id());
    let prepared = tx.prepare(&mut store).unwrap();
    let publication = prepare_semantic_publication(&prepared, &index, &reachability).unwrap();
    assert!(publication.value_transaction().is_empty());
    let patches = publication.conservative_entry_patches(&prepared);
    let [noon_compile::ExecutionPatch::CreateObject(object)] = patches.as_slice() else {
        panic!("one newly reachable object with its existing attachment")
    };
    assert_eq!(
        object.glow.as_ref().unwrap().attachment,
        prepared
            .store()
            .effect_by_name(detached.node_id(), "glow")
            .unwrap()
            .unwrap()
    );
    drop(prepared);
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn repeated_semantic_parameter_writes_still_reject_the_whole_batch() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.4).unwrap();
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(0.4))
        .unwrap();
    let effect = scene.get_effect(&dot, "glow").unwrap().node_id();
    let mut store = scene.integration_store().borrow_mut();
    let revision = store.scene_revision();
    let original = store.semantic_effect_state(effect).unwrap().definition();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.update_effect(effect, GlowUpdate::default().color(Color::BLUE));
    transaction.update_effect(effect, GlowUpdate::default().intensity(0.8));
    transaction.update_effect(effect, GlowUpdate::default().intensity(1.5));
    assert!(transaction.prepare(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store.semantic_effect_state(effect).unwrap().definition(),
        original
    );
}
