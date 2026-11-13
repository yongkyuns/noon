//! Scene-owned pointwise family edits.

use super::CanonicalAuthoringSceneContext;
use crate::authoring_error::js_error;
use crate::WasmAuthoringFamilyHandle;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = applyMatrixFamily)]
    pub fn apply_matrix_family(
        &mut self,
        handle: &WasmAuthoringFamilyHandle,
        values: Vec<f64>,
        rows: u32,
        columns: u32,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), JsValue> {
        let family = handle.semantic_family()?;
        self.inner
            .place(|scene| {
                scene.apply_matrix_to_family(
                    &family,
                    &values,
                    rows as usize,
                    columns as usize,
                    about_x,
                    about_y,
                )
            })
            .map_err(js_error)
    }
}
