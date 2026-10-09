//! Value-only projections of existing shared center observations.
//!
//! Direct Rust calls below return ordinary Rust values. Only the coordinate
//! vector crosses the language boundary: no WASM layout wrapper is exported,
//! and no persistent semantic state or identity is created here.
use wasm_bindgen::prelude::*;

use crate::{CanonicalAuthoringSceneContext, WasmAuthoringMobjectHandle};

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = queryMobjectCenter)]
    pub fn query_mobject_center(
        &mut self,
        handle: &WasmAuthoringMobjectHandle,
    ) -> Result<Vec<f64>, JsValue> {
        // Keep the existing authored/effective selection and all ownership,
        // provenance, freshness and layout semantics in the shared query.
        let layout = self.query_mobject_layout(handle)?;
        Ok(vec![layout.center_x(), layout.center_y()])
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = centerCoordinates)]
    pub fn center_coordinates(&self) -> Result<Vec<f64>, JsValue> {
        let (x, y) = self
            .semantic_mobject()
            .center()
            .map_err(crate::authoring_error::js_error)?;
        Ok(vec![x, y])
    }
}
