use crate::{Axes2DState, Circle, IntoSnapshot, Line, NumberPlaneLineStyle};
use noon_core::{Color, ObjectSnapshot, Vec2};

const CAIRO_WIDTH_SCALE: f64 = 0.01;
const AXIS_TOLERANCE: f64 = 1.0e-6;

/// Direction used when enumerating PolarPlane radial members.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolarPlaneAzimuthDirection {
    #[default]
    CounterClockwise,
    Clockwise,
}

impl PolarPlaneAzimuthDirection {
    const fn sign(self) -> f64 {
        match self {
            Self::CounterClockwise => 1.0,
            Self::Clockwise => -1.0,
        }
    }
}

/// One retained radial line in a Manim-compatible `PolarPlane` background grid.
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

/// One retained concentric circle in a Manim-compatible `PolarPlane` background grid.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneCircle {
    radius: f64,
    snapshot: ObjectSnapshot,
}

impl PolarPlaneCircle {
    pub const fn radius(&self) -> f64 {
        self.radius
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

/// Renderer-independent retained background geometry for ManimCE v0.21 `PolarPlane`.
///
/// The caller supplies the normalized symmetric `Axes2DState`. This plan owns polar
/// subdivision, azimuth direction/offset, faded classification, NumberLine-unit-size
/// scaling, and upstream family order. It emits only existing retained `Line` and
/// analytic `Circle` snapshots; no PolarPlane renderer primitive is introduced.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneGridPlan {
    radial_lines: Vec<PolarPlaneRadialLine>,
    circles: Vec<PolarPlaneCircle>,
    faded_radial_lines: Vec<PolarPlaneRadialLine>,
    faded_circles: Vec<PolarPlaneCircle>,
}

impl PolarPlaneGridPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        axes: Axes2DState,
        azimuth_step: f64,
        azimuth_direction: PolarPlaneAzimuthDirection,
        azimuth_offset: f64,
        faded_line_ratio: usize,
        background_style: NumberPlaneLineStyle,
        faded_style: NumberPlaneLineStyle,
    ) -> Result<Self, PolarPlaneAuthoringError> {
        validate_polar_axes(axes)?;
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

        let ratio = faded_line_ratio.max(1);
        let radius_max = axes.x_axis().range().max();
        let radial_step = axes.x_axis().range().step() / ratio as f64;
        let angular_step = std::f64::consts::TAU / azimuth_step / ratio as f64;
        if !radial_step.is_finite() || radial_step <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidRadiusStep(radial_step));
        }
        if !angular_step.is_finite() || angular_step <= 0.0 {
            return Err(PolarPlaneAuthoringError::InvalidAngularStep(angular_step));
        }

        let origin = axes
            .origin()
            .map_err(PolarPlaneAuthoringError::Coordinates)?;
        let unit_size = checked_f32(axes.x_axis().unit_size())?;
        let positive_x = axes
            .x_axis()
            .number_to_point(radius_max)
            .map_err(PolarPlaneAuthoringError::Coordinates)?;
        let radial_vector = positive_x - origin;

        let (circles, faded_circles) = build_circles(
            origin,
            radius_max,
            radial_step,
            unit_size,
            ratio,
            background_style,
            faded_style,
        )?;
        let (radial_lines, faded_radial_lines) = build_radial_lines(
            origin,
            radial_vector,
            angular_step,
            azimuth_direction,
            azimuth_offset,
            ratio,
            background_style,
            faded_style,
        )?;

        Ok(Self {
            radial_lines,
            circles,
            faded_radial_lines,
            faded_circles,
        })
    }

    pub fn radial_lines(&self) -> &[PolarPlaneRadialLine] {
        &self.radial_lines
    }

    pub fn circles(&self) -> &[PolarPlaneCircle] {
        &self.circles
    }

    pub fn faded_radial_lines(&self) -> &[PolarPlaneRadialLine] {
        &self.faded_radial_lines
    }

    pub fn faded_circles(&self) -> &[PolarPlaneCircle] {
        &self.faded_circles
    }

    /// Manim family order: ordinary radials, ordinary circles, faded radials,
    /// then faded circles.
    pub fn snapshots_in_manim_order(&self) -> impl Iterator<Item = &ObjectSnapshot> {
        self.radial_lines
            .iter()
            .map(PolarPlaneRadialLine::snapshot)
            .chain(self.circles.iter().map(PolarPlaneCircle::snapshot))
            .chain(
                self.faded_radial_lines
                    .iter()
                    .map(PolarPlaneRadialLine::snapshot),
            )
            .chain(self.faded_circles.iter().map(PolarPlaneCircle::snapshot))
    }

    pub fn background_member_count(&self) -> usize {
        self.radial_lines.len() + self.circles.len()
    }

    pub fn faded_member_count(&self) -> usize {
        self.faded_radial_lines.len() + self.faded_circles.len()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_circles(
    origin: Vec2,
    radius_max: f64,
    radial_step: f64,
    unit_size: f32,
    ratio: usize,
    background_style: NumberPlaneLineStyle,
    faded_style: NumberPlaneLineStyle,
) -> Result<(Vec<PolarPlaneCircle>, Vec<PolarPlaneCircle>), PolarPlaneAuthoringError> {
    let mut background = Vec::new();
    let mut faded = Vec::new();
    let stop = radius_max + radial_step;
    let mut radius = 0.0;
    let mut index = 0_usize;

    // Upstream uses arange(0, radius_max + rstep, rstep), including radius zero.
    while radius < stop {
        let is_background = index.is_multiple_of(ratio);
        let style = if is_background {
            background_style
        } else {
            faded_style
        };
        let member = PolarPlaneCircle {
            radius,
            snapshot: styled_circle(origin, checked_f32(radius)? * unit_size, style)?,
        };
        if is_background {
            background.push(member);
        } else {
            faded.push(member);
        }
        radius += radial_step;
        index += 1;
    }

    Ok((background, faded))
}

#[allow(clippy::too_many_arguments)]
fn build_radial_lines(
    origin: Vec2,
    radial_vector: Vec2,
    angular_step: f64,
    direction: PolarPlaneAzimuthDirection,
    azimuth_offset: f64,
    ratio: usize,
    background_style: NumberPlaneLineStyle,
    faded_style: NumberPlaneLineStyle,
) -> Result<(Vec<PolarPlaneRadialLine>, Vec<PolarPlaneRadialLine>), PolarPlaneAuthoringError> {
    let mut background = Vec::new();
    let mut faded = Vec::new();
    let mut base_angle = 0.0;
    let mut index = 0_usize;

    // Keep the arange(0, TAU, astep) enumeration before applying direction and
    // offset so non-integral azimuth divisions preserve Manim's final partial member.
    while base_angle < std::f64::consts::TAU {
        let angle = direction.sign() * base_angle + azimuth_offset;
        let endpoint = origin + rotate(radial_vector, angle)?;
        let is_background = index.is_multiple_of(ratio);
        let style = if is_background {
            background_style
        } else {
            faded_style
        };
        let member = PolarPlaneRadialLine {
            angle,
            snapshot: styled_line(origin, endpoint, style)?,
        };
        if is_background {
            background.push(member);
        } else {
            faded.push(member);
        }
        base_angle += angular_step;
        index += 1;
    }

    Ok((background, faded))
}

fn rotate(vector: Vec2, angle: f64) -> Result<Vec2, PolarPlaneAuthoringError> {
    let angle = checked_f32(angle)?;
    let (sin, cos) = angle.sin_cos();
    let rotated = Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    );
    if rotated.x.is_finite() && rotated.y.is_finite() {
        Ok(rotated)
    } else {
        Err(PolarPlaneAuthoringError::NonFiniteGeometry)
    }
}

