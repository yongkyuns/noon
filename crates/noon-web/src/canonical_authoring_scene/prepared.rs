//! Typed preparation boundary for the existing Python/optional-language owner.
//! No extra source, timeline, identity map, runtime or transport is created here.
use super::{AuthoringFailure, CanonicalAuthoringScene};
use noon::{PrefixBatch, PrefixPublisher, PrefixReader};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourcePreparation {
    Realtime,
    AutomaticAudio,
    Prepared,
}

impl CanonicalAuthoringScene {
    /// Select source-only admission before the first ordinary segment. This is
    /// an invocation policy, not the reusable realtime run/refresh boundary.
    pub fn select_prepared_source(&mut self) -> Result<(), AuthoringFailure> {
        self.select_source_preparation(SourcePreparation::Prepared)
    }

    pub fn allow_prepared_audio_source(&mut self) -> Result<(), AuthoringFailure> {
        self.select_source_preparation(SourcePreparation::AutomaticAudio)
    }

    fn select_source_preparation(
        &mut self,
        preparation: SourcePreparation,
    ) -> Result<(), AuthoringFailure> {
        if !self.player_ownership.is_unstarted() || self.scene.time() != 0.0 {
            return Err(AuthoringFailure::new(
                "ownership",
                "prepared.source_already_started",
                "select headless source preparation before canonical execution begins",
            ));
        }
        self.source_preparation = preparation;
        Ok(())
    }

    pub(crate) fn validate_prepared_publisher(
        &mut self,
        publisher: &PrefixPublisher,
    ) -> Result<(), AuthoringFailure> {
        self.active_live_player()?
            .validate_prepared_publisher(publisher)
    }

    /// Headlessly complete one actual pending source segment. The caller still
    /// owns interpreter suspension, cancellation and demand. Never retry source
    /// statements on failure, and never treat completion as a display receipt.
    pub fn prepare_live_segment(&mut self) -> Result<f64, AuthoringFailure> {
        self.active_live_player()?.live_prepare_segment()
    }

    /// Bootstrap independent history after completion. This does not transfer
    /// the authoring player; the next source statement still queries its frontier.
    /// Publisher/reader are ordinary typed Rust values, not frontend snapshots.
    pub fn create_prepared_prefix(
        &mut self,
    ) -> Result<(PrefixPublisher, PrefixReader), AuthoringFailure> {
        self.active_live_player()?.create_prepared_prefix()
    }

    /// Extract only the new completed suffix. The caller owns ordered delivery
    /// and must retain the original batch if receiver admission fails.
    pub fn publish_prepared_prefix(
        &mut self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, AuthoringFailure> {
        self.active_live_player()?
            .publish_prepared_prefix(publisher)
    }

    /// Explicitly declare source completion, including final same-time edits.
    /// A segment endpoint alone never implies that the interpreter has finished.
    pub fn seal_prepared_source(&mut self) -> Result<(), AuthoringFailure> {
        self.active_live_player()?.seal_prepared_source()
    }

    /// Extract completion only after explicit sealing; reader adoption alone
    /// makes the exact final endpoint available to presentation.
    pub fn finish_prepared_prefix(
        &mut self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, AuthoringFailure> {
        self.active_live_player()?.finish_prepared_prefix(publisher)
    }
}

#[cfg(test)]
mod tests;
