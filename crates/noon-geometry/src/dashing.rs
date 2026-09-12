use noon_core::{PathCommand, SemanticVec3, VectorPath};

use crate::{subcurve_path, PathProportionError, PathProportionPlan};

const MANIM_LENGTH_SAMPLE_POINTS: usize = 10;

/// ManimCE v0.21 dash intervals in global VMobject curve-parameter space.
///
/// Callers validate finite `dashed_ratio`/`dash_offset` inputs. Closed paths use
/// one gap per dash; open paths start and end with a dash. `dash_offset` is in
/// units of one complete dash+gap period, matching Manim's constructor.
pub fn dash_intervals(
    num_dashes: usize,
    dashed_ratio: f64,
    dash_offset: f64,
    closed: bool,
) -> Vec<(f64, f64)> {
    if num_dashes == 0 {
        return Vec::new();
    }
    debug_assert!(dashed_ratio.is_finite());
    debug_assert!((0.0..=1.0).contains(&dashed_ratio));
    debug_assert!(dash_offset.is_finite());

    let count = num_dashes as f64;
    let dash_len = dashed_ratio / count;
    let void_len = if closed {
        (1.0 - dashed_ratio) / count
    } else if num_dashes == 1 {
        1.0 - dashed_ratio
    } else {
        (1.0 - dashed_ratio) / (count - 1.0)
    };
    let period = dash_len + void_len;
    let phase_shift = dash_offset.rem_euclid(1.0) * period;
    let pattern_len = if closed { 1.0 } else { 1.0 + void_len };

    let mut starts = Vec::with_capacity(num_dashes + 1);
    let mut ends = Vec::with_capacity(num_dashes + 1);
    for index in 0..num_dashes {
        let origin = index as f64 * period + phase_shift;
        starts.push(origin.rem_euclid(pattern_len));
        ends.push((origin + dash_len).rem_euclid(pattern_len));
    }

    // Pinned Manim gives open paths special handling at the trailing overflow so
    // their pattern still starts/ends on visible dash geometry. Manim's VGroup
    // representation may also append a redundant zero-length seam child when a
    // solid pattern lands exactly on 1.0. Noon retains visible path geometry rather
    // than dash-submobject identity, so omit only that extra zero-length seam child.
    if !closed {
        let last = starts.len() - 1;
        if ends[last] > 1.0 && starts[last] > 1.0 {
            starts.pop();
            ends.pop();
        } else if ends[last] < dash_len {
            if starts[last] < 1.0 {
                let wrapped_end = ends[last];
                ends[last] = 1.0;
                if wrapped_end != 0.0 {
                    starts.push(0.0);
                    ends.push(wrapped_end);
                }
            } else {
                starts[last] = 0.0;
            }
        } else if starts[last] > 1.0 - dash_len {
            ends[last] = 1.0;
        }
    }

    starts.into_iter().zip(ends).collect()
}

/// Convert one retained path into ordinary retained dash subpaths.
///
/// No renderer dash primitive is introduced. `equal_lengths=false` uses the
/// global curve-count parameter directly. `equal_lengths=true` mirrors Manim's
/// ten-sample-per-curve cumulative-length interpolation before selecting each
/// retained subcurve.
pub fn dashed_path(
    path: &VectorPath,
    closed: bool,
    num_dashes: usize,
    dashed_ratio: f64,
    dash_offset: f64,
    equal_lengths: bool,
) -> Result<VectorPath, PathProportionError> {
    let intervals = dash_intervals(num_dashes, dashed_ratio, dash_offset, closed);
    if intervals.is_empty() {
        return Ok(VectorPath::new());
    }

    let parameter_map = if equal_lengths {
        SampledLengthParameterMap::new(path)?
    } else {
        None
    };
    // A path with no complete curves remains an empty dashed object, matching
    // the observable result of dashing an empty VMobject.
    if equal_lengths && parameter_map.is_none() {
        return Ok(VectorPath::new());
    }

    let mut result = VectorPath::new();
    for (start, end) in intervals {
        let (start, end) = if let Some(map) = parameter_map.as_ref() {
            (map.parameter(start), map.parameter(end))
        } else {
            (start, end)
        };
        let selected = subcurve_path(path, start as f32, end as f32)?;
        result = append_path(result, &selected);
    }
    Ok(result)
}

#[derive(Clone, Debug)]
struct SampledLengthParameterMap {
    cumulative: Vec<f64>,
    total: f64,
}

