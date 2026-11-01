//! Interpreter-driven preparation uses this player's actual continuation receipt.
//! Source calls remain with the interpreter; completion, history and identity do not.
use super::{AuthoringFailure, LiveSegmentReceipt, SemanticExecutionPlayer};
use noon::{ExecutionSegment, PrefixBatch, PrefixPublisher, PrefixReader};

fn prefix_error(error: noon::PrefixError) -> AuthoringFailure {
    AuthoringFailure::unclassified("prepared.prefix", &error)
}

impl SemanticExecutionPlayer {
    pub(crate) fn prefix_source_requirement(
        &self,
        publisher: &PrefixPublisher,
        finished: bool,
    ) -> Result<Option<noon::SourceExecutionRequirement>, AuthoringFailure> {
        if finished {
            self.completed_prepared_boundary()?;
        }
        self.session
            .preflight_prepared_source(Some(publisher))
            .map_err(prefix_error)
    }

    pub(crate) fn begin_observed_prefix_source(
        &mut self,
        publisher: &PrefixPublisher,
        reader: &PrefixReader,
        playing: bool,
        finished: bool,
    ) -> Result<(), AuthoringFailure> {
        if finished {
            self.completed_prepared_boundary()?;
        } else {
            self.live_segment()?;
        }
        if self
            .session
            .begin_observed_prefix_source(publisher, reader)
            .map_err(prefix_error)?
            .is_none()
        {
            return Err("source does not require observed execution".into());
        }
        if playing && !finished {
            self.resume();
        } else {
            self.pause();
        }
        Ok(())
    }
    pub(crate) fn begin_observed_prepared_source(
        &mut self,
        effects: noon::ContinuationEffects,
    ) -> Result<bool, AuthoringFailure> {
        self.live_segment()?;
        // An interpreter invocation may perform arbitrary host work after this
        // await. Scene replay metadata is not a source-purity proof. Keep its
        // original stack behind the ordinary observed completion protocol.
        self.session
            .begin_observed_prepared_source(effects)
            .map(|requirement| requirement.is_some())
            .map_err(prefix_error)
    }

    pub(crate) fn validate_prepared_publisher(
        &self,
        publisher: &PrefixPublisher,
    ) -> Result<(), AuthoringFailure> {
        self.session
            .validate_prepared_publisher(publisher)
            .map_err(prefix_error)
    }

    /// Complete the pending admitted segment without waiting for presentation.
    /// This does not resume source, publish a renderer delta or authorize input.
    /// A rejected source domain leaves its pending segment at the original time.
    pub(crate) fn live_prepare_segment(&mut self) -> Result<f64, AuthoringFailure> {
        self.live_prepare_prefix_segment(None)
    }

    pub(crate) fn live_prepare_prefix_segment(
        &mut self,
        publisher: Option<&PrefixPublisher>,
    ) -> Result<f64, AuthoringFailure> {
        if let Some(requirement) = self
            .session
            .preflight_prepared_source(publisher)
            .map_err(prefix_error)?
        {
            return Err(AuthoringFailure::new(
                "unsupported_domain",
                "prepared.execution_required",
                format!("source requires observed execution: {requirement:?}"),
            ));
        }
        let segment = self.live_segment()?;
        self.live_advance_segment_to(segment.end_time())?;
        self.live_complete_segment()?;
        Ok(segment.end_time())
    }

    fn completed_prepared_boundary(&self) -> Result<ExecutionSegment, AuthoringFailure> {
        let Some(LiveSegmentReceipt::Completed(segment)) = self.live_segment else {
            return Err(AuthoringFailure::new(
                "pending_work",
                "prepared.segment_pending",
                "complete the source segment before publishing prepared history",
            ));
        };
        if self.session.frame().time != segment.end_time()
            || !self.session.segment_state(segment).is_complete()
        {
            return Err(prefix_error(noon::PrefixError::NotReady));
        }
        Ok(segment)
    }

    pub(crate) fn prepare_initial_source_segment(&mut self) -> Result<bool, AuthoringFailure> {
        self.live_prepare_segment()?;
        self.session
            .has_prepared_prefix_interval(self.completed_prepared_boundary()?)
            .map_err(prefix_error)
    }

    pub(crate) fn create_finished_static_reader(
        &mut self,
    ) -> Result<PrefixReader, AuthoringFailure> {
        let boundary = self.completed_prepared_boundary()?;
        if boundary.end_time() != 0.0 || self.session.finite_timeline_end_time() != Some(0.0) {
            return Err(prefix_error(noon::PrefixError::NotReady));
        }
        self.seal_prepared_source()?;
        self.session
            .create_finished_prepared_reader()
            .map_err(prefix_error)
    }

    pub(crate) fn create_prepared_prefix(
        &self,
    ) -> Result<(PrefixPublisher, PrefixReader), AuthoringFailure> {
        self.session
            .create_prepared_prefix(self.completed_prepared_boundary()?)
            .map_err(prefix_error)
    }

    pub(crate) fn publish_prepared_prefix(
        &self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, AuthoringFailure> {
        self.session
            .publish_prepared_prefix(self.completed_prepared_boundary()?, publisher)
            .map_err(prefix_error)
    }

    /// Explicit source completion is distinct from segment completion/delivery.
    /// Keep sealing separate from extraction so a foreign publisher cannot seal
    /// an otherwise live source as an incidental side effect of a rejected call.
    pub(crate) fn seal_prepared_source(&mut self) -> Result<(), AuthoringFailure> {
        self.completed_prepared_boundary()?;
        self.session
            .validate_prepared_source_domain()
            .map_err(prefix_error)?;
        self.session
            .seal_replay()
            .map_err(|error| AuthoringFailure::unclassified("prepared.seal", &error))
    }

    pub(crate) fn finish_prepared_prefix(
        &self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, AuthoringFailure> {
        self.session
            .finish_prepared_prefix(self.completed_prepared_boundary()?, publisher)
            .map_err(prefix_error)
    }
}
