//! Finite PCM clip authoring through the Scene's normal transaction boundary.

use super::*;
use noon_core::{AudioClipDefinition, AudioClipOptions, AudioResource};

impl Scene {
    /// Attach a finite PCM clip to this Scene's root scope.
    ///
    /// `options.start_time` is absolute authored time and defaults to zero. Use
    /// `AudioClipOptions { start_time: scene.time(), ..Default::default() }` to
    /// start at the current authoring cursor. This declaration does not advance
    /// that cursor or insert a wait. Decode or generate PCM during preparation,
    /// then retain the returned semantic clip identity.
    ///
    /// The resource and clip publish atomically through the same transaction
    /// route as other Scene operations. Invalid options or rejected live
    /// publication release the fresh resource without changing the scene.
    /// Cloning and reusing one prepared resource shares its retained identity
    /// and PCM across clips; admission does not rescan or hash its samples.
    /// Audio currently supports finite predeclared execution; adding or editing
    /// clips after execution bootstrap remains explicitly gated by #2263.
    pub fn audio(
        &mut self,
        resource: AudioResource,
        options: AudioClipOptions,
    ) -> Result<SemanticNodeId, AuthoringError> {
        let root = self.root;
        let mut store = self.store.borrow_mut();
        if let Some(execution) = self.execution.as_ref() {
            execution.require_resource_creation_at_root(&store, root)?;
        }
        let (result, token) = store.with_audio_resource(resource, |store, handle| {
            let definition = AudioClipDefinition::new(
                handle,
                store
                    .audio_resources()
                    .get(handle)
                    .expect("audio resource is admitted for this transaction"),
                options,
            )?;
            let mut transaction = SemanticMutationTransaction::new();
            let token = transaction.create_audio_clip(root, definition);
            let result = match self.execution.as_mut() {
                Some(execution) => {
                    execution.apply_semantic_transaction_at_root(store, root, transaction)?
                }
                None => transaction.apply(store)?,
            };
            Ok::<_, AuthoringError>((result, token))
        })?;
        result
            .resolve(token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn pcm(samples: Arc<[f32]>) -> AudioResource {
        AudioResource::from_interleaved(8_000, 1, samples, 64_000).unwrap()
    }

    #[test]
    fn audio_uses_absolute_time_and_preserves_the_pcm_and_authoring_cursor() {
        let mut scene = Scene::new();
        scene.wait(7.0).unwrap();
        let samples: Arc<[f32]> = vec![0.125; 8_000].into();
        let source_ptr = samples.as_ptr();
        let before = scene.revision();
        let clip = scene
            .audio(pcm(samples), AudioClipOptions::default())
            .unwrap();
        assert_ne!(scene.revision(), before);
        assert_eq!(scene.time(), 7.0);
        let store = scene.integration_store().borrow();
        let state = store.semantic_audio_clip_state(clip).unwrap();
        assert_eq!(state.scope(), scene.root());
        assert_eq!(state.definition().start_time(), 0.0);
        assert_eq!(state.definition().end_time(), 1.0);
        assert_eq!(
            store
                .audio_resources()
                .get(state.definition().resource())
                .unwrap()
                .samples()
                .as_ptr(),
            source_ptr
        );
        assert_eq!(
            store
                .semantic_scoped_audio_clips(scene.root())
                .unwrap()
                .collect::<Vec<_>>(),
            [clip]
        );
    }

    #[test]
    fn invalid_clip_releases_fresh_pcm_without_publishing() {
        let mut scene = Scene::new();
        let before = {
            let store = scene.integration_store().borrow();
            (
                scene.revision(),
                store.len(),
                store.audio_resources().stats(),
            )
        };
        let samples: Arc<[f32]> = vec![0.125; 8_000].into();
        let weak = Arc::downgrade(&samples);
        let error = scene
            .audio(
                pcm(samples),
                AudioClipOptions {
                    fade_in: 2.0,
                    ..AudioClipOptions::default()
                },
            )
            .unwrap_err();
        assert!(matches!(error, AuthoringError::AudioClip(_)));
        assert!(weak.upgrade().is_none());
        let store = scene.integration_store().borrow();
        assert_eq!(
            before,
            (
                scene.revision(),
                store.len(),
                store.audio_resources().stats()
            )
        );
        assert!(!store.has_audio_clips());
    }

    #[test]
    fn reused_source_survives_failed_admission_and_retires_after_its_last_clip() {
        let mut scene = Scene::new();
        let source = pcm(vec![0.125; 8_000].into());
        let first = scene
            .audio(source.clone(), AudioClipOptions::default())
            .unwrap();
        let second = scene
            .audio(
                source.clone(),
                AudioClipOptions {
                    start_time: 0.5,
                    gain: 0.2,
                    ..AudioClipOptions::default()
                },
            )
            .unwrap();
        let (resource, before) = {
            let store = scene.integration_store().borrow();
            let resource = store
                .semantic_audio_clip_state(first)
                .unwrap()
                .definition()
                .resource();
            assert_eq!(
                store
                    .semantic_audio_clip_state(second)
                    .unwrap()
                    .definition()
                    .resource(),
                resource
            );
            assert_eq!(store.audio_resources().len(), 1);
            assert_eq!(
                store
                    .audio_resources()
                    .get(resource)
                    .unwrap()
                    .samples()
                    .as_ptr(),
                source.samples().as_ptr()
            );
            (
                resource,
                (
                    scene.revision(),
                    store.len(),
                    store.audio_resources().stats(),
                ),
            )
        };
        assert!(scene
            .audio(
                source.clone(),
                AudioClipOptions {
                    fade_in: 2.0,
                    ..AudioClipOptions::default()
                },
            )
            .is_err());
        {
            let store = scene.integration_store().borrow();
            assert_eq!(
                before,
                (
                    scene.revision(),
                    store.len(),
                    store.audio_resources().stats()
                )
            );
            assert!(store.audio_resources().get(resource).is_some());
            assert!(store.semantic_audio_clip_state(first).is_ok());
            assert!(store.semantic_audio_clip_state(second).is_ok());
        }

        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(first);
        scene.apply_semantic_transaction(transaction).unwrap();
        assert!(scene
            .integration_store()
            .borrow()
            .audio_resources()
            .get(resource)
            .is_some());
        {
            let session = scene.execution_session().unwrap();
            let mut playback = session.audio_playback_at(0.5).unwrap();
            let retained = playback.next().unwrap();
            assert_eq!(retained.clip, second);
            assert_eq!(retained.resource, resource);
            assert!(playback.next().is_none());
        }

        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(second);
        scene.apply_semantic_transaction(transaction).unwrap();
        assert!(scene
            .integration_store()
            .borrow()
            .audio_resources()
            .get(resource)
            .is_none());
        let third = scene.audio(source, AudioClipOptions::default()).unwrap();
        let store = scene.integration_store().borrow();
        assert_eq!(store.audio_resources().len(), 1);
        assert_ne!(
            store
                .semantic_audio_clip_state(third)
                .unwrap()
                .definition()
                .resource(),
            resource
        );
    }

    #[test]
    fn rejected_running_audio_admission_preserves_the_coherent_publication() {
        let mut scene = Scene::new();
        let session = scene.execution_session().unwrap();
        scene.install_execution(session);
        let before = {
            let store = scene.integration_store().borrow();
            (
                scene.revision(),
                store.len(),
                store.audio_resources().stats(),
                scene.owned_execution().publication_context(),
            )
        };
        let samples: Arc<[f32]> = vec![0.125; 8_000].into();
        let weak = Arc::downgrade(&samples);
        assert!(scene
            .audio(pcm(samples), AudioClipOptions::default())
            .is_err());
        assert!(weak.upgrade().is_none());
        let store = scene.integration_store().borrow();
        assert_eq!(
            before,
            (
                scene.revision(),
                store.len(),
                store.audio_resources().stats(),
                scene.owned_execution().publication_context(),
            )
        );
        assert!(!store.has_audio_clips());
    }
}
