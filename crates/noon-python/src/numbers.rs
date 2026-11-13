//! DecimalNumber and coordinate-label bindings for the native LaTeX provider.
use crate::{
    context::{Context, Store},
    coordinates::{label_failure, NumberLabelOptions},
    engine_error,
    family::FamilyHandle,
    latex::LatexCompiler,
    mobject::MobjectHandle,
};
use noon::{DecimalFormat, DecimalNumber, Mobject, NumericAuthoringError};
use pyo3::prelude::*;
use std::rc::Rc;

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct DecimalHandle {
    number: DecimalNumber,
    compiler: LatexCompiler,
}

#[pymethods]
impl DecimalHandle {
    fn mobject(&self) -> MobjectHandle {
        MobjectHandle::from_semantic_mobject(self.number.mobject().clone())
    }

    fn text(&self) -> PyResult<String> {
        self.number.text().map_err(engine_error)
    }

    fn value(&self, context: Option<&Context>) -> PyResult<f64> {
        match context {
            Some(context) => {
                context.require_active()?;
                context.scene.effective_decimal_value(&self.number)
            }
            None => self.number.value(),
        }
        .map_err(engine_error)
    }

    #[pyo3(name = "fontSize")]
    fn font_size(&self, context: Option<&Context>) -> PyResult<f64> {
        match context {
            Some(context) => {
                context.require_active()?;
                context.scene.effective_decimal_font_size(&self.number)
            }
            None => self.number.font_size(),
        }
        .map_err(engine_error)
    }

    #[pyo3(name = "integerValue")]
    fn integer_value(&self, context: Option<&Context>) -> PyResult<i64> {
        noon::integer_value(self.value(context)?).map_err(engine_error)
    }

    #[pyo3(name = "setValue")]
    fn set_value(&mut self, value: f64) -> PyResult<()> {
        self.compiler.with_backend(|backend| {
            self.number
                .set_value(backend, value)
                .map(|_| ())
                .map_err(engine_error)
        })
    }

    #[pyo3(name = "incrementValue")]
    fn increment_value(&mut self, delta: f64) -> PyResult<()> {
        self.set_value(self.number.value().map_err(engine_error)? + delta)
    }

    #[pyo3(name = "setValueLive")]
    fn set_value_live(&self, context: &mut Context, value: f64) -> PyResult<()> {
        context.available()?;
        self.compiler.with_backend(|backend| {
            context
                .scene
                .set_decimal_value(&self.number, backend, value)
                .map_err(|error| numeric_failure(error, context.has_execution()))
        })
    }

    #[pyo3(name = "incrementValueLive")]
    fn increment_value_live(&self, context: &mut Context, delta: f64) -> PyResult<()> {
        self.set_value_live(context, self.number.value().map_err(engine_error)? + delta)
    }
}

fn numeric_failure(error: NumericAuthoringError, running: bool) -> PyErr {
    if running {
        engine_error(noon::integration::live_numeric_failure(error))
    } else {
        engine_error(error)
    }
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (store, value, places, sign, commas, ellipsis, unit, size, context))]
fn create_decimal(
    store: &Store,
    value: f64,
    places: u32,
    sign: bool,
    commas: bool,
    ellipsis: bool,
    unit: Option<String>,
    size: f64,
    context: Option<&mut Context>,
) -> PyResult<DecimalHandle> {
    let size =
        noon::integration::authoring_render_f64("font size", size).map_err(engine_error)? as f32;
    let format = DecimalFormat {
        decimal_places: places,
        include_sign: sign,
        group_with_commas: commas,
        show_ellipsis: ellipsis,
        unit,
    };
    let number = store.latex.with_backend(|backend| match context {
        Some(context) => {
            context.available()?;
            context
                .scene
                .decimal_number(backend, value, format, size)
                .map_err(|error| numeric_failure(error, context.has_execution()))
        }
        None => {
            DecimalNumber::with_font_size(Rc::clone(&store.semantics), backend, value, format, size)
                .map_err(engine_error)
        }
    })?;
    Ok(DecimalHandle {
        number,
        compiler: store.latex.clone(),
    })
}

#[pyfunction]
fn decimal_from_mobject(store: &Store, object: &MobjectHandle) -> PyResult<DecimalHandle> {
    DecimalNumber::from_mobject(object.handle.clone())
        .map(|number| DecimalHandle {
            number,
            compiler: store.latex.clone(),
        })
        .map_err(engine_error)
}

fn numbers(values: &[f64], automatic: bool) -> PyResult<Option<&[f64]>> {
    if automatic && !values.is_empty() {
        return Err(label_failure(
            "automatic labels cannot supply explicit values",
        ));
    }
    Ok((!automatic).then_some(values))
}

#[pyfunction]
fn line_decimal_labels(
    store: &Store,
    family: &FamilyHandle,
    values: Vec<f64>,
    automatic: bool,
    options: &NumberLabelOptions,
    attach: bool,
) -> PyResult<FamilyHandle> {
    let line = noon::ManimNumberLine::from_family(family.family.clone()).map_err(label_failure)?;
    let values = numbers(&values, automatic)?;
    store.latex.with_backend(|backend| {
        if attach {
            line.add_decimal_numbers(backend, values, &options.options)
        } else {
            line.decimal_number_labels(backend, values, &options.options)
        }
        .map(FamilyHandle::from_semantic_family)
        .map_err(label_failure)
    })
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn decimal_coordinates(
    store: &Store,
    family: &FamilyHandle,
    x: Vec<f64>,
    automatic_x: bool,
    y: Vec<f64>,
    automatic_y: bool,
    x_options: &NumberLabelOptions,
    y_options: &NumberLabelOptions,
    plane: bool,
) -> PyResult<(FamilyHandle, FamilyHandle)> {
    let x = numbers(&x, automatic_x)?;
    let y = numbers(&y, automatic_y)?;
    let x_options = &x_options.options;
    let y_options = &y_options.options;
    let [x, y] = store.latex.with_backend(|backend| {
        if plane {
            noon::ManimNumberPlane::from_family(family.family.clone())
                .map_err(label_failure)?
                .add_decimal_coordinates(backend, x, y, x_options, y_options)
        } else {
            noon::ManimAxes::from_family(family.family.clone())
                .map_err(label_failure)?
                .add_decimal_coordinates(backend, x, y, x_options, y_options)
        }
        .map_err(label_failure)
    })?;
    Ok((
        FamilyHandle::from_semantic_family(x),
        FamilyHandle::from_semantic_family(y),
    ))
}

#[pyfunction]
fn decimal_label_members(store: &Store, family: &FamilyHandle) -> PyResult<Vec<DecimalHandle>> {
    let family = &family.family;
    let semantics = family.integration_store();
    let members = semantics
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .map_err(label_failure)?;
    members
        .into_iter()
        .map(|node| {
            let object = Mobject::from_node(Rc::clone(semantics), node).map_err(label_failure)?;
            let number = DecimalNumber::from_mobject(object).map_err(label_failure)?;
            Ok(DecimalHandle {
                number,
                compiler: store.latex.clone(),
            })
        })
        .collect()
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<DecimalHandle>()?;
    module.add_function(wrap_pyfunction!(create_decimal, module)?)?;
    module.add_function(wrap_pyfunction!(decimal_from_mobject, module)?)?;
    module.add_function(wrap_pyfunction!(line_decimal_labels, module)?)?;
    module.add_function(wrap_pyfunction!(decimal_coordinates, module)?)?;
    module.add_function(wrap_pyfunction!(decimal_label_members, module)?)
}
