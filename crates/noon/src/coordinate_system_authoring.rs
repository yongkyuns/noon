use noon_core::Vec2;

const RANGE_EPSILON: f64 = 1.0e-6;

/// Canonical linear coordinate range for the supported Manim NumberLine/Axes subset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    min: f64,
    max: f64,
    step: f64,
}

impl NumberRange {
    pub fn new(min: f64, max: f64, step: f64) -> Result<Self, CoordinateSystemError> {
        if !min.is_finite() || !max.is_finite() || !step.is_finite() {
            return Err(CoordinateSystemError::NonFiniteRange { min, max, step });
        }
        if max <= min {
            return Err(CoordinateSystemError::NonIncreasingRange { min, max });
        }
        if step <= 0.0 {
            return Err(CoordinateSystemError::InvalidStep(step));
        }
        Ok(Self { min, max, step })
    }

    pub fn with_default_step(min: f64, max: f64) -> Result<Self, CoordinateSystemError> {
        Self::new(min, max, 1.0)
    }

    pub const fn min(self) -> f64 {
        self.min
    }

    pub const fn max(self) -> f64 {
        self.max
    }

    pub const fn step(self) -> f64 {
        self.step
    }

    pub fn span(self) -> f64 {
        self.max - self.min
    }

    pub fn midpoint(self) -> f64 {
        (self.min + self.max) * 0.5
    }

    /// Matches Manim's `_origin_shift`: zero when the range spans zero,
    /// otherwise the endpoint nearest zero.
    pub fn origin_shift(self) -> f64 {
        if self.min > 0.0 {
            self.min
        } else if self.max < 0.0 {
            self.max
        } else {
            0.0
        }
    }
}

/// Renderer-independent placement/mapping state for the initial linear NumberLine subset.
///
/// The state stores final scene-space endpoints, so transforms and frontends do not need
/// to duplicate interpolation or projection rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberLineState {
    range: NumberRange,
    start: Vec2,
    end: Vec2,
}

impl NumberLineState {
    /// Construct a NumberLine centered on the scene origin before caller-owned placement.
    pub fn centered(
        range: NumberRange,
        length: f32,
        rotation: f32,
    ) -> Result<Self, CoordinateSystemError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(CoordinateSystemError::InvalidLength(length));
        }
        if !rotation.is_finite() {
            return Err(CoordinateSystemError::InvalidRotation(rotation));
        }

        let direction = Vec2::new(rotation.cos(), rotation.sin());
        let half = direction * (length * 0.5);
        Ok(Self {
            range,
            start: -half,
            end: half,
        })
    }

    pub const fn range(self) -> NumberRange {
        self.range
    }

    pub const fn start(self) -> Vec2 {
        self.start
    }

    pub const fn end(self) -> Vec2 {
        self.end
    }

    pub fn length(self) -> f64 {
        f64::from((self.end - self.start).length())
    }

    pub fn unit_size(self) -> f64 {
        self.length() / self.range.span()
    }

    pub fn translated(mut self, offset: Vec2) -> Result<Self, CoordinateSystemError> {
        if !vec2_is_finite(offset) {
            return Err(CoordinateSystemError::NonFinitePoint(offset));
        }
        self.start += offset;
        self.end += offset;
        Ok(self)
    }

    /// Manim-compatible linear `number_to_point` / `n2p` mapping.
    pub fn number_to_point(self, number: f64) -> Result<Vec2, CoordinateSystemError> {
        if !number.is_finite() {
            return Err(CoordinateSystemError::NonFiniteValue(number));
        }

        let alpha = (number - self.range.min) / self.range.span();
        let point = self.start + (self.end - self.start) * alpha as f32;
        if !vec2_is_finite(point) {
            return Err(CoordinateSystemError::NonFinitePoint(point));
        }
        Ok(point)
    }

    /// Manim-compatible linear `point_to_number` / `p2n` mapping. Points need not
    /// lie on the line; only their projection along the line direction contributes.
    pub fn point_to_number(self, point: Vec2) -> Result<f64, CoordinateSystemError> {
        if !vec2_is_finite(point) {
            return Err(CoordinateSystemError::NonFinitePoint(point));
        }

        let delta = self.end - self.start;
        let length_squared =
            f64::from(delta.x) * f64::from(delta.x) + f64::from(delta.y) * f64::from(delta.y);
        if !length_squared.is_finite() || length_squared <= 0.0 {
            return Err(CoordinateSystemError::DegenerateLine);
        }

        let from_start = point - self.start;
        let projection = f64::from(from_start.x) * f64::from(delta.x)
            + f64::from(from_start.y) * f64::from(delta.y);
        let alpha = projection / length_squared;
        Ok(self.range.min + alpha * self.range.span())
    }

    /// Tick values for Manim's default linear, no-tip NumberLine behavior.
    /// Ranges spanning zero anchor tick multiples at zero instead of at `min`.
    pub fn tick_values(self, exclude_origin_tick: bool) -> Vec<f64> {
        let range = self.range;
        if range.min > 0.0 || range.max < 0.0 {
            let mut values = Vec::new();
            let mut value = range.min;
            while value <= range.max + RANGE_EPSILON {
                values.push(normalize_zero(value));
                value += range.step;
            }
            return values;
        }

        let first_multiple = if exclude_origin_tick { 1_u64 } else { 0_u64 };
        let mut negative = Vec::new();
        let mut multiple = first_multiple;
        loop {
            let magnitude = multiple as f64 * range.step;
            if magnitude > range.min.abs() + RANGE_EPSILON {
                break;
            }
            if magnitude > 0.0 {
                negative.push(-magnitude);
            }
            multiple += 1;
        }
        negative.reverse();

        let mut values = negative;
        if !exclude_origin_tick {
            values.push(0.0);
        }

        multiple = 1;
        loop {
            let value = multiple as f64 * range.step;
            if value > range.max + RANGE_EPSILON {
                break;
            }
            values.push(value);
            multiple += 1;
        }
        values
    }
}

