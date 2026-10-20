//! Thin WASM handles for retained shared Table families.

use crate::authoring_composite::entry_handle;

use crate::{authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmTableOptions {
    pub(crate) options: noon::TableOptions,
}

#[wasm_bindgen]
impl WasmTableOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(v_buff: f64, h_buff: f64, include_outer_lines: bool) -> Self {
        Self {
            options: noon::TableOptions {
                v_buff,
                h_buff,
                include_outer_lines,
                ..Default::default()
            },
        }
    }

    /// Match ManimCE's default-on inner grid lines while preserving the
    /// existing three-argument constructor for JavaScript callers.
    #[wasm_bindgen(js_name = setIncludeInnerLines)]
    pub fn set_include_inner_lines(&mut self, include_inner_lines: bool) {
        self.options.include_inner_lines = include_inner_lines;
    }

    #[wasm_bindgen(js_name = setLineStrokeColor)]
    pub fn set_line_stroke_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) {
        self.options.line_stroke_color = noon::Color {
            red: red as f32,
            green: green as f32,
            blue: blue as f32,
            alpha: alpha as f32,
        };
    }

    #[wasm_bindgen(js_name = setLineStrokeWidth)]
    pub fn set_line_stroke_width(&mut self, width: f64) {
        self.options.line_stroke_width = width;
    }

    #[wasm_bindgen(js_name = setLineStrokeOpacity)]
    pub fn set_line_stroke_opacity(&mut self, opacity: f64) {
        self.options.line_stroke_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setLineStrokeJoin)]
    pub fn set_line_stroke_join(&mut self, join: &str) -> Result<(), JsValue> {
        self.options.line_stroke_join = match join {
            "round" => noon::StrokeJoin::Round,
            "miter" | "auto" => noon::StrokeJoin::Miter,
            "bevel" => noon::StrokeJoin::Bevel,
            _ => {
                return Err(js_error(
                    "line_config.joint_type must be auto, round, miter, or bevel",
                ))
            }
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = setLineStrokeCap)]
    pub fn set_line_stroke_cap(&mut self, cap: &str) -> Result<(), JsValue> {
        self.options.line_stroke_cap = match cap {
            "round" => noon::StrokeCap::Round,
            "butt" | "auto" => noon::StrokeCap::Butt,
            "square" => noon::StrokeCap::Square,
            _ => {
                return Err(js_error(
                    "line_config.cap_style must be auto, round, butt, or square",
                ))
            }
        };
        Ok(())
    }

    /// Apply the qualified arrange-in-grid cell-alignment directions.
    #[wasm_bindgen(js_name = setCellAlignment)]
    pub fn set_cell_alignment(&mut self, x: f64, y: f64) {
        self.options.cell_alignment_x = x;
        self.options.cell_alignment_y = y;
    }
}

#[wasm_bindgen]
pub struct WasmTableHandle {
    table: noon::Table,
}
impl WasmTableHandle {
    pub(crate) fn new(table: noon::Table) -> Self {
        Self { table }
    }
    pub(crate) fn table(&self) -> &noon::Table {
        &self.table
    }
}

/// Direct WASM renderer counterpart of the native and Python retained Table
/// example. It exercises native text, LaTeX math, and a family-valued entry
/// through the same Rust scene builder.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createTableRenderer)]
pub async fn create_table_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut crate::WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::table::session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct WASM renderer counterpart of the configurable DecimalTable fixture.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createDecimalTableFormatRenderer)]
pub async fn create_decimal_table_format_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut crate::WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session =
        noon::example_scenes::table::decimal_format_session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct WASM renderer counterpart of the grouped-label table fixture.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createMobjectTableFamilyLabelsRenderer)]
pub async fn create_mobject_table_family_labels_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::table::family_labels_session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct WASM renderer counterpart of the plain-text Table labels fixture.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createTableLabelsRenderer)]
pub async fn create_table_labels_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::table::labels_session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct WASM renderer counterpart of the Table line-configuration fixture.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createTableLineConfigRenderer)]
pub async fn create_table_line_config_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::table::line_config_session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = asTable)]
    pub fn as_table(&self) -> Result<WasmTableHandle, JsValue> {
        noon::Table::from_family(self.semantic_family()?)
            .map(WasmTableHandle::new)
            .map_err(js_error)
    }
}

fn entries_array(
    entries: Result<Vec<noon::TableEntry>, noon::TableAuthoringError>,
) -> Result<js_sys::Array, JsValue> {
    let result = js_sys::Array::new();
    for entry in entries.map_err(js_error)? {
        result.push(&entry_handle(entry));
    }
    Ok(result)
}

#[wasm_bindgen]
impl WasmTableHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.table.family().clone())
    }
    #[wasm_bindgen(js_name = entryFamily)]
    pub fn entry_family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.table.entry_family().clone())
    }
    #[wasm_bindgen(js_name = entries)]
    pub fn entries(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for entry in self.table.entries().map_err(js_error)? {
            result.push(&entry_handle(entry));
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = bodyEntries)]
    pub fn body_entries(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for entry in self.table.get_entries_without_labels().map_err(js_error)? {
            result.push(&entry_handle(entry));
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = entryAtPosition)]
    pub fn entry_at_position(&self, row: i32, column: i32) -> Result<JsValue, JsValue> {
        self.table
            .entry_at_position(i128::from(row), i128::from(column))
            .map(entry_handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = bodyEntryAtPosition)]
    pub fn body_entry_at_position(&self, row: i32, column: i32) -> Result<JsValue, JsValue> {
        self.table
            .body_entry_at_position(i128::from(row), i128::from(column))
            .map(entry_handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = rows)]
    pub fn rows(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for row in self.table.get_rows().map_err(js_error)? {
            let values = js_sys::Array::new();
            for entry in row {
                values.push(&entry_handle(entry));
            }
            result.push(&values);
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = rowLabels)]
    pub fn row_labels(&self) -> Result<js_sys::Array, JsValue> {
        entries_array(self.table.get_row_labels())
    }
    #[wasm_bindgen(js_name = columnLabels)]
    pub fn column_labels(&self) -> Result<js_sys::Array, JsValue> {
        entries_array(self.table.get_col_labels())
    }
    #[wasm_bindgen(js_name = labels)]
    pub fn labels(&self) -> Result<js_sys::Array, JsValue> {
        entries_array(self.table.get_labels())
    }
    #[wasm_bindgen(js_name = columns)]
    pub fn columns(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for column in self.table.columns().map_err(js_error)? {
            let values = js_sys::Array::new();
            for entry in column {
                values.push(&entry_handle(entry));
            }
            result.push(&values);
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = getHorizontalLines)]
    pub fn get_horizontal_lines(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for line in self.table.get_horizontal_lines().map_err(js_error)? {
            result.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(line).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = getVerticalLines)]
    pub fn get_vertical_lines(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for line in self.table.get_vertical_lines().map_err(js_error)? {
            result.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(line).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = shape)]
    pub fn shape(&self) -> Result<js_sys::Array, JsValue> {
        let (rows, columns) = self.table.shape().map_err(js_error)?;
        let result = js_sys::Array::new();
        result.push(&JsValue::from_f64(rows as f64));
        result.push(&JsValue::from_f64(columns as f64));
        Ok(result)
    }
    #[wasm_bindgen(js_name = rowFamilies)]
    pub fn row_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.table.row_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = columnFamilies)]
    pub fn column_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.table.column_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = getCell)]
    pub fn get_cell(
        &self,
        row: usize,
        column: usize,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.table
            .get_cell(row, column)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}
