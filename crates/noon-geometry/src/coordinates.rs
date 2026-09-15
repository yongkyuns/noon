//! Shared scalar coordinate queries over one caller-supplied snapshot.
//!
//! These small values are disposable query inputs. Persistent ranges belong to
//! the Semantic Scene; endpoints must come from either authored state or one
//! coherent effective frame, never a mixture of the two.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateError {
    InvalidRange,
    InvalidLength,
    InvalidPoint,
    DegenerateAxis,
    TickLimitExceeded,
    AllocationFailed,
}

impl std::fmt::Display for CoordinateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRange => "coordinate range must be finite and increasing with positive step",
            Self::InvalidLength => "axis length must be finite and positive",
            Self::InvalidPoint => "coordinate query requires finite inputs and output",
            Self::DegenerateAxis => "coordinate query cannot invert a collapsed axis",
            Self::TickLimitExceeded => "number-line tick limit exceeded",
            Self::AllocationFailed => "coordinate preparation allocation failed",
        })
    }
}

impl std::error::Error for CoordinateError {}

pub fn validate_coordinate_range(range: [f64; 3]) -> Result<(), CoordinateError> {
    let [start, end, step] = range;
    if range.iter().any(|value| !value.is_finite())
        || start >= end
        || step <= 0.0
        || !(end - start).is_finite()
    {
        return Err(CoordinateError::InvalidRange);
    }
    Ok(())
}

/// Number-line coordinates derived from public endpoints in a single snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberLineFrame {
    range: [f64; 3],
    start: [f64; 2],
    end: [f64; 2],
}

impl NumberLineFrame {
    pub fn new(
        range: [f64; 3],
        start: [f64; 2],
        end: [f64; 2],
    ) -> Result<Self, CoordinateError> {
        validate_coordinate_range(range)?;
        finite_point(start)?;
        finite_point(end)?;
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        if !length.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        // Forward mapping remains meaningful when an animation collapses the
        // line. Only inverse queries reject that singular state.
        Ok(Self { range, start, end })
    }

    /// Prepare the centered line before applying the requested rotation.
    pub fn centered(
        range: [f64; 3],
        length: f64,
        rotation: f64,
    ) -> Result<Self, CoordinateError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(CoordinateError::InvalidLength);
        }
        if !rotation.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let (sin, cos) = rotation.sin_cos();
        let half = length * 0.5;
        Self::new(range, [-half * cos, -half * sin], [half * cos, half * sin])
    }

    pub const fn range(self) -> [f64; 3] {
        self.range
    }

    pub const fn start(self) -> [f64; 2] {
        self.start
    }

    pub const fn end(self) -> [f64; 2] {
        self.end
    }

    pub fn number_to_point(self, number: f64) -> Result<[f64; 2], CoordinateError> {
        if !number.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let alpha = (number - self.range[0]) / (self.range[1] - self.range[0]);
        finite_point([
            (1.0 - alpha) * self.start[0] + alpha * self.end[0],
            (1.0 - alpha) * self.start[1] + alpha * self.end[1],
        ])
    }

    /// Project onto the current line, matching scalar Manim `p2n` semantics.
    pub fn point_to_number(self, point: [f64; 2]) -> Result<f64, CoordinateError> {
        finite_point(point)?;
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let length = dx.hypot(dy);
        if length == 0.0 {
            return Err(CoordinateError::DegenerateAxis);
        }
        let alpha = ((point[0] - self.start[0]) * (dx / length)
            + (point[1] - self.start[1]) * (dy / length))
            / length;
        let number = (1.0 - alpha) * self.range[0] + alpha * self.range[1];
        if !number.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        Ok(number)
    }

    pub fn unit_size(self) -> f64 {
        (self.end[0] - self.start[0]).hypot(self.end[1] - self.start[1])
            / (self.range[1] - self.range[0])
    }
}

/// Two ordinary number-line snapshots with Manim's clamped-origin convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxesFrame {
    x: NumberLineFrame,
    y: NumberLineFrame,
}

impl AxesFrame {
    pub const fn new(x: NumberLineFrame, y: NumberLineFrame) -> Self {
        Self { x, y }
    }