fn styled_circle(
    center: Vec2,
    radius: f32,
    style: NumberPlaneLineStyle,
) -> Result<ObjectSnapshot, PolarPlaneAuthoringError> {
    let (color, width) = lowered_style(style)?;
    Ok(Circle::new(radius)
        .move_to(center)
        .set_stroke(Some(color), Some(width))
        .into_snapshot())
}

fn styled_line(
    start: Vec2,
    end: Vec2,
    style: NumberPlaneLineStyle,
) -> Result<ObjectSnapshot, PolarPlaneAuthoringError> {
    let (color, width) = lowered_style(style)?;
    Ok(Line::new(start, end)
        .color(color)
        .set_stroke(Some(color), Some(width))
        .into_snapshot())
}

fn lowered_style(style: NumberPlaneLineStyle) -> Result<(Color, f32), PolarPlaneAuthoringError> {
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

fn validate_polar_axes(axes: Axes2DState) -> Result<(), PolarPlaneAuthoringError> {
    let x = axes.x_axis().range();
    let y = axes.y_axis().range();
    let symmetric = approximately_equal(x.min(), -x.max())
        && approximately_equal(y.min(), -y.max())
        && approximately_equal(x.max(), y.max())
        && approximately_equal(x.step(), y.step())
        && approximately_equal(axes.x_axis().unit_size(), axes.y_axis().unit_size());
    if !symmetric || x.max() <= 0.0 {
        return Err(PolarPlaneAuthoringError::NonPolarAxes);
    }
    Ok(())
}

fn approximately_equal(lhs: f64, rhs: f64) -> bool {
    let scale = lhs.abs().max(rhs.abs()).max(1.0);
    (lhs - rhs).abs() <= AXIS_TOLERANCE * scale
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
    Coordinates(crate::CoordinateSystemError),
    NonPolarAxes,
    InvalidRadiusStep(f64),
    InvalidAzimuthStep(f64),
    InvalidAzimuthOffset(f64),
    InvalidAngularStep(f64),
    InvalidStrokeWidth(f64),
    InvalidStrokeOpacity(f64),
    NonFiniteColor,
    NonFiniteGeometry,
    Overflow(f64),
}

impl std::fmt::Display for PolarPlaneAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinates(error) => error.fmt(formatter),
            Self::NonPolarAxes => formatter.write_str(
                "PolarPlane requires symmetric x/y ranges with equal step and unit size",
            ),
            Self::InvalidRadiusStep(value) => {
                write!(formatter, "invalid PolarPlane radial step: {value}")
            }
            Self::InvalidAzimuthStep(value) => {
                write!(formatter, "invalid PolarPlane azimuth divisions: {value}")
            }
            Self::InvalidAzimuthOffset(value) => {
                write!(formatter, "invalid PolarPlane azimuth offset: {value}")
            }
            Self::InvalidAngularStep(value) => {
                write!(formatter, "invalid PolarPlane angular step: {value}")
            }
            Self::InvalidStrokeWidth(value) => {
                write!(formatter, "invalid PolarPlane stroke width: {value}")
            }
            Self::InvalidStrokeOpacity(value) => {
                write!(formatter, "invalid PolarPlane stroke opacity: {value}")
            }
            Self::NonFiniteColor => formatter.write_str("PolarPlane grid color must be finite"),
            Self::NonFiniteGeometry => formatter.write_str("PolarPlane geometry must be finite"),
            Self::Overflow(value) => write!(
                formatter,
                "PolarPlane geometry cannot be represented as f32: {value}"
            ),
        }
    }
}

