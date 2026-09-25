//! Thin WASM projection for shared Rust Brace geometry authoring.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;
use crate::{
    CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle,
    WasmLayoutAnchor, WasmManimGeometryOptions,
};

#[wasm_bindgen]
pub struct WasmBraceLabelHandle {
    inner: noon::BraceLabel,
}

#[wasm_bindgen]
impl WasmBraceLabelHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.inner.family().clone())
    }

    #[wasm_bindgen(js_name = brace)]
    pub fn brace(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.inner.brace().object().clone())
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    /// Create the retained Brace/label relationship through the active live owner.
    #[wasm_bindgen(js_name = liveCreateBraceLabel)]
    pub fn live_create_brace_label(
        &mut self,
        target: &WasmLayoutAnchor,
        label: &WasmLayoutAnchor,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
        label_buff: f64,
    ) -> Result<WasmBraceLabelHandle, JsValue> {
        self.create_live_brace_label(
            &target.anchor,
            label.anchor.clone(),
            noon::BraceOptions {
                direction: (direction_x, direction_y),
                buff,
                sharpness,
                label_buff,
            },
        )
        .map(|inner| WasmBraceLabelHandle { inner })
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveShiftBraceLabel)]
    pub fn live_shift_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        target: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        self.shift_live_brace_label(&mut brace.inner, &target.anchor)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveReplaceBraceLabel)]
    pub fn live_replace_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        label: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        self.replace_live_brace_label(&mut brace.inner, label.anchor.clone())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveChangeBraceLabel)]
    pub fn live_change_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        target: &WasmLayoutAnchor,
        label: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        self.change_live_brace_label(&mut brace.inner, &target.anchor, label.anchor.clone())
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    /// Observe this object through shared Rust and return inert Brace geometry.
    #[wasm_bindgen(js_name = beginBrace)]
    pub fn begin_brace(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        let object = self.semantic_mobject();
        let target = noon::LayoutAnchor::from(object);
        noon::ManimGeometryOptions::brace(&target, (direction_x, direction_y), buff, sharpness)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = asBraceLabel)]
    pub fn as_brace_label(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
        label_buff: f64,
    ) -> Result<WasmBraceLabelHandle, JsValue> {
        noon::BraceLabel::from_family(
            self.semantic_family()?,
            noon::BraceOptions {
                direction: (direction_x, direction_y),
                buff,
                sharpness,
                label_buff,
            },
        )
        .map(|inner| WasmBraceLabelHandle { inner })
        .map_err(js_error)
    }

    /// Observe this semantic family through shared Rust and return inert Brace geometry.
    #[wasm_bindgen(js_name = beginBrace)]
    pub fn begin_brace(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        let family = self.semantic_family()?;
        let target = noon::LayoutAnchor::from(&family);
        noon::ManimGeometryOptions::brace(&target, (direction_x, direction_y), buff, sharpness)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    /// Pure shared-Rust BraceBetweenPoints constructor; no temporary Line identity.
    #[wasm_bindgen(js_name = braceBetweenPoints)]
    pub fn brace_between_points(
        point_1_x: f64,
        point_1_y: f64,
        point_2_x: f64,
        point_2_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::brace_between_points(
            (point_1_x, point_1_y),
            (point_2_x, point_2_y),
            (direction_x, direction_y),
            buff,
            sharpness,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }
}

#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createBraceTextRenderer)]
pub async fn create_brace_text_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::brace_text::session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
