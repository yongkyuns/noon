use crate::{Axes2DState, IntoSnapshot, Line, NumberLineState};
use noon_core::{Color, ObjectSnapshot, Vec2};

const CAIRO_WIDTH_SCALE: f64 = 0.01;

/// Style applied to one of Manim's NumberPlane grid families.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberPlaneLineStyle {
    pub color: Color,
    pub stroke_width: f64,
    pub stroke_opacity: f64,
}

impl NumberPlaneLineStyle {
    pub const fn new(color: Color, stroke_width: f64, stroke_opacity: f64) -> Self {
        Self {
            color,
            stroke_width,
            stroke_opacity,
        }
    }
}

/// One retained NumberPlane grid line together with its logical perpendicular offset.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberPlaneGridLine {
    offset: f64,
    snapshot: ObjectSnapshot,
}

impl NumberPlaneGridLine {
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

/// Renderer-independent ManimCE v0.21 linear NumberPlane grid geometry.
///
/// The plan deliberately stores ordinary retained `Line` snapshots. It reuses the
/// already-qualified `Axes2DState` / `NumberLineState` mapping and reproduces
/// Manim's `_get_lines_parallel_to_axis` ordering and faded-line classification.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberPlaneGridPlan {
    x_lines: Vec<NumberPlaneGridLine>,
    y_lines: Vec<NumberPlaneGridLine>,
    faded_x_lines: Vec<NumberPlaneGridLine>,
    faded_y_lines: Vec<NumberPlaneGridLine>,
}

impl NumberPlaneGridPlan {
    pub fn new(
        axes: Axes2DState,
        faded_line_ratio: usize,
        background_style: NumberPlaneLineStyle,
        faded_style: NumberPlaneLineStyle,
    ) -> Result<Self, NumberPlaneAuthoringError> {
        validate_style(background_style)?;
        validate_style(faded_style)?;

        let ratio = faded_line_ratio.max(1);
        let (x_lines, faded_x_lines) = lines_parallel_to_axis(
            axes.x_axis(),
            axes.y_axis(),
            axes.y_axis().range().step(),
            ratio,
            background_style,
            faded_style,
        )?;
        let (y_lines, faded_y_lines) = lines_parallel_to_axis(
            axes.y_axis(),
            axes.x_axis(),
            axes.x_axis().range().step(),
            ratio,
            background_style,
            faded_style,
        )?;

        Ok(Self {
            x_lines,
            y_lines,
            faded_x_lines,
            faded_y_lines,
        })
    }

    /// Non-faded lines parallel to the x-axis, matching Manim's `x_lines`.
    pub fn x_lines(&self) -> &[NumberPlaneGridLine] {
        &self.x_lines
    }

    /// Non-faded lines parallel to the y-axis, matching Manim's `y_lines`.
    pub fn y_lines(&self) -> &[NumberPlaneGridLine] {
        &self.y_lines
    }

    pub fn faded_x_lines(&self) -> &[NumberPlaneGridLine] {
        &self.faded_x_lines
    }

    pub fn faded_y_lines(&self) -> &[NumberPlaneGridLine] {
        &self.faded_y_lines
    }

    pub fn background_line_count(&self) -> usize {
        self.x_lines.len() + self.y_lines.len()
    }

    pub fn faded_line_count(&self) -> usize {
        self.faded_x_lines.len() + self.faded_y_lines.len()
    }
}

fn lines_parallel_to_axis(
    parallel: NumberLineState,
    perpendicular: NumberLineState,
    frequency: f64,
    ratio: usize,
    background_style: NumberPlaneLineStyle,
    faded_style: NumberPlaneLineStyle,
) -> Result<(Vec<NumberPlaneGridLine>, Vec<NumberPlaneGridLine>), NumberPlaneAuthoringError> {
    let step = frequency / ratio as f64;
    if !step.is_finite() || step <= 0.0 {
        return Err(NumberPlaneAuthoringError::InvalidGridStep(step));
    }

    let range = perpendicular.range();
    let positive_stop = range.span().min(range.max());
    let negative_stop = (-range.span()).max(range.min());
    let shift_unit = scaled_unit_vector(perpendicular)?;

    let mut background = Vec::new();
    let mut faded = Vec::new();

    // Upstream handles the zero range separately, then resets the enumeration for
    // the positive and negative half-open ranges. Preserve those reset points: they
    // determine which lines are faded when `ratio > 1`.
    push_grid_line(&mut background, 0.0, parallel, shift_unit, background_style)?;

    let mut value = step;
    let mut index = 0_usize;
    while value < positive_stop {
        let is_background = (index + 1).is_multiple_of(ratio);
        push_classified_grid_line(
            &mut background,
            &mut faded,
            value,
            is_background,
            parallel,
            shift_unit,
            background_style,
            faded_style,
        )?;
        value += step;
        index += 1;
    }

    value = -step;
    index = 0;
    while value > negative_stop {
        let is_background = (index + 1).is_multiple_of(ratio);
        push_classified_grid_line(
            &mut background,
            &mut faded,
            value,
            is_background,
            parallel,
            shift_unit,
            background_style,
            faded_style,
        )?;
        value -= step;
        index += 1;
    }

    Ok((background, faded))
}

