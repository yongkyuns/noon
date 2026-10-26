use noon_compile::{
    lower_semantic_execution, lower_semantic_execution_root, validate_semantic_publication,
    CompiledResourceError, CompiledScene, SemanticCompiledSceneError, SemanticExecutionIndex,
    SemanticPublicationLoweringError,
};
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticNodeId, SemanticObjectState, SemanticStore, StoredGeometry,
};

fn visual_family(store: &mut SemanticStore) -> (SemanticNodeId, SemanticNodeId) {
    let root = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_member(root, object).unwrap();
    (root, object)
}

fn audio_definition(store: &mut SemanticStore) -> AudioClipDefinition {
    let resource = AudioResource::from_interleaved(8_000, 1, vec![0.125; 800], 3_200).unwrap();
    let handle = store.insert_audio_resource(resource).unwrap();
    AudioClipDefinition::new(
        handle,
        store.audio_resources().get(handle).unwrap(),
        AudioClipOptions::default(),
    )
    .unwrap()
}

fn audio_clip(store: &mut SemanticStore, scope: SemanticNodeId) -> SemanticNodeId {
    let definition = audio_definition(store);
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_audio_clip(scope, definition);
    transaction.apply(store).unwrap().resolve(pending).unwrap()
}

#[test]
fn initial_audio_lowers_without_drawable_rows_or_another_identity() {
    let mut store = SemanticStore::new();
    let (root, visual) = visual_family(&mut store);
    let empty = store.insert_family();
    store.add_member(root, empty).unwrap();
    let clip = audio_clip(&mut store, empty);
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution_root(&store, root, &mut index).unwrap();
    assert_eq!(lowered.compiled().live_object_count(), 1);
    assert_eq!(lowered.compiled().audio_clips().len(), 1);
    let compiled = &lowered.compiled().audio_clips()[0];
    assert_eq!(compiled.clip(), clip);
    assert_eq!(compiled.scope(), empty);
    assert_eq!(lowered.compiled().audio_end_time(), Some(0.1));
    assert_eq!(index.len(), 1);
    assert!(index.execution_object_id(visual).is_some());
    assert_eq!(index.execution_object_id(clip), None);
    assert_eq!(lowered.compiled().resources().audio_count(), 1);
    assert_eq!(lowered.compiled().resources().audio_pcm_bytes(), 3_200);
}

#[test]
fn only_audio_reachable_from_selected_roots_enters_execution() {
    let mut store = SemanticStore::new();
    let (root, _) = visual_family(&mut store);
    store.attach_to_scene(root).unwrap();
    let audio_scope = store.insert_family();
    let clip = audio_clip(&mut store, audio_scope);
    let mut index = SemanticExecutionIndex::new();
    assert!(!lower_semantic_execution(&store, &mut index)
        .unwrap()
        .compiled()
        .has_audio());
    store.attach_to_scene(audio_scope).unwrap();
    assert!(lower_semantic_execution(&store, &mut index)
        .unwrap()
        .compiled()
        .has_audio());
    assert!(!lower_semantic_execution_root(&store, root, &mut index)
        .unwrap()
        .compiled()
        .has_audio());
    store.detach_from_scene(audio_scope).unwrap();
    store.add_member(root, audio_scope).unwrap();
    let lowered = lower_semantic_execution_root(&store, root, &mut index).unwrap();
    assert_eq!(lowered.compiled().audio_clips()[0].clip(), clip);
}

