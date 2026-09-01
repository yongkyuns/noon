//! Shared Rust authoring semantics for Manim-compatible straight `DashedLine` geometry.
//!
//! ManimCE constructs a straight `Line`, computes a dash count from the line's
//! length, requested dash length, and dashed ratio, then replaces the source line
//! with equally spaced partial subcurves. For a straight line, arc-length and path
//! proportion are identical, so Noon can retain the exact visible result as one
//! [`VectorPath`] containing ordered line subpaths. No renderer-specific dash
//! primitive or frontend-owned segmentation is required.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, ObjectSnapshot, Vec2, VectorPath};

pub const DEFAULT_DASH_LENGTH: f64 = 0.05;
pub const DEFAULT_DASHED_RATIO: f64 = 0.5;

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

/// ManimCE-compatible dashed straight line represented by retained path subpaths.
#[derive(Clone, Debug, PartialEq)]
pub struct DashedLine {
    snapshot: ObjectSnapshot,
    num_dashes: usize,
}

impl DashedLine {
    pub fn new(start: Vec2, end: Vec2) -> Result<Self, DashedLineAuthoringError> {
        Self::with_options(start, end, DEFAULT_DASH_LENGTH, DEFAULT_DASHED_RATIO)
    }

    /// Construct the straight point-to-point subset of ManimCE v0.21 `DashedLine`.
    ///
    /// This intentionally does not synthesize `Line`'s `buff`, `path_arc`, or
    /// mobject-boundary semantics. Those belong to the shared Line facade and can
    /// be composed here once that constructor surface is canonicalized.
    ///
    /// Dash scalars stay `f64` because Manim/Python evaluates the dash-count
    /// `ceil()` at double precision. Retained coordinates are quantized to `Vec2`
    /// only after that semantic decision has been made.
    pub fn with_options(
        start: Vec2,
        end: Vec2,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<Self, DashedLineAuthoringError> {
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

        Ok(Self {
            snapshot: Path::new(path).into_snapshot(),
            num_dashes,
        })
    }

    pub const fn num_dashes(&self) -> usize {
        self.num_dashes
    }

    pub fn color(mut self, color: Color) -> Self {
        let fill_alpha = self.snapshot.style.fill.map_or(0.0, |fill| fill.alpha);
        let mut fill = color;
        fill.alpha = fill_alpha;
        self.snapshot.style.fill = Some(fill);
        self.snapshot.style.stroke = Some(color);
        self
    }

    pub fn shift(mut self, offset: Vec2) -> Self {
        self.snapshot = self.snapshot.shift(offset);
        self
    }

    pub fn move_to(mut self, point: Vec2) -> Self {
        self.snapshot = self.snapshot.move_to(point);
        self
    }

    pub fn scale(mut self, factor: f32) -> Self {
        self.snapshot = self.snapshot.scale_by(factor);
        self
    }

    pub fn scale_xy(mut self, factor: Vec2) -> Self {
        self.snapshot = self.snapshot.scale_xy(factor);
        self
    }

    pub fn rotate(mut self, angle: f32) -> Self {
        self.snapshot = self.snapshot.rotate_by(angle);
        self
    }

    pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
        self.snapshot = self.snapshot.set_fill(color, opacity);
        self
    }

    pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
        self.snapshot = self.snapshot.set_stroke(color, width);
        self
    }

    pub fn set_opacity(mut self, opacity: f32) -> Self {
        self.snapshot = self.snapshot.set_opacity(opacity);
        self
    }

    pub const fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
}

impl IntoSnapshot for DashedLine {
    fn into_snapshot(self) -> ObjectSnapshot {
        self.snapshot
    }
}

