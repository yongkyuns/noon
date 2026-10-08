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
