//! Thin WASM projection for shared Rust Brace geometry authoring.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;
use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmManimGeometryOptions};

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
