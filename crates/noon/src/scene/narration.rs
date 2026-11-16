//! Narration placement and waits use the existing Scene audio and time owners.

use std::{cell::RefCell, rc::Weak, sync::Arc};

use super::*;
use crate::{
    narration::NarrationMetadata, AudioClipDefinition, AudioClipOptions, NarrationCue,
    NarrationError, PreparedNarration,
};

/// A receipt for one admitted full narration clip, using its semantic identity.
///
/// Timing getters describe the immutable placement receipt, not a running
/// observation. Scene wait helpers validate the owning store/root, live clip
/// identity, and unchanged definition before using these cues. The receipt owns
/// no clock, scene, PCM, or additional identity allocator.
#[derive(Clone, Debug)]
pub struct NarrationHandle {
    store: Weak<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    clip: SemanticNodeId,
    definition: AudioClipDefinition,
    metadata: Arc<NarrationMetadata>,
}

impl NarrationHandle {
    pub const fn node_id(&self) -> SemanticNodeId {
        self.clip
    }

    pub const fn start_time(&self) -> f64 {
        self.definition.start_time()
    }

    pub const fn end_time(&self) -> f64 {
        self.definition.end_time()
    }

    pub const fn duration_seconds(&self) -> f64 {
        self.definition.duration()
    }

    pub fn spoken_text(&self) -> &str {
        &self.metadata.spoken_text
    }

    pub fn caption_text(&self) -> &str {
        &self.metadata.caption_text
    }

    pub fn cues(&self) -> &[NarrationCue] {
        &self.metadata.cues
    }
}

impl Scene {
    /// Place prepared narration at the current authored cursor without waiting.
    ///
    /// The complete recording is admitted once at unit gain through
    /// [`Self::audio`]. No captions are automatically displayed and manual cues
    /// remain unverified against speech. Finite predeclared execution retains
    /// the existing audio profile; running audio admission stays explicitly gated.
    pub fn narrate(
        &mut self,
        narration: &PreparedNarration,
    ) -> Result<NarrationHandle, AuthoringError> {
        let start = self.time();
        validate_placement(start, narration)?;
        let clip = self.audio(
            narration.audio().clone(),
            AudioClipOptions {
                start_time: start,
                ..AudioClipOptions::default()
            },
        )?;
        let definition = *self
            .store
            .borrow()
            .semantic_audio_clip_state(clip)?
            .definition();
        Ok(NarrationHandle {
            store: Rc::downgrade(&self.store),
            root: self.root,
            clip,
            definition,
            metadata: Arc::clone(&narration.metadata),
        })
    }

    /// Wait only for the remaining recording, using the Scene's authored cursor.
    /// An already-reached endpoint is a no-op; it never rewinds or stretches play.
    pub fn wait_for_narration(
        &mut self,
        narration: &NarrationHandle,
    ) -> Result<(), AuthoringError> {
        let definition = self.narration_definition(narration)?;
        self.wait_for_narration_target(definition.end_time())
    }

    /// Wait for the start of one manually supplied source-audio cue.
    /// Unknown cues fail before changing time; already-passed cues are a no-op.
    pub fn wait_for_narration_cue(
        &mut self,
        narration: &NarrationHandle,
        name: &str,
    ) -> Result<(), AuthoringError> {
        let definition = self.narration_definition(narration)?;
        let cue = narration.metadata.cue(name)?;
        self.wait_for_narration_target(definition.start_time() + cue.start_seconds())
    }

    fn narration_definition(
        &self,
        narration: &NarrationHandle,
    ) -> Result<AudioClipDefinition, AuthoringError> {
        let store = narration
            .store
            .upgrade()
            .ok_or(NarrationError::ForeignScene)?;
        if !Rc::ptr_eq(&self.store, &store) || self.root != narration.root {
            return Err(NarrationError::ForeignScene.into());
        }
        let store = self.store.borrow();
        let state = store.semantic_audio_clip_state(narration.clip)?;
        if state.scope() != self.root || *state.definition() != narration.definition {
            return Err(NarrationError::ChangedClip.into());
        }
        Ok(*state.definition())
    }

    fn wait_for_narration_target(&mut self, target: f64) -> Result<(), AuthoringError> {
        if self.execution.is_some() {
            return Err(NarrationError::RunningWait.into());
        }
        let current = self.time();
        if !target.is_finite() || target < 0.0 || !current.is_finite() || current < 0.0 {
            return Err(NarrationError::InvalidTimeline.into());
        }
        if target <= current {
            return Ok(());
        }
        self.wait(target - current)
            .map_err(|_| NarrationError::InvalidTimeline)?;
        // A rounded subtraction followed by addition can stop one ULP before
        // the absolute cue/end (for example 2802 / 8000 after a 0.1 s wait).
        // Publish the validated target on this same Scene-owned cursor so an
        // exclusive audio endpoint and the next clip's origin agree exactly.
        self.cursor = target;
        Ok(())
    }
}

fn validate_placement(start: f64, narration: &PreparedNarration) -> Result<(), NarrationError> {
    let end = start + narration.duration_seconds();
    if !start.is_finite() || start < 0.0 || !end.is_finite() || end <= start {
        return Err(NarrationError::InvalidTimeline);
    }
    for cue in narration.cues() {
        let cue_start = start + cue.start_seconds();
        let cue_end = start + cue.end_seconds();
        if cue_start < start
            || (cue.start_seconds() > 0.0 && cue_start <= start)
            || cue_end <= cue_start
            || cue_end > end
        {
            return Err(NarrationError::InvalidTimeline);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
