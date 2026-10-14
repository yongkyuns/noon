//! Native projections of immutable shared Rust path observations.
use crate::engine_error;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct PathQuery {
    value: Option<noon::PathQuery>,
}
impl PathQuery {
    pub(crate) fn new(value: noon::PathQuery) -> Self {
        Self { value: Some(value) }
    }
    fn value(&self) -> PyResult<&noon::PathQuery> {
        self.value
            .as_ref()
            .ok_or_else(|| engine_error("path query has been freed"))
    }
}
fn coordinates(points: impl IntoIterator<Item = (f64, f64)>) -> Vec<f64> {
    points.into_iter().flat_map(|(x, y)| [x, y]).collect()
}
#[pymethods]
impl PathQuery {
    fn free(&mut self) {
        self.value.take();
    }
    #[getter(curveCount)]
    fn curve_count(&self) -> PyResult<usize> {
        Ok(self.value()?.curve_count())
    }
    #[pyo3(name = "curvePoints")]
    fn curve_points(&self, index: usize) -> PyResult<Vec<f64>> {
        self.value()?
            .curve_points(index)
            .map(coordinates)
            .map_err(engine_error)
    }
    #[pyo3(name = "isClosed")]
    fn is_closed(&self) -> PyResult<bool> {
        self.value()?.is_closed().map_err(engine_error)
    }
    fn subpaths(&self) -> PyResult<Vec<Vec<f64>>> {
        Ok(self
            .value()?
            .subpaths()
            .into_iter()
            .map(coordinates)
            .collect())
    }
    #[pyo3(name = "vertexGroups")]
    fn vertex_groups(&self) -> PyResult<Vec<Vec<f64>>> {
        Ok(self
            .value()?
            .vertex_groups()
            .into_iter()
            .map(coordinates)
            .collect())
    }
    #[pyo3(name = "startAnchors")]
    fn start_anchors(&self) -> PyResult<Vec<f64>> {
        Ok(coordinates(self.value()?.start_anchors()))
    }
    #[pyo3(name = "endAnchors")]
    fn end_anchors(&self) -> PyResult<Vec<f64>> {
        Ok(coordinates(self.value()?.end_anchors()))
    }
    #[pyo3(name = "firstHandles")]
    fn first_handles(&self) -> PyResult<Vec<f64>> {
        Ok(coordinates(self.value()?.first_handles()))
    }
    #[pyo3(name = "secondHandles")]
    fn second_handles(&self) -> PyResult<Vec<f64>> {
        Ok(coordinates(self.value()?.second_handles()))
    }
    fn anchors(&self) -> PyResult<Vec<f64>> {
        Ok(coordinates(self.value()?.anchors()))
    }
    #[pyo3(name = "pointFromProportion")]
    fn point_from_proportion(&self, alpha: f64) -> PyResult<(f64, f64)> {
        self.value()?
            .point_from_proportion(alpha)
            .map_err(engine_error)
    }
    fn start(&self) -> PyResult<(f64, f64)> {
        self.value()?.start().map_err(engine_error)
    }
    fn end(&self) -> PyResult<(f64, f64)> {
        self.value()?.end().map_err(engine_error)
    }
    #[pyo3(name = "arcLength", signature = (samples=None))]
    fn arc_length(&self, samples: Option<usize>) -> PyResult<f64> {
        self.value()?.arc_length(samples).map_err(engine_error)
    }
}
