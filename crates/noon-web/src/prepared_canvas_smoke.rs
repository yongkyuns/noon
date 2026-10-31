//! Qualification-only incremental producer. Its typed batches remain in WASM;
//! JavaScript supplies only canvas lifecycle and explicit preparation triggers.
use crate::WasmExecutionCanvasRenderer;
use noon::{
    AnimationOptions, ExecutionSession, IndicateOptions, Mobject, PrefixPublisher, PrefixReader,
    Scene,
};
use noon_runtime::ReplayLimits;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
struct Producer {
    scene: Scene,
    target: Mobject,
    session: ExecutionSession,
    publisher: PrefixPublisher,
}

#[wasm_bindgen]
pub struct PreparedCanvasFixture {
    source: Option<Producer>,
    reader: Option<PrefixReader>,
}

#[wasm_bindgen]
impl PreparedCanvasFixture {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Self, JsValue> {
        let mut scene = Scene::new();
        let mut target = scene.circle(0.8).map_err(js_error)?;
        target.set_fill(0.0, 0.0, 1.0, 1.0).map_err(js_error)?;
        target.set_stroke_width(0.0).map_err(js_error)?;
        scene.add(&target).map_err(js_error)?;
        scene
            .on_click_indicate(
                &target,
                IndicateOptions::default(),
                AnimationOptions::new().run_time(0.5),
            )
            .map_err(js_error)?;
        let mut session = scene.execution_session().map_err(js_error)?;
        session
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .map_err(js_error)?;
        let first = session.wait_segment(1.0).map_err(js_error)?;
        session.advance_to(1.0).map_err(js_error)?;
        let (publisher, mut reader) = session.create_prepared_prefix(first).map_err(js_error)?;
        reader.seek(0.5).map_err(js_error)?;
        Ok(Self {
            source: Some(Producer {
                scene,
                target,
                session,
                publisher,
            }),
            reader: Some(reader),
        })
    }

    #[wasm_bindgen(js_name = createCanvas)]
    pub async fn create_canvas(
        &mut self,
        canvas: OffscreenCanvas,
    ) -> Result<WasmExecutionCanvasRenderer, JsValue> {
        let reader = self
            .reader
            .take()
            .ok_or_else(|| js_error("fixture reader already moved"))?;
        WasmExecutionCanvasRenderer::create_from_prepared_prefix(canvas, reader).await
    }

    /// Generate the next source segment only when explicitly called. It may be
    /// delivered during a paused display/press/effect. Terminal delivery drops
    /// the entire producer, so later seeks and clicks cannot consult it.
    #[wasm_bindgen(js_name = publishNext)]
    pub fn publish_next(
        &mut self,
        canvas: &mut WasmExecutionCanvasRenderer,
        terminal: bool,
    ) -> Result<(), JsValue> {
        let batch = self.next_batch(terminal)?;
        canvas.reserve_prepared_prefix_capacity(batch.required_object_capacity())?;
        canvas.preflight_prepared_prefix(&batch)?;
        canvas.append_prepared_prefix(batch)?;
        if terminal {
            self.source = None;
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = producerAlive)]
    pub fn producer_alive(&self) -> bool {
        self.source.is_some()
    }
}

impl PreparedCanvasFixture {
    fn next_batch(&mut self, terminal: bool) -> Result<noon::PrefixBatch, JsValue> {
        let source = self
            .source
            .as_mut()
            .ok_or_else(|| js_error("producer already finished"))?;
        let second = source.session.frame().time >= 2.0;
        source
            .scene
            .live(&mut source.session)
            .set_fill(
                &source.target,
                if second { 0.0 } else { 1.0 },
                if second { 1.0 } else { 0.0 },
                0.0,
                1.0,
            )
            .map_err(js_error)?;
        source
            .scene
            .live(&mut source.session)
            .shift(&source.target, 1.0, 0.0)
            .map_err(js_error)?;
        let segment = source.session.wait_segment(1.0).map_err(js_error)?;
        source
            .session
            .advance_to(segment.end_time())
            .map_err(js_error)?;
        let batch = if terminal {
            source.session.seal_replay().map_err(js_error)?;
            source
                .session
                .finish_prepared_prefix(segment, &mut source.publisher)
                .map_err(js_error)?
        } else {
            source
                .session
                .publish_prepared_prefix(segment, &mut source.publisher)
                .map_err(js_error)?
        };
        Ok(batch)
    }
}

