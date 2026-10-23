use noon_compile::{
    lower_semantic_execution, lower_semantic_execution_root, validate_semantic_publication,
    SemanticExecutionIndex, SemanticExecutionLoweringError, SemanticLoweringError,
    SemanticPublicationLoweringError,
};
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, ObjectId, SemanticMutationTransaction,
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

fn assert_only_prior_object_indexed(
    index: &SemanticExecutionIndex,
    prior: SemanticNodeId,
    prior_execution: ObjectId,
    rejected: SemanticNodeId,
    clip: SemanticNodeId,
) {
    assert_eq!(index.len(), 1);
    assert_eq!(index.execution_object_id(prior), Some(prior_execution));
    assert_eq!(index.execution_object_id(rejected), None);
    assert_eq!(index.execution_object_id(clip), None);
}

fn assert_initial_audio_rejection(nested: bool) {
    let mut store = SemanticStore::new();
    let (prior_root, prior) = visual_family(&mut store);
    let mut index = SemanticExecutionIndex::new();
    index.lower_root(&store, prior_root).unwrap();
    let prior_execution = index.execution_object_id(prior).unwrap();

    let (root, visual) = visual_family(&mut store);
    let scope = if nested {
        let branch = store.insert_family();
        let empty_scope = store.insert_family();
        store.add_member(root, branch).unwrap();
        store.add_member(branch, empty_scope).unwrap();
        empty_scope
    } else {
        root
    };
    let clip = audio_clip(&mut store, scope);
    if nested {
        assert_eq!(store.node(scope).unwrap().member_count(), 0);
    }
    store.attach_to_scene(root).unwrap();
    let revision = store.scene_revision();
    let expected = SemanticLoweringError::AudioOutputUnavailable { clip };

    assert_eq!(index.lower_root(&store, root).unwrap_err(), expected);
    assert_only_prior_object_indexed(&index, prior, prior_execution, visual, clip);
    assert_eq!(index.lower_scene(&store).unwrap_err(), expected);
    assert_only_prior_object_indexed(&index, prior, prior_execution, visual, clip);
    assert_eq!(
        lower_semantic_execution_root(&store, root, &mut index).unwrap_err(),
        SemanticExecutionLoweringError::Object(expected.clone())
    );
    assert_only_prior_object_indexed(&index, prior, prior_execution, visual, clip);
    assert_eq!(
        lower_semantic_execution(&store, &mut index).unwrap_err(),
        SemanticExecutionLoweringError::Object(expected)
    );
    assert_only_prior_object_indexed(&index, prior, prior_execution, visual, clip);
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn root_audio_is_rejected_before_initial_index_mutation() {
    assert_initial_audio_rejection(false);
}

#[test]
fn audio_on_an_empty_nested_family_is_rejected_before_initial_index_mutation() {
    assert_initial_audio_rejection(true);
}

#[test]
fn only_audio_reachable_from_the_selected_roots_blocks_lowering() {
    let mut store = SemanticStore::new();
    let (root, visual) = visual_family(&mut store);
    store.attach_to_scene(root).unwrap();
    let audio_scope = store.insert_family();
    let clip = audio_clip(&mut store, audio_scope);
    assert!(store.has_audio_clips());

    let mut index = SemanticExecutionIndex::new();
    let projection = index.lower_root(&store, root).unwrap();
    assert_eq!(projection.objects().len(), 1);
    assert_eq!(projection.objects()[0].semantic_id, visual);
    let projection = index.lower_scene(&store).unwrap();
    assert_eq!(projection.objects().len(), 1);
    assert_eq!(projection.objects()[0].semantic_id, visual);
    assert_eq!(
        lower_semantic_execution_root(&store, root, &mut index)
            .unwrap()
            .compiled()
            .live_object_count(),
        1
    );
    assert_eq!(
        lower_semantic_execution(&store, &mut index)
            .unwrap()
            .compiled()
            .live_object_count(),
        1
    );
    let execution = index.execution_object_id(visual).unwrap();
    let expected = SemanticLoweringError::AudioOutputUnavailable { clip };

    // An independent attached audio scope blocks a full-scene handoff while
    // leaving an explicitly selected visual sibling runnable.
    store.attach_to_scene(audio_scope).unwrap();
    assert_eq!(index.lower_scene(&store).unwrap_err(), expected);
    assert_eq!(
        lower_semantic_execution(&store, &mut index).unwrap_err(),
        SemanticExecutionLoweringError::Object(expected.clone())
    );
    assert_eq!(index.lower_root(&store, root).unwrap().len(), 1);
    assert_eq!(
        lower_semantic_execution_root(&store, root, &mut index)
            .unwrap()
            .compiled()
            .live_object_count(),
        1
    );

    // Making that same scope a descendant puts the audio inside the selected
    // root even though the scope still has no drawable leaves.
    store.detach_from_scene(audio_scope).unwrap();
    store.add_member(root, audio_scope).unwrap();
    assert_eq!(index.lower_root(&store, root).unwrap_err(), expected);
    assert_eq!(
        lower_semantic_execution_root(&store, root, &mut index).unwrap_err(),
        SemanticExecutionLoweringError::Object(expected)
    );
    assert_only_prior_object_indexed(&index, visual, execution, audio_scope, clip);
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
