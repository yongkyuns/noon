//! Thin native Text and DecimalNumber label adapters over shared Rust operations.
use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmCoordinateOptions,
    WasmDecimalNumberHandle, WasmLatexCompiler,
};
use noon::plot_presentation::NumberLabelOptions;
use noon::{ManimAxes, ManimNumberLine, ManimNumberPlane, Mobject};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

pub(crate) fn failure(error: impl std::fmt::Display) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "plot.number_labels",
        error.to_string(),
    ))
}

fn label_family_handles(families: [noon::MobjectFamily; 2]) -> js_sys::Array {
    let result = js_sys::Array::new();
    for family in families {
        result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
    }
    result
}

/// Inert presentation only. A family is created only by the consuming operation.
#[wasm_bindgen]
pub struct WasmNumberLabelOptions {
    pub(crate) options: NumberLabelOptions,
    excluded_numbers: Vec<f64>,
}

#[wasm_bindgen]
impl WasmCoordinateOptions {
    #[wasm_bindgen(js_name = numberLabels)]
    pub fn number_labels(
        font: &str,
        font_size: f64,
        decimal_places: f64,
        buff: f64,
    ) -> Result<WasmNumberLabelOptions, JsValue> {
        if !decimal_places.is_finite()
            || decimal_places.fract() != 0.0
            || !(0.0..=12.0).contains(&decimal_places)
        {
            return Err(failure(
                "number-label precision must be an integer between 0 and 12",
            ));
        }
        let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(js_error)?;
        Ok(WasmNumberLabelOptions {
            options: NumberLabelOptions {
                font: font.into(),
                font_size,
                decimal_places: decimal_places as u32,
                buff,
                ..Default::default()
            },
            excluded_numbers: Vec::new(),
        })
    }
}

#[wasm_bindgen]
impl WasmNumberLabelOptions {
    #[wasm_bindgen(js_name = setExcludedNumbers)]
    pub fn set_excluded_numbers(&mut self, values: &[f64]) -> Result<(), JsValue> {
        if values.len() > noon::plot_presentation::MAX_PLOT_LABELS {
            return Err(failure("number-label exclusion limit exceeded"));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(failure("number-label exclusions must be finite"));
        }
        let mut excluded_numbers = Vec::new();
        excluded_numbers
            .try_reserve_exact(values.len())
            .map_err(|_| failure("number-label exclusion allocation failed"))?;
        excluded_numbers.extend_from_slice(values);
        self.excluded_numbers = excluded_numbers;
        Ok(())
    }

    #[wasm_bindgen(js_name = setDirection)]
    pub fn set_direction(&mut self, x: f64, y: f64) {
        self.options.direction = [x, y];
    }
    #[wasm_bindgen(js_name = setExcludeZero)]
    pub fn set_exclude_zero(&mut self, exclude: bool) {
        self.options.exclude_zero = exclude;
    }
    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.color = crate::authoring_mobject::family_color(true, red, green, blue, alpha)
            .map_err(failure)?
            .expect("enabled label color");
        Ok(())
    }
}

