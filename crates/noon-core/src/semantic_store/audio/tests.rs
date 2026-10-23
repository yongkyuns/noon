use std::sync::Arc;

use super::*;
use crate::{
    AudioClipOptions, SemanticMutationImpact, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticNodeCreation, SemanticObjectProperty,
    SemanticObjectState, SemanticVec3, StoredGeometry,
};

#[derive(Debug, PartialEq)]
enum AdmissionError {
    Resource(AudioResourceError),
    Semantic(SemanticMutationTransactionError),
}

impl From<AudioResourceError> for AdmissionError {
    fn from(error: AudioResourceError) -> Self {
        Self::Resource(error)
    }
}

impl From<SemanticMutationTransactionError> for AdmissionError {
    fn from(error: SemanticMutationTransactionError) -> Self {
        Self::Semantic(error)
    }
}

fn pcm(frames: usize) -> AudioResource {
    AudioResource::from_interleaved(8_000, 1, vec![0.125; frames], 64_000).unwrap()
}

fn admit(store: &mut SemanticStore) -> AudioResourceHandle {
    store.insert_audio_resource(pcm(8_000)).unwrap()
}

fn definition(
    store: &SemanticStore,
    handle: AudioResourceHandle,
    gain: f64,
) -> AudioClipDefinition {
    AudioClipDefinition::new(
        handle,
        store.audio_resources().get(handle).unwrap(),
        AudioClipOptions {
            gain,
            ..AudioClipOptions::default()
        },
    )
    .unwrap()
}

fn attach(
    store: &mut SemanticStore,
    scope: SemanticNodeId,
    handle: AudioResourceHandle,
) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let token = tx.create_audio_clip(scope, definition(store, handle, 0.2));
    tx.apply(store).unwrap().resolve(token).unwrap()
}

#[test]
fn failed_atomic_admission_releases_pcm_and_budget_without_changing_existing_state() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let existing_resource = admit(&mut store);
    let existing_clip = attach(&mut store, scope, existing_resource);
    let before = (
        store.audio_resources().stats(),
        store.len(),
        store.scene_revision(),
        store.preview_node_allocations().next(),
    );
    let samples: Arc<[f32]> = vec![0.25; 8_000].into();
    let weak = Arc::downgrade(&samples);
    let resource = AudioResource::from_interleaved(8_000, 1, samples, 64_000).unwrap();
    let mut failed_handle = None;
    let result = store.with_audio_resource(resource, |store, handle| {
        failed_handle = Some(handle);
        let mut tx = SemanticMutationTransaction::new();
        tx.create_audio_clip(scope, definition(store, handle, 0.2));
        tx.create_audio_clip(
            SemanticNodeId::new(u32::MAX, 0),
            definition(store, handle, 0.2),
        );
        tx.apply(store).map_err(AdmissionError::from)
    });
    assert!(matches!(result, Err(AdmissionError::Semantic(_))));
    assert!(weak.upgrade().is_none());
    let failed_handle = failed_handle.unwrap();
    assert!(store.audio_resources().get(failed_handle).is_none());
    assert!(store.audio_resources().get(existing_resource).is_some());
    assert_eq!(
        store
            .semantic_scoped_audio_clips(scope)
            .unwrap()
            .collect::<Vec<_>>(),
        [existing_clip]
    );
    assert_eq!(
        before,
        (
            store.audio_resources().stats(),
            store.len(),
            store.scene_revision(),
            store.preview_node_allocations().next(),
        )
    );
    let retry_resource = admit(&mut store);
    assert_ne!(retry_resource, failed_handle);
}

#[test]
fn successful_atomic_admission_retains_pcm_until_its_last_clip_is_removed() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let (clip, handle) = store
        .with_audio_resource(pcm(8_000), |store, handle| {
            let mut tx = SemanticMutationTransaction::new();
            let clip = tx.create_audio_clip(scope, definition(store, handle, 0.2));
            let result = tx.apply(store)?;
            Ok::<_, AdmissionError>((result.resolve(clip).unwrap(), handle))
        })
        .unwrap();
    assert!(store.audio_resources().get(handle).is_some());
    assert_eq!(
        store
            .semantic_audio_clip_state(clip)
            .unwrap()
            .definition()
            .resource(),
        handle
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(clip);
    tx.apply(&mut store).unwrap();
    assert!(store.audio_resources().get(handle).is_none());
    assert!(store.audio_resources().is_empty());
}

