use super::wasm::CanonicalAuthoringSceneContext;
use crate::authoring_composite::{number_rows, text_rows};
use crate::{
    authoring_error::js_error, WasmCompositeRows, WasmLatexCompiler, WasmMatrixHandle,
    WasmMatrixOptions,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = createMatrix)]
    pub fn create_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = text_rows(rows, "Matrix")?;
        noon::Matrix::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            options.options,
        )
        .map(WasmMatrixHandle::new)
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createIntegerMatrix)]
    pub fn create_integer_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = number_rows(rows, "Matrix")?;
        noon::IntegerMatrix::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            options.options,
        )
        .map(|value| WasmMatrixHandle::new(value.into_matrix()))
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createDecimalMatrix)]
    pub fn create_decimal_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = number_rows(rows, "Matrix")?;
        noon::DecimalMatrix::from_rows_with_format_and_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            noon::DecimalFormat {
                decimal_places: 1,
                ..Default::default()
            },
            options.options,
        )
        .map(|value| WasmMatrixHandle::new(value.into_matrix()))
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMobjectMatrix)]
    pub fn create_mobject_matrix(
        &mut self,
        rows: &WasmCompositeRows,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = rows.targets();
        noon::MobjectMatrix::from_target_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            options.options,
        )
        .map(|value| WasmMatrixHandle::new(value.into_matrix()))
        .map_err(js_error)
    }
}