fn numbers(values: &[f64], automatic: bool) -> Result<Option<&[f64]>, JsValue> {
    if automatic && !values.is_empty() {
        return Err(failure("automatic labels cannot supply explicit values"));
    }
    Ok(if automatic { None } else { Some(values) })
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Cold-only construction. Python rejects this call after live bootstrap.
    #[wasm_bindgen(js_name = numberLabelFamily)]
    pub fn number_label_family(
        &self,
        values: &[f64],
        automatic: bool,
        options: WasmNumberLabelOptions,
        attach: bool,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let line = ManimNumberLine::from_family(self.semantic_family()?).map_err(failure)?;
        let values = numbers(values, automatic)?;
        let family = if attach {
            line.add_numbers_excluding(values, &options.excluded_numbers, &options.options)
        } else {
            line.get_number_mobjects_excluding(values, &options.excluded_numbers, &options.options)
        }
        .map_err(failure)?;
        Ok(Self::from_semantic_family(family))
    }

    /// Attach caller-provided labels at Rust-mapped NumberLine positions.
    #[wasm_bindgen(js_name = numberLineCustomLabelFamily)]
    pub fn number_line_custom_label_family(
        &self,
        values: &[f64],
        texts: js_sys::Array,
        options: WasmNumberLabelOptions,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        if texts.length() as usize != values.len() {
            return Err(failure(
                "custom label positions and texts must have equal lengths",
            ));
        }
        if values.len() > noon::plot_presentation::MAX_PLOT_LABELS {
            return Err(failure("number-label limit exceeded"));
        }
        let mut labels = Vec::new();
        labels
            .try_reserve_exact(values.len())
            .map_err(|_| failure("number-label allocation failed"))?;
        for (index, number) in values.iter().enumerate() {
            let text = texts
                .get(index as u32)
                .as_string()
                .ok_or_else(|| failure("custom number-line label text must be a string"))?;
            labels.push((*number, text));
        }
        let line = ManimNumberLine::from_family(self.semantic_family()?).map_err(failure)?;
        let family = line
            .add_labels(&labels, &options.options)
            .map_err(failure)?;
        Ok(Self::from_semantic_family(family))
    }

    #[wasm_bindgen(js_name = coordinateLabelFamilies)]
    pub fn coordinate_label_families(
        &self,
        x_values: &[f64],
        automatic_x: bool,
        y_values: &[f64],
        automatic_y: bool,
        x_options: WasmNumberLabelOptions,
        y_options: WasmNumberLabelOptions,
    ) -> Result<js_sys::Array, JsValue> {
        let axes = ManimAxes::from_family(self.semantic_family()?).map_err(failure)?;
        let families = axes
            .add_coordinates(
                numbers(x_values, automatic_x)?,
                numbers(y_values, automatic_y)?,
                &x_options.options,
                &y_options.options,
            )
            .map_err(failure)?;
        Ok(label_family_handles(families))
    }

    #[wasm_bindgen(js_name = numberPlaneCoordinateLabelFamilies)]
    pub fn number_plane_coordinate_label_families(
        &self,
        x_values: &[f64],
        automatic_x: bool,
        y_values: &[f64],
        automatic_y: bool,
        x_options: WasmNumberLabelOptions,
        y_options: WasmNumberLabelOptions,
    ) -> Result<js_sys::Array, JsValue> {
        let plane = ManimNumberPlane::from_family(self.semantic_family()?).map_err(failure)?;
        let families = plane
            .add_coordinates(
                numbers(x_values, automatic_x)?,
                numbers(y_values, automatic_y)?,
                &x_options.options,
                &y_options.options,
            )
            .map_err(failure)?;
        Ok(label_family_handles(families))
    }

    #[wasm_bindgen(js_name = numberPlaneDecimalCoordinateLabelFamilies)]
    pub fn number_plane_decimal_coordinate_label_families(
        &self,
        x_values: &[f64],
        automatic_x: bool,
        y_values: &[f64],
        automatic_y: bool,
        x_options: WasmNumberLabelOptions,
        y_options: WasmNumberLabelOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<js_sys::Array, JsValue> {
        let plane = ManimNumberPlane::from_family(self.semantic_family()?).map_err(failure)?;
        let families = plane
            .add_decimal_coordinates(
                compiler,
                numbers(x_values, automatic_x)?,
                numbers(y_values, automatic_y)?,
                &x_options.options,
                &y_options.options,
            )
            .map_err(failure)?;
        Ok(label_family_handles(families))
    }

    /// Construct and attach DecimalNumber coordinate labels to one plane axis.
    #[wasm_bindgen(js_name = numberPlaneDecimalCoordinateLabelFamily)]
    pub fn number_plane_decimal_coordinate_label_family(
        &self,
        axis_index: u32,
        values: &[f64],
        automatic: bool,
        options: WasmNumberLabelOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let plane = ManimNumberPlane::from_family(self.semantic_family()?).map_err(failure)?;
        let axis = match axis_index {
            0 => plane.x_axis(),
            1 => plane.y_axis(),
            _ => return Err(failure("NumberPlane axis index must be 0 or 1")),
        }
        .map_err(failure)?;
        let family = axis
            .add_decimal_numbers(compiler, numbers(values, automatic)?, &options.options)
            .map_err(failure)?;
        Ok(Self::from_semantic_family(family))
    }

    /// Materialize wrappers for this newly returned label family only. Source
    /// strings are ordinary Text wrapper metadata, not layout or semantic state.
    #[wasm_bindgen(js_name = numberLabelMembers)]
    pub fn number_label_members(&self) -> Result<js_sys::Array, JsValue> {
        let family = self.semantic_family()?;
        let members = family
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(failure)?;
        let result = js_sys::Array::new();
        for member in members {
            let object = Mobject::from_node(Rc::clone(family.integration_store()), member)
                .map_err(failure)?;
            let handle = object
                .state()
                .map_err(failure)?
                .content
                .text()
                .ok_or_else(|| failure("label member is not Text"))?;
            let source = family
                .integration_store()
                .borrow()
                .text_resources()
                .get(handle)
                .ok_or_else(|| failure("label text resource is missing"))?
                .source
                .to_string();
            let pair = js_sys::Array::new();
            pair.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(object).into());
            pair.push(&JsValue::from_str(&source));
            result.push(&pair);
        }
        Ok(result)
    }

    #[wasm_bindgen(js_name = decimalNumberLabelMembers)]
    pub fn decimal_number_label_members(&self) -> Result<js_sys::Array, JsValue> {
        let family = self.semantic_family()?;
        let members = family
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(failure)?;
        let result = js_sys::Array::new();
        for member in members {
            let object = Mobject::from_node(Rc::clone(family.integration_store()), member)
                .map_err(failure)?;
            let number = noon::DecimalNumber::from_mobject(object).map_err(failure)?;
            result.push(&WasmDecimalNumberHandle::from_number(number).into());
        }
        Ok(result)
    }
}
