//! Interpreter-driven preparation uses this player's actual continuation receipt.
//! Source calls remain with the interpreter; completion, history and identity do not.
use super::{AuthoringFailure, LiveSegmentReceipt, SemanticExecutionPlayer};
use noon::{ExecutionSegment, PrefixBatch, PrefixPublisher, PrefixReader};

fn prefix_error(error: noon::PrefixError) -> AuthoringFailure {
    AuthoringFailure::unclassified("prepared.prefix", &error)
}

impl SemanticExecutionPlayer {
    pub(crate) fn from_prepared_source_session(
        session: noon::ExecutionSession,
        semantics: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        semantic_root: noon_core::SemanticNodeId,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        let scene = noon::Scene::with_integration_root(semantics, semantic_root)
            .map_err(|error| error.to_string())?;
        let source = scene
            .into_execution_with_session(session)
            .map_err(|error| error.to_string())?;
        Self::from_scene_execution_with_admission(
            source,
            duration,
            transport_session,
            super::ExecutionAdmission::HeadlessSource,
        )
    }

    pub(crate) fn begin_prepared_replay_retention(&mut self) -> Result<(), String> {
        self.session
            .validate_prepared_source_domain()
            .map_err(|error| error.to_string())?;
        self.session
            .begin_replay_retention(noon_runtime::ReplayLimits::default())
            .map_err(|error| error.to_string())
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
        self.session
            .validate_prepared_source_domain()
            .map_err(prefix_error)?;
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