#[wasm_bindgen]
impl PreparedCanvasFixture {
    /// Move the same reader to the existing retained worker publication channel.
    #[wasm_bindgen(js_name = createWorkerPlayer)]
    pub fn create_worker_player(
        &mut self,
        session: u32,
    ) -> Result<crate::PreparedExecutionPlayer, JsValue> {
        let reader = self
            .reader
            .take()
            .ok_or_else(|| js_error("fixture reader already moved"))?;
        crate::PreparedExecutionPlayer::from_reader(reader, session).map_err(js_error)
    }

    #[wasm_bindgen(js_name = publishNextToWorker)]
    pub fn publish_next_to_worker(
        &mut self,
        player: &mut crate::PreparedExecutionPlayer,
        terminal: bool,
    ) -> Result<(), JsValue> {
        let batch = self.next_batch(terminal)?;
        player
            .reserve_object_capacity(batch.required_object_capacity())
            .map_err(js_error)?;
        player.preflight_append(&batch).map_err(js_error)?;
        player.append(batch).map_err(js_error)?;
        if terminal {
            self.source = None;
        }
        Ok(())
    }
}

// Cooperative fixture below uses the public shared source driver. Only lifecycle
// calls cross WASM; the source, histories and batches never leave Rust.
use noon::{
    ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, PreparedSource,
    PreparedSourceStatus,
};
use std::{cell::Cell, rc::Rc};

struct CooperativeScene {
    marker: Mobject,
    calls: Rc<Cell<u32>>,
    dropped: Rc<Cell<bool>>,
    steps: u32,
    fail_at: u32,
    animated: bool,
}
impl Drop for CooperativeScene {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}
impl LiveContinuation for CooperativeScene {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let stage = self.calls.get();
        self.calls.set(stage + 1);
        if stage == self.fail_at {
            return live.wait_segment(-1.0).map(ContinuationStep::Await);
        }
        if stage == 0 {
            return Ok(ContinuationStep::Await(live.wait_segment(1.0)?));
        }
        if stage > self.steps {
            return Ok(ContinuationStep::Finished);
        }
        let x = -2.0 + f64::from(stage % 8) * 0.25;
        if self.animated {
            let target = live.target_editor(&self.marker)?;
            live.set_translation(&target, x, 2.0)?;
            return Ok(ContinuationStep::Await(
                live.declare_and_activate_transform_to(
                    &self.marker,
                    &target,
                    AnimationOptions::new()
                        .run_time(0.25)
                        .rate_func(noon_core::RateFunction::Linear),
                )?,
            ));
        }
        live.set_translation(&self.marker, x, 2.0)?;
        Ok(ContinuationStep::Await(live.wait_segment(0.25)?))
    }
}
#[wasm_bindgen]
pub struct CooperativeCanvasFixture {
    source: PreparedSource<CooperativeScene>,
    calls: Rc<Cell<u32>>,
    dropped: Rc<Cell<bool>>,
}
#[wasm_bindgen]
impl CooperativeCanvasFixture {
    #[wasm_bindgen(constructor)]
    pub fn new(steps: u32, fail_at: u32, animated: bool) -> Result<Self, JsValue> {
        if steps == 0 || steps > 256 {
            return Err(js_error("fixture steps must be 1..256"));
        }
        let mut scene = Scene::new();
        let mut target = scene.circle(0.8).map_err(js_error)?;
        target.set_fill(0.0, 0.0, 1.0, 1.0).map_err(js_error)?;
        target.set_stroke_width(0.0).map_err(js_error)?;
        scene.add(&target).map_err(js_error)?;
        scene
            .on_click_indicate(
                &target,
                IndicateOptions::default(),
                AnimationOptions::new().run_time(0.5),
            )
            .map_err(js_error)?;
        let mut marker = scene.circle(0.15).map_err(js_error)?;
        marker.set_fill(1.0, 0.0, 0.0, 1.0).map_err(js_error)?;
        marker.set_translation(-2.0, 2.0).map_err(js_error)?;
        scene.add(&marker).map_err(js_error)?;
        let calls = Rc::new(Cell::new(0));
        let dropped = Rc::new(Cell::new(false));
        let source = scene
            .into_prepared_source(
                CooperativeScene {
                    marker,
                    calls: calls.clone(),
                    dropped: dropped.clone(),
                    steps,
                    fail_at,
                    animated,
                },
                ReplayLimits {
                    revisions: 2048,
                    payloads: 32768,
                },
            )
            .map_err(js_error)?;
        Ok(Self {
            source,
            calls,
            dropped,
        })
    }
    #[wasm_bindgen(js_name = createCanvas)]
    pub async fn create_canvas(
        &mut self,
        canvas: OffscreenCanvas,
    ) -> Result<WasmExecutionCanvasRenderer, JsValue> {
        self.source.prepare_next().map_err(js_error)?;
        let mut reader = self
            .source
            .take_reader()
            .ok_or_else(|| js_error("fixture reader already moved"))?;
        reader.seek(0.5).map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_prepared_prefix(canvas, reader).await
    }
    #[wasm_bindgen(js_name = prepareNext)]
    pub fn prepare_next(
        &mut self,
        canvas: &mut WasmExecutionCanvasRenderer,
        lookahead_seconds: f64,
    ) -> Result<String, JsValue> {
        let status = canvas.prepare_source_step(&mut self.source, lookahead_seconds)?;
        Ok(match status {
            None => "waiting",
            Some(PreparedSourceStatus::Ready) => "continue",
            Some(_) => "finished",
        }
        .into())
    }
    pub fn cancel(&mut self) {
        self.source.cancel();
    }
    pub fn calls(&self) -> u32 {
        self.calls.get()
    }
    #[wasm_bindgen(js_name = producerDropped)]
    pub fn producer_dropped(&self) -> bool {
        self.dropped.get()
    }
}

