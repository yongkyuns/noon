//! Scene-owned retained matrix construction and typed entry observations.
use crate::{
    context::{Context, Store},
    engine_error,
    family::{entry_handle, read_entry, FamilyHandle},
};
use noon::{CompositeEntryHandle, DecimalFormat, Matrix, MatrixOptions};
use pyo3::prelude::*;

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct MatrixHandle(Matrix);

#[pymethods]
impl MatrixHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn entries(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.0
            .entries()
            .map_err(engine_error)?
            .into_iter()
            .map(|entry| entry_handle(py, Some(entry)))
            .collect()
    }

    fn columns(&self, py: Python<'_>) -> PyResult<Vec<Vec<Py<PyAny>>>> {
        self.0
            .columns()
            .map_err(engine_error)?
            .into_iter()
            .map(|column| {
                column
                    .into_iter()
                    .map(|entry| entry_handle(py, Some(entry)))
                    .collect()
            })
            .collect()
    }
}

fn matrix_scene(context: Option<&mut Context>) -> PyResult<&mut noon::Scene> {
    let context =
        context.ok_or_else(|| engine_error("Matrix requires a Scene authoring context"))?;
    context.available()?;
    Ok(&mut context.scene)
}

#[pyfunction]
#[pyo3(signature = (store, rows, options, context))]
fn create_matrix(
    store: &Store,
    rows: Vec<Vec<String>>,
    options: (f64, f64, f64, f64, bool),
    context: Option<&mut Context>,
) -> PyResult<MatrixHandle> {
    let scene = matrix_scene(context)?;
    store.latex.with_backend(|backend| {
        Matrix::from_rows_with_options(scene, backend, rows, matrix_options(options))
            .map(MatrixHandle)
            .map_err(engine_error)
    })
}

#[pyfunction]
#[pyo3(signature = (store, rows, options, decimal, context))]
fn create_numeric_matrix(
    store: &Store,
    rows: Vec<Vec<f64>>,
    options: (f64, f64, f64, f64, bool),
    decimal: bool,
    context: Option<&mut Context>,
) -> PyResult<MatrixHandle> {
    let scene = matrix_scene(context)?;
    let options = matrix_options(options);
    store.latex.with_backend(|backend| {
        let matrix = if decimal {
            noon::DecimalMatrix::from_rows_with_format_and_options(
                scene,
                backend,
                rows,
                DecimalFormat {
                    decimal_places: 1,
                    ..Default::default()
                },
                options,
            )
            .map(noon::DecimalMatrix::into_matrix)
        } else {
            noon::IntegerMatrix::from_rows_with_options(scene, backend, rows, options)
                .map(noon::IntegerMatrix::into_matrix)
        };
        matrix.map(MatrixHandle).map_err(engine_error)
    })
}

#[pyfunction]
#[pyo3(signature = (store, rows, options, context))]
fn create_mobject_matrix(
    store: &Store,
    rows: Vec<Vec<Bound<'_, PyAny>>>,
    options: (f64, f64, f64, f64, bool),
    context: Option<&mut Context>,
) -> PyResult<MatrixHandle> {
    let scene = matrix_scene(context)?;
    let rows = rows
        .iter()
        .map(|row| row.iter().map(read_entry).collect::<PyResult<Vec<_>>>())
        .collect::<PyResult<Vec<_>>>()?;
    store.latex.with_backend(|backend| {
        noon::MobjectMatrix::from_target_rows_with_options(
            scene,
            backend,
            rows.iter()
                .map(|row| row.iter().map(CompositeEntryHandle::as_target)),
            matrix_options(options),
        )
        .map(|value| MatrixHandle(value.into_matrix()))
        .map_err(engine_error)
    })
}

fn matrix_options(
    (v_buff, h_buff, bracket_h_buff, bracket_v_buff, stretch_brackets): (f64, f64, f64, f64, bool),
) -> MatrixOptions {
    MatrixOptions {
        v_buff,
        h_buff,
        bracket_h_buff,
        bracket_v_buff,
        stretch_brackets,
    }
}

#[pyfunction]
fn matrix_from_family(family: &FamilyHandle) -> PyResult<MatrixHandle> {
    Matrix::from_family(family.family.clone())
        .map(MatrixHandle)
        .map_err(engine_error)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<MatrixHandle>()?;
    module.add_function(wrap_pyfunction!(create_matrix, module)?)?;
    module.add_function(wrap_pyfunction!(create_numeric_matrix, module)?)?;
    module.add_function(wrap_pyfunction!(create_mobject_matrix, module)?)?;
    module.add_function(wrap_pyfunction!(matrix_from_family, module)?)
}
