use crate::{CoordinateSystemError, IntoSnapshot, Line, NumberLineState};
use noon_core::{Color, ObjectSnapshot, Vec2, WHITE};

const CAIRO_WIDTH_SCALE: f64 = 0.01;
const IS_CLOSE_RTOL: f64 = 1.0e-5;
const IS_CLOSE_ATOL: f64 = 1.0e-8;

#[derive(Clone, Debug, PartialEq)]
pub struct NumberLineTickOptions {
    pub include_ticks: bool,
    pub tick_size: f64,
    pub elongated_values: Vec<f64>,
    pub longer_tick_multiple: usize,
    pub exclude_origin_tick: bool,
    pub color: Color,
    pub stroke_width: f64,
}

impl Default for NumberLineTickOptions {
    fn default() -> Self {
        Self {
            include_ticks: true,
            tick_size: 0.1,
            elongated_values: Vec::new(),
            longer_tick_multiple: 2,
            exclude_origin_tick: false,
            color: WHITE,
            stroke_width: 2.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumberLineTick {
    value: f64,
    size: f64,
    snapshot: ObjectSnapshot,
}

impl NumberLineTick {
    pub const fn value(&self) -> f64 {
        self.value
    }

    pub const fn size(&self) -> f64 {
        self.size
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumberLineGeometryPlan {
    line: ObjectSnapshot,
    ticks: Vec<NumberLineTick>,
}

impl NumberLineGeometryPlan {
    pub fn new(
        state: NumberLineState,
        options: &NumberLineTickOptions,
    ) -> Result<Self, AxisTickError> {
        validate_options(options)?;
        let width = checked_f32(options.stroke_width * CAIRO_WIDTH_SCALE)?;
        let line = styled_line(state.start(), state.end(), options.color, width);
        let ticks = if options.include_ticks {
            build_ticks(state, options, width)?
        } else {
            Vec::new()
        };
        Ok(Self { line, ticks })
    }

    pub fn line(&self) -> &ObjectSnapshot {
        &self.line
    }

    pub fn ticks(&self) -> &[NumberLineTick] {
        &self.ticks
    }
}

fn build_ticks(
    state: NumberLineState,
    options: &NumberLineTickOptions,
    width: f32,
) -> Result<Vec<NumberLineTick>, AxisTickError> {
    let delta = state.end() - state.start();
    let length = delta.length();
    if !length.is_finite() || length <= 0.0 {
        return Err(CoordinateSystemError::DegenerateLine.into());
    }
    let tangent = delta / length;
    let normal = Vec2::new(-tangent.y, tangent.x);
    let range_min = state.range().min();

    state
        .tick_values(options.exclude_origin_tick)
        .into_iter()
        .map(|value| {
            let elongated = options
                .elongated_values
                .iter()
                .any(|candidate| is_close(value - range_min, *candidate - range_min));
            let size = options.tick_size
                * if elongated {
                    options.longer_tick_multiple as f64
                } else {
                    1.0
                };
            let center = state.number_to_point(value)?;
            let offset = normal * checked_f32(size)?;
            Ok(NumberLineTick {
                value,
                size,
                snapshot: styled_line(center - offset, center + offset, options.color, width),
            })
        })
        .collect()
}

fn styled_line(start: Vec2, end: Vec2, color: Color, width: f32) -> ObjectSnapshot {
    Line::new(start, end)
        .color(color)
        .set_stroke(Some(color), Some(width))
        .into_snapshot()
}

fn validate_options(options: &NumberLineTickOptions) -> Result<(), AxisTickError> {
    if !options.tick_size.is_finite() || options.tick_size < 0.0 {
        return Err(AxisTickError::InvalidTickSize(options.tick_size));
    }
    if options.longer_tick_multiple == 0 {
        return Err(AxisTickError::InvalidLongerTickMultiple);
    }
    if !options.stroke_width.is_finite() || options.stroke_width < 0.0 {
        return Err(AxisTickError::InvalidStrokeWidth(options.stroke_width));
    }
    if options
        .elongated_values
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err(AxisTickError::NonFiniteElongatedValue);
    }
    Ok(())
}

fn is_close(lhs: f64, rhs: f64) -> bool {
    (lhs - rhs).abs() <= IS_CLOSE_ATOL + IS_CLOSE_RTOL * rhs.abs()
}

fn checked_f32(value: f64) -> Result<f32, AxisTickError> {
    let lowered = value as f32;
    if lowered.is_finite() {
        Ok(lowered)
    } else {
        Err(AxisTickError::Overflow(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisTickError {
    Coordinates(CoordinateSystemError),
    InvalidTickSize(f64),
    InvalidLongerTickMultiple,
    InvalidStrokeWidth(f64),
    NonFiniteElongatedValue,
    Overflow(f64),
}

impl From<CoordinateSystemError> for AxisTickError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl std::fmt::Display for AxisTickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinates(error) => error.fmt(f),
            Self::InvalidTickSize(value) => write!(f, "invalid tick size: {value}"),
            Self::InvalidLongerTickMultiple => {
                f.write_str("longer tick multiple must be positive")
            }
            Self::InvalidStrokeWidth(value) => write!(f, "invalid stroke width: {value}"),
            Self::NonFiniteElongatedValue => f.write_str("elongated tick values must be finite"),
            Self::Overflow(value) => {
                write!(f, "tick geometry cannot be represented as f32: {value}")
            }
        }
    }
}

impl std::error::Error for AxisTickError {}