#[test]
fn clips_share_allocator_and_pending_family_publication_without_painter_membership() {
    let mut store = SemanticStore::new();
    let revision = store.scene_revision();
    let handle = admit(&mut store);
    assert_eq!(store.scene_revision(), revision);
    let mut tx = SemanticMutationTransaction::new();
    let scope = tx.create_node(SemanticNodeCreation::family());
    let clip = tx.create_audio_clip(scope, definition(&store, handle, 0.2));
    assert_eq!(store.len(), 0);
    assert!(!store.has_audio_clips());
    let result = tx.apply(&mut store).unwrap();
    let scope = result.resolve(scope).unwrap();
    let clip = result.resolve(clip).unwrap();

    assert_ne!(scope, clip);
    assert_eq!(store.len(), 2);
    assert_eq!(
        store.semantic_audio_clip_state(clip).unwrap().scope(),
        scope
    );
    assert_eq!(
        store
            .semantic_scoped_audio_clips(scope)
            .unwrap()
            .collect::<Vec<_>>(),
        [clip]
    );
    assert!(store.has_audio_clips());
    assert_eq!(store.node(scope).unwrap().member_count(), 0);
    assert!(store.node(clip).unwrap().parents().is_empty());
    assert!(!store.node(clip).unwrap().is_scene_owned());
    assert_eq!(store.scene_root_count(), 0);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert!(result
        .impacts()
        .contains(&SemanticMutationImpact::AudioClip { scope, clip }));

    let before = store.scene_revision();
    let mut membership = SemanticMutationTransaction::new();
    membership.add_member(scope, clip);
    assert!(membership.apply(&mut store).is_err());
    assert!(
        matches!(store.add_member(scope, clip), Err(SemanticStoreError::AudioClipCannotBeMember(id)) if id == clip)
    );
    assert!(
        matches!(store.attach_to_scene(clip), Err(SemanticStoreError::AudioClipCannotBeMember(id)) if id == clip)
    );
    assert_eq!(store.scene_revision(), before);
    assert_eq!(store.node(scope).unwrap().member_count(), 0);
}

#[test]
fn foreign_stale_and_mismatched_pcm_fail_before_any_batch_write() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let handle = admit(&mut store);
    let mut foreign = SemanticStore::new();
    let foreign_handle = admit(&mut foreign);
    let different_metadata = pcm(4_000);
    let invalid = [
        definition(&foreign, foreign_handle, 0.2),
        AudioClipDefinition::new(
            AudioResourceHandle {
                version: handle.version + 1,
                ..handle
            },
            store.audio_resources().get(handle).unwrap(),
            AudioClipOptions::default(),
        )
        .unwrap(),
        AudioClipDefinition::new(handle, &different_metadata, AudioClipOptions::default()).unwrap(),
    ];
    let before = (
        store.len(),
        store.scene_revision(),
        store.preview_node_allocations().next(),
        store.semantic_object_state_checked(object).unwrap().clone(),
        store.audio_resources().stats(),
    );
    for (index, invalid) in invalid.into_iter().enumerate() {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_property(
            object,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(7.0, 0.0, 0.0),
        );
        tx.create_node(SemanticNodeCreation::family());
        tx.create_audio_clip(scope, invalid);
        let error = tx.apply(&mut store).unwrap_err();
        if index == 2 {
            assert!(matches!(
                error,
                SemanticMutationTransactionError::AudioClipDefinition { .. }
            ));
        } else {
            assert!(matches!(
                error,
                SemanticMutationTransactionError::InvalidAudioResource { .. }
            ));
        }
        assert_eq!(
            before,
            (
                store.len(),
                store.scene_revision(),
                store.preview_node_allocations().next(),
                store.semantic_object_state_checked(object).unwrap().clone(),
                store.audio_resources().stats(),
            )
        );
        assert!(!store.has_audio_clips());
        assert!(store
            .semantic_scoped_audio_clips(scope)
            .unwrap()
            .next()
            .is_none());
    }
}

