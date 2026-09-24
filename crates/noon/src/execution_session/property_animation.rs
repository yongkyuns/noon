//! Semantic action preparation and effect driving on the existing session.
//!
//! The action is lowered from an inert, uncommitted semantic request. Runtime
//! alone owns claims, elapsed time and release. No authored node, timeline track,
//! continuation segment, or second animation evaluator is created by activation.

use noon_core::{
    AnimationOptions, SemanticMutationTransaction, SemanticNodeId, SemanticStore, SemanticVec3,
    TrackId,
};
use noon_runtime::{FrameState, PropertyAnimationError, PropertyAnimationToken};

use super::{ExecutionSession, ExecutionSessionAnimationError};
use crate::IndicateOptions;

impl ExecutionSession {
    /// Start the shared restoring Indicate action at the current effective state.
    ///
    /// `scale_center` is an explicit semantic pivot. The public LiveSession facade
    /// resolves the default from the target's effective layout at activation, not
    /// from a stale authored position. Preparation reuses ordinary semantic
    /// Indicate scheduling/lowering, with an operation-relative origin of zero.
    /// The staged declaration is never committed or retained as authored content.
    ///
    /// This does not acquire the scene-wide authored-publication gate. An active
    /// unrelated segment may continue, while Runtime arbitrates overlapping
    /// channels. Required callbacks and sealed replay remain explicit exclusions.
    /// An action with no changed channels returns None without allocating a token,
    /// publishing a frame, or consuming track identity. A busy target is an error;
    /// the caller must not treat arbitrary errors as an ignored duplicate click.
    pub fn start_indicate_effect(
        &mut self,
        store: &mut SemanticStore,
        target: SemanticNodeId,
        scale_center: SemanticVec3,
        indication: IndicateOptions,
        options: AnimationOptions,
    ) -> Result<Option<PropertyAnimationToken>, ExecutionSessionAnimationError> {
        self.require_published_store(store)
            .map_err(ExecutionSessionAnimationError::AuthoredPublication)?;
        self.require_property_animation_ingress()?;
        if self.runtime.replay_is_sealed() {
            return Err(ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::ReplaySealed,
            ));
        }
        // Validate target liveness even when lowering would emit no channels.
        let observed = self
            .effective_semantic_object(store, target)
            .map_err(ExecutionSessionAnimationError::AuthoredPublication)?;
        let object = observed.object.id;
        let index = self.runtime.frame_index_for_object(object).ok_or(
            ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::UnknownObject(object),
            ),
        )?;
        if !self.runtime.frame().presences[index] {
            return Err(ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::TargetNotPresent(object),
            ));
        }
        if observed.object.content.geometry().is_none() {
            return Err(ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::UnsupportedContent(observed.object.id),
            ));
        }
        if !observed.authored_content_layout_applicable() {
            return Err(ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::TargetHasRenderOverride(observed.object.id),
            ));
        }
        let mut declaration = SemanticMutationTransaction::new();
        let root =
            self.stage_indicate_leaf(target, indication, scale_center, options, &mut declaration)?;
        let prepared = declaration.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                super::ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        let projection = noon_compile::lower_prepared_semantic_animation_composition(
            &prepared,
            &self.execution_index,
            root,
            0.0,
            AnimationOptions::new(),
            |object| {
                let index = self.runtime.frame_index_for_object(object)?;
                let frame = self.runtime.frame();
                let row = frame.objects.get(index)?;
                Some(noon_compile::EffectiveAnimationProperties {
                    z_index: row.z_index,
                    transform: row.transform,
                    style: row.style,
                    appearance: row.appearance,
                    reveal: *frame.reveals.get(index)?,
                })
            },
        )?;
        if !projection.family_animations().is_empty()
            || projection
                .tracks()
                .iter()
                .any(|track| track.completion != noon_compile::SemanticAnimationCompletion::Release)
        {
            return Err(ExecutionSessionAnimationError::InvalidComposition(
                "independent Indicate requires restoring property channels".into(),
            ));
        }
        if projection.is_empty() {
            return Ok(None);
        }
        let mut next_track_id = self.next_activation_track_id;
        let mut tracks = Vec::with_capacity(projection.tracks().len());
        for track in projection.tracks() {
            let id = next_track_id.ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            tracks.push(
                track
                    .with_track_id(TrackId::new(id))
                    .map_err(ExecutionSessionAnimationError::PreparedTrack)?,
            );
            next_track_id = id.checked_add(1);
        }
        let token = self
            .runtime
            .start_restoring_property_animation(&tracks)
            .map_err(ExecutionSessionAnimationError::PropertyAnimation)?;
        // Runtime activation is atomic. Only now consume this session's existing
        // execution-track identity range; the authored cursor/segment stay intact.
        self.next_activation_track_id = next_track_id;
        Ok(Some(token))
    }

    /// Advance only runtime-owned property effects by an explicit elapsed delta.
    ///
    /// The host supplies elapsed input, not authored time or interpolation. This
    /// publication updates the ordinary spatial/render paths and invalidates old
    /// pointer snapshots. No callback, source continuation, or authored timeline
    /// is driven here, including during an unrelated pending animation segment.
    pub fn advance_property_animations_by(
        &mut self,
        delta: f64,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        self.require_property_animation_ingress()?;
        self.runtime
            .advance_property_animations_by(delta)
            .map_err(ExecutionSessionAnimationError::PropertyAnimation)?;
        self.sync_spatial_index();
        Ok(self.frame())
    }

    /// Restore and release one runtime-bound operation, preserving other effects.
    pub fn cancel_property_animation(
        &mut self,
        token: PropertyAnimationToken,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        self.require_property_animation_ingress()?;
        self.runtime
            .cancel_property_animation(token)
            .map_err(ExecutionSessionAnimationError::PropertyAnimation)?;
        self.sync_spatial_index();
        Ok(self.frame())
    }

    pub fn property_animation_elapsed(&self, token: PropertyAnimationToken) -> Option<f64> {
        self.runtime.property_animation_elapsed(token)
    }

    pub fn has_property_animations(&self) -> bool {
        self.runtime.has_property_animations()
    }

    pub(super) fn require_property_animation_ingress(
        &self,
    ) -> Result<(), ExecutionSessionAnimationError> {
        if self.callback_termination.is_some() {
            return Err(ExecutionSessionAnimationError::CallbackTerminated);
        }
        self.ensure_direct_input_ingress_available()
            .map_err(ExecutionSessionAnimationError::EffectInput)
    }
}

#[cfg(test)]
mod tests;
