//! Thin WASM handles for retained shared Table families.

use crate::authoring_composite::entry_handle;

use crate::{
    authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle,
    WasmAuthoringStore,
};
use wasm_bindgen::prelude::*;

pub(crate) struct CompositeTableLabels<'a> {
    pub rows: Option<Vec<noon::MobjectTarget<'a>>>,
    pub columns: Option<Vec<noon::MobjectTarget<'a>>>,
    pub corner: Option<noon::MobjectTarget<'a>>,
}
impl<'a> CompositeTableLabels<'a> {
    pub fn read(
        rows: &'a crate::WasmCompositeRows,
        columns: &'a crate::WasmCompositeRows,
        corner: &'a crate::WasmCompositeRows,
        kind: &str,
    ) -> Result<Self, JsValue> {
        let rows = rows.optional_single_row_targets()?;
        let columns = columns.optional_single_row_targets()?;
        let mut corner = corner.single_row_targets()?;
        let corner = match corner.len() {
            0 => None,
            1 => Some(corner.remove(0)),
            _ => {
                return Err(js_error(format!(
                    "{kind} top_left_entry must be one shared object or family"
                )))
            }
        };
        Ok(Self {
            rows,
            columns,
            corner,
        })
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createTable)]
    pub fn create_table(
        &self,
        rows: js_sys::Array,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        noon::Table::from_rows_in_store(
            std::rc::Rc::clone(&self.semantics),
            crate::authoring_composite::text_rows(rows, "Table")?,
            options.options,
        )
        .map(WasmTableHandle::new)
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createTableWithLabels)]
    pub fn create_table_with_labels(
        &self,
        rows: js_sys::Array,
        row_labels: &crate::WasmCompositeRows,
        column_labels: &crate::WasmCompositeRows,
        top_left_entry: &crate::WasmCompositeRows,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let rows = crate::authoring_composite::text_rows(rows, "Table")?;
        let labels =
            CompositeTableLabels::read(row_labels, column_labels, top_left_entry, "Table")?;
        noon::Table::from_rows_with_composite_labels_in_store(
            std::rc::Rc::clone(&self.semantics),
            rows,
            labels.rows,
            labels.columns,
            labels.corner,
            options.options,
        )
        .map(WasmTableHandle::new)
        .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMobjectTableWithLabels)]
    pub fn create_mobject_table_with_labels(
        &self,
        rows: &crate::WasmCompositeRows,
        row_labels: &crate::WasmCompositeRows,
        column_labels: &crate::WasmCompositeRows,
        top_left_entry: &crate::WasmCompositeRows,
        options: WasmTableOptions,
    ) -> Result<WasmTableHandle, JsValue> {
        let labels =
            CompositeTableLabels::read(row_labels, column_labels, top_left_entry, "MobjectTable")?;
        noon::MobjectTable::from_target_rows_with_optional_composite_labels_in_store(
            std::rc::Rc::clone(&self.semantics),
            rows.targets(),
            labels.rows,
            labels.columns,
            labels.corner,
            options.options,
        )
        .map(|table| WasmTableHandle::new(table.into_table()))
        .map_err(js_error)
    }
}

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
    pub fn entry_at_position(&self, row: &str, column: &str) -> Result<JsValue, JsValue> {
        self.table
            .entry_at_position(integer_index(row)?, integer_index(column)?)
            .map(entry_handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = bodyEntryAtPosition)]
    pub fn body_entry_at_position(&self, row: &str, column: &str) -> Result<JsValue, JsValue> {
        self.table
            .body_entry_at_position(integer_index(row)?, integer_index(column)?)
            .map(entry_handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = entrySelection)]
    pub fn entry_selection(
        &self,
        without_labels: bool,
    ) -> Result<WasmTableEntrySelection, JsValue> {
        let selection = if without_labels {
            self.table.body_entries_indexing()
        } else {
            self.table.entries_indexing()
        }
        .map_err(js_error)?;
        Ok(WasmTableEntrySelection { selection })
    }
    #[wasm_bindgen(js_name = cellRows)]
    pub fn cell_rows(&self) -> Result<WasmTableCellRows, JsValue> {
        Ok(WasmTableCellRows {
            rows: self.table.cell_rows().map_err(js_error)?,
        })
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
    pub fn get_cell(&self, row: &str, column: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.table
            .get_cell(integer_index(row)?, integer_index(column)?)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}

/// Decimal integer text is only an explicit language-boundary coercion. The
/// native engine receives typed arbitrary precision integers, never a wire scene.
pub(crate) fn integer_index(value: &str) -> Result<noon::IntegerIndex, JsValue> {
    value
        .parse()
        .map_err(|_| js_error(noon::TableAuthoringError::InvalidEntryIndex))
}

#[wasm_bindgen]
pub struct WasmTableEntrySelection {
    selection: noon::TableEntrySelection,
}
#[wasm_bindgen]
impl WasmTableEntrySelection {
    #[wasm_bindgen(js_name = rowStride)]
    pub fn row_stride(&self) -> usize {
        self.selection.row_stride()
    }
    #[wasm_bindgen(js_name = columnOffset)]
    pub fn column_offset(&self) -> usize {
        self.selection.column_offset()
    }
    pub fn length(&self) -> Result<usize, JsValue> {
        self.selection.len().map_err(js_error)
    }
    #[wasm_bindgen(js_name = selectIndex)]
    pub fn select_index(&self, index: &str) -> Result<JsValue, JsValue> {
        self.selection
            .select_index(integer_index(index)?)
            .map(entry_handle)
            .map_err(js_error)
    }
}
#[wasm_bindgen]
pub struct WasmTableCellRows {
    rows: noon::TableCellRows,
}
#[wasm_bindgen]
impl WasmTableCellRows {
    pub fn length(&self) -> usize {
        self.rows.len()
    }
    #[wasm_bindgen(js_name = selectRow)]
    pub fn select_row(&self, row: &str) -> Result<WasmTableCellColumns, JsValue> {
        Ok(WasmTableCellColumns {
            selection: self
                .rows
                .select_row(integer_index(row)?)
                .map_err(js_error)?,
        })
    }
}
#[wasm_bindgen]
pub struct WasmTableCellColumns {
    pub(crate) selection: noon::TableCellColumns,
}
#[wasm_bindgen]
impl WasmTableCellColumns {
    pub fn length(&self) -> usize {
        self.selection.len()
    }
    #[wasm_bindgen(js_name = getCell)]
    pub fn get_cell(&self, column: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.selection
            .get_cell(integer_index(column)?)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = getHighlightedCell)]
    pub fn get_highlighted_cell(
        &self,
        column: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        self.selection
            .get_highlighted_cell(integer_index(column)?, color, opacity)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = highlightCell)]
    pub fn highlight_cell(
        &self,
        column: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        self.selection
            .highlight_cell(integer_index(column)?, color, opacity)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}
