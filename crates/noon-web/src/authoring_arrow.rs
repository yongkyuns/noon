#![cfg(target_arch = "wasm32")]

use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;

/// Inert typed Arrow-family constructor intent. No semantic identity exists
/// until `WasmAuthoringStore::create_manim_arrow` consumes the whole request.
#[wasm_bindgen]
pub struct WasmManimArrowOptions {
    options: noon::ManimArrowOptions,
}

#[wasm_bindgen]
impl WasmManimArrowOptions {
    pub fn arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::arrow(start_x, start_y, end_x, end_y)
            .map(|options| Self { options })
            .map_err(js_error)
    }

    pub fn vector(direction_x: f64, direction_y: f64) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::vector(direction_x, direction_y)
            .map(|options| Self { options })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = doubleArrow)]
    pub fn double_arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::double_arrow(start_x, start_y, end_x, end_y)
            .map(|options| Self { options })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setBuff)]
    pub fn set_buff(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_buff(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setTipLength)]
    pub fn set_tip_length(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_tip_length(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setMaxTipLengthToLengthRatio)]
    pub fn set_max_tip_length_to_length_ratio(&mut self, value: f64) -> Result<(), JsValue> {
        self.options
            .set_max_tip_length_to_length_ratio(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setMaxStrokeWidthToLengthRatio)]
    pub fn set_max_stroke_width_to_length_ratio(&mut self, value: f64) -> Result<(), JsValue> {
        self.options
            .set_max_stroke_width_to_length_ratio(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setZIndex)]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_z_index(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setTranslation)]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.options.set_translation(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setScale)]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.options.set_scale(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setRotation)]
    pub fn set_rotation(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_rotation(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_color(red, green, blue, alpha)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_stroke_width(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidthMode)]
    pub fn set_stroke_width_mode(&mut self, value: &str) -> Result<(), JsValue> {
        self.options.set_stroke_width_mode(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeJoin)]
    pub fn set_stroke_join(&mut self, value: &str) -> Result<(), JsValue> {
        self.options.set_stroke_join(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeCap)]
    pub fn set_stroke_cap(&mut self, value: &str) -> Result<(), JsValue> {
        self.options.set_stroke_cap(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setObjectOpacity)]
    pub fn set_object_opacity(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_object_opacity(value).map_err(js_error)
    }
}

/// Opaque wrapper around one atomically-published shared Arrow family.
#[wasm_bindgen]
pub struct WasmAuthoringArrowHandle {
    arrow: noon::ManimArrow,
}

#[wasm_bindgen]
impl WasmAuthoringArrowHandle {
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.arrow.family().clone())
    }

    pub fn shaft(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.arrow.shaft().clone())
    }

    #[wasm_bindgen(js_name = endTip)]
    pub fn end_tip(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.arrow.end_tip().clone())
    }

    #[wasm_bindgen(getter, js_name = hasStartTip)]
    pub fn has_start_tip(&self) -> bool {
        self.arrow.start_tip().is_some()
    }

    #[wasm_bindgen(js_name = startTip)]
    pub fn start_tip(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.arrow
            .start_tip()
            .cloned()
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .ok_or_else(|| JsValue::from_str("Arrow has no start tip"))
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    /// Consume the full inert Arrow request and publish shaft, tip resources,
    /// component identities and family membership in one Rust transaction.
    #[wasm_bindgen(js_name = createManimArrow)]
    pub fn create_manim_arrow(
        &self,
        candidate: WasmManimArrowOptions,
    ) -> Result<WasmAuthoringArrowHandle, JsValue> {
        noon::ManimArrow::create(Rc::clone(&self.semantics), candidate.options)
            .map(|arrow| WasmAuthoringArrowHandle { arrow })
            .map_err(js_error)
    }
}
