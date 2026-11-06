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
fn point_groups(groups: Vec<Vec<(f64, f64)>>) -> Vec<Vec<f64>> {
    groups.into_iter().map(coordinates).collect()
}
fn flat_points(points: Vec<(f64, f64)>) -> Vec<f64> {
    coordinates(points)
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl PathQuery {
    fn free(&mut self) {
        self.value.take();
    }

    #[getter(pointCount)]
    pub fn point_count(&self) -> PyResult<u32> {
        Ok(self.value()?.point_count() as u32)
    }
    pub fn points(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .points()
            .map(flat_points)
            .map_err(engine_error)
    }
    #[pyo3(name = "points3D")]
    pub fn points_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .points_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }
    #[getter(curveCount)]
    pub fn curve_count(&self) -> PyResult<u32> {
        Ok(self.value()?.curve_count() as u32)
    }
    #[pyo3(name = "hasNewPathStarted")]
    pub fn has_new_path_started(&self, stride: Option<i32>) -> PyResult<bool> {
        Ok(self
            .value()?
            .has_new_path_started_with_stride(stride.unwrap_or(4)))
    }
    pub fn direction(&self) -> PyResult<String> {
        Ok(self.value()?.direction().to_owned())
    }
    #[pyo3(name = "directionWithStride")]
    pub fn direction_with_stride(&self, stride: i32) -> Result<String, PyErr> {
        self.value()?
            .direction_with_stride(stride)
            .map(str::to_owned)
            .ok_or_else(|| engine_error("slice step cannot be zero"))
    }
    #[pyo3(name = "curvePoints")]
    pub fn curve_points(&self, index: u32) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .curve_points(index as usize)
            .map(|points| points.into_iter().flat_map(|(x, y)| [x, y]).collect())
            .map_err(engine_error)
    }
    #[pyo3(name = "curvePointsAt")]
    pub fn curve_points_at(&self, index: u32, parameters: Vec<f64>) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .curve_points_at(index as usize, &parameters)
            .map(|points| points.into_iter().flat_map(|(x, y, z)| [x, y, z]).collect())
            .map_err(engine_error)
    }
    #[pyo3(name = "bezierPointsAtRange")]
    pub fn bezier_points_at_range(
        &self,
        start: u32,
        stop: u32,
        parameters: Vec<f64>,
    ) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .bezier_points_at_range(start as usize, stop as usize, &parameters)
            .map(|points| points.into_iter().flat_map(|(x, y, z)| [x, y, z]).collect())
            .map_err(engine_error)
    }
    #[pyo3(name = "bezierParametersForPointRange")]
    pub fn bezier_parameters_for_point_range(
        &self,
        start: u32,
        stop: u32,
        point_x: f64,
        point_y: f64,
        point_z: f64,
    ) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .bezier_parameters_for_point_range(
                start as usize,
                stop as usize,
                (point_x, point_y, point_z),
            )
            .map_err(engine_error)
    }
    #[pyo3(name = "bezierLengthPiecesRange")]
    pub fn bezier_length_pieces_range(
        &self,
        start: u32,
        stop: u32,
        sample_points: Option<u32>,
    ) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .bezier_length_pieces_range(
                start as usize,
                stop as usize,
                sample_points.map(|n| n as usize),
            )
            .map_err(engine_error)
    }
    #[pyo3(name = "curveLengthPieces")]
    pub fn curve_length_pieces(
        &self,
        index: u32,
        sample_points: Option<u32>,
    ) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .curve_length_pieces(index as usize, sample_points.map(|n| n as usize))
            .map_err(engine_error)
    }
    #[pyo3(name = "curveLength")]
    pub fn curve_length(&self, index: u32, sample_points: Option<u32>) -> Result<f64, PyErr> {
        self.value()?
            .curve_length(index as usize, sample_points.map(|n| n as usize))
            .map_err(engine_error)
    }
    #[pyo3(name = "isClosed")]
    pub fn is_closed(&self, tolerance: f64) -> Result<bool, PyErr> {
        self.value()?
            .is_closed_with_tolerance(tolerance)
            .map_err(engine_error)
    }
    pub fn subpaths(&self) -> PyResult<Vec<Vec<f64>>> {
        Ok(point_groups(self.value()?.subpaths()))
    }
    #[pyo3(name = "vertexGroups")]
    pub fn vertex_groups(&self) -> PyResult<Vec<Vec<f64>>> {
        Ok(point_groups(self.value()?.vertex_groups()))
    }
    #[pyo3(name = "startAnchors3D")]
    pub fn start_anchors_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .start_anchors_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "firstHandles3D")]
    pub fn first_handles_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .first_handles_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "secondHandles3D")]
    pub fn second_handles_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .second_handles_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "endAnchors3D")]
    pub fn end_anchors_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .end_anchors_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "anchors3D")]
    pub fn anchors_3d(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .anchors_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "startAnchors")]
    pub fn start_anchors(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .value()?
            .start_anchors()
            .into_iter()
            .flat_map(|(x, y)| [x, y])
            .collect())
    }
    #[pyo3(name = "endAnchors")]
    pub fn end_anchors(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .value()?
            .end_anchors()
            .into_iter()
            .flat_map(|(x, y)| [x, y])
            .collect())
    }
    #[pyo3(name = "firstHandles")]
    pub fn first_handles(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .value()?
            .first_handles()
            .into_iter()
            .flat_map(|(x, y)| [x, y])
            .collect())
    }
    #[pyo3(name = "secondHandles")]
    pub fn second_handles(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .value()?
            .second_handles()
            .into_iter()
            .flat_map(|(x, y)| [x, y])
            .collect())
    }
    pub fn anchors(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .anchors()
            .map(flat_points)
            .map_err(engine_error)
    }

    #[pyo3(name = "pointFromProportion")]
    pub fn point_from_proportion(&self, alpha: f64) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .point_from_proportion(alpha)
            .map(|(x, y)| vec![x, y])
            .map_err(engine_error)
    }
    #[pyo3(name = "proportionFromPoint")]
    pub fn proportion_from_point(&self, x: f64, y: f64, z: f64) -> Result<Option<f64>, PyErr> {
        self.value()?
            .proportion_from_point((x, y, z))
            .map_err(engine_error)
    }
    #[pyo3(name = "proportionFromPointForRanges")]
    pub fn proportion_from_point_for_ranges(
        &self,
        ranges: Vec<u32>,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<Option<f64>, PyErr> {
        if !ranges.len().is_multiple_of(2) {
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "invalid_input",
                "path_query.invalid_curve_ranges",
                "curve ranges require start/stop index pairs",
            )));
        }
        let ranges = ranges
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (pair[0] as usize, pair[1] as usize))
            .collect::<Vec<_>>();
        self.value()?
            .proportion_from_point_in_ranges(&ranges, (x, y, z))
            .map_err(engine_error)
    }
    pub fn start(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .start()
            .map(|(x, y)| vec![x, y])
            .map_err(engine_error)
    }
    pub fn end(&self) -> Result<Vec<f64>, PyErr> {
        self.value()?
            .end()
            .map(|(x, y)| vec![x, y])
            .map_err(engine_error)
    }
    #[pyo3(name = "arcLength")]
    pub fn arc_length(&self, samples: Option<u32>) -> Result<f64, PyErr> {
        self.value()?
            .arc_length(samples.map(|n| n as usize))
            .map_err(engine_error)
    }
}
