#![cfg(target_arch = "wasm32")]

use crate::WasmManimArrowOptions;
use wasm_bindgen::prelude::*;

fn defaults() -> noon::VectorFieldRanges2D {
    noon::manim_default_vector_field_ranges_2d()
}

#[wasm_bindgen]
impl WasmManimArrowOptions {
    /// Start one static field draft while resolving omitted axes from the shared
    /// Manim-compatible default-frame range contract in Rust.
    #[wasm_bindgen(js_name = vectorFieldWithDefaultRanges)]
    #[allow(clippy::too_many_arguments)]
    pub fn vector_field_with_default_ranges(
        default_x: bool,
        x_start: f64,
        x_end: f64,
        x_step: f64,
        default_y: bool,
        y_start: f64,
        y_end: f64,
        y_step: f64,
        custom_length: bool,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        let default_ranges = defaults();
        let x = if default_x {
            default_ranges.x
        } else {
            noon::VectorFieldAxisRange::new(x_start, x_end, x_step)
        };
        let y = if default_y {
            default_ranges.y
        } else {
            noon::VectorFieldAxisRange::new(y_start, y_end, y_step)
        };
        WasmManimArrowOptions::vector_field(
            x.start,
            x.end,
            x.step,
            y.start,
            y.end,
            y.step,
            custom_length,
        )
    }

    #[wasm_bindgen(js_name = defaultVectorFieldXStart)]
    pub fn default_vector_field_x_start() -> f64 {
        defaults().x.start
    }

    #[wasm_bindgen(js_name = defaultVectorFieldXEnd)]
    pub fn default_vector_field_x_end() -> f64 {
        defaults().x.end
    }

    #[wasm_bindgen(js_name = defaultVectorFieldXStep)]
    pub fn default_vector_field_x_step() -> f64 {
        defaults().x.step
    }

    #[wasm_bindgen(js_name = defaultVectorFieldYStart)]
    pub fn default_vector_field_y_start() -> f64 {
        defaults().y.start
    }

    #[wasm_bindgen(js_name = defaultVectorFieldYEnd)]
    pub fn default_vector_field_y_end() -> f64 {
        defaults().y.end
    }

    #[wasm_bindgen(js_name = defaultVectorFieldYStep)]
    pub fn default_vector_field_y_step() -> f64 {
        defaults().y.step
    }
}