/// Shared coordinate mapping for the initial 2D linear Manim Axes subset.
///
/// This models Manim's axis crossing rule and final group-centering step without
/// creating an Axes-specific renderer primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axes2DState {
    x_axis: NumberLineState,
    y_axis: NumberLineState,
}

impl Axes2DState {
    pub fn new(
        x_range: NumberRange,
        y_range: NumberRange,
        x_length: f32,
        y_length: f32,
    ) -> Result<Self, CoordinateSystemError> {
        let x_unit = checked_unit_size(x_range, x_length)?;
        let y_unit = checked_unit_size(y_range, y_length)?;

        // Manim first shifts each NumberLine so the axis-crossing logical value
        // (`origin_shift`) is at the origin, then centers the combined Axes group.
        // The final crossing offsets reduce to the values below.
        let x_crossing = checked_f32((x_range.origin_shift() - x_range.midpoint()) * x_unit)?;
        let y_crossing = checked_f32((y_range.origin_shift() - y_range.midpoint()) * y_unit)?;

        let x_axis = NumberLineState::centered(x_range, x_length, 0.0)?
            .translated(Vec2::new(0.0, y_crossing))?;
        let y_axis = NumberLineState::centered(y_range, y_length, std::f32::consts::FRAC_PI_2)?
            .translated(Vec2::new(x_crossing, 0.0))?;

        Ok(Self { x_axis, y_axis })
    }

    pub const fn x_axis(self) -> NumberLineState {
        self.x_axis
    }

    pub const fn y_axis(self) -> NumberLineState {
        self.y_axis
    }

    pub fn coords_to_point(self, x: f64, y: f64) -> Result<Vec2, CoordinateSystemError> {
        if !x.is_finite() {
            return Err(CoordinateSystemError::NonFiniteValue(x));
        }
        if !y.is_finite() {
            return Err(CoordinateSystemError::NonFiniteValue(y));
        }

        let x_point = self.x_axis.number_to_point(x)?;
        let y_point = self.y_axis.number_to_point(y)?;
        let point = Vec2::new(x_point.x, y_point.y);
        if !vec2_is_finite(point) {
            return Err(CoordinateSystemError::NonFinitePoint(point));
        }
        Ok(point)
    }

    pub fn point_to_coords(self, point: Vec2) -> Result<(f64, f64), CoordinateSystemError> {
        Ok((
            self.x_axis.point_to_number(point)?,
            self.y_axis.point_to_number(point)?,
        ))
    }

    pub fn origin(self) -> Result<Vec2, CoordinateSystemError> {
        self.coords_to_point(0.0, 0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoordinateSystemError {
    NonFiniteRange { min: f64, max: f64, step: f64 },
    NonIncreasingRange { min: f64, max: f64 },
    InvalidStep(f64),
    InvalidLength(f32),
    InvalidRotation(f32),
    NonFiniteValue(f64),
    NonFinitePoint(Vec2),
    DegenerateLine,
    CoordinateOverflow(f64),
}

impl std::fmt::Display for CoordinateSystemError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteRange { min, max, step } => {
                write!(
                    formatter,
                    "number range must be finite: [{min}, {max}, {step}]"
                )
            }
            Self::NonIncreasingRange { min, max } => {
                write!(
                    formatter,
                    "number range must increase: min={min}, max={max}"
                )
            }
            Self::InvalidStep(step) => {
                write!(formatter, "number range step must be positive: {step}")
            }
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "number line length must be finite and positive: {length}"
                )
            }
            Self::InvalidRotation(rotation) => {
                write!(formatter, "number line rotation must be finite: {rotation}")
            }
            Self::NonFiniteValue(value) => {
                write!(formatter, "coordinate value must be finite: {value}")
            }
            Self::NonFinitePoint(point) => write!(
                formatter,
                "coordinate point must be finite: ({}, {})",
                point.x, point.y
            ),
            Self::DegenerateLine => write!(formatter, "number line endpoints must not coincide"),
            Self::CoordinateOverflow(value) => {
                write!(
                    formatter,
                    "coordinate value cannot be represented as f32: {value}"
                )
            }
        }
    }
}

impl std::error::Error for CoordinateSystemError {}

