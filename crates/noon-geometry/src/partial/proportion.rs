//! Prepared, derived path measure; authored raw points keep semantic precision.
use super::{
    collect_curves, cubic_bezier_point_f64, curve_controls, curve_point_f64,
    validate_proportion_f64, Curve, PathProportionError, MANIM_LENGTH_SAMPLE_POINTS,
};
use noon_core::{SemanticVec3, Vec2, VectorPath};

#[derive(Clone, Debug)]
enum MeasuredCurves {
    Commands(Vec<Curve>),
    Raw(Vec<[(f64, f64); 4]>),
}

impl MeasuredCurves {
    fn len(&self) -> usize {
        match self {
            Self::Commands(curves) => curves.len(),
            Self::Raw(curves) => curves.len(),
        }
    }
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Commands(curves) => std::mem::size_of_val(curves.as_slice()),
            Self::Raw(curves) => std::mem::size_of_val(curves.as_slice()),
        }
    }
    fn get(&self, index: usize) -> Option<MeasuredCurve> {
        match self {
            Self::Commands(curves) => curves.get(index).copied().map(MeasuredCurve::Command),
            Self::Raw(curves) => curves.get(index).copied().map(MeasuredCurve::Raw),
        }
    }
    fn iter(&self) -> impl Iterator<Item = MeasuredCurve> + '_ {
        (0..self.len()).map(|index| self.get(index).unwrap())
    }
}

#[derive(Clone, Copy)]
enum MeasuredCurve {
    Command(Curve),
    Raw([(f64, f64); 4]),
}
impl MeasuredCurve {
    fn controls(self) -> [SemanticVec3; 4] {
        match self {
            Self::Command(curve) => curve_controls(curve),
            Self::Raw(points) => points.map(|(x, y)| SemanticVec3::new(x, y, 0.0)),
        }
    }
    fn point(self, parameter: f64) -> SemanticVec3 {
        match self {
            Self::Command(curve) => curve_point_f64(curve, parameter),
            Self::Raw(_) => cubic_bezier_point_f64(self.controls(), parameter),
        }
    }
}

/// Reusable ManimCE v0.21 path-proportion measure for repeated point queries.
///
/// Construction collects drawable curves and performs Manim's ten-sample Bezier
/// length approximation exactly once. [`Self::point`] then reuses those immutable
/// sampled lengths, avoiding repeated curve collection, allocation, and length
/// sampling during animation playback. Finite paths additionally keep cumulative
/// curve lengths so each point query selects its curve in logarithmic time.
#[derive(Clone, Debug)]
pub struct PathProportionPlan {
    curves: MeasuredCurves,
    lengths: Vec<f64>,
    cumulative_lengths: Vec<f64>,
    total_length: f64,
    scale: (f64, f64),
}

