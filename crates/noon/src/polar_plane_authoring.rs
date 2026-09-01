use crate::{Axes2DState, Circle, IntoSnapshot, Line, NumberPlaneLineStyle};
use noon_core::{ObjectSnapshot, Vec2};

const CAIRO_WIDTH_SCALE: f64 = 0.01;

#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneRadialLine {
    angle: f64,
    snapshot: ObjectSnapshot,
}

impl PolarPlaneRadialLine {
    pub const fn angle(&self) -> f64 {
        self.angle
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneRadiusCircle {
    radius: f64,
    snapshot: ObjectSnapshot,
}

impl PolarPlaneRadiusCircle {
    pub const fn radius(&self) -> f64 {
        self.radius
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

/// Renderer-independent ManimCE v0.21 PolarPlane background geometry.
///
/// The plan stores ordinary retained `Line` and `Circle` snapshots. Axes geometry
/// remains owned by the existing `Axes2DState`; this type owns only the polar
/// background subdivision, faded classification, style, and upstream family order.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneGridPlan {
    radial_lines: Vec<PolarPlaneRadialLine>,
    circles: Vec<PolarPlaneRadiusCircle>,
    faded_radial_lines: Vec<PolarPlaneRadialLine>,
    faded_circles: Vec<PolarPlaneRadiusCircle>,
}

impl PolarPlaneGridPlan {
    pub fn new(
        axes: Axes2DState,
        azimuth_step: f64,
        azimuth_offset: f64,
        faded_line_ratio: usize,
        background_style: NumberPlaneLineStyle,
        faded_style: NumberPlaneLineStyle,
    ) -> Result<Self, PolarPlaneAuthoringError> {
        validate_style(background_style)?;
        validate_style(faded_style)?;
        if !azimuth_step.is_finite() || azimuth_step <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidAzimuthStep(azimuth_step));
        }
        if !azimuth_offset.is_finite() {
            return Err(PolarPlaneAuthoringError::InvalidAzimuthOffset(
                azimuth_offset,
            ));
        }

        let x_axis = axes.x_axis();
        let range = x_axis.range();
        if (range.min() + range.max()).abs() > 1.0e-9 {
            return Err(PolarPlaneAuthoringError::AsymmetricRadiusRange {
                min: range.min(),
                max: range.max(),
            });
        }

        let ratio = faded_line_ratio.max(1);
        let radius_step = range.step() / ratio as f64;
        let azimuth_increment = std::f64::consts::TAU / azimuth_step / ratio as f64;
        if !radius_step.is_finite() || radius_step <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidRadiusStep(radius_step));
        }
        if !azimuth_increment.is_finite() || azimuth_increment <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidAzimuthStep(azimuth_step));
        }

        let unit_size = x_axis.unit_size();
        if !unit_size.is_finite() || unit_size <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidUnitSize(unit_size));
        }
        let center = axes.origin()?;
        let radial_vector = x_axis.end() - center;

        let mut circles = Vec::new();
        let mut faded_circles = Vec::new();
        let radius_stop = range.max() + radius_step;
        let mut logical_radius = 0.0;
        let mut index = 0_usize;
        while logical_radius < radius_stop {
            let is_background = index.is_multiple_of(ratio);
            let style = if is_background {
                background_style
            } else {
                faded_style
            };
            let circle = PolarPlaneRadiusCircle {
                radius: logical_radius,
                snapshot: styled_circle(logical_radius * unit_size, style)?,
            };
            if is_background {
                circles.push(circle);
            } else {
                faded_circles.push(circle);
            }
            logical_radius += radius_step;
            index += 1;
        }

        let mut radial_lines = Vec::new();
        let mut faded_radial_lines = Vec::new();
        let mut angle = 0.0;
        index = 0;
        while angle < std::f64::consts::TAU {
            let final_angle = angle + azimuth_offset;
            let end = center + rotate(radial_vector, final_angle)?;
            let is_background = index.is_multiple_of(ratio);
            let style = if is_background {
                background_style
            } else {
                faded_style
            };
            let line = PolarPlaneRadialLine {
                angle: final_angle,
                snapshot: styled_line(center, end, style)?,
            };
            if is_background {
                radial_lines.push(line);
            } else {
                faded_radial_lines.push(line);
            }
            angle += azimuth_increment;
            index += 1;
        }

        Ok(Self {
            radial_lines,
            circles,
            faded_radial_lines,
            faded_circles,
        })
    }

    /// Non-faded radial lines. These precede circles in Manim's background family.
    pub fn radial_lines(&self) -> &[PolarPlaneRadialLine] {
        &self.radial_lines
    }

    /// Non-faded concentric circles, following radial lines upstream.
    pub fn circles(&self) -> &[PolarPlaneRadiusCircle] {
        &self.circles
    }

    pub fn faded_radial_lines(&self) -> &[PolarPlaneRadialLine] {
        &self.faded_radial_lines
    }

    pub fn faded_circles(&self) -> &[PolarPlaneRadiusCircle] {
        &self.faded_circles
    }
}

