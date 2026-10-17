//! Shared scalar coordinate queries over one caller-supplied snapshot.
//!
//! These small values are disposable query inputs. Persistent ranges belong to
//! the Semantic Scene; endpoints must come from either authored state or one
//! coherent effective frame, never a mixture of the two.

use noon_core::SemanticNumberLineScale;

/// Default admission budget shared by linear NumberLine construction and queries.
pub const DEFAULT_NUMBER_LINE_TICK_LIMIT: usize = 10_000;

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
            Self::InvalidRange => {
                "coordinate range must be finite and increasing with positive step"
            }
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
    scale: SemanticNumberLineScale,
}

impl NumberLineFrame {
    pub fn new(range: [f64; 3], start: [f64; 2], end: [f64; 2]) -> Result<Self, CoordinateError> {
        Self::with_scale(range, start, end, SemanticNumberLineScale::default())
    }

    pub fn with_scale(
        range: [f64; 3],
        start: [f64; 2],
        end: [f64; 2],
        scale: SemanticNumberLineScale,
    ) -> Result<Self, CoordinateError> {
        validate_coordinate_range(range)?;
        finite_point(start)?;
        finite_point(end)?;
        let mapped_start = scale.map(range[0]);
        let mapped_end = scale.map(range[1]);
        if mapped_start.is_none() || mapped_end.is_none() || mapped_start == mapped_end {
            return Err(CoordinateError::InvalidRange);
        }
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        if !length.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        // Forward mapping remains meaningful when an animation collapses the
        // line. Only inverse queries reject that singular state.
        Ok(Self {
            range,
            start,
            end,
            scale,
        })
    }

    /// Prepare the centered line before applying the requested rotation.
    pub fn centered(range: [f64; 3], length: f64, rotation: f64) -> Result<Self, CoordinateError> {
        Self::centered_with_scale(range, length, rotation, SemanticNumberLineScale::default())
    }

