//! Typed shape matcher bindings over a Scene or detached authored handles.
#![cfg(target_arch = "wasm32")]

use noon::{ManimGeometryOptions, MobjectTarget};
use noon_core::Bounds2D64;
use wasm_bindgen::prelude::*;

use crate::{
    authoring_error::js_error, CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle,
    WasmAuthoringMobjectHandle, WasmManimGeometryOptions,
};

fn coordinates(bounds: Bounds2D64) -> Vec<f64> {
    vec![bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
}

fn parse_bounds(values: Vec<f64>) -> Result<Bounds2D64, JsValue> {
    let [min_x, min_y, max_x, max_y] = values
        .try_into()
        .map_err(|_| js_error("shape matcher bounds require four coordinates"))?;
    Ok(Bounds2D64 {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = beginUnderline)]
    pub fn begin_underline(&self, buff: f64) -> Result<WasmManimGeometryOptions, JsValue> {
        self.semantic_mobject()
            .underline_options(buff)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = shapeMatcherBounds)]
    pub fn shape_matcher_bounds(&self) -> Result<Vec<f64>, JsValue> {
        MobjectTarget::from(self.semantic_mobject())
            .shape_matcher_bounds()
            .map(coordinates)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = shapeMatcherBounds)]
    pub fn shape_matcher_bounds(&self) -> Result<Vec<f64>, JsValue> {
        MobjectTarget::from(&self.semantic_family()?)
            .shape_matcher_bounds()
            .map(coordinates)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = mobjectShapeMatcherBounds)]
    pub fn mobject_shape_matcher_bounds(
        &mut self,
        target: &WasmAuthoringMobjectHandle,
    ) -> Result<Vec<f64>, JsValue> {
        self.inner
            .local_authoring_scene()
            .map_err(js_error)?
            .shape_matcher_bounds(target.semantic_mobject().into())
            .map(coordinates)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = familyShapeMatcherBounds)]
    pub fn family_shape_matcher_bounds(
        &mut self,
        target: &WasmAuthoringFamilyHandle,
    ) -> Result<Vec<f64>, JsValue> {
        self.inner
            .local_authoring_scene()
            .map_err(js_error)?
            .shape_matcher_bounds((&target.semantic_family()?).into())
            .map(coordinates)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    #[wasm_bindgen(js_name = surroundingRectangle)]
    pub fn surrounding_rectangle(
        bounds: Vec<f64>,
        buff_x: f64,
        buff_y: f64,
        radius: f64,
    ) -> Result<Self, JsValue> {
        ManimGeometryOptions::surrounding_rectangle(parse_bounds(bounds)?, buff_x, buff_y, radius)
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = backgroundRectangle)]
    pub fn background_rectangle(
        bounds: Vec<f64>,
        buff_x: f64,
        buff_y: f64,
        radius: f64,
        opacity: f64,
    ) -> Result<Self, JsValue> {
        ManimGeometryOptions::background_rectangle(
            parse_bounds(bounds)?,
            buff_x,
            buff_y,
            radius,
            opacity,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = crossLines)]
    pub fn cross_lines(bounds: Option<Vec<f64>>, scale: f64) -> Result<js_sys::Array, JsValue> {
        let lines = ManimGeometryOptions::cross_lines(bounds.map(parse_bounds).transpose()?, scale)
            .map_err(js_error)?;
        Ok(lines
            .into_iter()
            .map(|line| JsValue::from(Self::from_options(line)))
            .collect())
    }
}
