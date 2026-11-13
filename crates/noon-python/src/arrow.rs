//! Native language projections of the shared Arrow family.

use crate::{context::Store, engine_error, family::FamilyHandle, mobject::MobjectHandle};
use noon::{ManimArrow, ManimArrowOptions, ManimLineEndpoints};
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
fn create_arrow(store: &Store, options: &ArrowOptions) -> PyResult<ArrowHandle> {
    ManimArrow::create(Rc::clone(&store.semantics), options.0.clone())
        .map(ArrowHandle)
        .map_err(engine_error)
}

#[pyfunction]
fn linear_transformation_path_arc(values: Vec<f64>, rows: usize, columns: usize) -> PyResult<f64> {
    noon::linear_transformation_path_arc(&values, rows, columns).map_err(engine_error)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ArrowOptions>()?;
    module.add_class::<ArrowHandle>()?;
    module.add_function(wrap_pyfunction!(create_arrow, module)?)?;
    module.add_function(wrap_pyfunction!(linear_transformation_path_arc, module)?)
}
