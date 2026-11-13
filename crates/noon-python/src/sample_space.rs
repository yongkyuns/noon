//! Native bindings for shared SampleSpace rectangles, families, and partitions.
use crate::{context::Store, engine_error, family::FamilyHandle, mobject::MobjectHandle};
use noon::{Color, SampleSpace, SampleSpaceError};
use pyo3::prelude::*;
use std::rc::Rc;

pub(crate) fn failure(error: impl std::fmt::Display) -> PyErr {
    engine_error(noon::integration::AuthoringFailure::new(
        "invalid_input",
        "sample_space.authoring",
        error,
    ))
}

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct SampleSpaceOptions(pub(crate) noon::SampleSpaceOptions);

#[pymethods]
impl SampleSpaceOptions {
    #[staticmethod]
    fn new(width: f64, height: f64) -> Self {
        Self(noon::SampleSpaceOptions {
            width,
            height,
            ..Default::default()
        })
    }

    #[pyo3(name = "setFillColor")]
    fn set_fill_color(&mut self, red: f64, green: f64, blue: f64) -> PyResult<()> {
        self.0.fill_color = color(red, green, blue, 1.0)?;
        Ok(())
    }

    #[pyo3(name = "setStrokeColor")]
    fn set_stroke_color(&mut self, red: f64, green: f64, blue: f64) -> PyResult<()> {
        self.0.stroke_color = color(red, green, blue, 1.0)?;
        Ok(())
    }

    #[pyo3(name = "setFillOpacity")]
    fn set_fill_opacity(&mut self, opacity: f64) {
        self.0.fill_opacity = opacity;
    }

    #[pyo3(name = "setStrokeWidth")]
    fn set_stroke_width(&mut self, width: f64) {
        self.0.stroke_width = width;
    }
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<Color> {
    noon::integration::family_color(true, red, green, blue, alpha)
        .map_err(failure)?
        .ok_or_else(|| failure("SampleSpace colors cannot be disabled"))
}

pub(crate) fn colors(values: Vec<f64>) -> PyResult<Vec<Color>> {
    if values.is_empty() || !values.len().is_multiple_of(4) {
        return Err(failure(
            "partition colors must contain one or more RGBA values",
        ));
    }
    values
        .chunks_exact(4)
        .map(|rgba| color(rgba[0], rgba[1], rgba[2], rgba[3]))
        .collect()
}

pub(crate) fn family(
    result: Result<noon::MobjectFamily, SampleSpaceError>,
) -> PyResult<FamilyHandle> {
    result
        .map(FamilyHandle::from_semantic_family)
        .map_err(failure)
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct SampleSpaceHandle(pub(crate) SampleSpace);

#[pymethods]
impl SampleSpaceHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn rectangle(&self) -> MobjectHandle {
        MobjectHandle::from_semantic_mobject(self.0.rectangle().clone())
    }

    #[pyo3(name = "horizontalParts")]
    fn horizontal_parts(&self) -> PyResult<Option<FamilyHandle>> {
        self.0
            .horizontal_parts()
            .map(|value| value.map(FamilyHandle::from_semantic_family))
            .map_err(failure)
    }

    #[pyo3(name = "verticalParts")]
    fn vertical_parts(&self) -> PyResult<Option<FamilyHandle>> {
        self.0
            .vertical_parts()
            .map(|value| value.map(FamilyHandle::from_semantic_family))
            .map_err(failure)
    }

    #[pyo3(name = "completePList")]
    fn complete_p_list(&self, probabilities: Vec<f64>) -> PyResult<Vec<f64>> {
        SampleSpace::complete_p_list(probabilities).map_err(failure)
    }

    #[pyo3(name = "getHorizontalDivision")]
    fn get_horizontal_division(
        &self,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<FamilyHandle> {
        family(
            self.0
                .get_horizontal_division(probabilities, &colors(rgba)?),
        )
    }

    #[pyo3(name = "getVerticalDivision")]
    fn get_vertical_division(
        &self,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<FamilyHandle> {
        family(self.0.get_vertical_division(probabilities, &colors(rgba)?))
    }

    #[pyo3(name = "divideHorizontally")]
    fn divide_horizontally(
        &mut self,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<FamilyHandle> {
        family(
            self.0
                .divide_horizontally_detached(probabilities, &colors(rgba)?),
        )
    }

    #[pyo3(name = "divideVertically")]
    fn divide_vertically(
        &mut self,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<FamilyHandle> {
        family(
            self.0
                .divide_vertically_detached(probabilities, &colors(rgba)?),
        )
    }
}

#[pyfunction]
fn create_detached_sample_space(
    store: &Store,
    options: &SampleSpaceOptions,
) -> PyResult<SampleSpaceHandle> {
    SampleSpace::detached(Rc::clone(&store.semantics), &options.0)
        .map(SampleSpaceHandle)
        .map_err(failure)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SampleSpaceOptions>()?;
    module.add_function(wrap_pyfunction!(create_detached_sample_space, module)?)
}
