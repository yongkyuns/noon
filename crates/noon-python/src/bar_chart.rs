//! Native BarChart handles and number labels backed by the shared Scene.
use crate::{
    context::{Context, Store},
    coordinates::NumberLabelOptions,
    engine_error,
    family::FamilyHandle,
    mobject::MobjectHandle,
};
use noon::integration::{authoring_render_f64, coordinate_failure, family_color, AuthoringFailure};
use noon::{BarLabelOptions, ManimBarChart, ManimBarChartOptions};
use pyo3::prelude::*;
use std::rc::Rc;

#[pyclass(skip_from_py_object, module = "_noon_native")]
struct BarChartOptions(ManimBarChartOptions);

#[pymethods]
impl BarChartOptions {
    #[new]
    fn new(values: Vec<f64>, y_range: Vec<f64>, x_length: f64, y_length: f64) -> PyResult<Self> {
        ManimBarChartOptions::manim_defaults(
            values,
            (!y_range.is_empty()).then_some(y_range.as_slice()),
            (x_length != 0.0).then_some(x_length),
            (y_length != 0.0).then_some(y_length),
        )
        .map(Self)
        .map_err(coordinate_failure)
        .map_err(engine_error)
    }

    #[pyo3(name = "setNames")]
    fn set_names(&mut self, names: Vec<String>, font_size: f64) -> PyResult<()> {
        self.0.name_font_size =
            authoring_render_f64("font size", font_size).map_err(engine_error)? as f32;
        self.0.bar_names = Some(names);
        Ok(())
    }

    #[pyo3(name = "setStyle")]
    fn set_style(&mut self, width: f64, opacity: f64, stroke_width: f64) {
        self.0.bar_width = width;
        self.0.bar_fill_opacity = opacity;
        self.0.bar_stroke_width = stroke_width;
    }

    #[pyo3(name = "setColors")]
    fn set_colors(&mut self, rgba: Vec<f64>) -> PyResult<()> {
        if rgba.is_empty() || !rgba.len().is_multiple_of(4) {
            return Err(engine_error("bar colors require one or more RGBA values"));
        }
        self.0.bar_colors = rgba
            .chunks_exact(4)
            .map(|c| {
                family_color(true, c[0], c[1], c[2], c[3])
                    .map(|color| color.expect("enabled bar color"))
                    .map_err(engine_error)
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(())
    }

    fn free(&self) {}
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct BarChartHandle(pub(crate) ManimBarChart);

#[pymethods]
impl BarChartHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn axes(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.axes().family().clone())
    }

    fn bars(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.bars().clone())
    }

    #[pyo3(name = "xLabels")]
    fn x_labels(&self) -> PyResult<Option<FamilyHandle>> {
        self.0
            .x_labels()
            .map(|value| value.map(FamilyHandle::from_semantic_family))
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "yLabels")]
    fn y_labels(&self) -> PyResult<FamilyHandle> {
        self.0
            .y_labels()
            .map(FamilyHandle::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "barPrefix")]
    fn bar_prefix(&self, count: usize) -> PyResult<Vec<MobjectHandle>> {
        self.0
            .bar_prefix(count)
            .map(|bars| {
                bars.into_iter()
                    .map(MobjectHandle::from_semantic_mobject)
                    .collect()
            })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    fn values(&self) -> PyResult<Vec<f64>> {
        self.0
            .values()
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "changeBarValues")]
    fn change_bar_values(&mut self, values: Vec<f64>, update_colors: bool) -> PyResult<()> {
        self.0
            .change_bar_values_cold(&values, update_colors)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }
}

#[pyfunction]
#[pyo3(signature = (store, options, labels, context))]
fn create_bar_chart(
    store: &Store,
    options: &BarChartOptions,
    labels: &NumberLabelOptions,
    context: Option<&mut Context>,
) -> PyResult<BarChartHandle> {
    store.latex.with_backend(|backend| {
        match context {
            Some(context) => {
                context.available()?;
                context
                    .scene
                    .bar_chart_with_axis_labels(&options.0, &labels.options, backend)
            }
            None => ManimBarChart::create_with_axis_labels(
                Rc::clone(&store.semantics),
                &options.0,
                &labels.options,
                backend,
            ),
        }
        .map(BarChartHandle)
        .map_err(|error| engine_error(AuthoringFailure::unclassified("bar_chart.create", &error)))
    })
}

#[pyfunction]
#[pyo3(signature = (store, chart, size, buff, math, rgba, context))]
fn bar_chart_labels(
    store: &Store,
    chart: &BarChartHandle,
    size: f64,
    buff: f64,
    math: bool,
    rgba: Vec<f64>,
    context: Option<&mut Context>,
) -> PyResult<FamilyHandle> {
    let color = match rgba.as_slice() {
        [] => None,
        [red, green, blue, alpha] => {
            family_color(true, *red, *green, *blue, *alpha).map_err(engine_error)?
        }
        _ => return Err(engine_error("label color requires four RGBA values")),
    };
    let font_size = authoring_render_f64("font size", size).map_err(engine_error)? as f32;
    let options = BarLabelOptions {
        font_size,
        buff,
        color,
        math,
    };
    store.latex.with_backend(|backend| {
        match context {
            Some(context) => {
                context.available()?;
                context.scene.bar_labels(&chart.0, backend, &options)
            }
            None => chart.0.get_bar_labels(backend, &options),
        }
        .map(FamilyHandle::from_semantic_family)
        .map_err(|error| engine_error(AuthoringFailure::unclassified("bar_chart.labels", &error)))
    })
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<BarChartOptions>()?;
    module.add_function(wrap_pyfunction!(create_bar_chart, module)?)?;
    module.add_function(wrap_pyfunction!(bar_chart_labels, module)?)
}
