//! Experimental source-owner admission; presentation remains independent.
use super::ExecutionSession;
use crate::ExecutionSegment;
use noon_runtime::{PrefixBatch, PrefixError, PrefixPublisher};

mod reader;
pub use reader::PrefixReader;

impl ExecutionSession {
    /// Reject source-only behavior before a cooperative producer drives time.
    /// This deliberately permits an unfinished ordinary segment, but not a
    /// callback/causal route that would need sampled source execution.
    pub fn validate_prepared_source_domain(&self) -> Result<(), PrefixError> {
        if !self.callback_schedule.is_empty()
            || self.derived_display_plan.is_some()
            || self.translation_drag.has_targets()
            || self.runtime.inset_2d_views().next().is_some()
            || self.has_audio()
            || !self.interaction_bindings.is_empty()
            || !self.signal_timeline.is_empty()
            || !self.reactive_projection.is_empty()
        {
            return Err(noon_runtime::ReplayError::UnsupportedDomain.into());
        }
        Ok(())
    }

    /// Check an existing stream against this source before headless completion.
    /// A matching stream does not grant permission to bypass segment/domain gates.
    pub fn validate_prepared_publisher(
        &self,
        publisher: &PrefixPublisher,
    ) -> Result<(), PrefixError> {
        self.runtime.validate_prefix_publisher(publisher)
    }

    fn ensure_prefix_boundary(&self, boundary: ExecutionSegment) -> Result<(), PrefixError> {
        // Check the original session, never a clone (which clears pending tokens).
        // Quiet waits also need the actual source-owned deadline supplied here.
        self.ensure_replay_completion()?;
        // Historical inset roles and reactive routes need their own admission;
        // do not silently drop source-only orchestration from an interactive reader.
        self.validate_prepared_source_domain()?;
        if self.frame().time != boundary.end_time() || !self.segment_state(boundary).is_complete() {
            return Err(PrefixError::NotReady);
        }
        Ok(())
    }

    /// Experimental, opt-in initial reader at an actual completed source segment.
    /// The source owner must supply its real continuation boundary, including a
    /// wait. Presentation-only pointer input uses the existing shared session.
    /// Arbitrary callbacks, causal edits and a complete realtime player remain
    /// outside admission. The initial range is half-open.
    pub fn create_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
    ) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
        self.create_prepared_prefix_with_resource_budget(
            boundary,
            noon_runtime::SceneInstance::DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES,
        )
    }

    /// Bootstrap the same reader with a host-selected immutable-resource byte cap.
    /// This only bounds retained resource payloads, not total process/GPU memory.
    /// Source completion/domain admission is identical to the default bootstrap.
    pub fn create_prepared_prefix_with_resource_budget(
        &self,
        boundary: ExecutionSegment,
        resource_budget_bytes: usize,
    ) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        let (publisher, runtime, receiver) = self
            .runtime
            .create_prefix_projection_with_resource_budget(resource_budget_bytes)?;
        // Pin every retained physical identity, including rows not live at the
        // bootstrap time. Selected membership is still proved by the runtime.
        let mut slots = noon_runtime::ExecutionSlotTable::new();
        slots
            .reserve_object_capacity(runtime.frame().objects.len())
            .map_err(PrefixError::Slots)?;
        for row in &runtime.frame().objects {
            slots.insert_object(row.id).map_err(PrefixError::Slots)?;
        }
        let session = Self::from_runtime_projection(
            self.store_identity.clone(),
            Default::default(),
            Default::default(),
            slots,
            Default::default(),
            runtime,
            self.camera_object,
            None,
            super::super::callback::CallbackSchedule::new(Default::default()),
        );
        Ok((publisher, PrefixReader::new(session, receiver)))
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