    /// Center the numerical range midpoints, including positive-only and
    /// negative-only ranges. This does not center a decoration's visual bounds.
    pub fn centered(
        x_range: [f64; 3],
        y_range: [f64; 3],
        x_length: f64,
        y_length: f64,
    ) -> Result<Self, CoordinateError> {
        validate_coordinate_range(x_range)?;
        validate_coordinate_range(y_range)?;
        if [x_length, y_length]
            .iter()
            .any(|length| !length.is_finite() || *length <= 0.0)
        {
            return Err(CoordinateError::InvalidLength);
        }
        let x_span = x_range[1] - x_range[0];
        let y_span = y_range[1] - y_range[0];
        let x_mid = x_range[0] + x_span * 0.5;
        let y_mid = y_range[0] + y_span * 0.5;
        let x_origin = (origin_shift(x_range) - x_mid) / x_span * x_length;
        let y_origin = (origin_shift(y_range) - y_mid) / y_span * y_length;
        Ok(Self::new(
            NumberLineFrame::new(
                x_range,
                [-x_length * 0.5, y_origin],
                [x_length * 0.5, y_origin],
            )?,
            NumberLineFrame::new(
                y_range,
                [x_origin, -y_length * 0.5],
                [x_origin, y_length * 0.5],
            )?,
        ))
    }

    pub const fn x(self) -> NumberLineFrame {
        self.x
    }

    pub const fn y(self) -> NumberLineFrame {
        self.y
    }

    pub fn coords_to_point(self, x: f64, y: f64) -> Result<[f64; 2], CoordinateError> {
        let origin = self.x.number_to_point(origin_shift(self.x.range))?;
        let x_point = self.x.number_to_point(x)?;
        let y_point = self.y.number_to_point(y)?;
        finite_point([
            x_point[0] + y_point[0] - origin[0],
            x_point[1] + y_point[1] - origin[1],
        ])
    }

    /// Match Manim's per-axis projections. This is an inverse for orthogonal
    /// axes, not a general inverse of independently sheared/nonorthogonal axes.
    pub fn point_to_coords(self, point: [f64; 2]) -> Result<[f64; 2], CoordinateError> {
        Ok([
            self.x.point_to_number(point)?,
            self.y.point_to_number(point)?,
        ])
    }
}

pub fn origin_shift(range: [f64; 3]) -> f64 {
    if range[0] > 0.0 {
        range[0]
    } else if range[1] < 0.0 {
        range[1]
    } else {
        0.0
    }
}

/// Pinned linear NumberLine tick positions. Tip presence excludes the upper
/// endpoint; origin exclusion and positive/negative-only ranges follow v0.21.
/// `limit` is admission policy, never a reason to truncate or coarsen ticks.
pub fn number_line_tick_values(
    range: [f64; 3],
    include_tip: bool,
    exclude_origin: bool,
    limit: usize,
) -> Result<Vec<f64>, CoordinateError> {
    validate_coordinate_range(range)?;
    let [start, end, step] = range;
    let stop = end + if include_tip { 0.0 } else { 1.0e-6 };
    let mut ticks = Vec::new();
    if (start < stop && stop < 0.0) || (stop > start && start > 0.0) {
        append_ticks(&mut ticks, start, stop, step, 1.0, limit)?;
    } else {
        let first = if exclude_origin { step } else { 0.0 };
        append_ticks(&mut ticks, first, start.abs() + 1.0e-6, step, -1.0, limit)?;
        append_ticks(&mut ticks, first, stop, step, 1.0, limit)?;
        ticks.sort_by(f64::total_cmp);
        ticks.dedup_by(|left, right| *left == *right);
    }
    Ok(ticks)
}

fn append_ticks(
    ticks: &mut Vec<f64>,
    start: f64,
    stop: f64,
    step: f64,
    sign: f64,
    limit: usize,
) -> Result<(), CoordinateError> {
    let count = ((stop - start) / step).ceil().max(0.0);
    if !stop.is_finite()
        || !count.is_finite()
        || count > limit.saturating_sub(ticks.len()) as f64
    {
        return Err(CoordinateError::TickLimitExceeded);
    }
    let count = count as usize;
    let increment = (start + step) - start;
    if !increment.is_finite() {
        return Err(CoordinateError::InvalidRange);
    }
    ticks
        .try_reserve_exact(count)
        .map_err(|_| CoordinateError::AllocationFailed)?;
    for index in 0..count {
        let value = sign * (start + increment * index as f64);
        if !value.is_finite() {
            return Err(CoordinateError::InvalidRange);
        }
        ticks.push(value);
    }
    Ok(())
}