#[test]
fn clip_replacement_is_local_no_op_aware_and_rejects_duplicate_writes() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let unrelated = store.insert_family();
    let handle = admit(&mut store);
    let clip = attach(&mut store, scope, handle);
    let other = attach(&mut store, unrelated, handle);
    let next = definition(&store, handle, 0.4);
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_audio_clip(clip, next);
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::AudioClip { scope, clip }]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        store.semantic_audio_clip_state(clip).unwrap().definition(),
        &next
    );
    assert_eq!(
        store
            .semantic_audio_clip_state(other)
            .unwrap()
            .definition()
            .gain(),
        0.2
    );

    let revision = store.scene_revision();
    let mut no_op = SemanticMutationTransaction::new();
    no_op.replace_audio_clip(clip, next);
    assert!(no_op.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate.replace_audio_clip(clip, definition(&store, handle, 0.5));
    duplicate.replace_audio_clip(clip, definition(&store, handle, 0.6));
    assert!(
        matches!(duplicate.apply(&mut store), Err(SemanticMutationTransactionError::DuplicateTarget { target, .. }) if target == clip)
    );
    assert_eq!(
        store.semantic_audio_clip_state(clip).unwrap().definition(),
        &next
    );
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn last_clip_reference_reclaims_pcm_while_pinned_snapshots_survive() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let first_resource = admit(&mut store);
    let first = attach(&mut store, scope, first_resource);
    let second = attach(&mut store, scope, first_resource);
    let pinned = store.audio_resources().get_shared(first_resource).unwrap();
    let second_resource = admit(&mut store);
    let next = definition(&store, second_resource, 0.3);
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_audio_clip(first, next);
    tx.apply(&mut store).unwrap();
    assert!(store.audio_resources().get(first_resource).is_some());
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(second);
    remove.apply(&mut store).unwrap();
    assert!(store.audio_resources().get(first_resource).is_none());
    assert_eq!(pinned.samples()[0], 0.125);
    assert_eq!(store.audio_resources().len(), 1);
    store.remove_node(scope).unwrap();
    assert!(store.audio_resources().is_empty());
    assert!(!store.has_audio_clips());
}

#[test]
fn scope_retirement_cascades_only_its_clips_and_emits_audio_impacts() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let other_scope = store.insert_family();
    let handle = admit(&mut store);
    let first = attach(&mut store, scope, handle);
    let second = attach(&mut store, scope, handle);
    let survivor = attach(&mut store, other_scope, handle);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(scope);
    let result = tx.apply(&mut store).unwrap();
    for clip in [first, second] {
        assert!(store.node(clip).is_none());
        assert!(result
            .impacts()
            .contains(&SemanticMutationImpact::NodeRemoved { node: clip }));
        assert!(result
            .impacts()
            .contains(&SemanticMutationImpact::AudioClip { scope, clip }));
    }
    assert!(store.node(scope).is_none());
    assert_eq!(
        store
            .semantic_scoped_audio_clips(other_scope)
            .unwrap()
            .collect::<Vec<_>>(),
        [survivor]
    );
    assert!(store.audio_resources().get(handle).is_some());
    assert!(store.has_audio_clips());
    store.remove_node(survivor).unwrap();
    assert!(store
        .semantic_scoped_audio_clips(other_scope)
        .unwrap()
        .next()
        .is_none());
    assert!(store.audio_resources().is_empty());
    assert!(!store.has_audio_clips());
}