    pub fn centered_with_scale(
        range: [f64; 3],
        length: f64,
        rotation: f64,
        scale: SemanticNumberLineScale,
    ) -> Result<Self, CoordinateError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(CoordinateError::InvalidLength);
        }
        if !rotation.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let (sin, cos) = rotation.sin_cos();
        let half = length * 0.5;
        Self::with_scale(
            range,
            [-half * cos, -half * sin],
            [half * cos, half * sin],
            scale,
        )
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

    pub const fn scale(self) -> SemanticNumberLineScale {
        self.scale
    }

    /// Geometric vector from the current line start to its end.
    pub fn vector(self) -> [f64; 2] {
        [self.end[0] - self.start[0], self.end[1] - self.start[1]]
    }

    /// Euclidean distance between the current line endpoints.
    pub fn length(self) -> f64 {
        let [dx, dy] = self.vector();
        dx.hypot(dy)
    }

    /// Orientation of the current line in the XY plane.
    pub fn angle(self) -> f64 {
        let [dx, dy] = self.vector();
        dy.atan2(dx)
    }

    /// Slope using the same angle/tangent convention as Manim's `Line`.
    pub fn slope(self) -> f64 {
        self.angle().tan()
    }

    /// Orthogonally project a point onto the infinite line through this frame.
    /// A degenerate line projects every finite point to its sole endpoint, as
    /// Manim's zero-vector normalization does for `Line.get_projection`.
    pub fn project_point(self, point: [f64; 2]) -> Result<[f64; 2], CoordinateError> {
        finite_point(point)?;
        let [dx, dy] = self.vector();
        let length = dx.hypot(dy);
        if length == 0.0 {
            return Ok(self.start);
        }
        if !length.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let unit = [dx / length, dy / length];
        let distance = (point[0] - self.start[0]) * unit[0] + (point[1] - self.start[1]) * unit[1];
        finite_point([
            self.start[0] + distance * unit[0],
            self.start[1] + distance * unit[1],
        ])
    }

    pub fn scaled_range_start(self) -> Result<f64, CoordinateError> {
        self.scale
            .map(self.range[0])
            .ok_or(CoordinateError::InvalidRange)
    }

    /// Return the value at which Manim clamps this axis to cross zero. The
    /// returned value is in the externally visible coordinate space (after
    /// applying this frame's scale).
    pub fn origin_value(self) -> Result<f64, CoordinateError> {
        let start = self
            .scale
            .map(self.range[0])
            .ok_or(CoordinateError::InvalidRange)?;
        let end = self
            .scale
            .map(self.range[1])
            .ok_or(CoordinateError::InvalidRange)?;
        let value = if start > 0.0 {
            start
        } else if end < 0.0 {
            end
        } else {
            0.0
        };
        self.scale
            .unmap(value)
            .map(|_| value)
            .ok_or(CoordinateError::InvalidRange)
    }

    fn origin_coordinate(self) -> Result<f64, CoordinateError> {
        self.scale
            .unmap(self.origin_value()?)
            .ok_or(CoordinateError::InvalidRange)
    }

    pub fn number_to_point(self, number: f64) -> Result<[f64; 2], CoordinateError> {
        let coordinate = self
            .scale
            .unmap(number)
            .ok_or(CoordinateError::InvalidPoint)?;
        let alpha = (coordinate - self.range[0]) / (self.range[1] - self.range[0]);
        finite_point([
            (1.0 - alpha) * self.start[0] + alpha * self.end[0],
            (1.0 - alpha) * self.start[1] + alpha * self.end[1],
        ])
    }

    /// Return the endpoints of a Manim-style tick centered on `number`.
    /// `half_length` is the distance from the center to either endpoint, so it
    /// matches Manim's `size` argument rather than half of its total length.
    pub fn tick_segment(
        self,
        number: f64,
        half_length: f64,
    ) -> Result<[[f64; 2]; 2], CoordinateError> {
        if !half_length.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let center = self.number_to_point(number)?;
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let length = dx.hypot(dy);
        if length == 0.0 {
            return Err(CoordinateError::DegenerateAxis);
        }
        if !length.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        let normal = [-dy / length, dx / length];
        let offset = [half_length * normal[0], half_length * normal[1]];
        Ok([
            finite_point([center[0] - offset[0], center[1] - offset[1]])?,
            finite_point([center[0] + offset[0], center[1] + offset[1]])?,
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
        let coordinate = (1.0 - alpha) * self.range[0] + alpha * self.range[1];
        self.scale
            .map(coordinate)
            .ok_or(CoordinateError::InvalidPoint)
    }

    pub fn unit_size(self) -> f64 {
        (self.end[0] - self.start[0]).hypot(self.end[1] - self.start[1])
            / (self.range[1] - self.range[0])
    }

    /// Return the pinned linear tick values for this data range. Construction
    /// and query share the same tick rule so callers do not reconstruct the
    /// range with frontend-specific floating-point logic.
    pub fn tick_values(
        self,
        include_tip: bool,
        exclude_origin: bool,
        limit: usize,
    ) -> Result<Vec<f64>, CoordinateError> {
        number_line_tick_values(self.range, include_tip, exclude_origin, limit)?
            .into_iter()
            .map(|value| self.scale.map(value).ok_or(CoordinateError::InvalidRange))
            .collect()
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
        Self::centered_with_scales(
            x_range,
            y_range,
            x_length,
            y_length,
            SemanticNumberLineScale::default(),
            SemanticNumberLineScale::default(),
        )
    }

    /// Center both numerical range midpoints while mapping each axis through
    /// its own Manim-compatible scale.
    pub fn centered_with_scales(
        x_range: [f64; 3],
        y_range: [f64; 3],
        x_length: f64,
        y_length: f64,
        x_scale: SemanticNumberLineScale,
        y_scale: SemanticNumberLineScale,
    ) -> Result<Self, CoordinateError> {
        validate_coordinate_range(x_range)?;
        validate_coordinate_range(y_range)?;
        if [x_length, y_length]
            .iter()
            .any(|length| !length.is_finite() || *length <= 0.0)
        {
            return Err(CoordinateError::InvalidLength);
        }
        let x_axis = NumberLineFrame::with_scale(
            x_range,
            [-x_length * 0.5, 0.0],
            [x_length * 0.5, 0.0],
            x_scale,
        )?;
        let y_axis = NumberLineFrame::with_scale(
            y_range,
            [0.0, -y_length * 0.5],
            [0.0, y_length * 0.5],
            y_scale,
        )?;
        let x_span = x_range[1] - x_range[0];
        let y_span = y_range[1] - y_range[0];
        // Normalize before centering: a large absolute range midpoint can
        // round onto an endpoint even though the span is representable.
        let x_origin = ((x_axis.origin_coordinate()? - x_range[0]) / x_span - 0.5) * x_length;
        let y_origin = ((y_axis.origin_coordinate()? - y_range[0]) / y_span - 0.5) * y_length;
        Ok(Self::new(
            NumberLineFrame::with_scale(
                x_range,
                [-x_length * 0.5, y_origin],
                [x_length * 0.5, y_origin],
                x_scale,
            )?,
            NumberLineFrame::with_scale(
                y_range,
                [x_origin, -y_length * 0.5],
                [x_origin, y_length * 0.5],
                y_scale,
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
        let origin = [
            self.x.number_to_point(self.x.origin_value()?)?[0],
            self.y.number_to_point(self.y.origin_value()?)?[1],
        ];
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

/// Polar queries over the same retained Cartesian axes snapshot. Manim's grid
/// azimuth offset/direction affect grid decoration and labels, not coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolarFrame {
    axes: AxesFrame,
}

impl PolarFrame {
    pub const fn new(axes: AxesFrame) -> Self {
        Self { axes }
    }
    pub const fn axes(self) -> AxesFrame {
        self.axes
    }
    pub fn polar_to_point(self, radius: f64, azimuth: f64) -> Result<[f64; 2], CoordinateError> {
        if !radius.is_finite() || !azimuth.is_finite() {
            return Err(CoordinateError::InvalidPoint);
        }
        self.axes
            .coords_to_point(radius * azimuth.cos(), radius * azimuth.sin())
    }
    pub fn point_to_polar(self, point: [f64; 2]) -> Result<[f64; 2], CoordinateError> {
        let [x, y] = self.axes.point_to_coords(point)?;
        finite_point([x.hypot(y), y.atan2(x)])
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
        append_ticks(&mut ticks, start, stop, step, 1.0, false, limit)?;
    } else {
        let first = if exclude_origin { step } else { 0.0 };
        append_ticks(
            &mut ticks,
            first,
            start.abs() + 1.0e-6,
            step,
            -1.0,
            false,
            limit,
        )?;
        // The negative pass already owns zero. Do not charge its duplicate to
        // the admission budget, or an exact-capacity valid line is rejected.
        let skip_origin = !exclude_origin && !ticks.is_empty();
        append_ticks(&mut ticks, first, stop, step, 1.0, skip_origin, limit)?;
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
    skip_first: bool,
    limit: usize,
) -> Result<(), CoordinateError> {
    let count = ((stop - start) / step).ceil().max(0.0);
    let skipped = usize::from(skip_first && count > 0.0);
    let appended = count - skipped as f64;
    if !stop.is_finite()
        || !count.is_finite()
        || appended > limit.saturating_sub(ticks.len()) as f64
    {
        return Err(CoordinateError::TickLimitExceeded);
    }
    let count = count as usize;
    let increment = (start + step) - start;
    if !increment.is_finite() {
        return Err(CoordinateError::InvalidRange);
    }
    ticks
        .try_reserve_exact(count - skipped)
        .map_err(|_| CoordinateError::AllocationFailed)?;
    for index in skipped..count {
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
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::SemanticNumberLineScale;

    fn near(left: [f64; 2], right: [f64; 2]) {
        assert!(
            (left[0] - right[0]).abs() < 1.0e-10,
            "{left:?} != {right:?}"
        );
        assert!(
            (left[1] - right[1]).abs() < 1.0e-10,
            "{left:?} != {right:?}"
        );
    }

    #[test]
    fn number_line_is_centered_before_rotation() {
        let frame = NumberLineFrame::centered([2.0, 6.0, 1.0], 8.0, 0.0).unwrap();
        near(frame.number_to_point(2.0).unwrap(), [-4.0, 0.0]);
        near(frame.number_to_point(4.0).unwrap(), [0.0, 0.0]);
        assert_eq!(frame.unit_size(), 2.0);
        let rotated =
            NumberLineFrame::centered([2.0, 6.0, 1.0], 8.0, std::f64::consts::FRAC_PI_2).unwrap();
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
    fn logarithmic_frames_map_queries_ticks_and_inverse_through_shared_scale() {
        let frame = NumberLineFrame::centered_with_scale(
            [-2.0, 2.0, 1.0],
            4.0,
            0.0,
            SemanticNumberLineScale::log(10.0),
        )
        .unwrap();
        near(frame.number_to_point(1.0).unwrap(), [0.0, 0.0]);
        near(frame.number_to_point(10.0).unwrap(), [1.0, 0.0]);
        assert!((frame.point_to_number([1.0, 0.0]).unwrap() - 10.0).abs() < 1.0e-10);
        assert!(frame.number_to_point(0.0).is_err());
        let ticks = frame.tick_values(false, false, 10).unwrap();
        for (actual, expected) in ticks.into_iter().zip([0.01, 0.1, 1.0, 10.0, 100.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        let tipped_ticks = frame.tick_values(true, false, 10).unwrap();
        assert_eq!(tipped_ticks.len(), 4);
        for (actual, expected) in tipped_ticks.into_iter().zip([0.01, 0.1, 1.0, 10.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
    }

    #[test]
    fn linear_scale_factor_preserves_inverse_queries_and_maps_tick_values() {
        let frame = NumberLineFrame::centered_with_scale(
            [-2.0, 2.0, 1.0],
            4.0,
            0.0,
            SemanticNumberLineScale::linear(2.0),
        )
        .unwrap();
        near(frame.number_to_point(0.0).unwrap(), [0.0, 0.0]);
        near(frame.number_to_point(2.0).unwrap(), [1.0, 0.0]);
        assert_eq!(
            frame.tick_values(false, false, 10).unwrap(),
            [-4.0, -2.0, 0.0, 2.0, 4.0]
        );
        assert!((frame.point_to_number([1.0, 0.0]).unwrap() - 2.0).abs() < 1.0e-12);
    }

    #[test]
    fn axes_frames_keep_linear_defaults_and_map_each_scaled_axis() {
        let default = AxesFrame::centered([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0).unwrap();
        let explicit_linear = AxesFrame::centered_with_scales(
            [-2.0, 2.0, 1.0],
            [-1.0, 1.0, 1.0],
            4.0,
            2.0,
            SemanticNumberLineScale::default(),
            SemanticNumberLineScale::default(),
        )
        .unwrap();
        assert_eq!(default, explicit_linear);

        let frame = AxesFrame::centered_with_scales(
            [0.0, 4.0, 1.0],
            [-2.0, 2.0, 1.0],
            8.0,
            4.0,
            SemanticNumberLineScale::default(),
            SemanticNumberLineScale::log(10.0),
        )
        .unwrap();
        assert_eq!(frame.y().origin_value().unwrap(), 0.01);
        near(frame.coords_to_point(1.0, 1.0).unwrap(), [-2.0, 0.0]);
        near(frame.coords_to_point(2.0, 10.0).unwrap(), [0.0, 1.0]);
        near(
            frame
                .point_to_coords(frame.coords_to_point(2.0, 10.0).unwrap())
                .unwrap(),
            [2.0, 10.0],
        );
        let ticks = frame.y().tick_values(false, false, 10).unwrap();
        for (actual, expected) in ticks.into_iter().zip([0.01, 0.1, 1.0, 10.0, 100.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
    }

    #[test]
    fn tick_segments_follow_the_current_line_normal_and_scale() {
        let frame = NumberLineFrame::new([-2.0, 2.0, 1.0], [3.0, 4.0], [3.0, 8.0]).unwrap();
        assert_eq!(
            frame.tick_segment(0.0, 0.25).unwrap(),
            [[3.25, 6.0], [2.75, 6.0]]
        );
        let rotated = NumberLineFrame::new([-2.0, 2.0, 1.0], [1.0, 2.0], [5.0, 2.0]).unwrap();
        assert_eq!(
            rotated.tick_segment(1.0, 0.5).unwrap(),
            [[4.0, 1.5], [4.0, 2.5]]
        );
        let diagonal = NumberLineFrame::new([-1.0, 1.0, 0.5], [0.0, 0.0], [4.0, 3.0]).unwrap();
        let [[start_x, start_y], [end_x, end_y]] = diagonal.tick_segment(0.0, 0.5).unwrap();
        near([start_x, start_y], [2.3, 1.1]);
        near([end_x, end_y], [1.7, 1.9]);
    }

    #[test]
    fn positive_and_negative_only_axes_have_centered_range_midpoints() {
        let frame = AxesFrame::centered([2.0, 6.0, 1.0], [-6.0, -2.0, 1.0], 8.0, 4.0).unwrap();
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
    fn number_line_frame_exposes_its_shared_tick_values() {
        let frame = NumberLineFrame::centered([-2.0, 2.0, 1.0], 4.0, 0.0).unwrap();
        assert_eq!(
            frame.tick_values(false, false, 10).unwrap(),
            vec![-2.0, -1.0, 0.0, 1.0, 2.0]
        );
        assert_eq!(
            frame.tick_values(true, true, 10).unwrap(),
            vec![-2.0, -1.0, 1.0]
        );
    }

    #[test]
    fn number_line_frame_exposes_geometric_line_observations() {
        let frame = NumberLineFrame::with_scale(
            [-2.0, 2.0, 1.0],
            [1.0, 2.0],
            [5.0, 2.0],
            SemanticNumberLineScale::log(10.0),
        )
        .unwrap();
        assert_eq!(frame.vector(), [4.0, 0.0]);
        assert_eq!(frame.length(), 4.0);
        assert_eq!(frame.angle(), 0.0);
        assert_eq!(frame.slope(), 0.0);
        near(frame.project_point([3.0, 6.0]).unwrap(), [3.0, 2.0]);

        let vertical = NumberLineFrame::new([-2.0, 2.0, 1.0], [0.0, 0.0], [0.0, 4.0]).unwrap();
        assert!((vertical.angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        near(vertical.project_point([3.0, 2.0]).unwrap(), [0.0, 2.0]);

        let diagonal = NumberLineFrame::new([-2.0, 2.0, 1.0], [1.0, 2.0], [4.0, 6.0]).unwrap();
        let reversed = NumberLineFrame::new([-2.0, 2.0, 1.0], [4.0, 6.0], [1.0, 2.0]).unwrap();
        near(diagonal.project_point([1.0, 7.0]).unwrap(), [3.4, 5.2]);
        near(reversed.project_point([1.0, 7.0]).unwrap(), [3.4, 5.2]);
        assert_eq!(diagonal.length(), 5.0);
        assert!((diagonal.slope() - 4.0 / 3.0).abs() < 1e-12);
        assert!((reversed.slope() - 4.0 / 3.0).abs() < 1e-12);

        let collapsed = NumberLineFrame::new([0.0, 1.0, 0.1], [2.0, 3.0], [2.0, 3.0]).unwrap();
        near(collapsed.project_point([8.0, -4.0]).unwrap(), [2.0, 3.0]);
        assert_eq!(
            collapsed.project_point([f64::NAN, 0.0]),
            Err(CoordinateError::InvalidPoint)
        );
    }

    #[test]
    fn invalid_and_singular_queries_are_not_silently_approximated() {
        assert!(NumberLineFrame::centered([0.0, 0.0, 1.0], 1.0, 0.0).is_err());
        assert!(NumberLineFrame::centered([0.0, 1.0, 1.0], 0.0, 0.0).is_err());
        let frame = NumberLineFrame::new([0.0, 1.0, 0.1], [2.0, 3.0], [2.0, 3.0]).unwrap();
        near(frame.number_to_point(0.5).unwrap(), [2.0, 3.0]);
        assert_eq!(
            frame.point_to_number([2.0, 3.0]),
            Err(CoordinateError::DegenerateAxis)
        );
        assert_eq!(
            frame.number_to_point(f64::NAN),
            Err(CoordinateError::InvalidPoint)
        );
        assert_eq!(
            frame.tick_segment(0.5, f64::INFINITY),
            Err(CoordinateError::InvalidPoint)
        );
        assert_eq!(
            frame.tick_segment(0.5, 0.1),
            Err(CoordinateError::DegenerateAxis)
        );
        assert_eq!(
            number_line_tick_values([0.0, 100.0, 0.01], false, false, 10),
            Err(CoordinateError::TickLimitExceeded)
        );
    }
}
