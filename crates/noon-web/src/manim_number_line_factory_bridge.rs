#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use crate::{WasmAxesAuthoringPlan, WasmNumberLineAuthoringPlan};

    /// Extend the already-exported Axes plan with a constructor for the sibling
    /// NumberLine plan. This keeps the worker bootstrap stable while the plans
    /// remain independent Rust semantic owners after construction.
    #[wasm_bindgen]
    impl WasmAxesAuthoringPlan {
        #[wasm_bindgen(js_name = createNumberLinePlan)]
        pub fn create_number_line_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmNumberLineAuthoringPlan, JsValue> {
            WasmNumberLineAuthoringPlan::new(request_json)
        }
    }
}