#[test]
fn stale_clip_and_scope_generations_never_target_replacements() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let handle = admit(&mut store);
    let old = attach(&mut store, scope, handle);
    let old_definition = *store.semantic_audio_clip_state(old).unwrap().definition();
    store.remove_node(old).unwrap();
    let new_handle = admit(&mut store);
    let current = attach(&mut store, scope, new_handle);
    assert_eq!(old.slot(), current.slot());
    assert_ne!(old.generation(), current.generation());
    let before = store.scene_revision();
    let mut stale = SemanticMutationTransaction::new();
    stale.replace_audio_clip(old, definition(&store, new_handle, 0.4));
    assert!(
        matches!(stale.apply(&mut store), Err(SemanticMutationTransactionError::Node { error: SemanticStoreError::UnknownNode(id), .. }) if id == old)
    );
    let mut stale_resource = SemanticMutationTransaction::new();
    stale_resource.replace_audio_clip(current, old_definition);
    assert!(matches!(
        stale_resource.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAudioResource { .. })
    ));
    assert_eq!(store.scene_revision(), before);
    store.remove_node(scope).unwrap();
    let replacement_scope = store.insert_family();
    assert_eq!(scope.slot(), replacement_scope.slot());
    let handle = admit(&mut store);
    let mut stale_scope = SemanticMutationTransaction::new();
    stale_scope.create_audio_clip(scope, definition(&store, handle, 0.2));
    assert!(stale_scope.apply(&mut store).is_err());
    assert!(store
        .semantic_scoped_audio_clips(replacement_scope)
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn canceled_pending_scope_cancels_clip_and_foreign_or_nonfamily_scopes_are_rejected() {
    let mut store = SemanticStore::new();
    let handle = admit(&mut store);
    let mut tx = SemanticMutationTransaction::new();
    let scope = tx.create_node(SemanticNodeCreation::family());
    let clip = tx.create_audio_clip(scope, definition(&store, handle, 0.2));
    tx.remove_node(scope);
    let before = (
        store.scene_revision(),
        store.preview_node_allocations().next(),
    );
    let result = tx.apply(&mut store).unwrap();
    assert!(result.resolve(scope).is_none());
    assert!(result.resolve(clip).is_none());
    assert!(result.impacts().is_empty());
    assert!(store.is_empty());
    assert_eq!(
        (
            store.scene_revision(),
            store.preview_node_allocations().next()
        ),
        before
    );
    assert!(!store.has_audio_clips());

    let mut foreign_tx = SemanticMutationTransaction::new();
    let foreign_scope = foreign_tx.create_node(SemanticNodeCreation::family());
    let mut tx = SemanticMutationTransaction::new();
    tx.create_audio_clip(foreign_scope, definition(&store, handle, 0.2));
    assert!(tx.apply(&mut store).is_err());
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let mut tx = SemanticMutationTransaction::new();
    tx.create_audio_clip(object, definition(&store, handle, 0.2));
    assert!(tx.apply(&mut store).is_err());
    assert!(!store.has_audio_clips());
}

#[test]
fn clone_uses_independent_resource_capabilities_and_shared_immutable_pcm() {
    let mut original = SemanticStore::new();
    let scope = original.insert_family();
    let handle = admit(&mut original);
    let clip = attach(&mut original, scope, handle);
    let mut cloned = original.clone();
    let cloned_handle = cloned
        .semantic_audio_clip_state(clip)
        .unwrap()
        .definition()
        .resource();
    assert_ne!(handle.arena, cloned_handle.arena);
    assert!(cloned.audio_resources().get(handle).is_none());
    assert!(original.audio_resources().get(cloned_handle).is_none());
    assert!(Arc::ptr_eq(
        &original.audio_resources().get_shared(handle).unwrap(),
        &cloned.audio_resources().get_shared(cloned_handle).unwrap()
    ));
    let cloned_definition = *cloned.semantic_audio_clip_state(clip).unwrap().definition();
    let mut foreign_replace = SemanticMutationTransaction::new();
    foreign_replace.replace_audio_clip(clip, cloned_definition);
    assert!(matches!(
        foreign_replace.apply(&mut original),
        Err(SemanticMutationTransactionError::InvalidAudioResource { .. })
    ));
    cloned.remove_node(scope).unwrap();
    assert!(cloned.audio_resources().is_empty());
    assert!(!cloned.has_audio_clips());
    assert!(original.audio_resources().get(handle).is_some());
    assert_eq!(
        original
            .semantic_scoped_audio_clips(scope)
            .unwrap()
            .collect::<Vec<_>>(),
        [clip]
    );
    assert!(original.has_audio_clips());
}

#[test]
fn removal_of_scope_cannot_be_combined_with_creating_or_replacing_its_clip() {
    let mut store = SemanticStore::new();
    let scope = store.insert_family();
    let handle = admit(&mut store);
    let clip = attach(&mut store, scope, handle);
    let before = (store.len(), store.scene_revision());
    let mut create = SemanticMutationTransaction::new();
    create.create_audio_clip(scope, definition(&store, handle, 0.3));
    create.remove_node(scope);
    assert!(
        matches!(create.apply(&mut store), Err(SemanticMutationTransactionError::NodeCreationUsesRemovedNode { node, .. }) if node == scope)
    );
    let mut replace = SemanticMutationTransaction::new();
    replace.replace_audio_clip(clip, definition(&store, handle, 0.4));
    replace.remove_node(scope);
    assert!(
        matches!(replace.apply(&mut store), Err(SemanticMutationTransactionError::TargetRemoved { target, .. }) if target == clip)
    );
    assert_eq!((store.len(), store.scene_revision()), before);
    assert_eq!(
        store
            .semantic_audio_clip_state(clip)
            .unwrap()
            .definition()
            .gain(),
        0.2
    );
}
