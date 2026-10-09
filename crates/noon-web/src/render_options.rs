//! Browser/WASM value binding to the same output resolver as native Rust/Python.
use noon::integration::{RenderOptionInputs, ResolvedRenderOptions};
use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;

#[wasm_bindgen]
pub struct WasmRenderOptionsResolution(ResolvedRenderOptions);

#[wasm_bindgen]
impl WasmRenderOptionsResolution {
    #[wasm_bindgen(getter, js_name = pixelWidth)]
    pub fn width(&self) -> u32 {
        self.0.pixel_width
    }
    #[wasm_bindgen(getter, js_name = pixelHeight)]
    pub fn height(&self) -> u32 {
        self.0.pixel_height
    }
    #[wasm_bindgen(getter, js_name = frameRateNumerator)]
    pub fn numerator(&self) -> u32 {
        self.0.frame_rate.numerator()
    }
    #[wasm_bindgen(getter, js_name = frameRateDenominator)]
    pub fn denominator(&self) -> u32 {
        self.0.frame_rate.denominator()
    }
    #[wasm_bindgen(getter, js_name = format)]
    pub fn format(&self) -> String {
        self.0.format.name().to_owned()
    }
}

#[wasm_bindgen(js_name = resolveRenderOptions)]
pub fn resolve_render_options(
    quality: Option<String>,
    resolution: Option<String>,
    fps: Option<String>,
    format: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<WasmRenderOptionsResolution, JsValue> {
    RenderOptionInputs {
        quality: quality.as_deref(),
        resolution: resolution.as_deref(),
        frame_rate: fps.as_deref(),
        format: format.as_deref(),
        pixel_width: width,
        pixel_height: height,
    }
    .resolve()
    .map(WasmRenderOptionsResolution)
    .map_err(js_error)
}
