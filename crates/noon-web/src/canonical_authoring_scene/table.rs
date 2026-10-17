use super::wasm::CanonicalAuthoringSceneContext;
use crate::authoring_composite::{number_rows, text_rows};
use crate::{
    authoring_error::js_error, WasmCompositeRows, WasmLatexCompiler, WasmTableHandle,
    WasmTableOptions,
};
use wasm_bindgen::prelude::*;

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
        let rows = text_rows(rows, "Table")?;
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
    #[wasm_bindgen(js_name = liveCreateTableWithLabels)]
    pub fn live_create_table_with_labels(
        &mut self,
        rows: js_sys::Array,
        row_labels: &WasmCompositeRows,
        column_labels: &WasmCompositeRows,
        top_left_entry: &WasmCompositeRows,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = text_rows(rows, "Table")?;
        let row_labels = row_labels.single_row_targets()?;
        let column_labels = column_labels.single_row_targets()?;
        let row_labels = (!row_labels.is_empty()).then_some(row_labels);
        let column_labels = (!column_labels.is_empty()).then_some(column_labels);
        let mut top_left_entries = top_left_entry.single_row_targets()?;
        let top_left_entry = match top_left_entries.len() {
            0 => None,
            1 => Some(top_left_entries.remove(0)),
            _ => {
                return Err(js_error(
                    "Table top_left_entry must be one shared object or family",
                ))
            }
        };
        if top_left_entry.is_some() && (row_labels.is_none() || column_labels.is_none()) {
            return Err(js_error(
                "top_left_entry requires both row_labels and col_labels",
            ));
        }
        if self.inner.player_ownership.is_unstarted() {
            noon::Table::from_rows_with_composite_labels(
                &mut self.inner.scene,
                rows,
                row_labels,
                column_labels,
                top_left_entry,
                options.options,
            )
            .map(WasmTableHandle::new)
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_table_with_labels(
                    rows,
                    row_labels,
                    column_labels,
                    top_left_entry,
                    options.options,
                )
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
        let rows = text_rows(rows, "Table")?;
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
        let rows = number_rows(rows, "Table")?;
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
        let rows = number_rows(rows, "Table")?;
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
        rows: &WasmCompositeRows,
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
    #[wasm_bindgen(js_name = liveCreateMobjectTableWithLabels)]
    pub fn live_create_mobject_table_with_labels(
        &mut self,
        rows: &WasmCompositeRows,
        row_labels: &WasmCompositeRows,
        column_labels: &WasmCompositeRows,
        top_left_entry: &WasmCompositeRows,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = rows.targets();
        let row_labels = row_labels.single_row_targets()?;
        let column_labels = column_labels.single_row_targets()?;
        let row_labels = (!row_labels.is_empty()).then_some(row_labels);
        let column_labels = (!column_labels.is_empty()).then_some(column_labels);
        let mut top_left_entries = top_left_entry.single_row_targets()?;
        let top_left_entry = match top_left_entries.len() {
            0 => None,
            1 => Some(top_left_entries.remove(0)),
            _ => {
                return Err(js_error(
                    "MobjectTable top_left_entry must be one shared object or family",
                ))
            }
        };
        if let Some(top_left_entry) = top_left_entry {
            let row_labels = row_labels.ok_or_else(|| {
                js_error("top_left_entry requires both row_labels and col_labels")
            })?;
            let column_labels = column_labels.ok_or_else(|| {
                js_error("top_left_entry requires both row_labels and col_labels")
            })?;
            if self.inner.player_ownership.is_unstarted() {
                noon::MobjectTable::from_target_rows_with_composite_labels_and_top_left_entry(
                    &mut self.inner.scene,
                    rows,
                    row_labels,
                    column_labels,
                    top_left_entry,
                    options.options,
                )
                .map(|value| WasmTableHandle::new(value.into_table()))
                .map_err(js_error)
            } else {
                self.inner
                    .active_live_player()
                    .map_err(js_error)?
                    .live_create_mobject_table_with_labels_and_top_left_entry(
                        rows,
                        row_labels,
                        column_labels,
                        top_left_entry,
                        options.options,
                    )
                    .map(|value| WasmTableHandle::new(value.into_table()))
                    .map_err(js_error)
            }
        } else if self.inner.player_ownership.is_unstarted() {
            noon::MobjectTable::from_target_rows_with_composite_labels(
                &mut self.inner.scene,
                rows,
                row_labels,
                column_labels,
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_mobject_table_with_labels(
                    rows,
                    row_labels,
                    column_labels,
                    options.options,
                )
                .map(|value| WasmTableHandle::new(value.into_table()))
                .map_err(js_error)
        }
    }
}