fn point_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn interpolate(start: Vec2, end: Vec2, alpha: f64) -> Vec2 {
    let x = f64::from(start.x) + (f64::from(end.x) - f64::from(start.x)) * alpha;
    let y = f64::from(start.y) + (f64::from(end.y) - f64::from(start.y)) * alpha;
    Vec2::new(x as f32, y as f32)
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode, WHITE};

    use super::*;

    fn commands(line: &DashedLine) -> &[PathCommand] {
        match &line.snapshot().geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected retained VectorPath geometry, got {other:?}"),
        }
    }

    fn command_point(command: &PathCommand) -> Vec2 {
        match command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => *to,
            other => panic!("expected line-subpath command, got {other:?}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_point_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn default_dashed_line_matches_manim_dash_count_endpoints_and_style() {
        let line = DashedLine::new(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)).unwrap();
        let commands = commands(&line);

        assert_eq!(line.num_dashes(), 20);
        assert_eq!(commands.len(), 40);
        assert_point_close(command_point(&commands[0]), Vec2::new(-1.0, 0.0));
        assert_point_close(command_point(&commands[1]), Vec2::new(-0.95, 0.0));
        assert_point_close(command_point(&commands[38]), Vec2::new(0.95, 0.0));
        assert_point_close(command_point(&commands[39]), Vec2::new(1.0, 0.0));

        assert_eq!(line.snapshot().style.stroke, Some(WHITE));
        assert_eq!(line.snapshot().style.fill.map(|fill| fill.alpha), Some(0.0));
        assert_close(line.snapshot().style.stroke_width, 0.04);
        assert_eq!(
            line.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
        assert_eq!(line.snapshot().style.stroke_join, StrokeJoin::Miter);
        assert_eq!(line.snapshot().style.stroke_cap, StrokeCap::Butt);
    }

    #[test]
    fn custom_dash_length_preserves_open_curve_end_dashes() {
        let line =
            DashedLine::with_options(Vec2::new(-4.0, 0.0), Vec2::new(4.0, 0.0), 2.0, 0.5).unwrap();
        let commands = commands(&line);

        assert_eq!(line.num_dashes(), 2);
        assert_eq!(commands.len(), 4);
        assert_point_close(command_point(&commands[0]), Vec2::new(-4.0, 0.0));
        assert_point_close(command_point(&commands[1]), Vec2::new(-2.0, 0.0));
        assert_point_close(command_point(&commands[2]), Vec2::new(2.0, 0.0));
        assert_point_close(command_point(&commands[3]), Vec2::new(4.0, 0.0));
    }

    #[test]
    fn reduced_dashed_ratio_matches_manim_count_and_physical_dash_length() {
        let line = DashedLine::with_options(
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            DEFAULT_DASH_LENGTH,
            0.1,
        )
        .unwrap();
        let commands = commands(&line);

        assert_eq!(line.num_dashes(), 4);
        assert_close(
            (command_point(&commands[1]) - command_point(&commands[0])).length(),
            0.05,
        );
        assert_point_close(command_point(commands.last().unwrap()), Vec2::new(1.0, 0.0));
    }

    #[test]
    fn diagonal_dashes_interpolate_along_the_original_line() {
        let line =
            DashedLine::with_options(Vec2::new(1.0, 2.0), Vec2::new(4.0, 6.0), 1.25, 0.5).unwrap();
        let commands = commands(&line);

        assert_eq!(line.num_dashes(), 2);
        assert_point_close(command_point(&commands[0]), Vec2::new(1.0, 2.0));
        assert_point_close(command_point(&commands[1]), Vec2::new(1.75, 3.0));
        assert_point_close(command_point(&commands[2]), Vec2::new(3.25, 5.0));
        assert_point_close(command_point(&commands[3]), Vec2::new(4.0, 6.0));
    }

    #[test]
    fn dashed_ratio_endpoints_remain_deterministic() {
        let empty = DashedLine::with_options(Vec2::ZERO, Vec2::new(2.0, 0.0), 0.5, 0.0).unwrap();
        assert_eq!(empty.num_dashes(), 2);
        let (empty_dashes, empty_remainder) = commands(&empty).as_chunks::<2>();
        assert!(empty_remainder.is_empty());
        assert!(empty_dashes
            .iter()
            .all(|dash| command_point(&dash[0]) == command_point(&dash[1])));

        let solid = DashedLine::with_options(Vec2::ZERO, Vec2::new(2.0, 0.0), 0.5, 1.0).unwrap();
        assert_eq!(solid.num_dashes(), 4);
        let solid_commands = commands(&solid);
        assert_point_close(command_point(&solid_commands[0]), Vec2::ZERO);
        assert_point_close(
            command_point(solid_commands.last().unwrap()),
            Vec2::new(2.0, 0.0),
        );
        let (solid_dashes, solid_remainder) = solid_commands.as_chunks::<2>();
        assert!(solid_remainder.is_empty());
        for pair in solid_dashes.windows(2) {
            assert_point_close(command_point(&pair[0][1]), command_point(&pair[1][0]));
        }
    }

    #[test]
    fn rejects_invalid_constructor_inputs() {
        assert!(matches!(
            DashedLine::new(Vec2::new(f32::NAN, 0.0), Vec2::ZERO),
            Err(DashedLineAuthoringError::NonFiniteStart(_))
        ));
        assert!(matches!(
            DashedLine::new(Vec2::ZERO, Vec2::new(0.0, f32::INFINITY)),
            Err(DashedLineAuthoringError::NonFiniteEnd(_))
        ));
        assert!(matches!(
            DashedLine::with_options(Vec2::ZERO, Vec2::ONE, 0.0, 0.5),
            Err(DashedLineAuthoringError::InvalidDashLength(0.0))
        ));
        assert!(matches!(
            DashedLine::with_options(Vec2::ZERO, Vec2::ONE, f64::NAN, 0.5),
            Err(DashedLineAuthoringError::InvalidDashLength(value)) if value.is_nan()
        ));
        assert!(matches!(
            DashedLine::with_options(Vec2::ZERO, Vec2::ONE, 0.05, 1.1),
            Err(DashedLineAuthoringError::InvalidDashedRatio(value)) if value == 1.1
        ));
        assert!(matches!(
            DashedLine::with_options(Vec2::ZERO, Vec2::ONE, 0.05, f64::NAN),
            Err(DashedLineAuthoringError::InvalidDashedRatio(value)) if value.is_nan()
        ));
        assert!(matches!(
            DashedLine::with_options(
                Vec2::new(-f32::MAX, 0.0),
                Vec2::new(f32::MAX, 0.0),
                f64::MIN_POSITIVE,
                1.0,
            ),
            Err(DashedLineAuthoringError::DashCountOverflow(_))
        ));
    }
}
