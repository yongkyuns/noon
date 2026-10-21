use super::wasm::CanonicalAuthoringSceneContext;
use crate::authoring_composite::{number_rows, text_rows};
use crate::authoring_table::{integer_index, CompositeTableLabels, WasmTableCellColumns};
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
        selection: &WasmTableCellColumns,
        column: &str,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let column = integer_index(column)?;
        if self.inner.player_ownership.is_unstarted() {
            selection
                .selection
                .get_cell(column)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_table_cell(&selection.selection, column)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveHighlightTableCell)]
    pub fn live_highlight_table_cell(
        &mut self,
        selection: &WasmTableCellColumns,
        column: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        let column = integer_index(column)?;
        if self.inner.player_ownership.is_unstarted() {
            selection
                .selection
                .highlight_cell(column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_highlight_table_cell(&selection.selection, column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }
    #[wasm_bindgen(js_name = liveGetHighlightedTableCell)]
    pub fn live_get_highlighted_table_cell(
        &mut self,
        selection: &WasmTableCellColumns,
        column: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        let column = integer_index(column)?;
        if self.inner.player_ownership.is_unstarted() {
            selection
                .selection
                .get_highlighted_cell(column, color, opacity)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_get_highlighted_table_cell(&selection.selection, column, color, opacity)
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
        let CompositeTableLabels {
            rows: row_labels,
            columns: column_labels,
            corner: top_left_entry,
        } = CompositeTableLabels::read(row_labels, column_labels, top_left_entry, "Table")?;
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
        decimal_places: u32,
        include_sign: bool,
        group_with_commas: bool,
        show_ellipsis: bool,
        unit: Option<String>,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = number_rows(rows, "Table")?;
        let format = noon::DecimalFormat {
            decimal_places,
            include_sign,
            group_with_commas,
            show_ellipsis,
            unit,
        };
        if self.inner.player_ownership.is_unstarted() {
            noon::DecimalTable::from_rows_with_options(
                &mut self.inner.scene,
                compiler,
                rows,
                format,
                options.options,
            )
            .map(|value| WasmTableHandle::new(value.into_table()))
            .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_decimal_table(compiler, rows, format, options.options)
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
        let CompositeTableLabels {
            rows: row_labels,
            columns: column_labels,
            corner: top_left_entry,
        } = CompositeTableLabels::read(row_labels, column_labels, top_left_entry, "MobjectTable")?;
        if self.inner.player_ownership.is_unstarted() {
            noon::MobjectTable::from_target_rows_with_optional_composite_labels(
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
    }
}
