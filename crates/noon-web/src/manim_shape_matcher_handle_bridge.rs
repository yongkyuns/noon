//! Shared bounds become inert geometry inputs, consumed by the ordinary or live owner.
#![cfg(target_arch = "wasm32")]

use noon::ManimGeometryOptions;
use noon_core::Bounds2D64;
use wasm_bindgen::prelude::*;

use crate::{
    WasmAuthoringFamilyLayout, WasmAuthoringMobjectHandle, WasmManimGeometryOptions,
    WasmMobjectLayoutObservation,
};

use crate::authoring_error::js_error;

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

fn cross_line(
    bounds: Bounds2D64,
    index: u32,
    scale_factor: f64,
) -> Result<WasmManimGeometryOptions, JsValue> {
    let [first, second] =
        ManimGeometryOptions::cross_lines(Some(bounds), scale_factor).map_err(js_error)?;
    match index {
        0 => Ok(WasmManimGeometryOptions::from_options(first)),
        1 => Ok(WasmManimGeometryOptions::from_options(second)),
        _ => Err(js_error("Cross line index must be 0 or 1")),
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = beginUnderline)]
    pub fn begin_underline(&self, buff: f64) -> Result<WasmManimGeometryOptions, JsValue> {
        let bounds = self
            .semantic_mobject()
            .layout_bounds()
            .map_err(js_error)?
            .ok_or_else(|| js_error("Underline target has no layout bounds"))?;
        ManimGeometryOptions::underline(bounds, buff)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = beginCrossLine)]
    pub fn begin_cross_line(
        &self,
        index: u32,
        scale_factor: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        cross_line(mobject_bounds(self)?, index, scale_factor)
    }

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
    #[wasm_bindgen(js_name = beginCrossLine)]
    pub fn begin_cross_line(
        &self,
        index: u32,
        scale_factor: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        cross_line(self.bounds(), index, scale_factor)
    }

    #[wasm_bindgen(js_name = beginSurroundingRectangle)]
    pub fn begin_surrounding_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::surrounding_rectangle(self.bounds(), buff_x, buff_y, corner_radius)
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
            self.bounds(),
            buff_x,
            buff_y,
            corner_radius,
            fill_opacity,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }
}

fn observed_bounds(observation: &WasmMobjectLayoutObservation) -> Bounds2D64 {
    Bounds2D64 {
        min_x: observation.critical_x(-1.0, 0.0),
        min_y: observation.critical_y(0.0, -1.0),
        max_x: observation.critical_x(1.0, 0.0),
        max_y: observation.critical_y(0.0, 1.0),
    }
}

// This immutable query result already came from the current Rust owner. Reuse
// that one observation for every candidate, including both Cross children.
#[wasm_bindgen]
impl WasmMobjectLayoutObservation {
    #[wasm_bindgen(js_name = beginCrossLine)]
    pub fn begin_cross_line(
        &self,
        index: u32,
        scale_factor: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        cross_line(observed_bounds(self), index, scale_factor)
    }

    #[wasm_bindgen(js_name = beginSurroundingRectangle)]
    pub fn begin_surrounding_rectangle(
        &self,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        ManimGeometryOptions::surrounding_rectangle(
            observed_bounds(self),
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
            observed_bounds(self),
            buff_x,
            buff_y,
            corner_radius,
            fill_opacity,
        )
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
    }
}
