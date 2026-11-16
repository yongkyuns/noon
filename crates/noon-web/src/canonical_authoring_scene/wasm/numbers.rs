//! Numeric constructors use the owning Scene.
use super::CanonicalAuthoringSceneContext;
use crate::{authoring_error::js_error, WasmDecimalNumberHandle, WasmVariableHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = createVariable)]
    #[allow(clippy::too_many_arguments)]
    pub fn create_variable(
        &mut self,
        label: String,
        value: f64,
        decimal_places: u32,
        include_sign: bool,
        group_with_commas: bool,
        show_ellipsis: bool,
        unit: Option<String>,
        font_size: f64,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmVariableHandle, JsValue> {
        let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(js_error)?;
        let store = std::rc::Rc::clone(self.inner.scene.integration_store());
        let format = noon::DecimalFormat {
            decimal_places,
            include_sign,
            group_with_commas,
            show_ellipsis,
            unit,
        };
        let running = self.inner.requires_live_mutation_routing();
        let variable = self
            .inner
            .local_authoring_scene()
            .map_err(js_error)?
            .variable(compiler, label, value, format, font_size)
            .map_err(|error| {
                if running {
                    noon::integration::live_variable_failure(error)
                } else {
                    error.into()
                }
            })
            .map_err(js_error)?;
        Ok(WasmVariableHandle::new(variable, store))
    }

    #[wasm_bindgen(js_name = createDecimalNumber)]
    #[allow(clippy::too_many_arguments)]
    pub fn create_decimal_number(
        &mut self,
        value: f64,
        decimal_places: u32,
        include_sign: bool,
        group_with_commas: bool,
        show_ellipsis: bool,
        unit: Option<String>,
        font_size: f64,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmDecimalNumberHandle, JsValue> {
        let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(js_error)?;
        let format = noon::DecimalFormat {
            decimal_places,
            include_sign,
            group_with_commas,
            show_ellipsis,
            unit,
        };
        self.inner
            .author_number(|scene| scene.decimal_number(compiler, value, format, font_size))
            .map(WasmDecimalNumberHandle::from_number)
            .map_err(js_error)
    }
}
