use std::{cell::RefCell, rc::Rc};

use noon::{ExecutionSession, Mobject, Text};
use noon_core::{
    AnimationOptions, Camera2DState, RateFunction, SemanticObjectProperty, SemanticObjectRole,
    SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Vec2,
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

/// Browser proof that the target-neutral Rust completion example preserves its
/// authored endpoint and renders through the direct single-context WASM path.
#[wasm_bindgen(js_name = createDirectAffineCompletionSmokeRenderer)]
pub async fn create_direct_affine_completion_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::live_affine_completion().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Browser proof that the target-neutral Rust scalar tracker example executes
/// and renders through the typed direct single-context WASM path.
#[wasm_bindgen(js_name = createDirectValueTrackerSmokeRenderer)]
pub async fn create_direct_value_tracker_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::live_value_tracker().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
