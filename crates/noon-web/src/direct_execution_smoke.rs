use std::{cell::RefCell, rc::Rc};

use noon::{ExecutionSession, Mobject, Text};
use noon_core::{
    AnimationOptions, Camera2DState, NativeEventSource, NativeInputValue, NativeStateSource,
    RateFunction, SemanticObjectProperty, SemanticObjectRole, SemanticObjectState, SemanticStore,
    SemanticVec3, StoredGeometry, Vec2,
};
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

use crate::WasmExecutionCanvasRenderer;

/// Browser proof that typed Rust semantic animation, camera, and canonical text
/// reach the direct mixed renderer without a scene document or execution mirror.
#[wasm_bindgen(js_name = createDirectExecutionSmokeRenderer)]
pub async fn create_direct_execution_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let store = Rc::new(RefCell::new(SemanticStore::new()));
    let animation = {
        let mut store = store.borrow_mut();
        let opacity = store
            .insert_semantic_input_signal(0.65_f64)
            .map_err(js_error)?;
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.5,
            }));
        store.attach_to_scene(object).map_err(js_error)?;
        store
            .bind_semantic_signal(opacity, object, SemanticObjectProperty::ObjectOpacity)
            .map_err(js_error)?;

        let mut target_state = store
            .semantic_object_state_checked(object)
            .map_err(js_error)?
            .clone();
        target_state.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .map_err(js_error)?;

        let mut camera_state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(12.0, 6.0),
        });
        camera_state.transform.translation = SemanticVec3::new(2.0, -1.0, 0.0);
        camera_state.set_role(SemanticObjectRole::Camera2D);
        let camera = store.insert_semantic_object(camera_state);
        store.attach_to_scene(camera).map_err(js_error)?;
        animation
    };

    let mut label = Mobject::from_text(Rc::clone(&store), Text::new("Noon").with_font_size(48.0))
        .map_err(js_error)?;
    label.shift(1.0, 0.0).map_err(js_error)?;
    store
        .borrow_mut()
        .attach_to_scene(label.node_id())
        .map_err(js_error)?;

    let mut session = ExecutionSession::from_semantic_store(&store.borrow()).map_err(js_error)?;
    let resolved_camera = session.camera().map_err(js_error)?;
    if resolved_camera
        != (Camera2DState {
            center: Vec2::new(2.0, -1.0),
            height: 6.0,
        })
    {
        return Err(JsValue::from_str(
            "direct Rust/WASM smoke did not resolve its authored semantic camera",
        ));
    }
    session
        .activate_animation(
            &store.borrow(),
            animation,
            AnimationOptions::new()
                .run_time(0.1)
                .rate_func(RateFunction::Linear),
        )
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that the same target-neutral Rust callback scene used by the
/// native example executes and renders entirely inside one WASM context.
#[wasm_bindgen(js_name = createDirectAffineCallbackSmokeRenderer)]
pub async fn create_direct_affine_callback_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (session, callbacks) = noon::example_scenes::live_affine_callbacks().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session_with_callbacks(
        canvas, session, callbacks,
    )
    .await
}

/// Browser proof that shared callback paint operations use the same typed Rust
/// scene and effective-style batch as the paired native example.
#[wasm_bindgen(js_name = createDirectCallbackPaintSmokeRenderer)]
pub async fn create_direct_callback_paint_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (session, callbacks) = noon::example_scenes::live_callback_paint().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session_with_callbacks(
        canvas, session, callbacks,
    )
    .await
}

/// The native Line callback example, executed in one typed Rust/WASM context.
#[wasm_bindgen(js_name = createDirectLineMatchSmokeRenderer)]
pub async fn create_direct_line_match_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (session, callbacks) =
        noon::example_scenes::live_line_match_callback().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session_with_callbacks(
        canvas, session, callbacks,
    )
    .await
}

/// Browser proof that the target-neutral Rust completion example preserves its
/// authored endpoint and renders through the direct single-context WASM path.
#[wasm_bindgen(js_name = createDirectAffineCompletionSmokeRenderer)]
pub async fn create_direct_affine_completion_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::live_affine_completion().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that sequential ordinary affine plays, waits, and authored
/// edits use the same target-neutral Rust session as the native example.
#[wasm_bindgen(js_name = createDirectOrdinaryAffinePlaySmokeRenderer)]
pub async fn create_direct_ordinary_affine_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::ordinary_affine_play().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that a direct Rust continuation keeps one scene/session across
/// visible play, wait, authored edit, late target, and exact renderer admission.
#[wasm_bindgen(js_name = createDirectOrdinaryAffineContinuationSmokeRenderer)]
pub async fn create_direct_ordinary_affine_continuation_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::example_scenes::ordinary_affine_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Browser proof that scalar track activation, persistent set, wait, and a
/// second track execute through one direct Rust/WASM continuation program.
#[wasm_bindgen(js_name = createDirectOrdinaryValueTrackerContinuationSmokeRenderer)]
pub async fn create_direct_ordinary_value_tracker_continuation_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program =
        noon::example_scenes::ordinary_value_tracker_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Browser proof that FadeIn/FadeOut, detached wait, and same-handle re-entry
