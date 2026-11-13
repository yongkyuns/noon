//! CPython projections of Scene-owned Table families and their cell selections.
use crate::{
    context::{Context, Store},
    engine_error,
    family::{entry_handle, read_entry, FamilyHandle},
    mobject::MobjectHandle,
};
use noon::{Color, CompositeEntryHandle, StrokeCap, StrokeJoin, Table, TableOptions};
use pyo3::prelude::*;
use std::rc::Rc;

type TablePresentation = (
    (f64, f64, bool, bool),
    [f64; 4],
    (f64, f64, String, String),
    (f64, f64),
);

fn table_options(
    (
        (v_buff, h_buff, include_outer_lines, include_inner_lines),
        [red, green, blue, alpha],
        (line_stroke_width, line_stroke_opacity, join, cap),
        (cell_alignment_x, cell_alignment_y),
    ): TablePresentation,
) -> PyResult<TableOptions> {
    let line_stroke_join = match join.as_str() {
        "round" => StrokeJoin::Round,
        "miter" | "auto" => StrokeJoin::Miter,
        "bevel" => StrokeJoin::Bevel,
        _ => {
            return Err(engine_error(
                "line_config.joint_type must be auto, round, miter, or bevel",
            ))
        }
    };
    let line_stroke_cap = match cap.as_str() {
        "round" => StrokeCap::Round,
        "butt" | "auto" => StrokeCap::Butt,
        "square" => StrokeCap::Square,
        _ => {
            return Err(engine_error(
                "line_config.cap_style must be auto, round, butt, or square",
            ))
        }
    };
    Ok(TableOptions {
        v_buff,
        h_buff,
        include_outer_lines,
        include_inner_lines,
        line_stroke_color: Color::rgba(red as f32, green as f32, blue as f32, alpha as f32),
        line_stroke_width,
        line_stroke_opacity,
        line_stroke_join,
        line_stroke_cap,
        cell_alignment_x,
        cell_alignment_y,
        ..Default::default()
    })
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct TableHandle(Table);

#[pymethods]
impl TableHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    #[pyo3(name = "entryFamily")]
    fn entry_family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.entry_family().clone())
    }

    #[pyo3(name = "entriesFamily")]
    fn entries_family(&self) -> PyResult<FamilyHandle> {
        self.0
            .entries_family()
            .map(FamilyHandle::from_semantic_family)
            .map_err(engine_error)
    }

    #[pyo3(name = "bodyEntriesFamily")]
    fn body_entries_family(&self) -> PyResult<FamilyHandle> {
        self.0
            .body_entries_family()
            .map(FamilyHandle::from_semantic_family)
            .map_err(engine_error)
    }

    #[pyo3(name = "entrySelection")]
    fn entry_selection(&self, body_only: bool) -> PyResult<EntrySelection> {
        if body_only {
            self.0.body_entries_indexing()
        } else {
            self.0.entries_indexing()
        }
        .map(EntrySelection)
        .map_err(engine_error)
    }

    #[pyo3(name = "cellRows")]
    fn cell_rows(&self) -> PyResult<CellRows> {
        self.0.cell_rows().map(CellRows).map_err(engine_error)
    }

    fn rows(&self, py: Python<'_>) -> PyResult<Vec<Vec<Py<PyAny>>>> {
        self.0
            .get_rows()
            .map_err(engine_error)?
            .into_iter()
            .map(|row| project_entries(py, row))
            .collect()
    }

    fn columns(&self, py: Python<'_>) -> PyResult<Vec<Vec<Py<PyAny>>>> {
        self.0
            .columns()
            .map_err(engine_error)?
            .into_iter()
            .map(|column| project_entries(py, column))
            .collect()
    }

    #[pyo3(name = "rowLabels")]
    fn row_labels(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        project_entries(py, self.0.get_row_labels().map_err(engine_error)?)
    }

    #[pyo3(name = "columnLabels")]
    fn column_labels(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        project_entries(py, self.0.get_col_labels().map_err(engine_error)?)
    }

    fn labels(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        project_entries(py, self.0.get_labels().map_err(engine_error)?)
    }

    #[pyo3(name = "getHorizontalLines")]
    fn horizontal_lines(&self) -> PyResult<Vec<MobjectHandle>> {
        self.0
            .get_horizontal_lines()
            .map(|lines| {
                lines
                    .into_iter()
                    .map(MobjectHandle::from_semantic_mobject)
                    .collect()
            })
            .map_err(engine_error)
    }

    #[pyo3(name = "getVerticalLines")]
    fn vertical_lines(&self) -> PyResult<Vec<MobjectHandle>> {
        self.0
            .get_vertical_lines()
            .map(|lines| {
                lines
                    .into_iter()
                    .map(MobjectHandle::from_semantic_mobject)
                    .collect()
            })
            .map_err(engine_error)
    }
}