#[allow(clippy::too_many_arguments)]
fn push_classified_grid_line(
    background: &mut Vec<NumberPlaneGridLine>,
    faded: &mut Vec<NumberPlaneGridLine>,
    offset: f64,
    is_background: bool,
    parallel: NumberLineState,
    shift_unit: Vec2,
    background_style: NumberPlaneLineStyle,
    faded_style: NumberPlaneLineStyle,
) -> Result<(), NumberPlaneAuthoringError> {
    if is_background {
        push_grid_line(background, offset, parallel, shift_unit, background_style)
    } else {
        push_grid_line(faded, offset, parallel, shift_unit, faded_style)
    }
}

fn push_grid_line(
    lines: &mut Vec<NumberPlaneGridLine>,
    offset: f64,
    parallel: NumberLineState,
    shift_unit: Vec2,
    style: NumberPlaneLineStyle,
) -> Result<(), NumberPlaneAuthoringError> {
    let scalar = checked_f32(offset)?;
    let shift = shift_unit * scalar;
    lines.push(NumberPlaneGridLine {
        offset,
        snapshot: styled_line(parallel.start() + shift, parallel.end() + shift, style)?,
    });
    Ok(())
}

fn scaled_unit_vector(state: NumberLineState) -> Result<Vec2, NumberPlaneAuthoringError> {
    // Manim NumberLine.get_unit_vector() is the normalized line direction multiplied
    // by `unit_size`, not merely a unit-length direction. This is what makes custom
    // NumberPlane x_length/y_length scale grid spacing with the axes.
    let delta = state.end() - state.start();
    let length = delta.length();
    if !length.is_finite() || length <= 0.0 {
        return Err(NumberPlaneAuthoringError::DegenerateAxis);
    }
    let unit_size = checked_f32(state.unit_size())?;
    Ok(delta / length * unit_size)
}

fn styled_line(
    start: Vec2,
    end: Vec2,
    style: NumberPlaneLineStyle,
) -> Result<ObjectSnapshot, NumberPlaneAuthoringError> {
    let width = checked_f32(style.stroke_width * CAIRO_WIDTH_SCALE)?;
    let mut color = style.color;
    color.alpha *= checked_f32(style.stroke_opacity)?;
    Ok(Line::new(start, end)
        .color(color)
        .set_stroke(Some(color), Some(width))
        .into_snapshot())
}

fn validate_style(style: NumberPlaneLineStyle) -> Result<(), NumberPlaneAuthoringError> {
    if !style.stroke_width.is_finite() || style.stroke_width < 0.0 {
        return Err(NumberPlaneAuthoringError::InvalidStrokeWidth(
            style.stroke_width,
        ));
    }
    if !style.stroke_opacity.is_finite() || !(0.0..=1.0).contains(&style.stroke_opacity) {
        return Err(NumberPlaneAuthoringError::InvalidStrokeOpacity(
            style.stroke_opacity,
        ));
    }
    if [
        style.color.red,
        style.color.green,
        style.color.blue,
        style.color.alpha,
    ]
    .iter()
    .any(|component| !component.is_finite())
    {
        return Err(NumberPlaneAuthoringError::NonFiniteColor);
    }
    Ok(())
}

