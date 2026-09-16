//! Thin numeric Text label-family construction over shared Rust operations.
use noon::plot_presentation::NumberLabelOptions;
use noon::{ManimAxes, ManimNumberLine, Mobject};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmCoordinateOptions};

fn failure(error: impl std::fmt::Display) -> JsValue {
    js_error(AuthoringFailure::new("invalid_input", "plot.number_labels", error.to_string()))
}

/// Inert presentation only. A family is created only by the consuming operation.
#[wasm_bindgen]
pub struct WasmNumberLabelOptions { options: NumberLabelOptions }

#[wasm_bindgen]
impl WasmCoordinateOptions {
    #[wasm_bindgen(js_name = numberLabels)]
    pub fn number_labels(font: &str, font_size: f64, decimal_places: f64, buff: f64)
        -> Result<WasmNumberLabelOptions, JsValue> {
        if !decimal_places.is_finite() || decimal_places.fract() != 0.0
            || !(0.0..=12.0).contains(&decimal_places) {
            return Err(failure("number-label precision must be an integer between 0 and 12"));
        }
        let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(js_error)?;
        Ok(WasmNumberLabelOptions { options: NumberLabelOptions {
            font: font.into(), font_size, decimal_places: decimal_places as u32, buff,
            ..Default::default()
        } })
    }
}

#[wasm_bindgen]
impl WasmNumberLabelOptions {
    #[wasm_bindgen(js_name = setDirection)]
    pub fn set_direction(&mut self, x: f64, y: f64) { self.options.direction = [x, y]; }
    #[wasm_bindgen(js_name = setExcludeZero)]
    pub fn set_exclude_zero(&mut self, exclude: bool) { self.options.exclude_zero = exclude; }
    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), JsValue> {
        self.options.color = crate::authoring_mobject::family_color(true, red, green, blue, alpha)
            .map_err(failure)?.expect("enabled label color");
        Ok(())
    }
}

fn numbers(values: &[f64], automatic: bool) -> Result<Option<&[f64]>, JsValue> {
    if automatic && !values.is_empty() { return Err(failure("automatic labels cannot supply explicit values")); }
    Ok(if automatic { None } else { Some(values) })
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Cold-only construction. Python rejects this call after live bootstrap.
    #[wasm_bindgen(js_name = numberLabelFamily)]
    pub fn number_label_family(&self, values: &[f64], automatic: bool,
        options: WasmNumberLabelOptions, attach: bool) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let line = ManimNumberLine::from_family(self.semantic_family()?).map_err(failure)?;
        let values = numbers(values, automatic)?;
        let family = if attach { line.add_numbers(values, &options.options) }
            else { line.get_number_mobjects(values, &options.options) }.map_err(failure)?;
        Ok(Self::from_semantic_family(family))
    }

    #[wasm_bindgen(js_name = coordinateLabelFamilies)]
    pub fn coordinate_label_families(&self, x_values: &[f64], automatic_x: bool,
        y_values: &[f64], automatic_y: bool,
        x_options: WasmNumberLabelOptions, y_options: WasmNumberLabelOptions) -> Result<js_sys::Array, JsValue> {
        let axes = ManimAxes::from_family(self.semantic_family()?).map_err(failure)?;
        let families = axes.add_coordinates(numbers(x_values, automatic_x)?, numbers(y_values, automatic_y)?,
            &x_options.options, &y_options.options).map_err(failure)?;
        let result = js_sys::Array::new();
        for family in families { result.push(&Self::from_semantic_family(family).into()); }
        Ok(result)
    }

    /// Materialize wrappers for this newly returned label family only. Source
    /// strings are ordinary Text wrapper metadata, not layout or semantic state.
    #[wasm_bindgen(js_name = numberLabelMembers)]
    pub fn number_label_members(&self) -> Result<js_sys::Array, JsValue> {
        let family = self.semantic_family()?;
        let members = family.integration_store().borrow()
            .semantic_family_members_checked(family.node_id()).map_err(failure)?;
        let result = js_sys::Array::new();
        for member in members {
            let object = Mobject::from_node(Rc::clone(family.integration_store()), member).map_err(failure)?;
            let handle = object.state().map_err(failure)?.content.text().ok_or_else(|| failure("label member is not Text"))?;
            let source = family.integration_store().borrow().text_resources().get(handle)
                .ok_or_else(|| failure("label text resource is missing"))?.source.to_string();
            let pair = js_sys::Array::new();
            pair.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(object).into());
            pair.push(&JsValue::from_str(&source));
            result.push(&pair);
        }
        Ok(result)
    }
}
