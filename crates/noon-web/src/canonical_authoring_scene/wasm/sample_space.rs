//! SampleSpace bindings use the same private canonical context as other Scene edits.
use super::CanonicalAuthoringSceneContext;
use crate::authoring_sample_space::{
    colors_from_rgba, WasmSampleSpaceHandle, WasmSampleSpaceOptions,
};
use crate::WasmAuthoringFamilyHandle;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = createSampleSpace)]
    pub fn create_sample_space(
        &mut self,
        options: WasmSampleSpaceOptions,
    ) -> Result<WasmSampleSpaceHandle, JsValue> {
        self.inner
            .create_sample_space(&options.options)
            .map(WasmSampleSpaceHandle::from_sample_space)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = getHorizontalDivision)]
    pub fn get_horizontal_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .get_sample_space_division(&sample_space.sample_space, probabilities, &colors, false)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = getVerticalDivision)]
    pub fn get_vertical_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .get_sample_space_division(&sample_space.sample_space, probabilities, &colors, true)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = divideHorizontally)]
    pub fn divide_horizontally(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .divide_sample_space(
                &mut sample_space.sample_space,
                probabilities,
                &colors,
                false,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = divideVertically)]
    pub fn divide_vertically(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .divide_sample_space(&mut sample_space.sample_space, probabilities, &colors, true)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }
}
