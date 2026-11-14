//! Native language projections of the shared Arrow family.

use crate::{context::Store, engine_error, family::FamilyHandle, mobject::MobjectHandle};
use noon::{
    integration::AuthoringFailure, manim_default_vector_field_ranges_2d, ArrowVectorFieldOptions,
    ManimArrow, ManimArrowOptions, ManimArrowVectorField, ManimLineEndpoints, Scene,
    VectorFieldAxisRange,
};
use pyo3::prelude::*;
use std::rc::Rc;

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct ArrowOptions(pub(crate) ManimArrowOptions);

#[pymethods]
impl ArrowOptions {
    #[staticmethod]
    #[pyo3(name = "arrow")]
    fn arrow(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> PyResult<Self> {
        ManimArrowOptions::arrow(start_x, start_y, end_x, end_y)
            .map(Self)
            .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name = "vector")]
    fn vector(x: f64, y: f64) -> PyResult<Self> {
        ManimArrowOptions::vector(x, y)
            .map(Self)
            .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name = "doubleArrow")]
    fn double_arrow(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> PyResult<Self> {
        ManimArrowOptions::double_arrow(start_x, start_y, end_x, end_y)
            .map(Self)
            .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name = "vectorField")]
    #[allow(clippy::too_many_arguments)]
    fn vector_field(
        x_start: f64,
        x_end: f64,
        x_step: f64,
        y_start: f64,
        y_end: f64,
        y_step: f64,
        custom_length: bool,
    ) -> PyResult<VectorFieldOptions> {
        Self::vector_field_with_default_ranges(
            false,
            x_start,
            x_end,
            x_step,
            false,
            y_start,
            y_end,
            y_step,
            custom_length,
        )
    }

    #[staticmethod]
    #[pyo3(name = "vectorFieldWithDefaultRanges")]
    #[allow(clippy::too_many_arguments)]
    fn vector_field_with_default_ranges(
        default_x: bool,
        x_start: f64,
        x_end: f64,
        x_step: f64,
        default_y: bool,
        y_start: f64,
        y_end: f64,
        y_step: f64,
        custom_length: bool,
    ) -> PyResult<VectorFieldOptions> {
        ArrowVectorFieldOptions::new(
            (!default_x).then(|| VectorFieldAxisRange::new(x_start, x_end, x_step)),
            (!default_y).then(|| VectorFieldAxisRange::new(y_start, y_end, y_step)),
            custom_length,
        )
        .map(VectorFieldOptions)
        .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldXStart")]
    fn default_vector_field_x_start() -> f64 {
        manim_default_vector_field_ranges_2d().x.start
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldXEnd")]
    fn default_vector_field_x_end() -> f64 {
        manim_default_vector_field_ranges_2d().x.end
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldXStep")]
    fn default_vector_field_x_step() -> f64 {
        manim_default_vector_field_ranges_2d().x.step
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldYStart")]
    fn default_vector_field_y_start() -> f64 {
        manim_default_vector_field_ranges_2d().y.start
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldYEnd")]
    fn default_vector_field_y_end() -> f64 {
        manim_default_vector_field_ranges_2d().y.end
    }

    #[staticmethod]
    #[pyo3(name = "defaultVectorFieldYStep")]
    fn default_vector_field_y_step() -> f64 {
        manim_default_vector_field_ranges_2d().y.step
    }

    #[staticmethod]
    #[pyo3(name = "arrowFromMobjects")]
    fn arrow_from_mobjects(start: &MobjectHandle, end: &MobjectHandle) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_from_mobjects(&start.handle, &end.handle)
            .map_err(engine_error)?;
        Self::arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[staticmethod]
    #[pyo3(name = "doubleArrowFromMobjects")]
    fn double_arrow_from_mobjects(start: &MobjectHandle, end: &MobjectHandle) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_from_mobjects(&start.handle, &end.handle)
            .map_err(engine_error)?;
        Self::double_arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[staticmethod]
    #[pyo3(name = "arrowFromMobject")]
    fn arrow_from_mobject(start: &MobjectHandle, end_x: f64, end_y: f64) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_from_mobject(&start.handle, end_x, end_y)
            .map_err(engine_error)?;
        Self::arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[staticmethod]
    #[pyo3(name = "doubleArrowFromMobject")]
    fn double_arrow_from_mobject(start: &MobjectHandle, end_x: f64, end_y: f64) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_from_mobject(&start.handle, end_x, end_y)
            .map_err(engine_error)?;
        Self::double_arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[staticmethod]
    #[pyo3(name = "arrowToMobject")]
    fn arrow_to_mobject(start_x: f64, start_y: f64, end: &MobjectHandle) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_to_mobject(start_x, start_y, &end.handle)
            .map_err(engine_error)?;
        Self::arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[staticmethod]
    #[pyo3(name = "doubleArrowToMobject")]
    fn double_arrow_to_mobject(start_x: f64, start_y: f64, end: &MobjectHandle) -> PyResult<Self> {
        let endpoints = noon::manim_arrow_endpoints_to_mobject(start_x, start_y, &end.handle)
            .map_err(engine_error)?;
        Self::double_arrow(
            endpoints.start.0,
            endpoints.start.1,
            endpoints.end.0,
            endpoints.end.1,
        )
    }

    #[pyo3(name = "setBuff")]
    fn set_buff(&mut self, value: f64) -> PyResult<()> {
        self.0.set_buff(value).map_err(engine_error)
    }

    #[pyo3(name = "setTipLength")]
    fn set_tip_length(&mut self, value: f64) -> PyResult<()> {
        self.0.set_tip_length(value).map_err(engine_error)
    }

    #[pyo3(name = "setMaxTipLengthToLengthRatio")]
    fn set_max_tip_length_to_length_ratio(&mut self, value: f64) -> PyResult<()> {
        self.0
            .set_max_tip_length_to_length_ratio(value)
            .map_err(engine_error)
    }

    #[pyo3(name = "setMaxStrokeWidthToLengthRatio")]
    fn set_max_stroke_width_to_length_ratio(&mut self, value: f64) -> PyResult<()> {
        self.0
            .set_max_stroke_width_to_length_ratio(value)
            .map_err(engine_error)
    }

    #[pyo3(name = "setZIndex")]
    fn set_z_index(&mut self, value: f64) -> PyResult<()> {
        self.0.set_z_index(value).map_err(engine_error)
    }

    #[pyo3(name = "setTranslation")]
    fn set_translation(&mut self, x: f64, y: f64) -> PyResult<()> {
        self.0.set_translation(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setScale")]
    fn set_scale(&mut self, x: f64, y: f64) -> PyResult<()> {
        self.0.set_scale(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setRotation")]
    fn set_rotation(&mut self, value: f64) -> PyResult<()> {
        self.0.set_rotation(value).map_err(engine_error)
    }

    #[pyo3(name = "setColor")]
    fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<()> {
        self.0
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidth")]
    fn set_stroke_width(&mut self, value: f64) -> PyResult<()> {
        self.0.set_stroke_width(value).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidthMode")]
    fn set_stroke_width_mode(&mut self, value: &str) -> PyResult<()> {
        self.0.set_stroke_width_mode(value).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeJoin")]
    fn set_stroke_join(&mut self, value: &str) -> PyResult<()> {
        self.0.set_stroke_join(value).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeCap")]
    fn set_stroke_cap(&mut self, value: &str) -> PyResult<()> {
        self.0.set_stroke_cap(value).map_err(engine_error)
    }

    #[pyo3(name = "setObjectOpacity")]
    fn set_object_opacity(&mut self, value: f64) -> PyResult<()> {
        self.0.set_object_opacity(value).map_err(engine_error)
    }
}

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct VectorFieldOptions(ArrowVectorFieldOptions);

#[pymethods]
impl VectorFieldOptions {
    #[getter(sampleCount)]
    fn sample_count(&self) -> usize {
        self.0.sample_count()
    }

    #[pyo3(name = "sampleX")]
    fn sample_x(&self, index: usize) -> PyResult<f64> {
        self.0
            .sample_point(index)
            .map(|point| point.x)
            .map_err(engine_error)
    }

    #[pyo3(name = "sampleY")]
    fn sample_y(&self, index: usize) -> PyResult<f64> {
        self.0
            .sample_point(index)
            .map(|point| point.y)
            .map_err(engine_error)
    }

    #[pyo3(name = "setVector")]
    fn set_vector(&mut self, index: usize, x: f64, y: f64) -> PyResult<()> {
        self.0.set_vector(index, x, y).map_err(engine_error)
    }

    #[pyo3(name = "setDisplayLength")]
    fn set_display_length(&mut self, index: usize, value: f64) -> PyResult<()> {
        self.0
            .set_display_length(index, value)
            .map_err(engine_error)
    }

    #[pyo3(name = "setFieldColor")]
    fn set_field_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<()> {
        self.0
            .set_field_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setColorGradient")]
    fn set_color_gradient(&mut self, min: f64, max: f64, custom_scheme: bool) {
        self.0.set_color_gradient(min, max, custom_scheme);
    }

    #[pyo3(name = "addColorGradientStop")]
    fn add_color_gradient_stop(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> PyResult<()> {
        self.0
            .add_color_gradient_stop(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setColorValue")]
    fn set_color_value(&mut self, index: usize, value: f64) -> PyResult<()> {
        self.0.set_color_value(index, value).map_err(engine_error)
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct VectorFieldHandle(ManimArrowVectorField);

#[pymethods]
impl VectorFieldHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    #[getter(vectorCount)]
    fn vector_count(&self) -> usize {
        self.0.vectors().len()
    }

    #[pyo3(name = "vectorAt")]
    fn vector_at(&self, index: usize) -> PyResult<ArrowHandle> {
        self.0
            .vectors()
            .get(index)
            .cloned()
            .map(ArrowHandle)
            .ok_or_else(|| {
                engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "vector_field.vector_index",
                    format!("vector-field vector index {index} is out of range"),
                ))
            })
    }
}

#[derive(FromPyObject)]
pub(crate) enum ArrowRequest<'py> {
    Arrow(PyRef<'py, ArrowOptions>),
    VectorField(PyRef<'py, VectorFieldOptions>),
}

impl ArrowRequest<'_> {
    pub(crate) fn publish_in_scene(
        self,
        py: Python<'_>,
        scene: &mut Scene,
        running: bool,
    ) -> PyResult<Py<PyAny>> {
        match self {
            Self::Arrow(options) => scene
                .manim_arrow(options.0.clone())
                .map_err(|error| {
                    engine_error(AuthoringFailure::from_scene_mutation(error, running))
                })
                .and_then(|arrow| Py::new(py, ArrowHandle(arrow)).map(Py::into_any)),
            Self::VectorField(options) => scene
                .manim_arrow_vector_field(options.0.clone())
                .map_err(|error| {
                    engine_error(AuthoringFailure::from_scene_vector_field(error, running))
                })
                .and_then(|field| Py::new(py, VectorFieldHandle(field)).map(Py::into_any)),
        }
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct ArrowHandle(pub(crate) ManimArrow);

#[pymethods]
impl ArrowHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn shaft(&self) -> MobjectHandle {
        MobjectHandle::from_semantic_mobject(self.0.shaft().clone())
    }

    #[pyo3(name = "endTip")]
    fn end_tip(&self) -> MobjectHandle {
        MobjectHandle::from_semantic_mobject(self.0.end_tip().clone())
    }

    #[getter(hasStartTip)]
    fn has_start_tip(&self) -> bool {
        self.0.start_tip().is_some()
    }

    #[pyo3(name = "startTip")]
    fn start_tip(&self) -> Option<MobjectHandle> {
        self.0
            .start_tip()
            .cloned()
            .map(MobjectHandle::from_semantic_mobject)
    }

    #[pyo3(name = "matrixTransformedEndpoints")]
    fn matrix_transformed_endpoints(
        &self,
        values: Vec<f64>,
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> PyResult<(f64, f64, f64, f64)> {
        let ManimLineEndpoints { start, end } = self
            .0
            .matrix_transformed_endpoints(&values, rows, columns, about_x, about_y)
            .map_err(engine_error)?;
        Ok((start.0, start.1, end.0, end.1))
    }

    #[pyo3(name = "startX")]
    fn start_x(&self) -> PyResult<f64> {
        self.0
            .manim_endpoints()
            .map(|value| value.start.0)
            .map_err(engine_error)
    }

    #[pyo3(name = "startY")]
    fn start_y(&self) -> PyResult<f64> {
        self.0
            .manim_endpoints()
            .map(|value| value.start.1)
            .map_err(engine_error)
    }

    #[pyo3(name = "endX")]
    fn end_x(&self) -> PyResult<f64> {
        self.0
            .manim_endpoints()
            .map(|value| value.end.0)
            .map_err(engine_error)
    }

    #[pyo3(name = "endY")]
    fn end_y(&self) -> PyResult<f64> {
        self.0
            .manim_endpoints()
            .map(|value| value.end.1)
            .map_err(engine_error)
    }

    #[pyo3(name = "vectorX")]
    fn vector_x(&self) -> PyResult<f64> {
        self.0
            .manim_vector()
            .map(|value| value.0)
            .map_err(engine_error)
    }

    #[pyo3(name = "vectorY")]
    fn vector_y(&self) -> PyResult<f64> {
        self.0
            .manim_vector()
            .map(|value| value.1)
            .map_err(engine_error)
    }

    #[pyo3(name = "unitVectorX")]
    fn unit_vector_x(&self) -> PyResult<f64> {
        self.0
            .manim_unit_vector()
            .map(|value| value.0)
            .map_err(engine_error)
    }

    #[pyo3(name = "unitVectorY")]
    fn unit_vector_y(&self) -> PyResult<f64> {
        self.0
            .manim_unit_vector()
            .map(|value| value.1)
            .map_err(engine_error)
    }

    #[pyo3(name = "length")]
    fn length(&self) -> PyResult<f64> {
        self.0.manim_length().map_err(engine_error)
    }

    #[pyo3(name = "angle")]
    fn angle(&self) -> PyResult<f64> {
        self.0.manim_angle().map_err(engine_error)
    }

    fn scale(&self, factor: f64, scale_tips: bool) -> PyResult<()> {
        self.0.scale(factor, scale_tips).map_err(scale_error)
    }
}

pub(crate) fn scale_error(error: noon::ArrowScaleError) -> PyErr {
    match error {
        noon::ArrowScaleError::Authoring(error) => engine_error(error),
        other => engine_error(noon::integration::AuthoringFailure::unclassified(
            "arrow.scale",
            &other,
        )),
    }
}

#[pyfunction]
fn create_arrow(py: Python<'_>, store: &Store, request: ArrowRequest<'_>) -> PyResult<Py<PyAny>> {
    match request {
        ArrowRequest::Arrow(options) => {
            ManimArrow::create(Rc::clone(&store.semantics), options.0.clone())
                .map_err(engine_error)
                .and_then(|arrow| Py::new(py, ArrowHandle(arrow)).map(Py::into_any))
        }
        ArrowRequest::VectorField(options) => options
            .0
            .clone()
            .publish_detached(Rc::clone(&store.semantics))
            .map_err(engine_error)
            .and_then(|field| Py::new(py, VectorFieldHandle(field)).map(Py::into_any)),
    }
}

#[pyfunction]
fn linear_transformation_path_arc(values: Vec<f64>, rows: usize, columns: usize) -> PyResult<f64> {
    noon::linear_transformation_path_arc(&values, rows, columns).map_err(engine_error)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ArrowOptions>()?;
    module.add_class::<ArrowHandle>()?;
    module.add_class::<VectorFieldOptions>()?;
    module.add_class::<VectorFieldHandle>()?;
    module.add_function(wrap_pyfunction!(create_arrow, module)?)?;
    module.add_function(wrap_pyfunction!(linear_transformation_path_arc, module)?)
}
