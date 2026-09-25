use super::wasm::CanonicalAuthoringSceneContext;
use crate::{
    authoring_error::js_error, WasmLatexCompiler, WasmMatrixHandle, WasmMatrixOptions,
    WasmMobjectMatrixRows,
};
use wasm_bindgen::prelude::*;

fn text_rows(rows: js_sys::Array) -> Result<Vec<Vec<String>>, JsValue> {
    rows.iter()
        .map(|row| {
            let row: js_sys::Array = row
                .dyn_into()
                .map_err(|_| js_error("Matrix rows must be arrays"))?;
            row.iter()
                .map(|entry| {
                    entry
                        .as_string()
                        .ok_or_else(|| js_error("Matrix entries must be strings"))
                })
                .collect()
        })
        .collect()
}
fn number_rows(rows: js_sys::Array) -> Result<Vec<Vec<f64>>, JsValue> {
    rows.iter()
        .map(|row| {
            let row: js_sys::Array = row
                .dyn_into()
                .map_err(|_| js_error("Matrix rows must be arrays"))?;
            row.iter()
                .map(|entry| {
                    entry
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| js_error("Matrix entries must be finite numbers"))
                })
                .collect()
        })
        .collect()
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateMatrix)]
    pub fn live_create_matrix(
        &mut self,
        rows: js_sys::Array,
        options: WasmMatrixOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmMatrixHandle, JsValue> {
        let rows = text_rows(rows)?;
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
        let rows = number_rows(rows)?;
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
        let rows = number_rows(rows)?;
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
        rows: &WasmMobjectMatrixRows,
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
