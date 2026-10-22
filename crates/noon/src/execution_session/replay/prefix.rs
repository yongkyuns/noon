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

    /// Publish the exact endpoint after the source has explicitly sealed replay.
    /// The caller declares that no further source continuation is expected, calls
    /// `seal_replay`, then supplies the actual completed final boundary here.
    /// This never seals implicitly, resumes source code, or acknowledges rendering.
    ///
    /// The terminal batch includes every still-undelivered write, including writes
    /// at the previous frontier's timestamp. Only its successful adoption admits
    /// that endpoint in the reader; the current frame and effects stay unchanged.
    /// There is no additional full-scene snapshot or terminal frame evaluation.
    pub fn finish_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime.prepare_prefix_finish(publisher)
    }
}
