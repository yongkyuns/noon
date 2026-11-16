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
    #[wasm_bindgen(js_name = tableCell)]
    pub fn table_cell(
        &mut self,
        selection: &WasmTableCellColumns,
        column: &str,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        let column = integer_index(column)?;
        selection
            .selection
            .get_cell_in_scene(
                self.inner.local_authoring_scene().map_err(js_error)?,
                column,
            )
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = highlightTableCell)]
    pub fn highlight_table_cell(
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
        selection
            .selection
            .highlight_cell_in_scene(
                self.inner.local_authoring_scene().map_err(js_error)?,
                column,
                color,
                opacity,
            )
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = getHighlightedTableCell)]
    pub fn get_highlighted_table_cell(
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
        selection
            .selection
            .get_highlighted_cell_in_scene(
                self.inner.local_authoring_scene().map_err(js_error)?,
                column,
                color,
                opacity,
            )
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createTable)]
    pub fn create_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = text_rows(rows, "Table")?;
        noon::Table::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            rows,
            options.options,
        )
        .map(WasmTableHandle::new)
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createTableWithLabels)]
    pub fn create_table_with_labels(
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
        noon::Table::from_rows_with_composite_labels(
            self.inner.local_authoring_scene().map_err(js_error)?,
            rows,
            row_labels,
            column_labels,
            top_left_entry,
            options.options,
        )
        .map(WasmTableHandle::new)
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMathTable)]
    pub fn create_math_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = text_rows(rows, "Table")?;
        noon::MathTable::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            options.options,
        )
        .map(|value| WasmTableHandle::new(value.into_table()))
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createIntegerTable)]
    pub fn create_integer_table(
        &mut self,
        rows: js_sys::Array,
        options: WasmTableOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = number_rows(rows, "Table")?;
        noon::IntegerTable::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            options.options,
        )
        .map(|value| WasmTableHandle::new(value.into_table()))
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createDecimalTable)]
    pub fn create_decimal_table(
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
        noon::DecimalTable::from_rows_with_options(
            self.inner.local_authoring_scene().map_err(js_error)?,
            compiler,
            rows,
            format,
            options.options,
        )
        .map(|value| WasmTableHandle::new(value.into_table()))
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMobjectTableWithLabels)]
    pub fn create_mobject_table_with_labels(
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
        noon::MobjectTable::from_target_rows_with_optional_composite_labels(
            self.inner.local_authoring_scene().map_err(js_error)?,
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
