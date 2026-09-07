//! Shared bounds become inert geometry inputs, consumed by the ordinary or live owner.
#![cfg(target_arch = "wasm32")]

use noon::ManimGeometryOptions;
use noon_core::Bounds2D64;
use wasm_bindgen::prelude::*;

use crate::{WasmAuthoringFamilyLayout, WasmAuthoringMobjectHandle, WasmManimGeometryOptions};

fn js_error(error: String) -> JsValue {
    JsValue::from_str(&error)
}

fn mobject_bounds(handle: &WasmAuthoringMobjectHandle) -> Result<Bounds2D64, JsValue> {
    let object = handle.semantic_mobject();
    Ok(match object.layout_bounds().map_err(js_error)? {
        Some(bounds) => bounds,
        None => {
            let point = object.state().map_err(js_error)?.transform.translation;
            Bounds2D64::point(point.x, point.y)
        }
    })
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = beginSurroundingRectangle)]
    pub fn begin_surrounding_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::surrounding_rectangle(
            mobject_bounds(self)?,
            buff_x,
            buff_y,
            corner_radius,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = beginBackgroundRectangle)]
    pub fn begin_background_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
        fill_opacity: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::background_rectangle(
            mobject_bounds(self)?,
            buff_x,
            buff_y,
            corner_radius,
            fill_opacity,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyLayout {
    #[wasm_bindgen(js_name = beginSurroundingRectangle)]
    pub fn begin_surrounding_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::surrounding_rectangle(
            self.completed_bounds()?,
            buff_x,
            buff_y,
            corner_radius,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = beginBackgroundRectangle)]
    pub fn begin_background_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
        fill_opacity: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::background_rectangle(
            self.completed_bounds()?,
            buff_x,
            buff_y,
            corner_radius,
            fill_opacity,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }
}