fn finite_point(point: [f64; 2]) -> Result<[f64; 2], CoordinateError> {
    if point.iter().all(|value| value.is_finite()) {
        Ok(point)
    } else {
        Err(CoordinateError::InvalidPoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(left: [f64; 2], right: [f64; 2]) {
        assert!((left[0] - right[0]).abs() < 1.0e-10, "{left:?} != {right:?}");
        assert!((left[1] - right[1]).abs() < 1.0e-10, "{left:?} != {right:?}");
    }

    #[test]
    fn number_line_is_centered_before_rotation() {
        let frame = NumberLineFrame::centered([2.0, 6.0, 1.0], 8.0, 0.0).unwrap();
        near(frame.number_to_point(2.0).unwrap(), [-4.0, 0.0]);
        near(frame.number_to_point(4.0).unwrap(), [0.0, 0.0]);
        assert_eq!(frame.unit_size(), 2.0);
        let rotated =
            NumberLineFrame::centered([2.0, 6.0, 1.0], 8.0, std::f64::consts::FRAC_PI_2)
                .unwrap();
        near(rotated.number_to_point(6.0).unwrap(), [0.0, 4.0]);
    }

    #[test]
    fn endpoint_queries_follow_translation_rotation_and_reflection() {
        let frame = NumberLineFrame::new([-2.0, 2.0, 0.5], [3.0, 7.0], [3.0, -1.0]).unwrap();
        for number in [-4.0, -2.0, 0.0, 0.75, 2.0, 4.0] {
            let point = frame.number_to_point(number).unwrap();
            assert!((frame.point_to_number(point).unwrap() - number).abs() < 1.0e-12);
        }
    }

    #[test]
    fn positive_and_negative_only_axes_have_centered_range_midpoints() {
        let frame = AxesFrame::centered([2.0, 6.0, 1.0], [-6.0, -2.0, 1.0], 8.0, 4.0)
            .unwrap();
        near(frame.coords_to_point(4.0, -4.0).unwrap(), [0.0, 0.0]);
        near(frame.coords_to_point(2.0, -6.0).unwrap(), [-4.0, -2.0]);
        near(frame.point_to_coords([4.0, 2.0]).unwrap(), [6.0, -2.0]);
        near(frame.x().start(), [-4.0, 2.0]);
        near(frame.y().start(), [-4.0, -2.0]);
    }

    #[test]
    fn axes_queries_use_supplied_current_endpoints() {
        let x = NumberLineFrame::new([-2.0, 2.0, 1.0], [3.0, -2.0], [3.0, 6.0]).unwrap();
        let y = NumberLineFrame::new([-1.0, 1.0, 1.0], [6.0, 2.0], [0.0, 2.0]).unwrap();
        let frame = AxesFrame::new(x, y);
        near(frame.coords_to_point(1.0, 0.5).unwrap(), [1.5, 4.0]);
        near(frame.point_to_coords([1.5, 4.0]).unwrap(), [1.0, 0.5]);
    }

    #[test]
    fn origin_tick_and_tip_endpoint_rules_are_explicit() {
        assert_eq!(
            number_line_tick_values([-2.0, 2.0, 1.0], false, false, 10).unwrap(),
            vec![-2.0, -1.0, 0.0, 1.0, 2.0]
        );
        assert_eq!(
            number_line_tick_values([-2.0, 2.0, 1.0], true, true, 10).unwrap(),
            vec![-2.0, -1.0, 1.0]
        );
        assert_eq!(
            number_line_tick_values([2.0, 4.0, 0.75], false, true, 10).unwrap(),
            vec![2.0, 2.75, 3.5]
        );
    }

    #[test]
    fn invalid_and_singular_queries_are_not_silently_approximated() {
        assert!(NumberLineFrame::centered([0.0, 0.0, 1.0], 1.0, 0.0).is_err());
        assert!(NumberLineFrame::centered([0.0, 1.0, 1.0], 0.0, 0.0).is_err());
        let frame = NumberLineFrame::new([0.0, 1.0, 0.1], [2.0, 3.0], [2.0, 3.0]).unwrap();
        near(frame.number_to_point(0.5).unwrap(), [2.0, 3.0]);
        assert_eq!(frame.point_to_number([2.0, 3.0]), Err(CoordinateError::DegenerateAxis));
        assert_eq!(frame.number_to_point(f64::NAN), Err(CoordinateError::InvalidPoint));
        assert_eq!(
            number_line_tick_values([0.0, 100.0, 0.01], false, false, 10),
            Err(CoordinateError::TickLimitExceeded)
        );
    }
}