#[test]
fn captured_pcm_is_shared_and_survives_semantic_retirement_and_compaction() {
    let mut store = SemanticStore::new();
    let (root, _) = visual_family(&mut store);
    let first = audio_clip(&mut store, root);
    let definition = *store.semantic_audio_clip_state(first).unwrap().definition();
    let mut tx = SemanticMutationTransaction::new();
    tx.create_audio_clip(root, definition);
    tx.apply(&mut store).unwrap();
    let pointer = store
        .audio_resources()
        .get(definition.resource())
        .unwrap()
        .samples()
        .as_ptr();
    let mut compiled =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new())
            .unwrap()
            .into_parts()
            .0;
    assert_eq!(compiled.audio_clips().len(), 2);
    assert_eq!(compiled.resources().audio_count(), 1);
    assert_eq!(
        compiled
            .resources()
            .audio(definition.resource())
            .unwrap()
            .samples()
            .as_ptr(),
        pointer
    );
    let mut retire = SemanticMutationTransaction::new();
    for clip in compiled.audio_clips() {
        retire.remove_node(clip.clip());
    }
    retire.apply(&mut store).unwrap();
    assert_eq!(store.audio_resources().len(), 0);
    drop(store);
    let cloned = compiled.clone();
    let object = compiled.objects()[0].id;
    compiled
        .apply_execution_patch(&noon_compile::ExecutionPatch::RemoveObject(object))
        .unwrap();
    let stats = compiled.compact_retired_object_slots().unwrap();
    assert_eq!(stats.object_slots_reclaimed, 1);
    assert_eq!(compiled.audio_clips().len(), 2);
    assert_eq!(
        compiled
            .resources()
            .audio(definition.resource())
            .unwrap()
            .samples()
            .as_ptr(),
        pointer
    );
    assert_eq!(
        cloned
            .resources()
            .audio(definition.resource())
            .unwrap()
            .samples()
            .as_ptr(),
        pointer
    );
}

#[test]
fn standalone_materialization_cannot_silently_drop_audio_resources() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let clip = audio_clip(&mut store, root);
    let resource = store
        .semantic_audio_clip_state(clip)
        .unwrap()
        .definition()
        .resource();
    let projection = SemanticExecutionIndex::new()
        .lower_root(&store, root)
        .unwrap();
    assert_eq!(
        CompiledScene::from_semantic_projection(&projection),
        Err(SemanticCompiledSceneError::Resource {
            node: clip,
            error: CompiledResourceError::MissingAudio(resource),
        })
    );
}

#[test]
fn pending_audio_creation_is_rejected_at_its_publication_mutation_index() {
    let mut store = SemanticStore::new();
    let definition = audio_definition(&mut store);
    let mut transaction = SemanticMutationTransaction::new();
    let scope = transaction.create_node(SemanticNodeCreation::family());
    let visual = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    transaction.add_member(scope, visual);
    let clip = transaction.create_audio_clip(scope, definition);

    assert_eq!(
        validate_semantic_publication(&transaction),
        Err(SemanticPublicationLoweringError::AudioOutputUnavailable { index: 3 })
    );
    // This is a valid authoring transaction. The compiler refuses publication
    // because output is unavailable, not because its pending scope is invalid.
    let result = transaction.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_audio_clip_state(result.resolve(clip).unwrap())
            .unwrap()
            .scope(),
        result.resolve(scope).unwrap()
    );
}

#[test]
fn audio_replacement_is_rejected_at_its_publication_mutation_index() {
    let mut store = SemanticStore::new();
    let (scope, visual) = visual_family(&mut store);
    let clip = audio_clip(&mut store, scope);
    let original = *store.semantic_audio_clip_state(clip).unwrap().definition();
    let replacement = AudioClipDefinition::new(
        original.resource(),
        store.audio_resources().get(original.resource()).unwrap(),
        AudioClipOptions {
            gain: 0.25,
            ..AudioClipOptions::default()
        },
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_draw_enabled(visual, false);
    transaction.replace_audio_clip(clip, replacement);

    assert_eq!(
        validate_semantic_publication(&transaction),
        Err(SemanticPublicationLoweringError::AudioOutputUnavailable { index: 1 })
    );
    assert_eq!(
        *store.semantic_audio_clip_state(clip).unwrap().definition(),
        original
    );
    transaction.apply(&mut store).unwrap();
    assert_eq!(
        *store.semantic_audio_clip_state(clip).unwrap().definition(),
        replacement
    );
}

#[test]
fn shared_scope_is_lowered_once_when_multiple_roots_reach_it() {
    let mut store = SemanticStore::new();
    let first = store.insert_family();
    let second = store.insert_family();
    let shared = store.insert_family();
    store.add_member(first, shared).unwrap();
    store.add_member(second, shared).unwrap();
    let clip = audio_clip(&mut store, shared);
    store.attach_to_scene(first).unwrap();
    store.attach_to_scene(second).unwrap();
    let lowered = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).unwrap();
    assert_eq!(lowered.compiled().audio_clips().len(), 1);
    assert_eq!(lowered.compiled().audio_clips()[0].clip(), clip);
    assert_eq!(lowered.compiled().resources().audio_count(), 1);
}
