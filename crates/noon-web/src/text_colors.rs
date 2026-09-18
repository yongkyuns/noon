//! Constructor-only text color arguments. Selection and paint projection stay in Rust.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Owned argument batch consumed by native Text construction, including on failure.
#[wasm_bindgen]
pub struct WasmTextColorBatch {
    pub(crate) colors: Vec<(Arc<str>, noon::Color)>,
    pub(crate) base_color: noon::Color,
}

impl Default for WasmTextColorBatch {
    fn default() -> Self {
        Self {
            colors: Vec::new(),
            base_color: noon::WHITE,
        }
    }
}

fn channel(value: f64) -> Result<f32, JsValue> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value as f32)
    } else {
        Err(crate::authoring_error::js_error(
            crate::AuthoringFailure::new(
                "invalid_input",
                "text.invalid_color",
                "text color channels must be finite and between zero and one",
            ),
        ))
    }
}

#[wasm_bindgen]
impl WasmTextColorBatch {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = setBaseColor)]
    pub fn set_base_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.base_color = noon::Color::rgba(
            channel(red)?,
            channel(green)?,
            channel(blue)?,
            channel(alpha)?,
        );
        Ok(())
    }

    pub fn push(
        &mut self,
        selector: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        let color = noon::Color::rgba(
            channel(red)?,
            channel(green)?,
            channel(blue)?,
            channel(alpha)?,
        );
        self.colors.push((Arc::from(selector), color));
        Ok(())
    }
}
