//! Production interpreter-to-reader handoff. Source semantics stay in the
//! canonical context; this host retains only its publisher and one pending batch.
//! No scene/history payload crosses JavaScript between these Rust owners.
use super::PreparedExecutionPlayer;
use crate::canonical_authoring_scene::CanonicalAuthoringScene;
use noon::{PrefixBatch, PrefixError, PrefixPublisher};

pub(super) struct SourceDelivery {
    publisher: PrefixPublisher,
    pending: Option<PrefixBatch>,
}

impl PreparedExecutionPlayer {
    /// Bootstrap only after the interpreter's actual first segment is complete.
    /// The canonical source keeps its existing live player and semantic store.
    pub fn from_completed_source(
        context: &mut CanonicalAuthoringScene,
        transport_session: u32,
    ) -> Result<Self, String> {
        let (publisher, reader) = context
            .create_prepared_prefix()
            .map_err(|e| e.to_string())?;
        let mut player = Self::from_reader(reader, transport_session);
        player.source = Some(SourceDelivery {
            publisher,
            pending: None,
        });
        Ok(player)
    }

    fn validate_source(&mut self, context: &mut CanonicalAuthoringScene) -> Result<(), String> {
        let source = self
            .source
            .as_ref()
            .ok_or("prepared source delivery is closed")?;
        if source.pending.is_some() {
            return Err(
                "deliver the pending prefix before completing another source segment".into(),
            );
        }
        // Reject a foreign, replaced or leased source before it is advanced or
        // sealed. Rejection here does not retire the rightful source binding.
        context
            .validate_prepared_publisher(&source.publisher)
            .map_err(|e| e.to_string())
    }

    /// Complete once and retain its new suffix until explicit adoption.
    /// Validation rejects foreign sources before they can advance.
    pub fn prepare_interpreter_segment(
        &mut self,
        context: &mut CanonicalAuthoringScene,
    ) -> Result<(), String> {
        self.validate_source(context)?;
        // An engine failure/unwind cannot leave partially completed work eligible
        // for another attempt. The historical reader remains independent. This
        // does not catch a panic, recover a WASM trap, or retry user statements.
        let mut source = self.source.take().expect("validated source delivery");
        let endpoint = context.prepare_live_segment().map_err(|e| e.to_string())?;
        if endpoint > source.publisher.frontier() {
            source.pending = Some(
                context
                    .publish_prepared_prefix(&mut source.publisher)
                    .map_err(|e| e.to_string())?,
            );
        } else if endpoint != source.publisher.frontier() {
            return Err("prepared source moved behind its publication frontier".into());
        }
        // A zero-duration segment may complete without a new readable interval.
        // Its same-time edits remain in ordinary source history for a later suffix.
        self.source = Some(source);
        Ok(())
    }

    /// Explicit interpreter completion retains a terminal suffix; only adoption
    /// makes the endpoint readable. The caller must have finished source cleanup.
    pub fn finish_interpreter_source(
        &mut self,
        context: &mut CanonicalAuthoringScene,
    ) -> Result<(), String> {
        self.validate_source(context)?;
        let mut source = self.source.take().expect("validated source delivery");
        context.seal_prepared_source().map_err(|e| e.to_string())?;
        source.pending = Some(
            context
                .finish_prepared_prefix(&mut source.publisher)
                .map_err(|e| e.to_string())?,
        );
        self.source = Some(source);
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl PreparedExecutionPlayer {
    /// Required physical-row capacity of the single undelivered suffix. This is
    /// allocation metadata, not semantic identity, authored time or a wire batch.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = pendingSourceObjectCapacity))]
    pub fn pending_source_object_capacity(&self) -> Option<usize> {
        self.source
            .as_ref()?
            .pending
            .as_ref()
            .map(PrefixBatch::required_object_capacity)
    }

    /// Explicit maintenance boundary, separate from extraction and adoption.
    /// A pending suffix must pass permanent receiver admission before any growth;
    /// only CapacityRequired is a recoverable rejection. The suffix is retained.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = reservePrefixObjectCapacity))]
    pub fn reserve_prefix_object_capacity(&mut self, total: usize) -> Result<usize, String> {
        if let Some(batch) = self
            .source
            .as_ref()
            .and_then(|source| source.pending.as_ref())
        {
            match self.preflight_append(batch) {
                Ok(_) | Err(PrefixError::CapacityRequired { .. }) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        self.reserve_object_capacity(total)
            .map_err(|e| e.to_string())
    }

    /// Adopt the same retained suffix, or fail before consuming it. A capacity
    /// rejection can be followed by explicit reservation and this call again;
    /// never call prepareSourceSegment again to regenerate lost output.
    /// Returns false when a completed zero-duration segment produced no suffix.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = deliverSourcePrefix))]
    pub fn deliver_source_prefix(&mut self) -> Result<bool, String> {
        let source = self
            .source
            .as_ref()
            .ok_or("prepared source delivery is closed")?;
        let Some(batch) = source.pending.as_ref() else {
            return Ok(false);
        };
        self.preflight_append(batch).map_err(|e| e.to_string())?;
        let batch = self
            .source
            .as_mut()
            .expect("validated source")
            .pending
            .take()
            .expect("validated pending batch");
        let terminal = batch.is_terminal();
        if let Err(error) = self.append(batch) {
            // The same-owner preflight above admits all fallible validation. Do
            // not permit source retries if that internal invariant ever changes.
            self.source = None;
            return Err(error.to_string());
        }
        if terminal {
            self.source = None;
        }
        Ok(true)
    }

    /// Retire unpublished output/cursor only. The caller owns interpreter
    /// cancellation and releasing its source context; the reader stays usable.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = stopSourcePreparation))]
    pub fn stop_source_preparation(&mut self) {
        self.source = None;
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::CanonicalAuthoringSceneContext;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    impl CanonicalAuthoringSceneContext {
        /// Completed static source uses the same reader/worker presentation path.
        #[wasm_bindgen(js_name = createFinishedPreparedExecutionPlayer)]
        pub fn create_finished_prepared_execution_player(
            &mut self,
            transport_session: u32,
        ) -> Result<PreparedExecutionPlayer, JsValue> {
            let reader = self
                .inner
                .create_finished_static_reader()
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            let mut player = PreparedExecutionPlayer::from_reader(reader, transport_session);
            player.set_prepared_playing(false);
            Ok(player)
        }

        /// Called after livePrepareSegment; no runtime lease leaves this context.
        #[wasm_bindgen(js_name = createPreparedExecutionPlayer)]
        pub fn create_prepared_execution_player(
            &mut self,
            transport_session: u32,
        ) -> Result<PreparedExecutionPlayer, JsValue> {
            PreparedExecutionPlayer::from_completed_source(&mut self.inner, transport_session)
                .map_err(|e| JsValue::from_str(&e))
        }
    }

    #[wasm_bindgen]
    impl PreparedExecutionPlayer {
        #[wasm_bindgen(js_name = prepareSourceSegment)]
        pub fn prepare_source_segment(
            &mut self,
            context: &mut CanonicalAuthoringSceneContext,
        ) -> Result<(), JsValue> {
            self.prepare_interpreter_segment(&mut context.inner)
                .map_err(|e| JsValue::from_str(&e))
        }

        #[wasm_bindgen(js_name = finishSourcePreparation)]
        pub fn finish_source_preparation(
            &mut self,
            context: &mut CanonicalAuthoringSceneContext,
        ) -> Result<(), JsValue> {
            self.finish_interpreter_source(&mut context.inner)
                .map_err(|e| JsValue::from_str(&e))
        }
    }
}

#[cfg(test)]
mod tests;
