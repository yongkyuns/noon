#![cfg(target_arch = "wasm32")]

use crate::{WasmAuthoringMobjectHandle, WasmManimArrowOptions};
use noon::ManimLineEndpoints;
use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;

fn options_from_endpoints(
    endpoints: ManimLineEndpoints,
    double_arrow: bool,
) -> Result<WasmManimArrowOptions, JsValue> {
    if double_arrow {
        WasmManimArrowOptions::double_arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    } else {
        WasmManimArrowOptions::arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }
}

fn from_mobjects(
    start: &WasmAuthoringMobjectHandle,
    end: &WasmAuthoringMobjectHandle,
    double_arrow: bool,
) -> Result<WasmManimArrowOptions, JsValue> {
    noon::manim_arrow_endpoints_from_mobjects(start.semantic_mobject(), end.semantic_mobject())
        .map_err(js_error)
        .and_then(|endpoints| options_from_endpoints(endpoints, double_arrow))
}

fn from_mobject(
    start: &WasmAuthoringMobjectHandle,
    end_x: f64,
    end_y: f64,
    double_arrow: bool,
) -> Result<WasmManimArrowOptions, JsValue> {
    noon::manim_arrow_endpoints_from_mobject(start.semantic_mobject(), end_x, end_y)
        .map_err(js_error)
        .and_then(|endpoints| options_from_endpoints(endpoints, double_arrow))
}

fn to_mobject(
    start_x: f64,
    start_y: f64,
    end: &WasmAuthoringMobjectHandle,
    double_arrow: bool,
) -> Result<WasmManimArrowOptions, JsValue> {
    noon::manim_arrow_endpoints_to_mobject(start_x, start_y, end.semantic_mobject())
        .map_err(js_error)
        .and_then(|endpoints| options_from_endpoints(endpoints, double_arrow))
}

/// Resolve VMobject boundaries through the same typed Arrow-options capability the
/// Python worker already exposes. The worker therefore does not need a second set
/// of feature-specific global functions just to reach shared Rust semantics.
#[wasm_bindgen]
impl WasmManimArrowOptions {
    #[wasm_bindgen(js_name = arrowFromMobjects)]
    pub fn arrow_from_mobjects_options(
        start: &WasmAuthoringMobjectHandle,
        end: &WasmAuthoringMobjectHandle,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        from_mobjects(start, end, false)
    }

    #[wasm_bindgen(js_name = arrowFromMobject)]
    pub fn arrow_from_mobject_options(
        start: &WasmAuthoringMobjectHandle,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        from_mobject(start, end_x, end_y, false)
    }

    #[wasm_bindgen(js_name = arrowToMobject)]
    pub fn arrow_to_mobject_options(
        start_x: f64,
        start_y: f64,
        end: &WasmAuthoringMobjectHandle,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        to_mobject(start_x, start_y, end, false)
    }

    #[wasm_bindgen(js_name = doubleArrowFromMobjects)]
    pub fn double_arrow_from_mobjects_options(
        start: &WasmAuthoringMobjectHandle,
        end: &WasmAuthoringMobjectHandle,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        from_mobjects(start, end, true)
    }

    #[wasm_bindgen(js_name = doubleArrowFromMobject)]
    pub fn double_arrow_from_mobject_options(
        start: &WasmAuthoringMobjectHandle,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        from_mobject(start, end_x, end_y, true)
    }

    #[wasm_bindgen(js_name = doubleArrowToMobject)]
    pub fn double_arrow_to_mobject_options(
        start_x: f64,
        start_y: f64,
        end: &WasmAuthoringMobjectHandle,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        to_mobject(start_x, start_y, end, true)
    }
}

/// Direct exports remain a narrow embedding surface for hosts that import the
/// generated WASM bindings themselves. Python authoring uses the typed options
/// bridge above so all existing worker hosts receive the capability automatically.
#[wasm_bindgen(js_name = noonAuthoringArrowFromMobjects)]
pub fn arrow_from_mobjects(
    start: &WasmAuthoringMobjectHandle,
    end: &WasmAuthoringMobjectHandle,
) -> Result<WasmManimArrowOptions, JsValue> {
    from_mobjects(start, end, false)
}

#[wasm_bindgen(js_name = noonAuthoringArrowFromMobject)]
pub fn arrow_from_mobject(
    start: &WasmAuthoringMobjectHandle,
    end_x: f64,
    end_y: f64,
) -> Result<WasmManimArrowOptions, JsValue> {
    from_mobject(start, end_x, end_y, false)
}

#[wasm_bindgen(js_name = noonAuthoringArrowToMobject)]
pub fn arrow_to_mobject(
    start_x: f64,
    start_y: f64,
    end: &WasmAuthoringMobjectHandle,
) -> Result<WasmManimArrowOptions, JsValue> {
    to_mobject(start_x, start_y, end, false)
}

#[wasm_bindgen(js_name = noonAuthoringDoubleArrowFromMobjects)]
pub fn double_arrow_from_mobjects(
    start: &WasmAuthoringMobjectHandle,
    end: &WasmAuthoringMobjectHandle,
) -> Result<WasmManimArrowOptions, JsValue> {
    from_mobjects(start, end, true)
}

#[wasm_bindgen(js_name = noonAuthoringDoubleArrowFromMobject)]
pub fn double_arrow_from_mobject(
    start: &WasmAuthoringMobjectHandle,
    end_x: f64,
    end_y: f64,
) -> Result<WasmManimArrowOptions, JsValue> {
    from_mobject(start, end_x, end_y, true)
}

#[wasm_bindgen(js_name = noonAuthoringDoubleArrowToMobject)]
pub fn double_arrow_to_mobject(
    start_x: f64,
    start_y: f64,
    end: &WasmAuthoringMobjectHandle,
) -> Result<WasmManimArrowOptions, JsValue> {
    to_mobject(start_x, start_y, end, true)
}
