use crate::WasmExecutionCanvasRenderer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;
fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Same typed scene and input contract as the native example, with no worker codec.
#[wasm_bindgen(js_name = createDirectPointerSelectionRenderer)]
pub async fn create_direct_pointer_selection_renderer(
    canvas: OffscreenCanvas,
    live_program: bool,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    if live_program {
        struct Finished;
        impl noon::LiveContinuation for Finished {
            type Error = std::convert::Infallible;
            fn resume(
                &mut self,
                _: &mut noon::LiveSession<'_>,
            ) -> Result<noon::ContinuationStep, Self::Error> {
                Ok(noon::ContinuationStep::Finished)
            }
        }
        let program = noon::example_scenes::pointer_selection::scene()
            .map_err(js_error)?
            .into_live_program(Finished)
            .map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
    } else {
        let session = noon::example_scenes::pointer_selection::session().map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
    }
}

/// Qualification-only live animation; not click-action dispatch. Inspection must
/// remain independent while the normal shared Indicate segment owns the shape.
#[wasm_bindgen(js_name = createDirectInspectionIndicateRenderer)]
pub async fn create_direct_inspection_indicate_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    struct Once {
        circle: noon::Mobject,
        started: bool,
    }
    impl noon::LiveContinuation for Once {
        type Error = String;
        fn resume(
            &mut self,
            live: &mut noon::LiveSession<'_>,
        ) -> Result<noon::ContinuationStep, String> {
            if self.started {
                return Ok(noon::ContinuationStep::Finished);
            }
            let segment = live
                .declare_and_activate_indicate(
                    &self.circle,
                    noon::IndicateOptions::default(),
                    noon::AnimationOptions::new().run_time(4.0),
                )
                .map_err(|error| error.to_string())?;
            self.started = true;
            Ok(noon::ContinuationStep::Await(segment))
        }
    }
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(0.8).map_err(js_error)?;
    circle.set_fill(0.0, 0.0, 1.0, 1.0).map_err(js_error)?;
    circle.set_stroke_width(0.0).map_err(js_error)?;
    scene.add(&circle).map_err(js_error)?;
    let program = scene
        .into_live_program(Once {
            circle,
            started: false,
        })
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}
