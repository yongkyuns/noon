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
