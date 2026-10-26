//! Immutable initial audio projection in the existing execution plan (#2263).

use noon_core::{AudioClipDefinition, AudioResource, AudioResourceHandle, SemanticNodeId};

use crate::{CompiledResources, CompiledScene};

/// One authored audio attachment lowered without allocating a second identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledAudioClip {
    pub(crate) clip: SemanticNodeId,
    pub(crate) scope: SemanticNodeId,
    pub(crate) definition: AudioClipDefinition,
}

impl CompiledAudioClip {
    pub const fn clip(&self) -> SemanticNodeId {
        self.clip
    }

    pub const fn scope(&self) -> SemanticNodeId {
        self.scope
    }

    pub const fn definition(&self) -> &AudioClipDefinition {
        &self.definition
    }
}

impl CompiledResources {
    /// Borrow a PCM dependency pinned by this plan, including after its authoring
    /// store has been dropped. Lookup never copies or scans samples.
    pub fn audio(&self, handle: AudioResourceHandle) -> Option<&AudioResource> {
        self.audio.get(&handle).map(std::sync::Arc::as_ref)
    }

    pub fn audio_count(&self) -> usize {
        self.audio.len()
    }

    /// PCM bytes retained by this immutable dependency closure. Shared handles
    /// are counted once; another plan may share the same underlying allocation.
    pub fn audio_pcm_bytes(&self) -> usize {
        self.audio
            .values()
            .map(|resource| resource.pcm_bytes())
            .sum()
    }

    pub(crate) fn capture_audio(
        &mut self,
        store: &noon_core::SemanticStore,
        definition: &AudioClipDefinition,
    ) -> Result<(), crate::CompiledResourceError> {
        let handle = definition.resource();
        let resource = store
            .audio_resources()
            .get_shared(handle)
            .ok_or(crate::CompiledResourceError::MissingAudio(handle))?;
        definition
            .validate_resource(&resource)
            .map_err(|_| crate::CompiledResourceError::AudioDefinitionMismatch(handle))?;
        self.audio.entry(handle).or_insert(resource);
        Ok(())
    }
}

impl CompiledScene {
    pub fn audio_clips(&self) -> &[CompiledAudioClip] {
        &self.audio_clips
    }

    /// Final authored audio boundary. This does not mutate a Scene cursor or
    /// invent a duration for another execution domain.
    pub const fn audio_end_time(&self) -> Option<f64> {
        self.audio_end_time
    }

    pub fn has_audio(&self) -> bool {
        !self.audio_clips.is_empty()
    }
}
