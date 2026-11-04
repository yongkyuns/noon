//! Production interpreter-to-reader handoff. Source semantics stay in the
//! canonical context; this host retains only its publisher and one pending batch.
//! No scene/history payload crosses JavaScript between these Rust owners.
use super::PreparedExecutionPlayer;
use crate::canonical_authoring_scene::CanonicalAuthoringScene;
use noon::{PrefixBatch, PrefixPublisher};

pub(crate) fn source_execution_requirement_json(
    requirement: Option<noon::SourceExecutionRequirement>,
) -> String {
    let Some(requirement) = requirement else {
        return "null".into();
    };
    let (code, detail) = match requirement {
        noon::SourceExecutionRequirement::HostEffects => ("host_effects", None),
        noon::SourceExecutionRequirement::HostCallbacks => ("host_callbacks", None),
        noon::SourceExecutionRequirement::EffectiveDisplay => ("effective_display", None),
        noon::SourceExecutionRequirement::NativeInput => ("native_input", None),
        noon::SourceExecutionRequirement::Signals => ("signals", None),
        noon::SourceExecutionRequirement::ReplayUnavailable(error) => {
            ("replay_unavailable", Some(error.to_string()))
        }
        noon::SourceExecutionRequirement::ExternalResources => ("external_resources", None),
    };
    let mut value = serde_json::json!({"code": code});
    if let Some(detail) = detail {
        value["detail"] = serde_json::Value::String(detail);
    }
    value.to_string()
}

pub(super) struct SourceDelivery {
    publisher: PrefixPublisher,
    pending: Option<PrefixBatch>,
    staged_navigation: Option<noon::SceneSectionNavigation>,
    required: Option<noon::SourceExecutionRequirement>,
    finished: bool,
}