impl SampledLengthParameterMap {
    fn new(path: &VectorPath) -> Result<Option<Self>, PathProportionError> {
        let plan = match PathProportionPlan::new(path) {
            Ok(plan) => plan,
            Err(PathProportionError::EmptyPath) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut cumulative =
            Vec::with_capacity(1 + plan.curve_count() * (MANIM_LENGTH_SAMPLE_POINTS - 1));
        cumulative.push(0.0);
        let mut total = 0.0;
        for curve in 0..plan.curve_count() {
            let controls = plan.curve_points(curve)?;
            let mut previous = controls[0];
            for sample in 1..MANIM_LENGTH_SAMPLE_POINTS {
                let t = sample as f64 / (MANIM_LENGTH_SAMPLE_POINTS - 1) as f64;
                let point = cubic_point(controls, t);
                total += (point.x - previous.x).hypot(point.y - previous.y);
                cumulative.push(total);
                previous = point;
            }
        }
        if !total.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        Ok(Some(Self { cumulative, total }))
    }

    fn parameter(&self, fraction: f64) -> f64 {
        if self.total == 0.0 {
            // numpy.interp over an all-zero xp array resolves x=0 to the final
            // sample; retain that degenerate-path behavior.
            return 1.0;
        }
        let target = fraction * self.total;
        if target >= self.total {
            return 1.0;
        }
        let upper = self.cumulative.partition_point(|&length| length <= target);
        if upper == 0 {
            return 0.0;
        }
        if upper >= self.cumulative.len() {
            return 1.0;
        }
        let lower = upper - 1;
        let lower_length = self.cumulative[lower];
        let upper_length = self.cumulative[upper];
        let denominator = (self.cumulative.len() - 1) as f64;
        let lower_parameter = lower as f64 / denominator;
        let upper_parameter = upper as f64 / denominator;
        if upper_length == lower_length {
            return upper_parameter;
        }
        lower_parameter
            + (target - lower_length) / (upper_length - lower_length)
                * (upper_parameter - lower_parameter)
    }
}

fn cubic_point(points: [SemanticVec3; 4], t: f64) -> SemanticVec3 {
    let omt = 1.0 - t;
    let weights = [
        omt * omt * omt,
        3.0 * omt * omt * t,
        3.0 * omt * t * t,
        t * t * t,
    ];
    SemanticVec3::new(
        points
            .iter()
            .zip(weights)
            .map(|(point, weight)| point.x * weight)
            .sum(),
        points
            .iter()
            .zip(weights)
            .map(|(point, weight)| point.y * weight)
            .sum(),
        points
            .iter()
            .zip(weights)
            .map(|(point, weight)| point.z * weight)
            .sum(),
    )
}

fn append_path(mut target: VectorPath, source: &VectorPath) -> VectorPath {
    for command in source.commands() {
        target = match *command {
            PathCommand::MoveTo { to } => target.move_to(to),
            PathCommand::LineTo { to } => target.line_to(to),
            PathCommand::QuadraticTo { control, to } => target.quadratic_to(control, to),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => target.cubic_to(control1, control2, to),
            PathCommand::Close => target.close(),
        };
    }
    target
}

#[cfg(test)]
mod tests {
    use noon_core::Vec2;

    use super::*;

    #[test]
    fn open_pattern_starts_and_ends_with_a_dash() {
        let intervals = dash_intervals(3, 0.5, 0.0, false);
        assert_eq!(intervals.len(), 3);
        assert!((intervals[0].0 - 0.0).abs() < 1e-12);
        assert!((intervals[0].1 - 1.0 / 6.0).abs() < 1e-12);
        assert!((intervals[2].0 - 5.0 / 6.0).abs() < 1e-12);
        assert!((intervals[2].1 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn closed_pattern_has_one_gap_per_dash_and_wraps_offset() {
        let intervals = dash_intervals(4, 0.5, -0.5, true);
        assert_eq!(intervals.len(), 4);
        // period=.25 and a -0.5 offset wraps to +0.125 phase.
        assert!((intervals[0].0 - 0.125).abs() < 1e-12);
        assert!((intervals[0].1 - 0.25).abs() < 1e-12);
    }

    #[test]
    fn solid_open_pattern_omits_only_the_redundant_wrapped_seam_child() {
        let intervals = dash_intervals(4, 1.0, 0.0, false);
        assert_eq!(
            intervals,
            vec![(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)]
        );
    }

    #[test]
    fn zero_ratio_keeps_requested_degenerate_dash_geometry() {
        let intervals = dash_intervals(2, 0.0, 0.0, false);
        assert_eq!(intervals, vec![(0.0, 0.0), (1.0, 1.0)]);
    }

    #[test]
    fn equal_lengths_inverts_the_same_sampled_measure_as_manim() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0))
            .line_to(Vec2::new(10.0, 0.0));
        let equal = dashed_path(&path, false, 1, 0.25, 0.0, true).unwrap();
        let legacy = dashed_path(&path, false, 1, 0.25, 0.0, false).unwrap();
        let equal_end = equal.endpoints().unwrap().1;
        let legacy_end = legacy.endpoints().unwrap().1;
        assert!((equal_end.x - 2.5).abs() < 1e-5);
        assert!((legacy_end.x - 0.5).abs() < 1e-5);
    }

    #[test]
    fn zero_dashes_produces_empty_retained_geometry() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0));
        assert!(dashed_path(&path, false, 0, 0.5, 0.0, true)
            .unwrap()
            .commands()
            .is_empty());
    }
}