fn checked_unit_size(range: NumberRange, length: f32) -> Result<f64, CoordinateSystemError> {
    if !length.is_finite() || length <= 0.0 {
        return Err(CoordinateSystemError::InvalidLength(length));
    }
    Ok(f64::from(length) / range.span())
}

fn checked_f32(value: f64) -> Result<f32, CoordinateSystemError> {
    let lowered = value as f32;
    if lowered.is_finite() {
        Ok(lowered)
    } else {
        Err(CoordinateSystemError::CoordinateOverflow(value))
    }
}

fn vec2_is_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

fn normalize_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert_close(f64::from(actual.x), f64::from(expected.x));
        assert_close(f64::from(actual.y), f64::from(expected.y));
    }

    #[test]
    fn range_rejects_invalid_semantics() {
        assert!(matches!(
            NumberRange::new(f64::NAN, 1.0, 1.0),
            Err(CoordinateSystemError::NonFiniteRange { .. })
        ));
        assert_eq!(
            NumberRange::new(1.0, 1.0, 1.0),
            Err(CoordinateSystemError::NonIncreasingRange { min: 1.0, max: 1.0 })
        );
        assert_eq!(
            NumberRange::new(-1.0, 1.0, 0.0),
            Err(CoordinateSystemError::InvalidStep(0.0))
        );
    }

    #[test]
    fn centered_number_line_maps_range_linearly() {
        let range = NumberRange::new(-2.0, 8.0, 1.0).unwrap();
        let line = NumberLineState::centered(range, 10.0, 0.0).unwrap();
        assert_point(line.number_to_point(-2.0).unwrap(), Vec2::new(-5.0, 0.0));
        assert_point(line.number_to_point(3.0).unwrap(), Vec2::ZERO);
        assert_point(line.number_to_point(8.0).unwrap(), Vec2::new(5.0, 0.0));
        assert_close(line.point_to_number(Vec2::new(2.5, 4.0)).unwrap(), 5.5);
        assert_close(line.unit_size(), 1.0);
    }

    #[test]
    fn number_line_rotation_and_translation_preserve_coordinate_mapping() {
        let range = NumberRange::new(-1.0, 1.0, 0.5).unwrap();
        let line = NumberLineState::centered(range, 4.0, std::f32::consts::FRAC_PI_2)
            .unwrap()
            .translated(Vec2::new(3.0, -2.0))
            .unwrap();
        assert_point(line.number_to_point(-1.0).unwrap(), Vec2::new(3.0, -4.0));
        assert_point(line.number_to_point(0.0).unwrap(), Vec2::new(3.0, -2.0));
        assert_point(line.number_to_point(1.0).unwrap(), Vec2::new(3.0, 0.0));
        assert_close(line.point_to_number(Vec2::new(9.0, -1.0)).unwrap(), 0.5);
    }

    #[test]
    fn tick_values_match_manim_zero_anchoring() {
        let mixed = NumberLineState::centered(NumberRange::new(-5.0, 5.0, 2.0).unwrap(), 10.0, 0.0)
            .unwrap();
        assert_eq!(mixed.tick_values(false), vec![-4.0, -2.0, 0.0, 2.0, 4.0]);
        assert_eq!(mixed.tick_values(true), vec![-4.0, -2.0, 2.0, 4.0]);

        let positive =
            NumberLineState::centered(NumberRange::new(1.0, 5.0, 1.0).unwrap(), 4.0, 0.0).unwrap();
        assert_eq!(positive.tick_values(false), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn axes_center_logical_midpoints_and_place_crossings_like_manim() {
        let axes = Axes2DState::new(
            NumberRange::new(2.0, 6.0, 1.0).unwrap(),
            NumberRange::new(-1.0, 3.0, 1.0).unwrap(),
            8.0,
            4.0,
        )
        .unwrap();

        assert_point(axes.coords_to_point(4.0, 1.0).unwrap(), Vec2::ZERO);
        assert_point(
            axes.coords_to_point(2.0, -1.0).unwrap(),
            Vec2::new(-4.0, -2.0),
        );
        assert_point(axes.coords_to_point(6.0, 3.0).unwrap(), Vec2::new(4.0, 2.0));

        // Positive-only x ranges make the y-axis cross at x_min. With the
        // combined axes centered, that crossing is at the left x endpoint.
        assert_close(f64::from(axes.y_axis().start().x), -4.0);
        assert_close(f64::from(axes.x_axis().start().y), -1.0);
        assert_point(axes.origin().unwrap(), Vec2::new(-8.0, -1.0));
    }

    #[test]
    fn axes_coordinate_round_trip_is_projection_stable() {
        let axes = Axes2DState::new(
            NumberRange::new(-3.0, 7.0, 0.5).unwrap(),
            NumberRange::new(-4.0, 2.0, 1.0).unwrap(),
            10.0,
            6.0,
        )
        .unwrap();
        let point = axes.coords_to_point(1.25, -0.75).unwrap();
        let (x, y) = axes.point_to_coords(point).unwrap();
        assert_close(x, 1.25);
        assert_close(y, -0.75);
    }
}