impl PathProportionPlan {
    /// Logical heap held by this prepared measure, excluding allocator bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.curves.retained_bytes()
            + std::mem::size_of_val(self.lengths.as_slice())
            + std::mem::size_of_val(self.cumulative_lengths.as_slice())
    }

    pub fn curve_count(&self) -> usize {
        self.curves.len()
    }

    /// Sample one curve into the straight-line length pieces used by ManimCE.
    /// `sample_points` is the number of Bezier evaluations, so the result has
    /// `sample_points - 1` entries when there are at least two samples.
    pub fn curve_length_pieces(
        &self,
        index: usize,
        sample_points: usize,
    ) -> Result<Vec<f64>, PathProportionError> {
        let curve = self
            .curves
            .get(index)
            .ok_or(PathProportionError::InvalidCurveIndex {
                index,
                count: self.curves.len(),
            })?;
        let mut pieces = Vec::new();
        if sample_points < 2 {
            return Ok(pieces);
        }
        pieces
            .try_reserve(sample_points - 1)
            .map_err(|_| PathProportionError::InvalidSampleCount)?;
        let step = 1.0 / (sample_points - 1) as f64;
        let mut previous = curve.point(0.0);
        for sample in 1..sample_points {
            let point = curve.point(if sample + 1 == sample_points {
                1.0
            } else {
                sample as f64 * step
            });
            pieces.push(
                ((point.x - previous.x) * self.scale.0)
                    .hypot((point.y - previous.y) * self.scale.1),
            );
            previous = point;
        }
        Ok(pieces)
    }

    /// Cubic controls for one retained curve, promoting lines and quadratics
    /// exactly. Points remain in local content space regardless of query metric.
    pub fn curve_points(&self, index: usize) -> Result<[SemanticVec3; 4], PathProportionError> {
        let curve = self
            .curves
            .get(index)
            .ok_or(PathProportionError::InvalidCurveIndex {
                index,
                count: self.curves.len(),
            })?;
        Ok(curve.controls())
    }

    /// Evaluate one retained curve at each supplied parameter. Parameters are
    /// intentionally not clamped: Manim's Bezier functions extrapolate for
    /// values outside the unit interval.
    pub fn curve_points_at(
        &self,
        index: usize,
        parameters: &[f64],
    ) -> Result<Vec<SemanticVec3>, PathProportionError> {
        let curve = self
            .curves
            .get(index)
            .ok_or(PathProportionError::InvalidCurveIndex {
                index,
                count: self.curves.len(),
            })?;
        Ok(parameters
            .iter()
            .map(|&parameter| curve.point(parameter))
            .collect())
    }

    /// Prepare the reusable local-space proportion measure for `path`.
    pub fn new(path: &VectorPath) -> Result<Self, PathProportionError> {
        Self::with_scale(path, (1.0, 1.0))
    }

    /// Prepare a measure in the metric induced by a planar scale.
    /// Rotation and translation preserve lengths; nonuniform scale changes
    /// which curve contains a given proportion. Points remain in local space.
    pub fn with_scale(path: &VectorPath, scale: (f64, f64)) -> Result<Self, PathProportionError> {
        if !scale.0.is_finite() || !scale.1.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        let curves = match path.manim_points() {
            Some(points) => MeasuredCurves::Raw(points.as_chunks::<4>().0.to_vec()),
            None => MeasuredCurves::Commands(collect_curves(path)),
        };
        if curves.len() == 0 {
            return Err(PathProportionError::EmptyPath);
        }
        let lengths: Vec<_> = curves
            .iter()
            .map(|curve| sampled_curve_length(curve, scale, MANIM_LENGTH_SAMPLE_POINTS))
            .collect();
        let mut cumulative_lengths = Vec::with_capacity(lengths.len());
        let mut total_length = 0.0;
        for &length in &lengths {
            total_length += length;
            cumulative_lengths.push(total_length);
        }
        if !total_length.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        Ok(Self {
            curves,
            lengths,
            cumulative_lengths,
            total_length,
            scale,
        })
    }

    /// Return a local point using this plan's metric and renderer precision.
    pub fn point(&self, alpha: f32) -> Result<Vec2, PathProportionError> {
        let point = self.point_f64(f64::from(alpha))?;
        Ok(Vec2::new(point.x as f32, point.y as f32))
    }

    /// Return a local point without quantizing proportion or interpolation.
    pub fn point_f64(&self, alpha: f64) -> Result<SemanticVec3, PathProportionError> {
        validate_proportion_f64(alpha)?;
        if alpha == 1.0 {
            return Ok(self.curves.get(self.curves.len() - 1).unwrap().controls()[3]);
        }
        let target = alpha * self.total_length;
        let index = self
            .cumulative_lengths
            .partition_point(|&end| end < target)
            .min(self.curves.len() - 1);
        let previous = index
            .checked_sub(1)
            .map_or(0.0, |i| self.cumulative_lengths[i]);
        let residue = if self.lengths[index] > 0.0 {
            ((target - previous) / self.lengths[index]).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Ok(self.curves.get(index).unwrap().point(residue))
    }

    /// Approximate arc length in this plan's metric. The default ten-sample
    /// measure is retained; an explicit sample count only visits this path.
    pub fn arc_length(&self, sample_points: Option<usize>) -> Result<f64, PathProportionError> {
        let samples = sample_points.unwrap_or(MANIM_LENGTH_SAMPLE_POINTS);
        if samples < 2 {
            return Ok(0.0);
        }
        if samples == MANIM_LENGTH_SAMPLE_POINTS {
            return Ok(self.total_length);
        }
        let length: f64 = self
            .curves
            .iter()
            .map(|curve| sampled_curve_length(curve, self.scale, samples))
            .sum();
        if !length.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        Ok(length)
    }
}

fn sampled_curve_length(curve: MeasuredCurve, scale: (f64, f64), samples: usize) -> f64 {
    let step = 1.0 / (samples - 1) as f64;
    let mut previous = curve.point(0.0);
    let mut length = 0.0;
    for sample in 1..samples {
        let point = curve.point(if sample + 1 == samples {
            1.0
        } else {
            sample as f64 * step
        });
        length += ((point.x - previous.x) * scale.0).hypot((point.y - previous.y) * scale.1);
        previous = point;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_proportion_plan_reuses_the_sampled_measure() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0));
        let plan = PathProportionPlan::new(&path).unwrap();

        assert_eq!(plan.curves.len(), 2);
        assert_eq!(plan.lengths, vec![3.0, 1.0]);
        assert_eq!(plan.cumulative_lengths, vec![3.0, 4.0]);
        assert_eq!(plan.total_length, 4.0);
        assert_eq!(plan.point(0.25).unwrap(), Vec2::new(1.0, 0.0));
        assert_eq!(plan.point(0.5).unwrap(), Vec2::new(2.0, 0.0));
        assert_eq!(plan.point(0.875).unwrap(), Vec2::new(3.0, 0.5));
    }

    #[test]
    fn prepared_proportion_plan_preserves_first_curve_at_exact_length_boundary() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0));
        let plan = PathProportionPlan::new(&path).unwrap();

        assert_eq!(plan.point(0.75).unwrap(), Vec2::new(3.0, 0.0));
        assert_eq!(
            plan.cumulative_lengths
                .partition_point(|&end_length| end_length < 3.0),
            0
        );
    }
}