fn styled_line(
    start: Vec2,
    end: Vec2,
    style: NumberPlaneLineStyle,
) -> Result<ObjectSnapshot, PolarPlaneAuthoringError> {
    let (color, width) = resolved_style(style)?;
    Ok(Line::new(start, end)
        .color(color)
        .set_stroke(Some(color), Some(width))
        .into_snapshot())
}

fn styled_circle(
    radius: f64,
    style: NumberPlaneLineStyle,
) -> Result<ObjectSnapshot, PolarPlaneAuthoringError> {
    let radius = checked_f32(radius)?;
    let (color, width) = resolved_style(style)?;
    Ok(Circle::new(radius)
        .color(color)
        .set_stroke(Some(color), Some(width))
        .into_snapshot())
}

fn resolved_style(
    style: NumberPlaneLineStyle,
) -> Result<(noon_core::Color, f32), PolarPlaneAuthoringError> {
    validate_style(style)?;
    let width = checked_f32(style.stroke_width * CAIRO_WIDTH_SCALE)?;
    let mut color = style.color;
    color.alpha *= checked_f32(style.stroke_opacity)?;
    Ok((color, width))
}

fn validate_style(style: NumberPlaneLineStyle) -> Result<(), PolarPlaneAuthoringError> {
    if !style.stroke_width.is_finite() || style.stroke_width < 0.0 {
        return Err(PolarPlaneAuthoringError::InvalidStrokeWidth(
            style.stroke_width,
        ));
    }
    if !style.stroke_opacity.is_finite() || !(0.0..=1.0).contains(&style.stroke_opacity) {
        return Err(PolarPlaneAuthoringError::InvalidStrokeOpacity(
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
        return Err(PolarPlaneAuthoringError::NonFiniteColor);
    }
    Ok(())
}

fn rotate(vector: Vec2, angle: f64) -> Result<Vec2, PolarPlaneAuthoringError> {
    let cosine = checked_f32(angle.cos())?;
    let sine = checked_f32(angle.sin())?;
    Ok(Vec2::new(
        vector.x * cosine - vector.y * sine,
        vector.x * sine + vector.y * cosine,
    ))
}

fn checked_f32(value: f64) -> Result<f32, PolarPlaneAuthoringError> {
    let lowered = value as f32;
    if lowered.is_finite() {
        Ok(lowered)
    } else {
        Err(PolarPlaneAuthoringError::Overflow(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PolarPlaneAuthoringError {
    InvalidAzimuthStep(f64),
    InvalidAzimuthOffset(f64),
    InvalidRadiusStep(f64),
    InvalidUnitSize(f64),
    InvalidStrokeWidth(f64),
    InvalidStrokeOpacity(f64),
    NonFiniteColor,
    AsymmetricRadiusRange { min: f64, max: f64 },
    Coordinates(crate::CoordinateSystemError),
    Overflow(f64),
}

impl std::fmt::Display for PolarPlaneAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::InvalidAzimuthStep(value) => {
                write!(formatter, "invalid PolarPlane azimuth step: {value}")
            }
            Self::InvalidAzimuthOffset(value) => {
                write!(formatter, "invalid PolarPlane azimuth offset: {value}")
            }
            Self::InvalidRadiusStep(value) => {
                write!(formatter, "invalid PolarPlane radius step: {value}")
            }
            Self::InvalidUnitSize(value) => {
                write!(formatter, "invalid PolarPlane unit size: {value}")
            }
            Self::InvalidStrokeWidth(value) => {
                write!(formatter, "invalid PolarPlane stroke width: {value}")
            }
            Self::InvalidStrokeOpacity(value) => {
                write!(formatter, "invalid PolarPlane stroke opacity: {value}")
            }
            Self::NonFiniteColor => formatter.write_str("PolarPlane grid color must be finite"),
            Self::AsymmetricRadiusRange { min, max } => write!(
                formatter,
                "PolarPlane radius range must be symmetric around zero: [{min}, {max}]"
            ),
            Self::Coordinates(ref error) => error.fmt(formatter),
            Self::Overflow(value) => write!(
                formatter,
                "PolarPlane geometry cannot be represented as f32: {value}"
            ),
        }
    }
}

impl std::error::Error for PolarPlaneAuthoringError {}

impl From<crate::CoordinateSystemError> for PolarPlaneAuthoringError {
    fn from(value: crate::CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberRange;
    use noon_core::{GeometryRef, BLUE_D};

    fn axes(radius_max: f64, radius_step: f64, size: f32) -> Axes2DState {
        let range = NumberRange::new(-radius_max, radius_max, radius_step).unwrap();
        Axes2DState::new(range, range, size, size).unwrap()
    }

    fn style(width: f64, opacity: f64) -> NumberPlaneLineStyle {
        NumberPlaneLineStyle::new(BLUE_D, width, opacity)
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn ratio_one_matches_upstream_radius_and_azimuth_families() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 4.0),
            4.0,
            0.0,
            1,
            style(2.0, 1.0),
            style(1.0, 0.5),
        )
        .unwrap();
        assert_eq!(
            plan.circles()
                .iter()
                .map(PolarPlaneRadiusCircle::radius)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0]
        );
        assert_eq!(plan.radial_lines().len(), 4);
        assert!(plan.faded_circles().is_empty());
        assert!(plan.faded_radial_lines().is_empty());
        for (actual, expected) in plan.radial_lines().iter().map(PolarPlaneRadialLine::angle).zip([
            0.0,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
            3.0 * std::f64::consts::FRAC_PI_2,
        ]) {
            assert_close(actual, expected);
        }
    }

    #[test]
    fn ratio_two_classifies_radius_and_azimuth_indices_independently() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 4.0),
            4.0,
            0.0,
            2,
            style(2.0, 1.0),
            style(1.0, 0.5),
        )
        .unwrap();
        assert_eq!(
            plan.circles()
                .iter()
                .map(PolarPlaneRadiusCircle::radius)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0]
        );
        assert_eq!(
            plan.faded_circles()
                .iter()
                .map(PolarPlaneRadiusCircle::radius)
                .collect::<Vec<_>>(),
            vec![0.5, 1.5]
        );
        assert_eq!(plan.radial_lines().len(), 4);
        assert_eq!(plan.faded_radial_lines().len(), 4);
    }

    #[test]
    fn ratio_zero_matches_ratio_one() {
        let axes = axes(2.0, 1.0, 4.0);
        let background = style(2.0, 1.0);
        let faded = style(1.0, 0.5);
        assert_eq!(
            PolarPlaneGridPlan::new(axes, 4.0, 0.0, 0, background, faded).unwrap(),
            PolarPlaneGridPlan::new(axes, 4.0, 0.0, 1, background, faded).unwrap()
        );
    }

    #[test]
    fn non_integer_azimuth_divisions_keep_the_partial_final_division() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 4.0),
            2.5,
            0.0,
            1,
            style(2.0, 1.0),
            style(1.0, 0.5),
        )
        .unwrap();
        assert_eq!(plan.radial_lines().len(), 3);
        assert_close(
            plan.radial_lines()[2].angle(),
            1.6 * std::f64::consts::PI,
        );
    }

    #[test]
    fn custom_size_scales_circle_radius_and_radial_endpoint() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 8.0),
            4.0,
            0.0,
            1,
            style(2.0, 1.0),
            style(1.0, 0.5),
        )
        .unwrap();
        let GeometryRef::Circle { radius } = &plan.circles()[1].snapshot().geometry else {
            panic!("PolarPlane circles must remain retained Circle geometry")
        };
        assert_close(f64::from(*radius), 2.0);
        let GeometryRef::Line { start, end } = &plan.radial_lines()[0].snapshot().geometry else {
            panic!("PolarPlane radial lines must remain retained Line geometry")
        };
        assert_close(f64::from(start.x), 0.0);
        assert_close(f64::from(end.x), 4.0);
    }

    #[test]
    fn azimuth_offset_rotates_retained_radial_endpoints() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 4.0),
            4.0,
            std::f64::consts::FRAC_PI_2,
            1,
            style(2.0, 1.0),
            style(1.0, 0.5),
        )
        .unwrap();
        let GeometryRef::Line { end, .. } = &plan.radial_lines()[0].snapshot().geometry else {
            panic!("PolarPlane radial lines must remain retained Line geometry")
        };
        assert_close(f64::from(end.x), 0.0);
        assert_close(f64::from(end.y), 2.0);
    }
}
