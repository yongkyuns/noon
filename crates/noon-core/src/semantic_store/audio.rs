//! Audio declarations in the existing Semantic Scene and identity allocator.
//! PCM admission is immutable; clips are scope-owned declarations, never painter
//! children or a second playback timeline.

use std::collections::BTreeSet;

use super::{SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};
use crate::{
    AudioClipDefinition, AudioResource, AudioResourceArena, AudioResourceError, AudioResourceHandle,
};

/// One immutable clip definition owned by a family/root execution scope.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAudioClipState {
    scope: SemanticNodeId,
    definition: AudioClipDefinition,
}

impl SemanticAudioClipState {
    pub const fn scope(&self) -> SemanticNodeId {
        self.scope
    }

    pub const fn definition(&self) -> &AudioClipDefinition {
        &self.definition
    }

    pub(super) fn remap_resource(&mut self, handle: AudioResourceHandle) {
        self.definition.remap_resource(handle);
    }
}

impl SemanticNode {
    pub fn semantic_audio_clip_state(&self) -> Option<&SemanticAudioClipState> {
        match &self.kind {
            SemanticNodeKind::AudioClip(state) => Some(state),
            _ => None,
        }
    }

    /// Scope-local deterministic identity order, separate from painter order.
    pub fn scoped_audio_clips(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.scoped_audio_clips
            .as_deref()
            .into_iter()
            .flat_map(|clips| clips.iter().copied())
    }
}

impl SemanticStore {
    pub fn audio_resources(&self) -> &AudioResourceArena {
        &self.audio_resources
    }

    /// Admit validated immutable PCM without publishing a semantic revision.
    /// The normal transaction attaches clips using the returned capability.
    pub fn insert_audio_resource(
        &mut self,
        resource: AudioResource,
    ) -> Result<AudioResourceHandle, AudioResourceError> {
        self.audio_resources.insert(resource)
    }

    /// Admit immutable PCM for one atomic semantic publication, reusing an
    /// earlier admitted clone of the same prepared source. The callback must
    /// use a preflighted/atomic transaction: an error or unwind discards only a
    /// fresh unpublished resource, preserving existing handles and clips.
    pub fn with_audio_resource<T, E>(
        &mut self,
        resource: AudioResource,
        publish: impl FnOnce(&mut Self, AudioResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<AudioResourceError>,
    {
        let (handle, fresh) = self.audio_resources.admit_shared(resource)?;
        let mut admission = AudioAdmission {
            store: self,
            handle,
            fresh,
            committed: false,
        };
        let result = publish(admission.store, handle);
        admission.committed = result.is_ok();
        result
    }

    /// Constant-time gate while runtime audio output is not supported.
    pub const fn has_audio_clips(&self) -> bool {
        self.audio_clip_nodes != 0
    }

    pub fn semantic_audio_clip_state(
        &self,
        clip: SemanticNodeId,
    ) -> Result<&SemanticAudioClipState, SemanticStoreError> {
        self.node(clip)
            .ok_or(SemanticStoreError::UnknownNode(clip))?
            .semantic_audio_clip_state()
            .ok_or(SemanticStoreError::NotAudioClip(clip))
    }

    pub fn semantic_scoped_audio_clips(
        &self,
        scope: SemanticNodeId,
    ) -> Result<impl Iterator<Item = SemanticNodeId> + '_, SemanticStoreError> {
        let node = self
            .node(scope)
            .ok_or(SemanticStoreError::UnknownNode(scope))?;
        if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
            return Err(SemanticStoreError::NotFamily(scope));
        }
        Ok(node.scoped_audio_clips())
    }

    pub(crate) fn insert_semantic_audio_clip(
        &mut self,
        scope: SemanticNodeId,
        definition: AudioClipDefinition,
    ) -> SemanticNodeId {
        self.retain_semantic_audio_resource(definition.resource());
        let clip = self.insert_kind(SemanticNodeKind::AudioClip(SemanticAudioClipState {
            scope,
            definition,
        }));
        self.node_mut(scope)
            .expect("preflighted audio clip scope")
            .scoped_audio_clips
            .get_or_insert_with(|| Box::new(BTreeSet::new()))
            .insert(clip);
        self.register_semantic_references_for_owner(clip);
        clip
    }

    pub(crate) fn replace_audio_clip_definition(
        &mut self,
        clip: SemanticNodeId,
        definition: AudioClipDefinition,
    ) {
        let previous = self
            .semantic_audio_clip_state(clip)
            .expect("preflighted audio clip")
            .definition()
            .resource();
        let next = definition.resource();
        if next != previous {
            self.retain_semantic_audio_resource(next);
        }
        let SemanticNodeKind::AudioClip(state) =
            &mut self.node_mut(clip).expect("preflighted audio clip").kind
        else {
            unreachable!("preflighted audio clip kind");
        };
        state.definition = definition;
        if next != previous {
            self.release_semantic_audio_resource(previous);
        }
    }

    pub(crate) fn unlink_audio_clip(&mut self, clip: SemanticNodeId, scope: SemanticNodeId) {
        if let Some(node) = self.node_mut(scope) {
            if let Some(clips) = &mut node.scoped_audio_clips {
                clips.remove(&clip);
                if clips.is_empty() {
                    node.scoped_audio_clips = None;
                }
            }
        }
    }
}

struct AudioAdmission<'a> {
    store: &'a mut SemanticStore,
    handle: AudioResourceHandle,
    fresh: bool,
    committed: bool,
}

impl Drop for AudioAdmission<'_> {
    fn drop(&mut self) {
        if self.fresh && !self.committed {
            self.store.audio_resources.remove(self.handle);
        }
    }
}

#[cfg(test)]
mod tests;
