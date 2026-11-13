//! Decode native argument shapes; shared Rust validates and publishes path values.
use crate::engine_error;
use noon::{SemanticVec3, Vec2};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
pub(crate) fn point_3d(x: f64, y: f64, z: f64) -> PyResult<SemanticVec3> {
    let point = SemanticVec3::new(x, y, z);
    point
        .lower_xy_f32()
        .map_err(noon::AuthoringError::from)
        .map_err(engine_error)?;
    Ok(point)
}
pub(crate) fn points_3d(values: &[f64]) -> PyResult<Vec<SemanticVec3>> {
    if !values.len().is_multiple_of(3) {
        return Err(engine_error(noon::AuthoringError::InvalidPointCoordinates(
            values.len(),
        )));
    }
    Ok(values
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| SemanticVec3::new(v[0], v[1], v[2]))
        .collect())
}
pub(crate) fn points_f64(values: &[f64]) -> PyResult<Vec<(f64, f64)>> {
    if !values.len().is_multiple_of(2) {
        return Err(engine_error(noon::AuthoringError::InvalidPointCoordinates(
            values.len(),
        )));
    }
    values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| {
            noon::integration::authoring_xy_f64(v[0], v[1])
                .map(|v| (v.x, v.y))
                .map_err(engine_error)
        })
        .collect()
}
pub(crate) fn points(values: &[f64]) -> PyResult<Vec<Vec2>> {
    points_f64(values)?
        .into_iter()
        .map(|(x, y)| noon::integration::authoring_path_point(x, y).map_err(engine_error))
        .collect()
}
pub(crate) fn cubic_curves_3d(values: &[f64]) -> PyResult<Vec<[SemanticVec3; 4]>> {
    let points = points_3d(values)?;
    if !points.len().is_multiple_of(4) {
        return Err(engine_error(
            "cubic Bézier point data must contain complete groups of four",
        ));
    }
    Ok(points
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| [v[0], v[1], v[2], v[3]])
        .collect())
}
pub(crate) fn point_array_shape(values: &[f64]) -> PyResult<noon_core::PointArrayShape> {
    values
        .iter()
        .map(|&v| {
            if v.is_finite() && v >= 0. && v.fract() == 0. && v <= f64::from(u32::MAX) {
                Ok(v as usize)
            } else {
                Err(noon_core::PointArrayShapeError::InvalidDimension)
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .and_then(noon_core::PointArrayShape::new)
        .map_err(noon::AuthoringError::from)
        .map_err(engine_error)
}
pub(crate) fn point_reduction_tree(
    py: Python<'_>,
    tree: noon::PointReductionTree,
) -> PyResult<Bound<'_, PyTuple>> {
    let points = PyList::new(py, flat_points_3d(tree.points))?;
    let children = tree
        .children
        .into_iter()
        .map(|child| point_reduction_tree(py, child))
        .collect::<PyResult<Vec<_>>>()?;
    let children = PyList::new(py, children)?;
    PyTuple::new(py, [points, children])
}

pub(crate) fn flat_points_3d(points: Vec<SemanticVec3>) -> Vec<f64> {
    points.into_iter().flat_map(|v| [v.x, v.y, v.z]).collect()
}

pub(crate) fn point_array(value: noon::PointArrayObservation) -> (Vec<f64>, Vec<usize>) {
    let (points, shape) = value.into_parts();
    (flat_points_3d(points), shape.dimensions().to_vec())
}