/// Qualification-only bridge for a real Python authoring context. The interpreter
/// retains its source; only the existing typed derived reader moves to the canvas.
/// JavaScript never receives or retransmits a history/scene representation.
#[wasm_bindgen]
pub struct InterpreterCanvasFixture {
    publisher: PrefixPublisher,
    reader: Option<PrefixReader>,
}

#[wasm_bindgen]
impl InterpreterCanvasFixture {
    #[wasm_bindgen(constructor)]
    pub fn new(context: &mut crate::CanonicalAuthoringSceneContext) -> Result<Self, JsValue> {
        let (publisher, mut reader) = context.inner.create_prepared_prefix().map_err(js_error)?;
        reader.seek(0.5).map_err(js_error)?;
        Ok(Self {
            publisher,
            reader: Some(reader),
        })
    }

    #[wasm_bindgen(js_name = createCanvas)]
    pub async fn create_canvas(
        &mut self,
        canvas: OffscreenCanvas,
    ) -> Result<WasmExecutionCanvasRenderer, JsValue> {
        let reader = self
            .reader
            .take()
            .ok_or_else(|| js_error("fixture reader already moved"))?;
        WasmExecutionCanvasRenderer::create_from_prepared_prefix(canvas, reader).await
    }

    /// Qualification projects only a Rust demand decision, never scene time or
    /// history data for JavaScript to reinterpret as scheduling authority.
    #[wasm_bindgen(js_name = needsPreparation)]
    pub fn needs_preparation(
        &self,
        canvas: &WasmExecutionCanvasRenderer,
        lookahead_seconds: f64,
    ) -> Result<bool, JsValue> {
        canvas.prepared_source_needs_work(lookahead_seconds)
    }

    /// Test delivery uses the same caller-owned typed batch as a future host.
    /// The fixture intentionally targets only its own correctly paired reader.
    #[wasm_bindgen(js_name = publishCompleted)]
    pub fn publish_completed(
        &mut self,
        context: &mut crate::CanonicalAuthoringSceneContext,
        canvas: &mut WasmExecutionCanvasRenderer,
        terminal: bool,
    ) -> Result<(), JsValue> {
        let batch = if terminal {
            context.inner.seal_prepared_source().map_err(js_error)?;
            context
                .inner
                .finish_prepared_prefix(&mut self.publisher)
                .map_err(js_error)?
        } else {
            context
                .inner
                .publish_prepared_prefix(&mut self.publisher)
                .map_err(js_error)?
        };
        canvas.reserve_prepared_prefix_capacity(batch.required_object_capacity())?;
        canvas.preflight_prepared_prefix(&batch)?;
        canvas.append_prepared_prefix(batch).map(|_| ())
    }
}
