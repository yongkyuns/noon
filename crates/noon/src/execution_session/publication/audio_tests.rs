use super::*;
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, SemanticObjectProperty,
    SemanticObjectState, SemanticVec3, StoredGeometry,
};

fn fixture() -> (
    SemanticStore,
    SemanticNodeId,
    SemanticNodeId,
    AudioClipDefinition,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_member(root, object).unwrap();
    let resource = store
        .insert_audio_resource(
            AudioResource::from_interleaved(8_000, 1, vec![0.0; 8_000], 32_000).unwrap(),
        )
        .unwrap();
    let definition = AudioClipDefinition::new(
        resource,
        store.audio_resources().get(resource).unwrap(),
        AudioClipOptions::default(),
    )
    .unwrap();
    (store, root, object, definition)
}

#[test]
fn unsupported_audio_creation_rejects_whole_live_batch_before_publication() {
    let (mut store, root, object, definition) = fixture();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let revision = store.scene_revision();
    let node_count = store.len();
    let resources = store.audio_resources().stats();
    let before = store.semantic_object_state_checked(object).unwrap().clone();
    let identity = session.execution_object_id(object);

    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        object,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, 0.0, 0.0),
    );
    transaction.create_audio_clip(root, definition);
    let error = session
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionSessionPublicationError::Lowering(
            SemanticPublicationLoweringError::AudioOutputUnavailable { index: 1 }
        )
    ));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), node_count);
    assert_eq!(store.audio_resources().stats(), resources);
    assert!(!store.has_audio_clips());
    assert_eq!(
        store.semantic_object_state_checked(object).unwrap(),
        &before
    );
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.execution_object_id(object), identity);
    assert!(session.take_frame_changes().is_empty());

    let mut recovery = SemanticMutationTransaction::new();
    recovery.set_property(
        object,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, 0.0, 0.0),
    );
    session
        .apply_semantic_transaction_at_root(&mut store, root, recovery)
        .unwrap();
    assert_eq!(session.execution_object_id(object), identity);
    assert_eq!(
        session
            .effective_semantic_object(&store, object)
            .unwrap()
            .object
            .transform
            .translation
            .x,
        3.0
    );
}