fn checked_f32(value: f64) -> Result<f32, NumberPlaneAuthoringError> {
    let lowered = value as f32;
    if lowered.is_finite() {
        Ok(lowered)
    } else {
        Err(NumberPlaneAuthoringError::Overflow(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberPlaneAuthoringError {
    InvalidGridStep(f64),
    InvalidStrokeWidth(f64),
    InvalidStrokeOpacity(f64),
    NonFiniteColor,
    DegenerateAxis,
    Overflow(f64),
}

impl std::fmt::Display for NumberPlaneAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGridStep(value) => {
                write!(formatter, "invalid NumberPlane grid step: {value}")
            }
            Self::InvalidStrokeWidth(value) => {
                write!(formatter, "invalid NumberPlane stroke width: {value}")
            }
            Self::InvalidStrokeOpacity(value) => {
                write!(formatter, "invalid NumberPlane stroke opacity: {value}")
            }
            Self::NonFiniteColor => formatter.write_str("NumberPlane grid color must be finite"),
            Self::DegenerateAxis => formatter.write_str("NumberPlane axis must not be degenerate"),
            Self::Overflow(value) => {
                write!(
                    formatter,
                    "NumberPlane geometry cannot be represented as f32: {value}"
                )
            }
        }
    }
}

impl std::error::Error for NumberPlaneAuthoringError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberRange;
    use noon_core::{GeometryRef, BLUE_D, RED};

    fn axes(x_range: [f64; 3], y_range: [f64; 3], x_length: f32, y_length: f32) -> Axes2DState {
        Axes2DState::new(
            NumberRange::new(x_range[0], x_range[1], x_range[2]).unwrap(),
            NumberRange::new(y_range[0], y_range[1], y_range[2]).unwrap(),
            x_length,
            y_length,
        )
        .unwrap()
    }

    fn style(color: Color, width: f64, opacity: f64) -> NumberPlaneLineStyle {
        NumberPlaneLineStyle::new(color, width, opacity)
    }

    fn line_points(line: &NumberPlaneGridLine) -> (Vec2, Vec2) {
        let snapshot = line.snapshot();
        let GeometryRef::Line { start, end } = &snapshot.geometry else {
            panic!("NumberPlane grid must lower to retained Line geometry");
        };
        (
            snapshot.transform.transform_point(*start),
            snapshot.transform.transform_point(*end),
        )
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn ratio_one_generates_only_background_lines_and_excludes_boundaries() {
        let plan = NumberPlaneGridPlan::new(
            axes([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], 6.0, 6.0),
            1,
            style(BLUE_D, 2.0, 1.0),
            style(BLUE_D, 1.0, 0.5),
        )
        .unwrap();

        assert_eq!(
            plan.x_lines()
                .iter()
                .map(NumberPlaneGridLine::offset)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, -1.0, -2.0]
        );
        assert_eq!(
            plan.y_lines()
                .iter()
                .map(NumberPlaneGridLine::offset)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, -1.0, -2.0]
        );
        assert_eq!(plan.background_line_count(), 10);
        assert_eq!(plan.faded_line_count(), 0);
    }

    #[test]
    fn ratio_two_matches_upstream_background_and_faded_classification() {
        let plan = NumberPlaneGridPlan::new(
            axes([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], 6.0, 6.0),
            2,
            style(BLUE_D, 2.0, 1.0),
            style(BLUE_D, 1.0, 0.5),
        )
        .unwrap();

        let background = plan
            .x_lines()
            .iter()
            .map(NumberPlaneGridLine::offset)
            .collect::<Vec<_>>();
        let faded = plan
            .faded_x_lines()
            .iter()
            .map(NumberPlaneGridLine::offset)
            .collect::<Vec<_>>();
        assert_eq!(background, vec![0.0, 1.0, 2.0, -1.0, -2.0]);
        assert_eq!(faded, vec![0.5, 1.5, 2.5, -0.5, -1.5, -2.5]);
    }

    #[test]
    fn zero_ratio_matches_ratio_one() {
        let axes = axes([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0], 4.0, 4.0);
        let background = style(BLUE_D, 2.0, 1.0);
        let faded = style(BLUE_D, 1.0, 0.5);
        assert_eq!(
            NumberPlaneGridPlan::new(axes, 0, background, faded).unwrap(),
            NumberPlaneGridPlan::new(axes, 1, background, faded).unwrap()
        );
    }

    #[test]
    fn custom_axis_lengths_scale_grid_spacing_by_number_line_unit_size() {
        let plan = NumberPlaneGridPlan::new(
            axes([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0], 8.0, 12.0),
            1,
            style(BLUE_D, 2.0, 1.0),
            style(BLUE_D, 1.0, 0.5),
        )
        .unwrap();

        let (horizontal_start, _) = line_points(&plan.x_lines()[1]);
        let (vertical_start, _) = line_points(&plan.y_lines()[1]);
        assert_close(f64::from(horizontal_start.y), 3.0);
        assert_close(f64::from(vertical_start.x), 2.0);
    }

    #[test]
    fn asymmetric_positive_range_uses_offset_span_from_axis_crossing() {
        let plan = NumberPlaneGridPlan::new(
            axes([2.0, 6.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0),
            1,
            style(BLUE_D, 2.0, 1.0),
            style(BLUE_D, 1.0, 0.5),
        )
        .unwrap();
        assert_eq!(
            plan.y_lines()
                .iter()
                .map(NumberPlaneGridLine::offset)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn styles_lower_to_retained_stroke_width_and_alpha() {
        let plan = NumberPlaneGridPlan::new(
            axes([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0], 4.0, 4.0),
            2,
            style(RED, 4.0, 0.6),
            style(BLUE_D, 2.0, 0.25),
        )
        .unwrap();
        let background = plan.x_lines()[0].snapshot();
        let faded = plan.faded_x_lines()[0].snapshot();
        assert_close(f64::from(background.style.stroke_width), 0.04);
        assert_close(f64::from(background.style.stroke.unwrap().alpha), 0.6);
        assert_close(f64::from(faded.style.stroke_width), 0.02);
        assert_close(f64::from(faded.style.stroke.unwrap().alpha), 0.25);
    }

    #[test]
    fn invalid_style_fails_closed() {
        assert!(matches!(
            NumberPlaneGridPlan::new(
                axes([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0], 2.0, 2.0),
                1,
                style(BLUE_D, -1.0, 1.0),
                style(BLUE_D, 1.0, 0.5),
            ),
            Err(NumberPlaneAuthoringError::InvalidStrokeWidth(-1.0))
        ));
    }
}
