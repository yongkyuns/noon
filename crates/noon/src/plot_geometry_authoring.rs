use crate::{Axes2DState, CoordinateSystemError, ParametricSamplePlan, PlotSamplingError};
use noon_core::{Vec2, VectorPath};
use noon_geometry::{smooth_cubic_path_from_subpaths, PathSmoothingError};

/// Compile sampled scene-space points into one ordinary retained `VectorPath`.
///
/// Function evaluation is authoring-time work. The returned path contains only
/// immutable geometry, so playback/seek never calls the source function again.
pub fn parametric_vector_path<F>(
    plan: &ParametricSamplePlan,
    mut point_from_parameter: F,
    use_smoothing: bool,
) -> Result<VectorPath, PlotGeometryError>
where
    F: FnMut(f64) -> Vec2,
{
    build_sampled_path(plan, use_smoothing, |parameter| {
        let point = point_from_parameter(parameter);
        if vec2_is_finite(point) {
            Ok(point)
        } else {
            Err(PlotGeometryError::NonFinitePoint { parameter, point })
        }
    })
}

/// Compile the initial Manim-compatible `Axes.plot(function)` subset.
///
/// Parameters come from [`ParametricSamplePlan`], `function` is evaluated once
/// per parameter, and [`Axes2DState`] owns the coordinate-to-scene mapping. With
/// `use_smoothing=true`, the mapped anchors pass through the shared Manim cubic
/// spline implementation in `noon-geometry`; otherwise they remain corners.
pub fn axes_function_vector_path<F>(
    axes: Axes2DState,
    plan: &ParametricSamplePlan,
    mut function: F,
    use_smoothing: bool,
) -> Result<VectorPath, PlotGeometryError>
where
    F: FnMut(f64) -> f64,
{
    build_sampled_path(plan, use_smoothing, |parameter| {
        let value = function(parameter);
        if !value.is_finite() {
            return Err(PlotGeometryError::NonFiniteFunctionValue { parameter, value });
        }
        Ok(axes.coords_to_point(parameter, value)?)
    })
}

/// Finish an `Axes.plot` after a host frontend evaluates the user callback.
///
/// Rust remains authoritative for parameter generation, sample cardinality,
/// coordinate mapping, finite-value validation, smoothing, and final geometry.
/// The host only supplies one scalar result for each parameter previously exposed
/// by [`ParametricSamplePlan::parameter_subpaths`].
pub fn axes_sampled_values_vector_path(
    axes: Axes2DState,
    plan: &ParametricSamplePlan,
    value_subpaths: &[Vec<f64>],
    use_smoothing: bool,
) -> Result<VectorPath, PlotGeometryError> {
    let parameter_subpaths = plan.parameter_subpaths()?;
    if value_subpaths.len() != parameter_subpaths.len() {
        return Err(PlotGeometryError::SampleSubpathCountMismatch {
            expected: parameter_subpaths.len(),
            actual: value_subpaths.len(),
        });
    }

    let mut point_subpaths = Vec::with_capacity(parameter_subpaths.len());
    for (subpath, (parameters, values)) in parameter_subpaths
        .iter()
        .zip(value_subpaths)
        .enumerate()
    {
        if values.len() != parameters.len() {
            return Err(PlotGeometryError::SampleValueCountMismatch {
                subpath,
                expected: parameters.len(),
                actual: values.len(),
            });
        }

        let mut points = Vec::with_capacity(parameters.len());
        for (&parameter, &value) in parameters.iter().zip(values) {
            if !value.is_finite() {
                return Err(PlotGeometryError::NonFiniteFunctionValue { parameter, value });
            }
            points.push(axes.coords_to_point(parameter, value)?);
        }
        point_subpaths.push(points);
    }

    finish_point_subpaths(&point_subpaths, use_smoothing)
}

