#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use crate::{
        WasmAuthoringStore, WasmAxesAuthoringPlan, WasmAxesPlotPlan, WasmAxesQueryPlan,
        WasmNumberLineAuthoringPlan, WasmNumberPlaneGridPlan, WasmParametricFunctionPlan,
        WasmPolarPlaneGridPlan,
    };

    /// Keep the browser worker coupled to one stable authoring-store import while
    /// feature-specific plans remain separate Rust/WASM modules.
    #[wasm_bindgen]
    impl WasmAuthoringStore {
        #[wasm_bindgen(js_name = createAxesAuthoringPlan)]
        pub fn create_axes_authoring_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmAxesAuthoringPlan, JsValue> {
            WasmAxesAuthoringPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createAxesQueryPlan)]
        pub fn create_axes_query_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmAxesQueryPlan, JsValue> {
            WasmAxesQueryPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createAxesPlotPlan)]
        pub fn create_axes_plot_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmAxesPlotPlan, JsValue> {
            WasmAxesPlotPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createParametricFunctionPlan)]
        pub fn create_parametric_function_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmParametricFunctionPlan, JsValue> {
            WasmParametricFunctionPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createNumberLineAuthoringPlan)]
        pub fn create_number_line_authoring_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmNumberLineAuthoringPlan, JsValue> {
            WasmNumberLineAuthoringPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createNumberPlaneGridPlan)]
        pub fn create_number_plane_grid_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmNumberPlaneGridPlan, JsValue> {
            WasmNumberPlaneGridPlan::new(request_json)
        }

        #[wasm_bindgen(js_name = createPolarPlaneGridPlan)]
        pub fn create_polar_plane_grid_plan(
            &self,
            request_json: &str,
        ) -> Result<WasmPolarPlaneGridPlan, JsValue> {
            WasmPolarPlaneGridPlan::new(request_json)
        }
    }
}
