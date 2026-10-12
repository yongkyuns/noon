use super::wasm::CanonicalAuthoringSceneContext;
use crate::authoring_composite::{number_rows, text_rows};
use crate::{
    authoring_error::js_error, WasmCompositeRows, WasmLatexCompiler, WasmMatrixHandle,
    WasmMatrixOptions,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateMatrix)]
    pub fn live_create_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = text_rows(rows, "Matrix")?;
        if self.inner.player_ownership.is_unstarted() {
            noon::Matrix::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                options.options,
            )
            .map(WasmMatrixHandle::new)
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_matrix(compiler, rows, options.options)
                .map(WasmMatrixHandle::new)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateIntegerMatrix)]
    pub fn live_create_integer_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = number_rows(rows, "Matrix")?;
        if self.inner.player_ownership.is_unstarted() {
            noon::IntegerMatrix::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                options.options,
            )
            .map(|value| WasmMatrixHandle::new(value.into_matrix()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_integer_matrix(compiler, rows, options.options)
                .map(|value| WasmMatrixHandle::new(value.into_matrix()))
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateDecimalMatrix)]
    pub fn live_create_decimal_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = number_rows(rows, "Matrix")?;
        if self.inner.player_ownership.is_unstarted() {
            noon::DecimalMatrix::from_rows_with_format_and_options(
                &mut self.inner.scene,
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
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_decimal_matrix(compiler, rows, options.options)
                .map(|value| WasmMatrixHandle::new(value.into_matrix()))
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateMobjectMatrix)]
    pub fn live_create_mobject_matrix(
        &mut self,
        rows: &WasmCompositeRows,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = rows.targets();
        if self.inner.player_ownership.is_unstarted() {
            noon::MobjectMatrix::from_target_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                options.options,
            )
            .map(|value| WasmMatrixHandle::new(value.into_matrix()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_mobject_matrix(compiler, rows, options.options)
                .map(|value| WasmMatrixHandle::new(value.into_matrix()))
                .map_err(js_error)
        }
    }
}
