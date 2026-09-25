use super::wasm::CanonicalAuthoringSceneContext;
use crate::{
    authoring_error::js_error, WasmLatexCompiler, WasmMobjectTableRows, WasmTableHandle,
    WasmTableOptions,
};
use wasm_bindgen::prelude::*;

fn text_rows(rows: js_sys::Array) -> Result<Vec<Vec<String>>, JsValue> {
    rows.iter()
        .map(|row| {
            let row: js_sys::Array = row
                .dyn_into()
                .map_err(|_| js_error("Table rows must be arrays"))?;
            row.iter()
                .map(|entry| {
                    entry
                        .as_string()
                        .ok_or_else(|| js_error("Table entries must be strings"))
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
                .map_err(|_| js_error("Table rows must be arrays"))?;
            row.iter()
                .map(|entry| {
                    entry
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| js_error("Table entries must be finite numbers"))
                })
                .collect()
        })
        .collect()
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveTableCell)]
    pub fn live_table_cell(
        &mut self,
        table: &WasmTableHandle,
        row: usize,
        column: usize,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        if self.inner.player_ownership.is_unstarted() {
            table
                .table()
                .get_cell(row, column)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_table_cell(table.table(), row, column)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveHighlightTableCell)]
    pub fn live_highlight_table_cell(
        &mut self,
        table: &WasmTableHandle,
        row: usize,
        column: usize,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        if self.inner.player_ownership.is_unstarted() {
            table
                .table()
                .highlight_cell(row, column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_highlight_table_cell(table.table(), row, column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveGetHighlightedTableCell)]
    pub fn live_get_highlighted_table_cell(
        &mut self,
        table: &WasmTableHandle,
        row: usize,
        column: usize,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        if self.inner.player_ownership.is_unstarted() {
            table
                .table()
                .get_highlighted_cell(row, column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_get_highlighted_table_cell(table.table(), row, column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateTable)]
    pub fn live_create_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = text_rows(rows)?;
        if self.inner.player_ownership.is_unstarted() {
            noon::Table::from_rows_with_options(&mut self.inner.scene, rows, options.options)
                .map(WasmTableHandle::new)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_table(rows, options.options)
                .map(WasmTableHandle::new)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateMathTable)]
    pub fn live_create_math_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = text_rows(rows)?;
        if self.inner.player_ownership.is_unstarted() {
            noon::MathTable::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_math_table(compiler, rows, options.options)
                .map(WasmTableHandle::new)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateIntegerTable)]
    pub fn live_create_integer_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = number_rows(rows)?;
        if self.inner.player_ownership.is_unstarted() {
            noon::IntegerTable::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_integer_table(compiler, rows, options.options)
                .map(|value| WasmTableHandle::new(value.into_table()))
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateDecimalTable)]
    pub fn live_create_decimal_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = number_rows(rows)?;
        if self.inner.player_ownership.is_unstarted() {
            noon::DecimalTable::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                noon::DecimalFormat {
                    decimal_places: 1,
                    ..Default::default()
                },
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_decimal_table(compiler, rows, options.options)
                .map(|value| WasmTableHandle::new(value.into_table()))
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveCreateMobjectTable)]
    pub fn live_create_mobject_table(
        &mut self,
        rows: &WasmMobjectTableRows,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = rows.targets();
        if self.inner.player_ownership.is_unstarted() {
            noon::MobjectTable::from_target_rows_with_options(
                &mut self.inner.scene,
                rows,
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_mobject_table(rows, options.options)
                .map(|value| WasmTableHandle::new(value.into_table()))
                .map_err(js_error)
        }
    }
}