fn build_sampled_path<F>(
    plan: &ParametricSamplePlan,
    use_smoothing: bool,
    mut point_from_parameter: F,
) -> Result<VectorPath, PlotGeometryError>
where
    F: FnMut(f64) -> Result<Vec2, PlotGeometryError>,
{
    let parameter_subpaths = plan.parameter_subpaths()?;
    let mut point_subpaths = Vec::with_capacity(parameter_subpaths.len());
    for parameters in parameter_subpaths {
        let mut points = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            points.push(point_from_parameter(parameter)?);
        }
        point_subpaths.push(points);
    }
    finish_point_subpaths(&point_subpaths, use_smoothing)
}

fn finish_point_subpaths(
    point_subpaths: &[Vec<Vec2>],
    use_smoothing: bool,
) -> Result<VectorPath, PlotGeometryError> {
    if use_smoothing {
        Ok(smooth_cubic_path_from_subpaths(point_subpaths)?)
    } else {
        Ok(corner_path_from_subpaths(point_subpaths))
    }
}

fn corner_path_from_subpaths(subpaths: &[Vec<Vec2>]) -> VectorPath {
    let mut path = VectorPath::new();
    for points in subpaths {
        let Some((&first, rest)) = points.split_first() else {
            continue;
        };
        path = path.move_to(first);
        for &point in rest {
            path = path.line_to(point);
        }
    }
    path
}

fn vec2_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlotGeometryError {
    Sampling(PlotSamplingError),
    Coordinates(CoordinateSystemError),
    Smoothing(PathSmoothingError),
    SampleSubpathCountMismatch {
        expected: usize,
        actual: usize,
    },
    SampleValueCountMismatch {
        subpath: usize,
        expected: usize,
        actual: usize,
    },
    NonFiniteFunctionValue { parameter: f64, value: f64 },
    NonFinitePoint { parameter: f64, point: Vec2 },
}

impl std::fmt::Display for PlotGeometryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sampling(error) => error.fmt(formatter),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Smoothing(error) => error.fmt(formatter),
            Self::SampleSubpathCountMismatch { expected, actual } => write!(
                formatter,
                "plot callback result has {actual} subpaths; expected {expected}"
            ),
            Self::SampleValueCountMismatch {
                subpath,
                expected,
                actual,
            } => write!(
                formatter,
                "plot callback result subpath {subpath} has {actual} values; expected {expected}"
            ),
            Self::NonFiniteFunctionValue { parameter, value } => write!(
                formatter,
                "plot function must return a finite value at {parameter}: {value}"
            ),
            Self::NonFinitePoint { parameter, point } => write!(
                formatter,
                "parametric function must return a finite point at {parameter}: ({}, {})",
                point.x, point.y
            ),
        }
    }
}

impl std::error::Error for PlotGeometryError {}

impl From<PlotSamplingError> for PlotGeometryError {
    fn from(value: PlotSamplingError) -> Self {
        Self::Sampling(value)
    }
}

