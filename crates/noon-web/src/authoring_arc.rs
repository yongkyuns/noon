//! Thin WASM exposure for shared Manim Arc geometry authoring.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::{authoring_error::js_error, authoring_geometry::WasmManimGeometryOptions};

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    /// Construct an Arc using the shared retained Rust geometry implementation.
    #[wasm_bindgen(js_name = arc)]
    pub fn arc(
        radius: f64,
        start_angle: f64,
        angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::arc(
            radius,
            start_angle,
            angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    /// Construct an ArcBetweenPoints without frontend-owned path mathematics.
    #[wasm_bindgen(js_name = arcBetweenPoints)]
    pub fn arc_between_points(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
        num_components: u32,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::arc_between_points(
            start_x,
            start_y,
            end_x,
            end_y,
            angle,
            radius,
            num_components,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }
}