/// execute through the same target-neutral Rust continuation as the native example.
#[wasm_bindgen(js_name = createDirectOrdinaryFadePlaySmokeRenderer)]
pub async fn create_direct_ordinary_fade_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::example_scenes::ordinary_fade_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Browser proof that ordinary Create uses the same target-neutral Rust continuation.
#[wasm_bindgen(js_name = createDirectOrdinaryCreatePlaySmokeRenderer)]
pub async fn create_direct_ordinary_create_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::example_scenes::ordinary_create_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Browser proof that the paired ordinary continuation and its ordered Rust
/// callback table execute through the normal direct single-context WASM host.
#[wasm_bindgen(js_name = createDirectOrdinaryAffineCallbackContinuationSmokeRenderer)]
pub async fn create_direct_ordinary_affine_callback_continuation_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (program, callbacks) =
        noon::example_scenes::ordinary_callback_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program_with_callbacks(canvas, program, callbacks)
        .await
}

/// Browser proof that a root-scoped unbound tracker and inactive object are
/// read through one revision-pinned callback phase in direct Rust/WASM.
#[wasm_bindgen(js_name = createDirectOrdinaryCallbackSparseReadsSmokeRenderer)]
pub async fn create_direct_ordinary_callback_sparse_reads_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (program, callbacks) =
        noon::example_scenes::ordinary_callback_sparse_reads_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program_with_callbacks(canvas, program, callbacks)
        .await
}

/// Browser proof that flat Parallel/Sequence composition uses the same typed
/// target-neutral session as the native Rust example.
#[wasm_bindgen(js_name = createDirectOrdinaryCompositionPlaySmokeRenderer)]
pub async fn create_direct_ordinary_composition_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::ordinary_composition_play().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that flat composition segments return to the same direct Rust
/// continuation program between their exact renderer publication barriers.
#[wasm_bindgen(js_name = createDirectOrdinaryCompositionContinuationSmokeRenderer)]
pub async fn create_direct_ordinary_composition_continuation_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program =
        noon::example_scenes::ordinary_composition_continuation_program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Browser proof that ordinary style animation and its following authored edit
/// execute through the same target-neutral Rust session as the native example.
#[wasm_bindgen(js_name = createDirectOrdinaryStylePlaySmokeRenderer)]
pub async fn create_direct_ordinary_style_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::ordinary_style_play().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that ordinary fill/stroke color and opacity animate through
/// the same target-neutral Rust session as the native example.
#[wasm_bindgen(js_name = createDirectOrdinaryPaintPlaySmokeRenderer)]
pub async fn create_direct_ordinary_paint_play_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::ordinary_paint_play().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof around the same canonical native-signal scene used by Rust hosts.
///
/// JavaScript supplies only normalized platform occurrences. Source routing,
/// semantic identity, reactive evaluation, runtime publication, and rendering all
/// remain in this one Rust/WASM execution context.
#[wasm_bindgen(js_name = createDirectNativeSignalsSmokeRenderer)]
pub async fn create_direct_native_signals_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<DirectNativeSignalsSmokeRenderer, JsValue> {
    let session = noon::example_scenes::live_native_signals().map_err(js_error)?;
    let renderer =
        WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await?;
    Ok(DirectNativeSignalsSmokeRenderer { renderer })
}

#[wasm_bindgen(js_name = DirectNativeSignalsSmokeRenderer)]
pub struct DirectNativeSignalsSmokeRenderer {
    renderer: WasmExecutionCanvasRenderer,
}

#[wasm_bindgen(js_class = DirectNativeSignalsSmokeRenderer)]
impl DirectNativeSignalsSmokeRenderer {
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
        self.renderer.resize(width, height)
    }

    pub fn render(&mut self) -> Result<bool, JsValue> {
        self.renderer.render()
    }

    #[wasm_bindgen(js_name = rendererBackend)]
    pub fn renderer_backend(&self) -> String {
        self.renderer.renderer_backend()
    }

    #[wasm_bindgen(js_name = objectCount)]
    pub fn object_count(&self) -> usize {
        self.renderer.object_count()
    }

    #[wasm_bindgen(js_name = lastDrawCalls)]
    pub fn last_draw_calls(&self) -> usize {
        self.renderer.last_draw_calls()
    }

    #[wasm_bindgen(js_name = setPointerPosition)]
    pub fn set_pointer_position(&mut self, x: f32, y: f32) -> Result<bool, JsValue> {
        self.renderer.set_native_state_input(
            NativeStateSource::PointerPosition,
            NativeInputValue::Vec2(Vec2::new(x, y)),
        )
    }

    #[wasm_bindgen(js_name = setSpaceKey)]
    pub fn set_space_key(&mut self, down: bool) -> Result<bool, JsValue> {
        self.renderer.set_native_state_input(
            NativeStateSource::Key {
                code: "Space".to_owned(),
            },
            NativeInputValue::Bool(down),
        )
    }

    #[wasm_bindgen(js_name = setOpacityControl)]
    pub fn set_opacity_control(&mut self, opacity: f32) -> Result<bool, JsValue> {
        self.renderer.set_native_state_input(
            NativeStateSource::Control {
                name: "opacity".to_owned(),
            },
            NativeInputValue::Scalar(opacity),
        )
    }

    #[wasm_bindgen(js_name = emitPrimaryPointerDown)]
    pub fn emit_primary_pointer_down(&mut self) -> Result<bool, JsValue> {
        self.renderer
            .emit_native_event(NativeEventSource::PointerDown { button: 0 })
    }
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
