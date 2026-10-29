//! Test-only holder for raw compiled-row fixtures. Product playback owns an
//! ExecutionSession instead; no picking/interaction policy is implemented here.
#![allow(dead_code)]
use noon_core::{ObjectId, SemanticClickIndicate};
use noon_runtime::{
    FrameState, PrefixBatch, PrefixBatchStats, PrefixError, PrefixPublisher, PrefixReceiver,
    PreparedTransientAnimation, RendererPublication, ReplayStats, SceneInstance,
};

#[derive(Debug)]
pub struct PrefixHarness {
    pub runtime: SceneInstance,
    pub receiver: PrefixReceiver,
}
pub fn bootstrap(source: &SceneInstance) -> Result<(PrefixPublisher, PrefixHarness), PrefixError> {
    let (publisher, runtime, receiver) = source.create_prefix_projection()?;
    Ok((publisher, PrefixHarness { runtime, receiver }))
}
impl PrefixHarness {
    pub fn frontier(&self) -> f64 {
        self.receiver.frontier()
    }
    pub fn end_time(&self) -> Option<f64> {
        self.receiver.end_time()
    }
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }
    pub fn replay_stats(&self) -> ReplayStats {
        self.runtime.replay_stats()
    }
    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.receiver.prepare_seek(&mut self.runtime, time)?;
        self.runtime.seek(time)?;
        Ok(self.runtime.frame())
    }
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.receiver.prepare_advance(&mut self.runtime, time)?;
        self.runtime.advance_to(time)?;
        Ok(self.runtime.frame())
    }
    pub fn reserve_object_capacity(&mut self, total: usize) -> Result<usize, PrefixError> {
        self.receiver
            .reserve_object_capacity(&mut self.runtime, total)
    }
    pub fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        self.receiver.append(&mut self.runtime, batch)
    }
    pub fn preflight_append(&self, batch: &PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        self.receiver.preflight_append(&self.runtime, batch)
    }
    pub fn prepare_click_indicate(
        &self,
        object: ObjectId,
        binding: SemanticClickIndicate,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.receiver
            .prepare_click_indicate(&self.runtime, object, binding)
    }
    pub fn prepare_bound_click_indicate(
        &self,
        object: ObjectId,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.receiver
            .prepare_bound_click_indicate(&self.runtime, object)
    }
    pub fn start_transient_animation(
        &mut self,
        prepared: PreparedTransientAnimation,
    ) -> Result<(), PrefixError> {
        self.runtime
            .start_transient_animation(prepared)
            .map_err(PrefixError::Interaction)
    }
    pub fn advance_interactions(&mut self, time: f64) -> Result<(), PrefixError> {
        self.runtime
            .advance_interactions(time)
            .map_err(PrefixError::Interaction)
    }
    pub fn interactions_active(&self) -> bool {
        self.runtime.interactions_active()
    }
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.runtime.take_renderer_publication()
    }
}