impl PreparedExecutionPlayer {
    /// Existing logical prefix charges, not additive allocation or total-memory
    /// measurements. Source history, scalar schedules, interpreter/JS heaps, WASM
    /// capacity and renderer/GPU ownership require separate observations.
    pub(super) fn retention_metrics_json(&self) -> String {
        let reader = &self.playback.reader;
        let stats = reader.replay_stats();
        let source = self.source.as_ref();
        let pending = source.and_then(|source| source.pending.as_ref());
        serde_json::json!({
            "reader": {
                "historyPayloadBytes": reader.retained_history_payload_bytes(),
                "historyPayloadBudgetBytes": reader.history_payload_budget_bytes(),
                "resourceBytes": reader.retained_resource_bytes(),
                "resourceBudgetBytes": reader.resource_budget_bytes(),
                "revisions": stats.revisions_retained,
                "payloads": stats.payloads_retained,
            },
            "extracted": source.map(|source| serde_json::json!({
                "historyPayloadBytes": source.publisher.extracted_history_payload_bytes(),
                "resourceBytes": source.publisher.extracted_resource_bytes(),
            })),
            "pending": {
                "batches": usize::from(pending.is_some()),
                "historyPayloadBytes": pending.map_or(0, PrefixBatch::history_payload_bytes),
                "resourceBytes": pending.map_or(0, PrefixBatch::resource_bytes),
            },
        })
        .to_string()
    }

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
        player.set_section_navigation(context.section_navigation_snapshot());
        player.source = Some(SourceDelivery {
            publisher,
            pending: None,
            staged_navigation: None,
            required: None,
            finished: false,
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
    ) -> Result<bool, String> {
        self.validate_source(context)?;
        let source = self.source.as_mut().expect("validated source delivery");
        if source.finished {
            return Err("prepared source already finished".into());
        }
        source.required = context
            .prefix_source_requirement(&source.publisher, false)
            .map_err(|e| e.to_string())?;
        if source.required.is_some() {
            return Ok(false);
        }
        // An engine failure/unwind cannot leave partially completed work eligible
        // for another attempt. The historical reader remains independent. This
        // does not catch a panic, recover a WASM trap, or retry user statements.
        let mut source = self.source.take().expect("validated source delivery");
        let endpoint = context
            .prepare_prefix_segment(&source.publisher)
            .map_err(|e| e.to_string())?;
        if endpoint > source.publisher.frontier() {
            source.pending = Some(
                context
                    .publish_prepared_prefix(&mut source.publisher)
                    .map_err(|e| e.to_string())?,
            );
        } else if endpoint != source.publisher.frontier() {
            return Err("prepared source moved behind its publication frontier".into());
        }
        source.staged_navigation = Some(context.section_navigation_snapshot());
        // A zero-duration segment may complete without a new readable interval.
        // Its same-time edits remain in ordinary source history for a later suffix.
        self.source = Some(source);
        Ok(true)
    }

    pub fn begin_source_execution(
        &mut self,
        context: &mut CanonicalAuthoringScene,
    ) -> Result<bool, String> {
        self.validate_source(context)?;
        if !self.source_execution_ready() {
            return Err("source execution requires actual playback demand at its frontier".into());
        }
        let playing = self.playback.playing();
        let source = self.source.as_ref().expect("validated source delivery");
        let finished = source.finished;
        context
            .begin_observed_prefix_source(
                &source.publisher,
                &self.playback.reader,
                playing,
                source.finished,
            )
            .map_err(|e| e.to_string())?;
        self.source = None;
        Ok(playing && !finished)
    }

    /// After source cleanup, stage admitted terminal history. False retains the
    /// completed original source for observed execution at actual reader demand;
    /// it does not advertise a final replay range.
    pub fn finish_interpreter_source(
        &mut self,
        context: &mut CanonicalAuthoringScene,
    ) -> Result<bool, String> {
        self.validate_source(context)?;
        let source = self.source.as_mut().expect("validated source delivery");
        if source.finished {
            return Err("prepared source already finished".into());
        }
        source.required = context
            .prefix_source_requirement(&source.publisher, true)
            .map_err(|e| e.to_string())?;
        source.finished = true;
        source.staged_navigation = Some(context.section_navigation_snapshot());
        if source.required.is_some() {
            return Ok(false);
        }
        let mut source = self.source.take().expect("validated source delivery");
        context.seal_prepared_source().map_err(|e| e.to_string())?;
        source.pending = Some(
            context
                .finish_prepared_prefix(&mut source.publisher)
                .map_err(|e| e.to_string())?,
        );
        self.source = Some(source);
        Ok(true)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl PreparedExecutionPlayer {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = sourceExecutionRequirementJson))]
    pub fn source_execution_requirement_json(&self) -> String {
        source_execution_requirement_json(self.source.as_ref().and_then(|source| source.required))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = sourceExecutionReady))]
    pub fn source_execution_ready(&self) -> bool {
        self.source
            .as_ref()
            .is_some_and(|source| source.required.is_some())
            && self.playback.buffering()
    }
    /// Explicit maintenance for the single borrowed pending suffix. Rust owns
    /// object/plan counts and permanent admission; JavaScript carries no batch.
    /// False denotes no pending output, including a zero-duration boundary.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = reserveSourcePrefixCapacity))]
    pub fn reserve_source_prefix_capacity(&mut self) -> Result<bool, String> {
        let Some(batch) = self
            .source
            .as_ref()
            .and_then(|source| source.pending.as_ref())
        else {
            return Ok(false);
        };
        self.playback
            .reader
            .reserve_append_capacity(batch)
            .map_err(|e| e.to_string())?;
        Ok(true)
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
        let navigation = self
            .source
            .as_mut()
            .expect("validated source")
            .staged_navigation
            .take();
        if let Err(error) = self.append(batch) {
            // The same-owner preflight above admits all fallible validation. Do
            // not permit source retries if that internal invariant ever changes.
            self.source = None;
            return Err(error.to_string());
        }
        if let Some(navigation) = navigation {
            self.set_section_navigation(navigation);
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
        /// Choose the original completed runtime when static history cannot
        /// represent all authored work. Source execution and cleanup are done.
        #[wasm_bindgen(js_name = prepareFinishedSource)]
        pub fn prepare_finished_source(&mut self) -> Result<bool, JsValue> {
            self.inner
                .prepare_finished_source()
                .map_err(|e| JsValue::from_str(&e))
        }

        /// Completed static source uses the same reader/worker presentation path.
        #[wasm_bindgen(js_name = createFinishedPreparedExecutionPlayer)]
        pub fn create_finished_prepared_execution_player(
            &mut self,
            transport_session: u32,
        ) -> Result<Option<PreparedExecutionPlayer>, JsValue> {
            let reader = self
                .inner
                .create_finished_static_reader()
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            let Some(reader) = reader else {
                return Ok(None);
            };
            let mut player = PreparedExecutionPlayer::from_reader(reader, transport_session);
            player.set_section_navigation(self.inner.section_navigation_snapshot());
            player.set_prepared_playing(false);
            Ok(Some(player))
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
        ) -> Result<bool, JsValue> {
            self.prepare_interpreter_segment(&mut context.inner)
                .map_err(|e| JsValue::from_str(&e))
        }

        #[wasm_bindgen(js_name = beginSourceExecution)]
        pub fn begin_source_execution_wasm(
            &mut self,
            context: &mut CanonicalAuthoringSceneContext,
        ) -> Result<bool, JsValue> {
            self.begin_source_execution(&mut context.inner)
                .map_err(|e| JsValue::from_str(&e))
        }

        #[wasm_bindgen(js_name = finishSourcePreparation)]
        pub fn finish_source_preparation(
            &mut self,
            context: &mut CanonicalAuthoringSceneContext,
        ) -> Result<bool, JsValue> {
            self.finish_interpreter_source(&mut context.inner)
                .map_err(|e| JsValue::from_str(&e))
        }
    }
}

#[cfg(test)]
mod tests;
