//! Pure dash segmentation for the shared straight-line semantic constructor.
use crate::arc_authoring::authored_f32;
use crate::AuthoringError;
use noon_core::{GeometryRef, Vec2, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DashedLineAuthoringError {
    NonFiniteStart(Vec2),
    NonFiniteEnd(Vec2),
    NonFiniteLineLength,
    InvalidDashLength(f64),
    InvalidDashedRatio(f64),
    DashCountOverflow(f64),
}

impl std::fmt::Display for DashedLineAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteStart(point) => {
                write!(formatter, "dashed line start must be finite, got {point:?}")
            }
            Self::NonFiniteEnd(point) => {
                write!(formatter, "dashed line end must be finite, got {point:?}")
            }
            Self::NonFiniteLineLength => write!(formatter, "dashed line length must be finite"),
            Self::InvalidDashLength(value) => {
                write!(
                    formatter,
                    "dash length must be positive and finite, got {value}"
                )
            }
            Self::InvalidDashedRatio(value) => write!(
                formatter,
                "dashed ratio must be finite and within [0, 1], got {value}"
            ),
            Self::DashCountOverflow(value) => {
                write!(
                    formatter,
                    "dashed line requires an unrepresentable dash count {value}"
                )
            }
        }
    }
}

impl std::error::Error for DashedLineAuthoringError {}

fn dashed_line_path(
    start: Vec2,
    end: Vec2,
    dash_length: f64,
    dashed_ratio: f64,
) -> Result<(VectorPath, usize), DashedLineAuthoringError> {
    if !point_is_finite(start) {
        return Err(DashedLineAuthoringError::NonFiniteStart(start));
    }
    if !point_is_finite(end) {
        return Err(DashedLineAuthoringError::NonFiniteEnd(end));
    }
    if !dash_length.is_finite() || dash_length <= 0.0 {
        return Err(DashedLineAuthoringError::InvalidDashLength(dash_length));
    }
    if !dashed_ratio.is_finite() || !(0.0..=1.0).contains(&dashed_ratio) {
        return Err(DashedLineAuthoringError::InvalidDashedRatio(dashed_ratio));
    }

    // Manim stores VMobject points in float64. Keep length and dash-proportion
    // arithmetic at the same precision, then quantize only the retained Vec2.
    // Computing end - start in f64 also avoids overflow for finite f32 endpoints.
    let delta_x = f64::from(end.x) - f64::from(start.x);
    let delta_y = f64::from(end.y) - f64::from(start.y);
    let length = delta_x.hypot(delta_y);
    if !length.is_finite() {
        return Err(DashedLineAuthoringError::NonFiniteLineLength);
    }

    // ManimCE v0.21: max(2, ceil(length / dash_length * dashed_ratio)).
    let requested = (length / dash_length * dashed_ratio).ceil().max(2.0);
    // Keep acceptance identical on 64-bit native and wasm32. Every u32 value
    // is also exactly representable as f64, so subsequent proportion math does
    // not introduce a target-dependent integer conversion boundary.
    if !requested.is_finite() || requested > f64::from(u32::MAX) {
        return Err(DashedLineAuthoringError::DashCountOverflow(requested));
    }
    let num_dashes = requested as usize;

    // DashedVMobject's default equal-length path is exact for a straight line.
    // Open curves start and end with a dash, so n dashes have n-1 equal gaps.
    let dash_fraction = dashed_ratio / num_dashes as f64;
    let gap_fraction = (1.0 - dashed_ratio) / (num_dashes - 1) as f64;
    let period = dash_fraction + gap_fraction;

    let mut path = VectorPath::new();
    for index in 0..num_dashes {
        let start_fraction = index as f64 * period;
        let end_fraction = (start_fraction + dash_fraction).min(1.0);
        path = path
            .move_to(interpolate(start, end, start_fraction))
            .line_to(interpolate(start, end, end_fraction));
    }
    Ok((path, num_dashes))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dashed_line_geometry(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    dash_length: f64,
    dashed_ratio: f64,
) -> Result<GeometryRef, AuthoringError> {
    let (path, _) = dashed_line_path(
        Vec2::new(
            authored_f32(start_x, "dashed line start x")?,
            authored_f32(start_y, "dashed line start y")?,
        ),
        Vec2::new(
            authored_f32(end_x, "dashed line end x")?,
            authored_f32(end_y, "dashed line end y")?,
        ),
        dash_length,
        dashed_ratio,
    )
    .map_err(AuthoringError::from)?;
    Ok(GeometryRef::VectorPath(path))
}

fn point_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn interpolate(start: Vec2, end: Vec2, alpha: f64) -> Vec2 {
    let x = f64::from(start.x) + (f64::from(end.x) - f64::from(start.x)) * alpha;
    let y = f64::from(start.y) + (f64::from(end.y) - f64::from(start.y)) * alpha;
    Vec2::new(x as f32, y as f32)
}