#[test]
fn detached_audio_replacement_cannot_bypass_live_admission() {
    let (mut store, root, _, definition) = fixture();
    let detached = store.insert_family();
    let mut declaration = SemanticMutationTransaction::new();
    let pending = declaration.create_audio_clip(detached, definition);
    let clip = declaration
        .apply(&mut store)
        .unwrap()
        .resolve(pending)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let revision = store.scene_revision();
    let publication = session.publication_context();
    let resource = definition.resource();
    let replacement = AudioClipDefinition::new(
        resource,
        store.audio_resources().get(resource).unwrap(),
        AudioClipOptions {
            gain: 0.1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_audio_clip(clip, replacement);
    assert!(matches!(
        session.apply_semantic_transaction_at_root(&mut store, root, transaction),
        Err(ExecutionSessionPublicationError::Lowering(
            SemanticPublicationLoweringError::AudioOutputUnavailable { index: 0 }
        ))
    ));
    assert_eq!(
        store.semantic_audio_clip_state(clip).unwrap().definition(),
        &definition
    );
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn adding_existing_empty_audio_scope_rejects_without_changing_membership() {
    let (mut store, root, object, definition) = fixture();
    let detached = store.insert_family();
    let mut declaration = SemanticMutationTransaction::new();
    let pending = declaration.create_audio_clip(detached, definition);
    let clip = declaration
        .apply(&mut store)
        .unwrap()
        .resolve(pending)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let revision = store.scene_revision();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_member(root, detached);
    let error = session
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionSessionPublicationError::Lowering(
            SemanticPublicationLoweringError::Value(
                noon_compile::SemanticLoweringError::AudioOutputUnavailable { clip: rejected }
            )
        ) if rejected == clip
    ));
    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[object]
    );
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn initial_audio_allows_visual_tracks_but_freezes_the_schedule_after_playback_starts() {
    let (mut store, root, object, definition) = fixture();
    let audio_resource = definition.resource();
    let replacement_definition = AudioClipDefinition::new(
        audio_resource,
        store.audio_resources().get(audio_resource).unwrap(),
        AudioClipOptions {
            gain: 0.25,
            ..AudioClipOptions::default()
        },
    )
    .unwrap();
    let mut declaration = SemanticMutationTransaction::new();
    let clip_token = declaration.create_audio_clip(root, definition);
    let second_token = declaration.create_audio_clip(root, definition);
    let declared = declaration.apply(&mut store).unwrap();
    let clip = declared.resolve(clip_token).unwrap();
    let second_clip = declared.resolve(second_token).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();

    session.seek(0.5).unwrap();
    session.take_frame_changes();

    let mut property = SemanticMutationTransaction::new();
    property.set_property(
        object,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(7.0, 0.0, 0.0),
    );
    session
        .apply_semantic_transaction_at_root(&mut store, root, property)
        .expect("unrelated visual tracks may share the retained audio scene");
    let mut remove_visual = SemanticMutationTransaction::new();
    remove_visual.remove_node(object);
    session
        .apply_semantic_transaction_at_root(&mut store, root, remove_visual)
        .expect("removing a visual node does not alter the retained audio schedule");
    assert_eq!(session.audio_playback().unwrap().count(), 2);
    session.take_frame_changes();

    let revision = store.scene_revision();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let resources = store.audio_resources().stats();

    let mut add_clip = SemanticMutationTransaction::new();
    add_clip.create_audio_clip(root, definition);
    let mut replace_clip = SemanticMutationTransaction::new();
    replace_clip.replace_audio_clip(clip, replacement_definition);
    let mut remove_clip = SemanticMutationTransaction::new();
    remove_clip.remove_node(clip);
    let mut remove_scope = SemanticMutationTransaction::new();
    remove_scope.remove_node(root);
    let mut remove_second_clip = SemanticMutationTransaction::new();
    remove_second_clip.remove_node(second_clip);
    for (index, transaction) in [
        add_clip,
        replace_clip,
        remove_clip,
        remove_scope,
        remove_second_clip,
    ]
    .into_iter()
    .enumerate()
    {
        let result = session.apply_semantic_transaction_at_root(&mut store, root, transaction);
        assert!(
            matches!(
                &result,
                Err(ExecutionSessionPublicationError::AudioExecutionImmutable)
            ),
            "audio mutation {index} returned {result:?}"
        );
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.audio_resources().stats(), resources);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
        assert!(session.take_frame_changes().is_empty());
        assert_eq!(session.audio_playback().unwrap().count(), 2);
    }
}

#[test]
fn initial_audio_seek_and_prefix_retention_preserve_immutable_schedule() {
    let (mut store, root, _, definition) = fixture();
    let mut declaration = SemanticMutationTransaction::new();
    declaration.create_audio_clip(root, definition);
    declaration.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    assert!(session.has_audio());
    assert_eq!(session.audio_end_time(), Some(1.0));
    let publication = session.publication_context();
    assert_eq!(
        session.begin_replay_retention(noon_runtime::ReplayLimits::default()),
        Ok(())
    );
    assert!(session.replay_scope_active());
    assert_eq!(session.publication_context(), publication);
    for time in [0.75, 0.25, 1.0, 0.0] {
        session.seek(time).unwrap();
        let mut playback = session.audio_playback().unwrap();
        if time == 1.0 {
            assert!(playback.next().is_none());
        } else {
            let output = playback.next().unwrap();
            assert_eq!(output.source_offset, time);
            assert_eq!(output.duration, 1.0 - time);
        }
    }
}

#[test]
fn audio_scene_cannot_enter_an_unqualified_source_continuation() {
    struct Continuation;
    impl crate::LiveContinuation for Continuation {
        type Error = String;
        fn resume(
            &mut self,
            _scene: &mut crate::ContinuationScene,
        ) -> Result<crate::ContinuationStep, Self::Error> {
            panic!("unsupported audio continuation must never run");
        }
    }
    let mut scene = crate::Scene::new();
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, vec![0.0; 8_000], 32_000).unwrap(),
            AudioClipOptions::default(),
        )
        .unwrap();
    assert!(matches!(
        scene.into_live_program(Continuation),
        Err(
            noon_compile::SemanticExecutionLoweringError::AudioExecutionUnavailable {
                operation: "live source continuation",
            }
        )
    ));
}
