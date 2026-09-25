//! SampleSpace bindings use the same private canonical context as other live edits.
use super::CanonicalAuthoringSceneContext;
use crate::authoring_sample_space::{
    colors_from_rgba, WasmSampleSpaceHandle, WasmSampleSpaceOptions,
};
use crate::WasmAuthoringFamilyHandle;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateSampleSpace)]
    pub fn live_create_sample_space(
        &mut self,
        options: WasmSampleSpaceOptions,
    ) -> Result<WasmSampleSpaceHandle, JsValue> {
        self.inner
            .live_create_sample_space(&options.options)
            .map(WasmSampleSpaceHandle::from_sample_space)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveGetHorizontalDivision)]
    pub fn live_get_horizontal_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_get_sample_space_division(
                &sample_space.sample_space,
                probabilities,
                &colors,
                false,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveGetVerticalDivision)]
    pub fn live_get_vertical_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_get_sample_space_division(
                &sample_space.sample_space,
                probabilities,
                &colors,
                true,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveDivideHorizontally)]
    pub fn live_divide_horizontally(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_divide_sample_space(
                &mut sample_space.sample_space,
                probabilities,
                &colors,
                false,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveDivideVertically)]
    pub fn live_divide_vertically(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_divide_sample_space(&mut sample_space.sample_space, probabilities, &colors, true)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }
}
