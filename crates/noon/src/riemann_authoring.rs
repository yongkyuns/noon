use crate::{
    CoordinateSystemError, IntoSnapshot, ParametricSamplePlan, PlotSamplingError, Rectangle,
    SampleRange, TransformedAxes2DState,
};
use noon_core::{ObjectSnapshot, Vec2};

pub const MANIM_DEFAULT_RIEMANN_DX: f64 = 0.1;
pub const MANIM_DEFAULT_RIEMANN_WIDTH_SCALE_FACTOR: f64 = 1.001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiemannSampleType {
    Left,
    Right,
    Center,
}

impl TryFrom<&str> for RiemannSampleType {
    type Error = RiemannAuthoringError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "center" => Ok(Self::Center),
            other => Err(RiemannAuthoringError::InvalidInputSampleType(
                other.to_owned(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiemannSample {
    x: f64,
    sample_x: f64,
    right_x: f64,
}

impl RiemannSample {
    pub const fn x(self) -> f64 {
        self.x
    }

    pub const fn sample_x(self) -> f64 {
        self.sample_x
    }

    pub const fn right_x(self) -> f64 {
        self.right_x
    }
}

/// Two-phase ManimCE v0.21 Riemann-rectangle plan.
///
/// Rust owns the deterministic half-open partition and all coordinate/rectangle
/// geometry. A host-language frontend evaluates authored graph callbacks only at
/// [`Self::graph_sample_x_values`] and an optional bounded graph only at
/// [`Self::baseline_x_values`], then returns those scalar y values to
/// [`Self::finish`]. This preserves Manim's exact authored-function fast path without
/// moving partition or geometry math into Python.
#[derive(Clone, Debug, PartialEq)]
pub struct RiemannSamplePlan {
    axes: TransformedAxes2DState,
    samples: Vec<RiemannSample>,
}

impl RiemannSamplePlan {
    pub fn new(
        axes: TransformedAxes2DState,
        x_min: f64,
        x_max: f64,
        dx: f64,
        input_sample_type: RiemannSampleType,
        width_scale_factor: f64,
    ) -> Result<Self, RiemannAuthoringError> {
        if !width_scale_factor.is_finite() {
            return Err(RiemannAuthoringError::InvalidWidthScaleFactor(
                width_scale_factor,
            ));
        }

        // Reuse the already-qualified Manim `np.arange` model from static plotting,
        // then remove the explicit endpoint that ParametricFunction appends. What
        // remains is exactly the half-open partition used by get_riemann_rectangles.
        let range = SampleRange::new(x_min, x_max, dx)?;
        let mut x_values = ParametricSamplePlan::without_discontinuities(range)
            .parameter_subpaths()?
            .pop()
            .unwrap_or_default();
        let endpoint = x_values.pop();
        debug_assert_eq!(endpoint, Some(x_max));

        let mut samples = Vec::new();
        samples
            .try_reserve_exact(x_values.len())
            .map_err(|_| RiemannAuthoringError::SampleAllocationFailed(x_values.len()))?;
        for x in x_values {
            let sample_x = match input_sample_type {
                RiemannSampleType::Left => x,
                RiemannSampleType::Right => x + dx,
                RiemannSampleType::Center => x + 0.5 * dx,
            };
            let right_x = x + width_scale_factor * dx;
            if !sample_x.is_finite() || !right_x.is_finite() {
                return Err(RiemannAuthoringError::NonFiniteSample {
                    x,
                    sample_x,
                    right_x,
                });
            }
            samples.push(RiemannSample {
                x,
                sample_x,
                right_x,
            });
        }
        Ok(Self { axes, samples })
    }

    pub fn samples(&self) -> &[RiemannSample] {
        &self.samples
    }

    pub fn graph_sample_x_values(&self) -> Vec<f64> {
        self.samples.iter().map(|sample| sample.sample_x).collect()
    }

    /// X values at which Manim evaluates an optional `bounded_graph` callback.
    /// These are the rectangle left edges, not the graph sample positions.
    pub fn baseline_x_values(&self) -> Vec<f64> {
        self.samples.iter().map(|sample| sample.x).collect()
    }

    pub fn finish(
        &self,
        graph_y_values: &[f64],
        bounded_graph_y_values: Option<&[f64]>,
    ) -> Result<Vec<RiemannRectangleGeometry>, RiemannAuthoringError> {
        self.validate_values("graph", graph_y_values)?;
        if let Some(values) = bounded_graph_y_values {
            self.validate_values("bounded graph", values)?;
        }

        let default_baseline_y = self
            .axes
            .axes()
            .y_axis()
            .range()
            .origin_shift();
        let mut rectangles = Vec::new();
        rectangles.try_reserve_exact(self.samples.len()).map_err(|_| {
            RiemannAuthoringError::SampleAllocationFailed(self.samples.len())
        })?;

        for (index, (sample, &graph_y)) in self
            .samples
            .iter()
            .zip(graph_y_values)
            .enumerate()
        {
            let baseline_y = bounded_graph_y_values
                .map(|values| values[index])
                .unwrap_or(default_baseline_y);
            let lower_left = self.axes.coords_to_point(sample.x, baseline_y)?;
            let lower_right = self.axes.coords_to_point(sample.right_x, baseline_y)?;
            let graph_point = self.axes.coords_to_point(sample.sample_x, graph_y)?;
            let snapshot = replacement_rectangle_snapshot([lower_left, lower_right, graph_point])?;
            rectangles.push(RiemannRectangleGeometry {
                snapshot,
                x: sample.x,
                sample_x: sample.sample_x,
                graph_y,
                baseline_y,
            });
        }
        Ok(rectangles)
    }

    fn validate_values(
        &self,
        source: &'static str,
        values: &[f64],
    ) -> Result<(), RiemannAuthoringError> {
        if values.len() != self.samples.len() {
            return Err(RiemannAuthoringError::ValueCountMismatch {
                source,
                expected: self.samples.len(),
                actual: values.len(),
            });
        }
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(RiemannAuthoringError::NonFiniteValue {
                source,
                index,
                value,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RiemannRectangleGeometry {
    snapshot: ObjectSnapshot,
    x: f64,
    sample_x: f64,
    graph_y: f64,
    baseline_y: f64,
}

impl RiemannRectangleGeometry {
    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }

    pub const fn x(&self) -> f64 {
        self.x
    }

    pub const fn sample_x(&self) -> f64 {
        self.sample_x
    }

    pub const fn graph_y(&self) -> f64 {
        self.graph_y
    }

    pub const fn baseline_y(&self) -> f64 {
        self.baseline_y
    }

    pub fn is_negative_signed_area(&self) -> bool {
        self.graph_y < self.baseline_y
    }

    pub fn into_snapshot(self) -> ObjectSnapshot {
        self.snapshot
    }
}

fn replacement_rectangle_snapshot(
    points: [Vec2; 3],
) -> Result<ObjectSnapshot, RiemannAuthoringError> {
    let min_x = points
        .iter()
        .map(|point| f64::from(point.x))
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| f64::from(point.x))
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| f64::from(point.y))
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| f64::from(point.y))
        .fold(f64::NEG_INFINITY, f64::max);
    let width = max_x - min_x;
    let height = max_y - min_y;
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    if !width.is_finite()
        || !height.is_finite()
        || !center_x.is_finite()
        || !center_y.is_finite()
        || width > f64::from(f32::MAX)
        || height > f64::from(f32::MAX)
        || center_x.abs() > f64::from(f32::MAX)
        || center_y.abs() > f64::from(f32::MAX)
    {
        return Err(RiemannAuthoringError::NonFiniteRectangleBounds);
    }
    Ok(Rectangle::new(width as f32, height as f32)
        .move_to(Vec2::new(center_x as f32, center_y as f32))
        .into_snapshot())
}

#[derive(Clone, Debug, PartialEq)]
pub enum RiemannAuthoringError {
    InvalidInputSampleType(String),
    InvalidWidthScaleFactor(f64),
    NonFiniteSample {
        x: f64,
        sample_x: f64,
        right_x: f64,
    },
    ValueCountMismatch {
        source: &'static str,
        expected: usize,
        actual: usize,
    },
    NonFiniteValue {
        source: &'static str,
        index: usize,
        value: f64,
    },
    NonFiniteRectangleBounds,
    SampleAllocationFailed(usize),
    Sampling(PlotSamplingError),
    Coordinates(CoordinateSystemError),
}

impl std::fmt::Display for RiemannAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInputSampleType(_) => formatter.write_str("Invalid input sample type"),
            Self::InvalidWidthScaleFactor(value) => write!(
                formatter,
                "Riemann width_scale_factor must be finite: {value}"
            ),
            Self::NonFiniteSample {
                x,
                sample_x,
                right_x,
            } => write!(
                formatter,
                "Riemann sample coordinates must be finite: x={x}, sample={sample_x}, right={right_x}"
            ),
            Self::ValueCountMismatch {
                source,
                expected,
                actual,
            } => write!(
                formatter,
                "Riemann {source} value count mismatch: expected {expected}, got {actual}"
            ),
            Self::NonFiniteValue {
                source,
                index,
                value,
            } => write!(
                formatter,
                "Riemann {source} value {index} must be finite: {value}"
            ),
            Self::NonFiniteRectangleBounds => {
                formatter.write_str("Riemann rectangle bounds are not representable")
            }
            Self::SampleAllocationFailed(count) => write!(
                formatter,
                "Riemann sample allocation failed for {count} rectangles"
            ),
            Self::Sampling(error) => error.fmt(formatter),
            Self::Coordinates(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RiemannAuthoringError {}

impl From<PlotSamplingError> for RiemannAuthoringError {
    fn from(value: PlotSamplingError) -> Self {
        Self::Sampling(value)
    }
}

impl From<CoordinateSystemError> for RiemannAuthoringError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Axes2DState, NumberRange};
    use noon_core::{GeometryRef, Transform2D};

    fn axes(transform: Transform2D) -> TransformedAxes2DState {
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        TransformedAxes2DState::new(axes, transform, transform)
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn half_open_partition_and_sample_modes_match_manim() {
        let axes = axes(Transform2D::IDENTITY);
        for (sample_type, expected) in [
            (RiemannSampleType::Left, vec![0.0, 0.25, 0.5, 0.75]),
            (RiemannSampleType::Right, vec![0.25, 0.5, 0.75, 1.0]),
            (RiemannSampleType::Center, vec![0.125, 0.375, 0.625, 0.875]),
        ] {
            let plan = RiemannSamplePlan::new(axes, 0.0, 1.0, 0.25, sample_type, 1.001)
                .unwrap();
            let actual = plan.graph_sample_x_values();
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert_close(actual, expected);
            }
        }
    }

    #[test]
    fn descending_and_away_steps_follow_arange_direction() {
        let axes = axes(Transform2D::IDENTITY);
        let descending = RiemannSamplePlan::new(
            axes,
            1.0,
            0.0,
            -0.4,
            RiemannSampleType::Left,
            1.001,
        )
        .unwrap();
        assert_eq!(descending.baseline_x_values(), vec![1.0, 0.6, 0.19999999999999996]);

        let empty = RiemannSamplePlan::new(
            axes,
            0.0,
            1.0,
            -0.1,
            RiemannSampleType::Left,
            1.001,
        )
        .unwrap();
        assert!(empty.samples().is_empty());
    }

    #[test]
    fn finish_builds_world_axis_aligned_replacement_rectangles() {
        let transform = Transform2D {
            translation: Vec2::new(1.0, -0.5),
            rotation: 0.25,
            scale: Vec2::new(1.2, 1.2),
        };
        let axes = axes(transform);
        let plan = RiemannSamplePlan::new(
            axes,
            0.0,
            1.0,
            0.5,
            RiemannSampleType::Center,
            1.001,
        )
        .unwrap();
        let rectangles = plan.finish(&[1.0, -0.5], None).unwrap();
        assert_eq!(rectangles.len(), 2);
        assert!(!rectangles[0].is_negative_signed_area());
        assert!(rectangles[1].is_negative_signed_area());
        for rectangle in rectangles {
            assert!(matches!(rectangle.snapshot().geometry, GeometryRef::Rectangle { .. }));
        }
    }

    #[test]
    fn bounded_baseline_uses_left_x_values_and_drives_signed_area_metadata() {
        let axes = axes(Transform2D::IDENTITY);
        let plan = RiemannSamplePlan::new(
            axes,
            -1.0,
            1.0,
            1.0,
            RiemannSampleType::Right,
            1.0,
        )
        .unwrap();
        assert_eq!(plan.graph_sample_x_values(), vec![0.0, 1.0]);
        assert_eq!(plan.baseline_x_values(), vec![-1.0, 0.0]);
        let rectangles = plan.finish(&[0.5, -0.5], Some(&[0.25, -1.0])).unwrap();
        assert!(!rectangles[0].is_negative_signed_area());
        assert!(!rectangles[1].is_negative_signed_area());
        assert_close(rectangles[0].baseline_y(), 0.25);
        assert_close(rectangles[1].baseline_y(), -1.0);
    }

    #[test]
    fn invalid_sample_type_and_value_cardinality_fail_closed() {
        assert_eq!(
            RiemannSampleType::try_from("other").unwrap_err(),
            RiemannAuthoringError::InvalidInputSampleType("other".to_owned())
        );
        let plan = RiemannSamplePlan::new(
            axes(Transform2D::IDENTITY),
            0.0,
            1.0,
            0.5,
            RiemannSampleType::Left,
            1.001,
        )
        .unwrap();
        assert!(matches!(
            plan.finish(&[1.0], None),
            Err(RiemannAuthoringError::ValueCountMismatch {
                source: "graph",
                expected: 2,
                actual: 1
            })
        ));
    }
}