impl From<CoordinateSystemError> for PlotGeometryError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<PathSmoothingError> for PlotGeometryError {
    fn from(value: PathSmoothingError) -> Self {
        Self::Smoothing(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NumberRange, SampleRange};
    use noon_core::PathCommand;

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() <= 1.0e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn test_axes() -> Axes2DState {
        Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap()
    }

    #[test]
    fn axes_function_maps_each_parameter_once_before_smoothing() {
        let axes = test_axes();
        let plan = ParametricSamplePlan::without_discontinuities(
            SampleRange::new(-1.0, 1.0, 1.0).unwrap(),
        );
        let mut calls = Vec::new();
        let path = axes_function_vector_path(
            axes,
            &plan,
            |x| {
                calls.push(x);
                x * x
            },
            true,
        )
        .unwrap();

        assert_eq!(calls, vec![-1.0, 0.0, 1.0]);
        assert_eq!(path.commands().len(), 3);
        match path.commands()[0] {
            PathCommand::MoveTo { to } => assert_point(to, Vec2::new(-1.0, 1.0)),
            ref other => panic!("expected MoveTo, got {other:?}"),
        }
        match path.commands()[1] {
            PathCommand::CubicTo { to, .. } => assert_point(to, Vec2::ZERO),
            ref other => panic!("expected CubicTo, got {other:?}"),
        }
        match path.commands()[2] {
            PathCommand::CubicTo { to, .. } => assert_point(to, Vec2::new(1.0, 1.0)),
            ref other => panic!("expected CubicTo, got {other:?}"),
        }
    }

    #[test]
    fn host_evaluated_samples_use_the_same_axes_mapping_and_smoothing() {
        let axes = test_axes();
        let plan = ParametricSamplePlan::without_discontinuities(
            SampleRange::new(-1.0, 1.0, 1.0).unwrap(),
        );
        let path = axes_sampled_values_vector_path(
            axes,
            &plan,
            &[vec![1.0, 0.0, 1.0]],
            true,
        )
        .unwrap();
        assert_eq!(path.commands().len(), 3);
        assert!(matches!(path.commands()[0], PathCommand::MoveTo { .. }));
        assert!(matches!(path.commands()[1], PathCommand::CubicTo { .. }));
        assert!(matches!(path.commands()[2], PathCommand::CubicTo { .. }));
    }

    #[test]
    fn host_evaluated_sample_shape_must_match_the_rust_plan() {
        let axes = test_axes();
        let plan = ParametricSamplePlan::without_discontinuities(
            SampleRange::new(-1.0, 1.0, 1.0).unwrap(),
        );
        assert_eq!(
            axes_sampled_values_vector_path(axes, &plan, &[], false).unwrap_err(),
            PlotGeometryError::SampleSubpathCountMismatch {
                expected: 1,
                actual: 0,
            }
        );
        assert_eq!(
            axes_sampled_values_vector_path(axes, &plan, &[vec![1.0]], false).unwrap_err(),
            PlotGeometryError::SampleValueCountMismatch {
                subpath: 0,
                expected: 3,
                actual: 1,
            }
        );
    }

    #[test]
    fn unsmoothed_discontinuities_remain_separate_corner_subpaths() {
        let plan = ParametricSamplePlan::new(
            SampleRange::new(-2.0, 2.0, 1.0).unwrap(),
            &[0.0],
            0.1,
        )
        .unwrap();
        let path = parametric_vector_path(
            &plan,
            |parameter| Vec2::new(parameter as f32, 0.0),
            false,
        )
        .unwrap();

        let move_count = path
            .commands()
            .iter()
            .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
            .count();
        assert_eq!(move_count, 2);
        assert!(path.commands().iter().all(|command| matches!(
            command,
            PathCommand::MoveTo { .. } | PathCommand::LineTo { .. }
        )));
    }

    #[test]
    fn non_finite_function_value_fails_before_coordinate_mapping() {
        let axes = Axes2DState::new(
            NumberRange::new(-1.0, 1.0, 1.0).unwrap(),
            NumberRange::new(-1.0, 1.0, 1.0).unwrap(),
            2.0,
            2.0,
        )
        .unwrap();
        let plan = ParametricSamplePlan::without_discontinuities(
            SampleRange::new(0.0, 1.0, 1.0).unwrap(),
        );
        let error = axes_function_vector_path(axes, &plan, |_| f64::INFINITY, true).unwrap_err();
        match error {
            PlotGeometryError::NonFiniteFunctionValue { parameter, value } => {
                assert_eq!(parameter, 0.0);
                assert!(value.is_infinite() && value.is_sign_positive());
            }
            other => panic!("expected non-finite function value error, got {other:?}"),
        }
    }

    #[test]
    fn non_finite_parametric_point_fails_before_geometry_creation() {
        let plan = ParametricSamplePlan::without_discontinuities(
            SampleRange::new(0.0, 1.0, 1.0).unwrap(),
        );
        let error = parametric_vector_path(&plan, |_| Vec2::new(f32::NAN, 0.0), false).unwrap_err();
        match error {
            PlotGeometryError::NonFinitePoint { parameter, point } => {
                assert_eq!(parameter, 0.0);
                assert!(point.x.is_nan());
                assert_eq!(point.y, 0.0);
            }
            other => panic!("expected non-finite point error, got {other:?}"),
        }
    }
}