fn project_entries(py: Python<'_>, entries: Vec<CompositeEntryHandle>) -> PyResult<Vec<Py<PyAny>>> {
    entries
        .into_iter()
        .map(|entry| entry_handle(py, Some(entry)))
        .collect()
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct EntrySelection(noon::TableEntrySelection);

#[pymethods]
impl EntrySelection {
    #[pyo3(name = "rowStride")]
    fn row_stride(&self) -> usize {
        self.0.row_stride()
    }

    #[pyo3(name = "columnOffset")]
    fn column_offset(&self) -> usize {
        self.0.column_offset()
    }

    fn length(&self) -> PyResult<usize> {
        self.0.len().map_err(engine_error)
    }

    #[pyo3(name = "selectNormalizedIndex")]
    fn select_normalized_index(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyAny>> {
        entry_handle(
            py,
            self.0
                .select_normalized_index(index)
                .map_err(engine_error)?,
        )
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct CellRows(noon::TableCellRows);

#[pymethods]
impl CellRows {
    fn length(&self) -> usize {
        self.0.len()
    }

    #[pyo3(name = "selectRow")]
    fn select_row(&self, row: &str) -> PyResult<CellColumns> {
        self.0
            .select_row(integer_index(row)?)
            .map(CellColumns)
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct CellColumns(noon::TableCellColumns);

#[pymethods]
impl CellColumns {
    fn length(&self) -> usize {
        self.0.len()
    }
}

fn integer_index(value: &str) -> PyResult<noon::IntegerIndex> {
    value
        .parse()
        .map_err(|_| engine_error(noon::TableAuthoringError::InvalidEntryIndex))
}

#[pyfunction]
#[pyo3(signature = (context, selection, column))]
fn table_cell(
    context: Option<&mut Context>,
    selection: &CellColumns,
    column: &str,
) -> PyResult<MobjectHandle> {
    let column = integer_index(column)?;
    match context {
        Some(context) => {
            context.available()?;
            selection.0.get_cell_in_scene(&mut context.scene, column)
        }
        None => selection.0.get_cell(column),
    }
    .map(MobjectHandle::from_semantic_mobject)
    .map_err(engine_error)
}

#[pyfunction]
#[pyo3(signature = (context, selection, column, rgba, opacity, attach))]
fn highlighted_cell(
    context: Option<&mut Context>,
    selection: &CellColumns,
    column: &str,
    rgba: [f64; 4],
    opacity: f64,
    attach: bool,
) -> PyResult<MobjectHandle> {
    let column = integer_index(column)?;
    let [red, green, blue, alpha] = rgba;
    let color = Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
    let cell = match context {
        Some(context) => {
            context.available()?;
            if attach {
                selection
                    .0
                    .highlight_cell_in_scene(&mut context.scene, column, color, opacity)
            } else {
                selection.0.get_highlighted_cell_in_scene(
                    &mut context.scene,
                    column,
                    color,
                    opacity,
                )
            }
        }
        None if attach => selection.0.highlight_cell(column, color, opacity),
        None => selection.0.get_highlighted_cell(column, color, opacity),
    };
    cell.map(MobjectHandle::from_semantic_mobject)
        .map_err(engine_error)
}

fn table_scene(context: Option<&mut Context>) -> PyResult<&mut noon::Scene> {
    let context =
        context.ok_or_else(|| engine_error("Table requires a Scene authoring context"))?;
    context.available()?;
    Ok(&mut context.scene)
}

fn labels(values: Option<Vec<Bound<'_, PyAny>>>) -> PyResult<Option<Vec<CompositeEntryHandle>>> {
    values
        .map(|values| values.iter().map(read_entry).collect())
        .transpose()
}

fn label_targets(values: Option<&[CompositeEntryHandle]>) -> Option<Vec<noon::MobjectTarget<'_>>> {
    values.map(|values| values.iter().map(CompositeEntryHandle::as_target).collect())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (store, rows, row_labels, column_labels, corner, options, context))]
fn create_text_table(
    store: &Store,
    rows: Vec<Vec<String>>,
    row_labels: Option<Vec<Bound<'_, PyAny>>>,
    column_labels: Option<Vec<Bound<'_, PyAny>>>,
    corner: Option<&Bound<'_, PyAny>>,
    options: TablePresentation,
    context: Option<&mut Context>,
) -> PyResult<TableHandle> {
    let options = table_options(options)?;
    let row_labels = labels(row_labels)?;
    let column_labels = labels(column_labels)?;
    let corner = corner.map(read_entry).transpose()?;
    let row_labels = label_targets(row_labels.as_deref());
    let column_labels = label_targets(column_labels.as_deref());
    let corner = corner.as_ref().map(CompositeEntryHandle::as_target);
    match context {
        Some(context) => {
            context.available()?;
            Table::from_rows_with_composite_labels(
                &mut context.scene,
                rows,
                row_labels,
                column_labels,
                corner,
                options,
            )
        }
        None => Table::from_rows_with_composite_labels_in_store(
            Rc::clone(&store.semantics),
            rows,
            row_labels,
            column_labels,
            corner,
            options,
        ),
    }
    .map(TableHandle)
    .map_err(engine_error)
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (store, rows, row_labels, column_labels, corner, options, context))]
fn create_mobject_table(
    store: &Store,
    rows: Vec<Vec<Bound<'_, PyAny>>>,
    row_labels: Option<Vec<Bound<'_, PyAny>>>,
    column_labels: Option<Vec<Bound<'_, PyAny>>>,
    corner: Option<&Bound<'_, PyAny>>,
    options: TablePresentation,
    context: Option<&mut Context>,
) -> PyResult<TableHandle> {
    let options = table_options(options)?;
    let entries = rows
        .iter()
        .map(|row| row.iter().map(read_entry).collect::<PyResult<Vec<_>>>())
        .collect::<PyResult<Vec<_>>>()?;
    let rows = entries
        .iter()
        .map(|row| row.iter().map(CompositeEntryHandle::as_target));
    let row_labels = labels(row_labels)?;
    let column_labels = labels(column_labels)?;
    let corner = corner.map(read_entry).transpose()?;
    let row_labels = label_targets(row_labels.as_deref());
    let column_labels = label_targets(column_labels.as_deref());
    let corner = corner.as_ref().map(CompositeEntryHandle::as_target);
    match context {
        Some(context) => {
            context.available()?;
            noon::MobjectTable::from_target_rows_with_optional_composite_labels(
                &mut context.scene,
                rows,
                row_labels,
                column_labels,
                corner,
                options,
            )
        }
        None => noon::MobjectTable::from_target_rows_with_optional_composite_labels_in_store(
            Rc::clone(&store.semantics),
            rows,
            row_labels,
            column_labels,
            corner,
            options,
        ),
    }
    .map(|value| TableHandle(value.into_table()))
    .map_err(engine_error)
}

#[pyfunction]
#[pyo3(signature = (store, rows, options, context))]
fn create_math_table(
    store: &Store,
    rows: Vec<Vec<String>>,
    options: TablePresentation,
    context: Option<&mut Context>,
) -> PyResult<TableHandle> {
    let scene = table_scene(context)?;
    let options = table_options(options)?;
    store.latex.with_backend(|backend| {
        noon::MathTable::from_rows_with_options(scene, backend, rows, options)
            .map(|value| TableHandle(value.into_table()))
            .map_err(engine_error)
    })
}

#[pyfunction]
#[pyo3(signature = (store, rows, format, options, context))]
fn create_numeric_table(
    store: &Store,
    rows: Vec<Vec<f64>>,
    format: Option<(u32, bool, bool, bool, Option<String>)>,
    options: TablePresentation,
    context: Option<&mut Context>,
) -> PyResult<TableHandle> {
    let scene = table_scene(context)?;
    let options = table_options(options)?;
    store.latex.with_backend(|backend| {
        match format {
            Some((decimal_places, include_sign, group_with_commas, show_ellipsis, unit)) => {
                let format = noon::DecimalFormat {
                    decimal_places,
                    include_sign,
                    group_with_commas,
                    show_ellipsis,
                    unit,
                };
                noon::DecimalTable::from_rows_with_options(scene, backend, rows, format, options)
                    .map(noon::DecimalTable::into_table)
            }
            None => noon::IntegerTable::from_rows_with_options(scene, backend, rows, options)
                .map(noon::IntegerTable::into_table),
        }
        .map(TableHandle)
        .map_err(engine_error)
    })
}

#[pyfunction]
fn table_from_family(family: &FamilyHandle) -> PyResult<TableHandle> {
    Table::from_family(family.family.clone())
        .map(TableHandle)
        .map_err(engine_error)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(table_cell, module)?)?;
    module.add_function(wrap_pyfunction!(highlighted_cell, module)?)?;
    module.add_function(wrap_pyfunction!(create_text_table, module)?)?;
    module.add_function(wrap_pyfunction!(create_mobject_table, module)?)?;
    module.add_function(wrap_pyfunction!(create_math_table, module)?)?;
    module.add_function(wrap_pyfunction!(create_numeric_table, module)?)?;
    module.add_function(wrap_pyfunction!(table_from_family, module)?)
}