impl std::error::Error for PolarPlaneAuthoringError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberRange;
    use noon_core::{GeometryRef, BLUE_D, RED};

    fn axes(radius_max: f64, radius_step: f64, size: f32) -> Axes2DState {
        let range = NumberRange::new(-radius_max, radius_max, radius_step).unwrap();
        Axes2DState::new(range, range, size, size).unwrap()
    }

    fn style(color: Color, width: f64, opacity: f64) -> NumberPlaneLineStyle {
        NumberPlaneLineStyle::new(color, width, opacity)
    }

    fn plan_with_direction(
        radius_max: f64,
        radius_step: f64,
        size: f32,
        azimuth_step: f64,
        direction: PolarPlaneAzimuthDirection,
        azimuth_offset: f64,
        ratio: usize,
    ) -> PolarPlaneGridPlan {
        PolarPlaneGridPlan::new(
            axes(radius_max, radius_step, size),
            azimuth_step,
            direction,
            azimuth_offset,
            ratio,
            style(BLUE_D, 2.0, 1.0),
            style(BLUE_D, 1.0, 0.5),
        )
        .unwrap()
    }

    fn plan(
        radius_max: f64,
        radius_step: f64,
        size: f32,
        azimuth_step: f64,
        azimuth_offset: f64,
        ratio: usize,
    ) -> PolarPlaneGridPlan {
        plan_with_direction(
            radius_max,
            radius_step,
            size,
            azimuth_step,
            PolarPlaneAzimuthDirection::CounterClockwise,
            azimuth_offset,
            ratio,
        )
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn circle_scene_radius(circle: &PolarPlaneCircle) -> f64 {
        let GeometryRef::Circle { radius } = &circle.snapshot().geometry else {
            panic!("PolarPlane circles must stay analytic Circle geometry");
        };
        f64::from(*radius)
    }

    fn line_points(line: &PolarPlaneRadialLine) -> (Vec2, Vec2) {
        let snapshot = line.snapshot();
        let GeometryRef::Line { start, end } = &snapshot.geometry else {
            panic!("PolarPlane radial members must lower to retained Line geometry");
        };
        (
            snapshot.transform.transform_point(*start),
            snapshot.transform.transform_point(*end),
        )
    }

    #[test]
    fn ratio_two_matches_independent_circle_and_radial_classification() {
        let plan = plan(4.0, 1.0, 8.0, 4.0, 0.0, 2);
        assert_eq!(
            plan.circles()
                .iter()
                .map(PolarPlaneCircle::radius)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(
            plan.faded_circles()
                .iter()
                .map(PolarPlaneCircle::radius)
                .collect::<Vec<_>>(),
            vec![0.5, 1.5, 2.5, 3.5]
        );
        assert_eq!(plan.radial_lines().len(), 4);
        assert_eq!(plan.faded_radial_lines().len(), 4);
        assert_eq!(plan.background_member_count(), 9);
        assert_eq!(plan.faded_member_count(), 8);
    }

    #[test]
    fn zero_ratio_is_exactly_ratio_one() {
        assert_eq!(
            plan(3.0, 1.0, 6.0, 6.0, 0.2, 0),
            plan(3.0, 1.0, 6.0, 6.0, 0.2, 1)
        );
    }

    #[test]
    fn custom_size_scales_circles_and_radials_through_number_line_unit_size() {
        let plan = plan(4.0, 1.0, 12.0, 4.0, 0.0, 1);
        assert_close(circle_scene_radius(plan.circles().last().unwrap()), 6.0);
        let (start, end) = line_points(&plan.radial_lines()[0]);
        assert_close(f64::from(start.x), 0.0);
        assert_close(f64::from(start.y), 0.0);
        assert_close(f64::from(end.x), 6.0);
        assert_close(f64::from(end.y), 0.0);
    }

    #[test]
    fn direction_and_offset_are_owned_by_the_shared_radial_planner() {
        let ccw = plan_with_direction(
            2.0,
            1.0,
            4.0,
            4.0,
            PolarPlaneAzimuthDirection::CounterClockwise,
            0.25,
            1,
        );
        let cw = plan_with_direction(
            2.0,
            1.0,
            4.0,
            4.0,
            PolarPlaneAzimuthDirection::Clockwise,
            0.25,
            1,
        );
        assert_close(
            ccw.radial_lines()[1].angle(),
            std::f64::consts::FRAC_PI_2 + 0.25,
        );
        assert_close(
            cw.radial_lines()[1].angle(),
            -std::f64::consts::FRAC_PI_2 + 0.25,
        );
        assert_eq!(ccw.radial_lines().len(), cw.radial_lines().len());
    }

    #[test]
    fn non_integer_azimuth_divisions_keep_the_final_partial_arange_member() {
        let plan = plan(2.0, 1.0, 4.0, 3.5, 0.0, 1);
        assert_eq!(plan.radial_lines().len(), 4);
        let last = plan.radial_lines().last().unwrap().angle();
        assert!(last < std::f64::consts::TAU);
        assert!(last > 3.0 * std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn retained_order_and_styles_use_only_existing_geometry_kinds() {
        let plan = PolarPlaneGridPlan::new(
            axes(2.0, 1.0, 4.0),
            4.0,
            PolarPlaneAzimuthDirection::CounterClockwise,
            0.0,
            2,
            style(BLUE_D, 2.0, 1.0),
            style(RED, 6.0, 0.25),
        )
        .unwrap();
        let ordered = plan.snapshots_in_manim_order().collect::<Vec<_>>();
        assert_eq!(ordered.len(), 13);

        let ordinary_circle_start = plan.radial_lines().len();
        let faded_radial_start = ordinary_circle_start + plan.circles().len();
        assert!(matches!(ordered[0].geometry, GeometryRef::Line { .. }));
        assert!(matches!(
            ordered[ordinary_circle_start].geometry,
            GeometryRef::Circle { .. }
        ));
        assert!(matches!(
            ordered[faded_radial_start].geometry,
            GeometryRef::Line { .. }
        ));
        assert!(matches!(
            ordered.last().unwrap().geometry,
            GeometryRef::Circle { .. }
        ));
        assert_close(
            f64::from(plan.radial_lines()[0].snapshot().style.stroke_width),
            0.02,
        );
        assert_close(
            f64::from(
                plan.faded_radial_lines()[0]
                    .snapshot()
                    .style
                    .stroke_width,
            ),
            0.06,
        );
        assert_close(
            f64::from(
                plan.faded_circles()[0]
                    .snapshot()
                    .style
                    .stroke
                    .unwrap()
                    .alpha,
            ),
            0.25,
        );
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        assert_eq!(
            PolarPlaneGridPlan::new(
                axes(2.0, 1.0, 4.0),
                0.0,
                PolarPlaneAzimuthDirection::CounterClockwise,
                0.0,
                1,
                style(BLUE_D, 2.0, 1.0),
                style(BLUE_D, 1.0, 0.5),
            ),
            Err(PolarPlaneAuthoringError::InvalidAzimuthStep(0.0))
        );

        let asymmetric = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-3.0, 3.0, 1.0).unwrap(),
            4.0,
            6.0,
        )
        .unwrap();
        assert!(matches!(
            PolarPlaneGridPlan::new(
                asymmetric,
                4.0,
                PolarPlaneAzimuthDirection::CounterClockwise,
                0.0,
                1,
                style(BLUE_D, 2.0, 1.0),
                style(BLUE_D, 1.0, 0.5),
            ),
            Err(PolarPlaneAuthoringError::NonPolarAxes)
        ));
    }
}
