#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;
use crate::{CanonicalAuthoringSceneContext, WasmAuthoringMobjectHandle, WasmManimGeometryOptions};

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    /// Derive an inert TangentLine geometry candidate from this retained source.
    /// Sampling and length normalization stay in shared Rust; the frontend may
    /// apply ordinary constructor style before publishing the returned candidate.
    #[wasm_bindgen(js_name = tangentLineOptions)]
    pub fn tangent_line_options(
        &self,
        alpha: f64,
        length: f64,
        d_alpha: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.semantic_mobject()
            .manim_tangent_line_options(alpha, length, d_alpha)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = prepareTangentLine)]
    pub fn prepare_tangent_line(
        &self,
        source: &WasmAuthoringMobjectHandle,
        alpha: f64,
        length: f64,
        d_alpha: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.inner
            .coordinate_scene()
            .map_err(js_error)?
            .tangent_line_options(source.semantic_mobject(), alpha, length, d_alpha)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
}
