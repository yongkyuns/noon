//! Prepared playback at the existing execution-to-render worker boundary.
//!
//! This host owns one PrefixReader and disposable transport/presentation state,
//! never source semantics. The same retained codec, pointer admission, evaluator
//! and clock policy as the existing hosts handle its selected historical frame.
use crate::browser_pointer_input::BrowserPointerBinding;
use crate::prepared_canvas_playback::PreparedCanvasPlayback;
use crate::semantic_execution_player::WasmExecutionWake;
use crate::worker_pointer_presentation::{WorkerPointerInput, WorkerPointerPresentation};
use crate::{
    BrowserExecutionCadence, BrowserExecutionWakeClock, BrowserExecutionWakePlan, BrowserHostWake,
    RetainedFamilyExecutionDeltaEncoder, RetainedFamilyExecutionDeltaEnvelope,
    RetainedResourceBundle,
};
use noon::{PrefixBatch, PrefixBatchStats, PrefixError, PrefixReader};

mod source;
use source::SourceDelivery;
mod audio;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct PreparedExecutionPlayer {
    playback: PreparedCanvasPlayback,
    clock: BrowserExecutionWakeClock,
    encoder: RetainedFamilyExecutionDeltaEncoder,
    snapshot_sent: bool,
    last_selection: Option<noon::integration::PointerSelectionPresentation>,
    pointer: WorkerPointerPresentation,
    binding: Option<BrowserPointerBinding>,
    sequence: u64,
    // Optional interpreter delivery cursor; never the source scene/runtime.
    source: Option<SourceDelivery>,
    // Ordered output anchors are disposable host transport state, not a clock.
    audio_sequence: u32,
    // Attained open frontier, held as a transport observation while the reader
    // keeps its last coherent frame. This is not a speculative runtime sample.
    held_frontier: Option<f64>,
}

impl PreparedExecutionPlayer {
    /// Move the independent reader into its worker host. The producer stays with
    /// its caller. No runtime, authoring context or history is cloned/transferred
    /// across WASM instances by this typed in-process constructor.
    pub fn from_reader(reader: PrefixReader, transport_session: u32) -> Result<Self, String> {
        Ok(Self {
            playback: PreparedCanvasPlayback::for_worker(reader),
            clock: BrowserExecutionWakeClock::default(),
            encoder: RetainedFamilyExecutionDeltaEncoder::new(transport_session),
            snapshot_sent: false,
            last_selection: None,
            pointer: WorkerPointerPresentation::default(),
            binding: None,
            sequence: 0,
            source: None,
            audio_sequence: 0,
            held_frontier: None,
        })
    }

    pub fn reserve_object_capacity(&mut self, total: usize) -> Result<usize, PrefixError> {
        self.playback.reader.reserve_object_capacity(total)
    }

    /// Borrow before transfer so failed admission cannot lose the host's batch.
    pub fn preflight_append(&self, batch: &PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        self.playback.reader.preflight_append(batch)
    }

