//! Typed preparation boundary for the existing Python/optional-language owner.
//! No extra source, timeline, identity map, runtime or transport is created here.
use super::{AuthoringFailure, CanonicalAuthoringScene};
use noon::{PrefixBatch, PrefixPublisher, PrefixReader};

impl CanonicalAuthoringScene {
    pub(crate) fn prefix_source_requirement(
        &mut self,
        publisher: &PrefixPublisher,
        finished: bool,
    ) -> Result<Option<noon::SourceExecutionRequirement>, AuthoringFailure> {
        self.active_live_player()?
            .prefix_source_requirement(publisher, finished)
    }

    pub(crate) fn begin_observed_prefix_source(
        &mut self,
        publisher: &PrefixPublisher,
        reader: &PrefixReader,
        playing: bool,
        finished: bool,
    ) -> Result<(), AuthoringFailure> {
        self.active_live_player()?
            .begin_observed_prefix_source(publisher, reader, playing, finished)
    }
    /// Initial admission only: keep the original pending segment and player for
    /// observed execution, releasing open history before any reader is created.
    pub fn begin_observed_prepared_source(
        &mut self,
        effects: noon::ContinuationEffects,
    ) -> Result<bool, AuthoringFailure> {
        self.active_live_player()?
            .begin_observed_prepared_source(effects)
    }

    pub(crate) fn prepare_prefix_segment(
        &mut self,
        publisher: &PrefixPublisher,
    ) -> Result<f64, AuthoringFailure> {
        self.active_live_player()?
            .live_prepare_prefix_segment(Some(publisher))
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

    /// Complete one original initial barrier and ask Rust whether its retained
    /// history has a readable interval. Zero waits do not invent a range.
    pub fn prepare_initial_source_segment(&mut self) -> Result<bool, AuthoringFailure> {
        self.active_live_player()?.prepare_initial_source_segment()
    }

    /// Bootstrap independent history after completion. This does not transfer
    /// the authoring player; the next source statement still queries its frontier.
    /// Publisher/reader are ordinary typed Rust values, not frontend snapshots.
    pub fn create_prepared_prefix(
        &mut self,
    ) -> Result<(PrefixPublisher, PrefixReader), AuthoringFailure> {
        self.active_live_player()?.create_prepared_prefix()
    }

    /// Completed static source uses the same historical reader at `[0, 0]`.
    /// Called only after the interpreter has finished its source and cleanup.
    pub fn create_finished_static_reader(&mut self) -> Result<PrefixReader, AuthoringFailure> {
        if !self.player_ownership.is_unstarted() {
            return self.active_live_player()?.create_finished_static_reader();
        }
        if self.scene.time() != 0.0 {
            return Err(AuthoringFailure::new(
                "unsupported_domain",
                "prepared.static_source",
                "completed static preparation requires an unstarted zero-duration source",
            ));
        }
        let mut session = self.lower_execution()?;
        if session.finite_playback_end_time() != Some(0.0) {
            return Err(AuthoringFailure::new(
                "unsupported_domain",
                "prepared.static_source",
                "completed static preparation cannot discard future authored work",
            ));
        }
        session
            .begin_replay_retention(noon_runtime::ReplayLimits::default())
            .map_err(|e| AuthoringFailure::unclassified("prepared.static_source", &e))?;
        session
            .seal_replay()
            .map_err(|e| AuthoringFailure::unclassified("prepared.static_source", &e))?;
        session
            .create_finished_prepared_reader()
            .map_err(|e| AuthoringFailure::unclassified("prepared.static_source", &e))
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
