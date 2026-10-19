//! Experimental source-owner admission; presentation remains independent.
use super::ExecutionSession;
use crate::ExecutionSegment;
use noon_runtime::{PrefixBatch, PrefixError, PrefixPublisher, PrefixReader};

impl ExecutionSession {
    fn ensure_prefix_boundary(&self, boundary: ExecutionSegment) -> Result<(), PrefixError> {
        // Check the original session, never a clone (which clears pending tokens).
        // Quiet waits also need the actual source-owned deadline supplied here.
        self.ensure_replay_completion()?;
        if self.frame().time != boundary.end_time() || !self.segment_state(boundary).is_complete() {
            return Err(PrefixError::NotReady);
        }
        Ok(())
    }

    /// Experimental, opt-in initial reader at an actual completed source segment.
    /// The source owner must supply its real continuation boundary, including a
    /// wait. This is not admission for arbitrary callbacks, native input, causal
    /// edits, or a complete interactive player. The resulting range is half-open.
    pub fn create_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
    ) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime.create_prefix_reader()
    }

    /// Publish a completed extension through the same Rust source admission.
    /// Only the new suffix is transferred; failure does not consume the cursor.
    pub fn publish_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime.prepare_prefix_update(publisher)
    }
}