    /// Delivery does not issue a render sequence or alter a displayed receipt.
    pub fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        let was_buffering = self.playback.buffering();
        let stats = self.playback.append(batch)?;
        if was_buffering && !self.playback.buffering() {
            self.clock = BrowserExecutionWakeClock::default();
        }
        Ok(stats)
    }

    fn delta(
        &mut self,
        snapshot: bool,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, String> {
        let reader = &mut self.playback.reader;
        let selection = reader.pointer_selection_presentation();
        let overlay = selection
            .as_ref()
            .map(crate::SelectionOverlayPresentation::from_presentation)
            .transpose()
            .map_err(|error| error.to_string())?;
        let camera = reader
            .inspection_camera()
            .map_err(|error| error.to_string())?;
        let frame = self.pointer.capture(reader)?;
        let refresh = self.pointer.needs_delta(reader);
        let publication = reader.take_renderer_publication();
        let Some(mut delta) = self.encoder.encode_renderer_publication(
            &publication,
            camera,
            snapshot || !self.snapshot_sent,
            refresh || selection != self.last_selection,
        )?
        else {
            return Ok(None);
        };
        // The prepared source gate rejects inset views; no frontend reconstructs
        // them. Selection and pointer receipts remain host-derived presentation.
        delta.selection_overlay = overlay;
        delta.pointer_view = self.pointer.view().filter(|view| view.drawable());
        self.pointer
            .issue(delta.retained.session, delta.retained.sequence, frame);
        self.snapshot_sent = true;
        self.last_selection = selection;
        self.encoder.compact_dense_rows(&mut delta);
        Ok(Some(delta))
    }

    fn validate_wall_time(wall_time_ms: f64) -> Result<(), String> {
        if !wall_time_ms.is_finite() || wall_time_ms < 0.0 {
            return Err("prepared playback requires finite non-negative wall time".into());
        }
        Ok(())
    }

    fn observe_clock(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<crate::BrowserExecutionWakeDirective, String> {
        Self::validate_wall_time(wall_time_ms)?;
        if self.prepared_needs_frame() {
            // Request one immediate evaluation once the held boundary is ready.
            // Until then observations stay frozen; none acknowledges its picture.
            self.clock
                .reanchor(wall_time_ms, self.held_frontier.unwrap())
                .ok_or("prepared held frontier cannot represent wall time")?;
        }
        self.clock
            .directive(
                BrowserExecutionWakePlan::from_runtime(self.wake_state()),
                wall_time_ms,
                self.playback.reader.frame().time,
            )
            .ok_or_else(|| "prepared playback clock cannot represent wall time".into())
    }

    fn tick(&mut self, wall_time_ms: f64) -> Result<(), String> {
        Self::validate_wall_time(wall_time_ms)?;
        if self.prepared_needs_frame() {
            self.playback
                .advance_clock_sample(self.held_frontier.unwrap())
                .map_err(|error| error.to_string())?;
            self.held_frontier = None;
            // The endpoint must present this now-readable boundary before it
            // starts the next interval. Neither buffering nor renderer latency
            // is charged to playback by this catch-up evaluation.
            self.clock = BrowserExecutionWakeClock::default();
            self.playback
                .reader
                .advance_interactions(wall_time_ms / 1_000.0)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        let plan = BrowserExecutionWakePlan::from_runtime(self.wake_state());
        let directive = self.observe_clock(wall_time_ms)?;
        let target = match (directive.wake(), plan.cadence()) {
            (BrowserHostWake::AnimationFrame, BrowserExecutionCadence::AnimationFrame) => Some(
                self.clock
                    .scene_time_at(wall_time_ms)
                    .ok_or("missing prepared playback epoch")?,
            ),
            (
                BrowserHostWake::TimerAfterMilliseconds(delay),
                BrowserExecutionCadence::TimerAtSceneTime(deadline),
            ) if delay <= 0.0 => Some(
                self.clock
                    .scene_time_at(wall_time_ms)
                    .ok_or("missing prepared playback epoch")?
                    .max(deadline),
            ),
            _ => None,
        };
        if let Some(target) = target {
            self.playback
                .advance_clock_sample(target)
                .map_err(|error| error.to_string())?;
            self.hold_buffered_frontier();
        }
        self.playback
            .reader
            .advance_interactions(wall_time_ms / 1_000.0)
            .map_err(|error| error.to_string())?;
        self.observe_clock(wall_time_ms)?;
        Ok(())
    }

    fn hold_buffered_frontier(&mut self) {
        if self.playback.buffering() && self.held_frontier.is_none() {
            self.held_frontier = Some(self.playback.reader.frontier());
        }
    }

    /// Observe elapsed host time against the old adopted availability before a
    /// suffix can extend it. Synchronous interpreter work may have prevented
    /// every playback wake while sound reached that old frontier. Such elapsed
    /// wall time must never be applied retroactively to newly delivered history.
    fn observe_delivery_clock(&mut self, wall_time_ms: f64) -> Result<(), String> {
        Self::validate_wall_time(wall_time_ms)?;
        if !self.playback.playing()
            || self.held_frontier.is_some()
            || self.playback.reader.end_time().is_some()
        {
            return Ok(());
        }
        let Some(observed) = self.clock.scene_time_at(wall_time_ms) else {
            // A cold host has no admitted clock; observation cannot start one.
            // An existing but unrepresentable mapping must fail closed instead.
            return if self.clock.scene_time_at(0.0).is_some() {
                Err("prepared delivery clock cannot represent wall time".into())
            } else {
                Ok(())
            };
        };
        let frontier = self.playback.reader.frontier();
        if observed >= frontier {
            // The exact old frontier remains unreadable here. This operation
            // only records missing-prefix backpressure, never a runtime frame.
            self.playback
                .advance_clock_sample(frontier)
                .map_err(|error| error.to_string())?;
            self.hold_buffered_frontier();
            self.clock = BrowserExecutionWakeClock::default();
        }
        Ok(())
    }

    fn wake_state(&self) -> noon_runtime::RuntimeWakeState {
        let wake = self.playback.wake_state();
        if self.prepared_needs_frame() {
            wake.with_additional_timeline(noon_runtime::TimelineWakeState::Deadline(
                self.held_frontier.unwrap(),
            ))
        } else {
            wake
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl PreparedExecutionPlayer {
    /// Valid empty bootstrap bundle: selected resources accompany the initial
    /// ordinary delta. Future resources are not scanned or sent at bootstrap.
    /// This host is one delivery stream, not a worker-restart checkpoint. Once
    /// a delta is drained the caller must retain/retry that exact output; a new
    /// receiver cannot be initialized with an incomplete resource inventory.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = resourceBundleBytes))]
    pub fn resource_bundle_bytes(&self) -> Result<Vec<u8>, String> {
        if self.snapshot_sent {
            return Err(
                "prepared transport has already started; resource bootstrap cannot be replayed"
                    .into(),
            );
        }
        RetainedResourceBundle::empty()
            .encode_binary()
            .map_err(|error| error.to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = initialDeltaJson))]
    pub fn initial_delta_json(&mut self) -> Result<String, String> {
        if self.snapshot_sent {
            return Err(
                "prepared transport has already started; initial delta cannot be repeated".into(),
            );
        }
        serde_json::to_string(
            &self
                .delta(true)?
                .ok_or("initial prepared snapshot missing")?,
        )
        .map_err(|error| error.to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = drainDeltaJson))]
    pub fn drain_delta_json(&mut self) -> Result<Option<String>, String> {
        self.delta(false)?
            .map(|delta| serde_json::to_string(&delta).map_err(|error| error.to_string()))
            .transpose()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = drainDeltaTransportJson))]
    pub fn drain_delta_transport_json(&mut self) -> Result<Option<String>, String> {
        self.delta(false)?
            .map(|delta| delta.encode_worker_carrier())
            .transpose()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = tickDeltaJson))]
    pub fn tick_delta_json(&mut self, wall_time_ms: f64) -> Result<Option<String>, String> {
        self.tick(wall_time_ms)?;
        self.drain_delta_json()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = executionWake))]
    pub fn execution_wake(&mut self, wall_time_ms: f64) -> Result<WasmExecutionWake, String> {
        let directive = self.observe_clock(wall_time_ms)?;
        let wake = self.wake_state();
        let wake = if self.playback.reader.interactions_active() {
            wake.with_additional_timeline(noon_runtime::TimelineWakeState::Continuous)
        } else {
            wake
        };
        let timer = match directive.wake() {
            BrowserHostWake::TimerAfterMilliseconds(delay) => Some(delay),
            _ => None,
        };
        Ok(WasmExecutionWake::from_plan(
            BrowserExecutionWakePlan::from_runtime(wake),
            timer,
        ))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = seekDeltaJson))]
    pub fn seek_delta_json(&mut self, time: f64) -> Result<Option<String>, String> {
        self.playback
            .seek(time)
            .map_err(|error| error.to_string())?;
        self.held_frontier = None;
        self.clock = BrowserExecutionWakeClock::default();
        self.drain_delta_json()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setPreparedPlaying))]
    pub fn set_prepared_playing(&mut self, playing: bool) {
        if self.playback.set_playing(playing) {
            self.clock = BrowserExecutionWakeClock::default();
        }
    }

    /// Last coherent evaluated sample, independent of the producer's frontier.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedTime))]
    pub fn prepared_time(&self) -> f64 {
        self.playback.reader.frame().time
    }

    /// Adopted history frontier. It is open until a terminal batch is adopted.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedFrontier))]
    pub fn prepared_frontier(&self) -> f64 {
        self.playback.reader.frontier()
    }

    /// Inclusive final time, available only after terminal history adoption.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedEndTime))]
    pub fn prepared_end_time(&self) -> Option<f64> {
        self.playback.reader.end_time()
    }

    /// User playback intent; buffering and terminal readiness remain separate.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedPlaying))]
    pub fn prepared_playing(&self) -> bool {
        self.playback.playing()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedBuffering))]
    pub fn prepared_buffering(&self) -> bool {
        self.playback.buffering()
    }

    /// A previously attained frontier is now readable but not yet evaluated.
    /// An ordinary tick selects it; append and this observation select nothing.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedNeedsFrame))]
    pub fn prepared_needs_frame(&self) -> bool {
        self.held_frontier.is_some() && self.playback.playing() && !self.playback.buffering()
    }

    /// Observe elapsed time in a sleeping static interval through the existing
    /// Rust clock. This never starts/reanchors that clock or evaluates a frame.
    /// Active animation stays at its last coherent sample; a static observation
    /// stops at the next runtime deadline, which may be the unreadable frontier.
    /// Observing that boundary does not authorize selecting it or prove display.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = preparedPlaybackTimeAt))]
    pub fn prepared_playback_time_at(&self, wall_time_ms: f64) -> Result<f64, String> {
        Self::validate_wall_time(wall_time_ms)?;
        if let Some(held) = self.held_frontier {
            return Ok(held);
        }
        let current = self.prepared_time();
        let BrowserExecutionCadence::TimerAtSceneTime(deadline) =
            BrowserExecutionWakePlan::from_runtime(self.playback.wake_state()).cadence()
        else {
            return Ok(current);
        };
        Ok(self
            .clock
            .scene_time_at(wall_time_ms)
            .unwrap_or(current)
            .min(deadline)
            .max(current))
    }

    /// Preserve elapsed interior static-wait time before pausing, without a
    /// frontend clock or an epsilon seek. Reaching an open frontier buffers at
    /// the last coherent sample instead. Any renderer delta remains available to
    /// the caller's ordinary ordered drain; this call never acknowledges display.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = pausePreparedAt))]
    pub fn pause_prepared_at(&mut self, wall_time_ms: f64) -> Result<(), String> {
        let time = self.prepared_playback_time_at(wall_time_ms)?;
        if time > self.prepared_time() {
            self.playback
                .advance(time)
                .map_err(|error| error.to_string())?;
            self.hold_buffered_frontier();
            if self.held_frontier == Some(self.prepared_time()) {
                self.held_frontier = None;
            }
        }
        self.set_prepared_playing(false);
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = needsPreparation))]
    pub fn needs_preparation(&self, lookahead_seconds: f64) -> Result<bool, String> {
        self.playback
            .needs_preparation(lookahead_seconds)
            .map_err(str::to_owned)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setPointerFillSelection))]
    pub fn set_pointer_fill_selection(&mut self, tolerance: Option<f32>) -> Result<(), String> {
        match tolerance {
            Some(tolerance) => self
                .playback
                .reader
                .enable_pointer_fill_selection(tolerance),
            None => self.playback.reader.disable_pointer_fill_selection(),
        }
        .map_err(|error| error.to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setBrowserPointerViewJson))]
    pub fn set_browser_pointer_view_json(&mut self, json: &str) -> Result<(), String> {
        let view =
            serde_json::from_str(json).map_err(|error| format!("invalid pointer view: {error}"))?;
        self.pointer.set_view(
            &mut self.playback.reader,
            &mut self.binding,
            &mut self.sequence,
            view,
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = notePointerPresentationJson))]
    pub fn note_pointer_presentation_json(&mut self, json: &str) -> Result<bool, String> {
        let receipt = serde_json::from_str(json)
            .map_err(|error| format!("invalid pointer receipt: {error}"))?;
        self.pointer
            .note_presented(&mut self.playback.reader, receipt)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = invalidatePointerPresentationJson))]
    pub fn invalidate_pointer_presentation_json(&mut self, json: &str) -> Result<bool, String> {
        let receipt = serde_json::from_str(json)
            .map_err(|error| format!("invalid pointer receipt: {error}"))?;
        self.pointer.invalidate(
            &mut self.playback.reader,
            &mut self.binding,
            &mut self.sequence,
            receipt,
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = submitBrowserPointerInputJson))]
    pub fn submit_browser_pointer_input_json(&mut self, json: &str) -> Result<bool, String> {
        let envelope: WorkerPointerInput = serde_json::from_str(json)
            .map_err(|error| format!("invalid browser pointer input: {error}"))?;
        self.pointer.submit(
            &mut self.playback.reader,
            &mut self.binding,
            &mut self.sequence,
            envelope.input,
            envelope.presentation,
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = scrollInspectionViewJson))]
    pub fn scroll_inspection_view_json(&mut self, json: &str) -> Result<Option<bool>, String> {
        let input = serde_json::from_str(json)
            .map_err(|error| format!("invalid inspection scroll: {error}"))?;
        self.pointer
            .scroll(&mut self.playback.reader, &mut self.binding, input)
    }

    /// Observations only. Hosts must use executionWake/needsPreparation to decide
    /// when to schedule work, never parse these diagnostics as another timeline.
    #[cfg(any(test, debug_assertions, feature = "renderer-smoke"))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = debugPreparedPlaybackJson))]
    pub fn debug_prepared_playback_json(&self) -> String {
        serde_json::json!({
            "time": self.playback.reader.frame().time,
            "frontier": self.playback.reader.frontier(),
            "end": self.playback.reader.end_time(),
            "playing": self.playback.playing(),
            "buffering": self.playback.buffering(),
            "interactions": self.playback.reader.interactions_active(),
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests;
