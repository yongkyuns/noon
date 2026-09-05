use serde::{Deserialize, Serialize};

use crate::{Color, PathCommand, StrokeCap, StrokeJoin, Style, Vec2, VectorPath};

/// High-precision authoring vector. The current renderer remains 2D/f32; this
/// type prevents frontend compatibility from being constrained by that backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl SemanticVec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const fn from_vec2(value: Vec2) -> Self {
        Self::new(value.x as f64, value.y as f64, 0.0)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Explicit render lowering. Z participates in semantic ordering but the
    /// current 2D backend consumes only x/y coordinates.
    pub fn lower_xy_f32(self) -> Result<Vec2, SemanticLoweringError> {
        if !self.is_finite() {
            return Err(SemanticLoweringError::NonFiniteVector(self));
        }
        if self.x.abs() > f32::MAX as f64 || self.y.abs() > f32::MAX as f64 {
            return Err(SemanticLoweringError::CoordinateOutOfRange(self));
        }
        Ok(Vec2::new(self.x as f32, self.y as f32))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticTransform2_5D {
    pub translation: SemanticVec3,
    pub scale: SemanticVec3,
    /// Rotation around the renderer-facing z axis in radians.
    pub rotation_z: f64,
}

impl Default for SemanticTransform2_5D {
    fn default() -> Self {
        Self {
            translation: SemanticVec3::ZERO,
            scale: SemanticVec3::new(1.0, 1.0, 1.0),
            rotation_z: 0.0,
        }
    }
}

/// Paint representation is intentionally extensible before gradients/text enter
/// the public API. The renderer may specialize solid colors aggressively.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPaint {
    Solid(Color),
    /// Stable resource identity for a future gradient/pattern paint resource.
    Resource(u64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeWidthMode {
    /// Compatibility mode: geometry/object scaling also scales stroke width.
    #[default]
    ScaleWithObject,
    /// Stroke width is invariant to object transforms; camera projection still
    /// maps the authored scene-space width to pixels.
    ScreenSpace,
}

impl StrokeWidthMode {
    pub const fn is_scale_with_object(&self) -> bool {
        matches!(self, Self::ScaleWithObject)
    }
}

/// Authoring-level style. Fill/stroke opacity and overall object opacity are
/// independent rather than collapsed into one legacy opacity value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticStyle {
    pub fill: Option<SemanticPaint>,
    pub fill_opacity: f64,
    pub stroke: Option<SemanticPaint>,
    pub stroke_opacity: f64,
    pub stroke_width: f64,
    pub stroke_width_mode: StrokeWidthMode,
    #[serde(default)]
    pub stroke_join: StrokeJoin,
    #[serde(default)]
    pub stroke_cap: StrokeCap,
    pub object_opacity: f64,
}

impl Default for SemanticStyle {
    fn default() -> Self {
        Self {
            fill: Some(SemanticPaint::Solid(Color::WHITE)),
            fill_opacity: 1.0,
            stroke: None,
            stroke_opacity: 1.0,
            stroke_width: 0.0,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: StrokeJoin::Round,
            stroke_cap: StrokeCap::Round,
            object_opacity: 1.0,
        }
    }
}

impl SemanticStyle {
    /// Compatibility adapter. Legacy `Style::opacity` becomes overall object
    /// opacity; existing color alpha remains part of each solid paint.
    pub fn from_legacy(style: Style) -> Self {
        Self {
            fill: style.fill.map(SemanticPaint::Solid),
            fill_opacity: 1.0,
            stroke: style.stroke.map(SemanticPaint::Solid),
            stroke_opacity: 1.0,
            stroke_width: style.stroke_width as f64,
            stroke_width_mode: style.stroke_width_mode,
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            object_opacity: style.opacity as f64,
        }
    }

    pub fn effective_fill_opacity(&self) -> f64 {
        self.fill_opacity * self.object_opacity
    }

    pub fn effective_stroke_opacity(&self) -> f64 {
        self.stroke_opacity * self.object_opacity
    }
}

/// Painter metadata is independent from transform hierarchy and style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticPresentation {
    pub z_index: i32,
    /// Stable insertion/painter tie-break assigned by the semantic store.
    pub insertion_order: u64,
}

impl SemanticPresentation {
    pub const fn order_key(self) -> (i32, u64) {
        (self.z_index, self.insertion_order)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds2D64 {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds2D64 {
    pub fn point(x: f64, y: f64) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    pub fn include(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    pub fn expand(self, amount: f64) -> Self {
        Self {
            min_x: self.min_x - amount,
            min_y: self.min_y - amount,
            max_x: self.max_x + amount,
            max_y: self.max_y + amount,
        }
    }

    pub fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(self) -> f64 {
        self.max_y - self.min_y
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticBounds {
    /// Tight geometry/layout bounds using curve extrema.
    pub layout: Option<Bounds2D64>,
    /// Cheap conservative bounds suitable for culling/index invalidation.
    pub conservative: Option<Bounds2D64>,
}

/// Calculate separate tight layout and conservative culling bounds for a path.
///
/// The conservative form includes Bezier control points. The layout form solves
/// quadratic/cubic extrema so layout operations do not inherit control-hull
/// overestimation.
pub fn semantic_path_bounds(path: &VectorPath, stroke_width: f64) -> SemanticBounds {
    let conservative = path
        .conservative_bounds()
        .map(|bounds| Bounds2D64 {
            min_x: bounds.min.x as f64,
            min_y: bounds.min.y as f64,
            max_x: bounds.max.x as f64,
            max_y: bounds.max.y as f64,
        })
        .map(|bounds| bounds.expand(stroke_width.max(0.0) * 0.5));

    let mut layout: Option<Bounds2D64> = None;
    let mut current: Option<(f64, f64)> = None;
    let mut subpath_start: Option<(f64, f64)> = None;

    let include = |bounds: &mut Option<Bounds2D64>, x: f64, y: f64| {
        if let Some(bounds) = bounds {
            bounds.include(x, y);
        } else {
            *bounds = Some(Bounds2D64::point(x, y));
        }
    };

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                let point = (to.x as f64, to.y as f64);
                include(&mut layout, point.0, point.1);
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCommand::LineTo { to } => {
                let end = (to.x as f64, to.y as f64);
                if let Some(start) = current {
                    include(&mut layout, start.0, start.1);
                }
                include(&mut layout, end.0, end.1);
                current = Some(end);
            }
            PathCommand::QuadraticTo { control, to } => {
                let Some(start) = current else {
                    let end = (to.x as f64, to.y as f64);
                    include(&mut layout, end.0, end.1);
                    current = Some(end);
                    continue;
                };
                let control = (control.x as f64, control.y as f64);
                let end = (to.x as f64, to.y as f64);
                include(&mut layout, start.0, start.1);
                include(&mut layout, end.0, end.1);
                include_quadratic_extrema(&mut layout, start, control, end);
                current = Some(end);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let Some(start) = current else {
                    let end = (to.x as f64, to.y as f64);
                    include(&mut layout, end.0, end.1);
                    current = Some(end);
                    continue;
                };
                let control1 = (control1.x as f64, control1.y as f64);
                let control2 = (control2.x as f64, control2.y as f64);
                let end = (to.x as f64, to.y as f64);
                include(&mut layout, start.0, start.1);
                include(&mut layout, end.0, end.1);
                include_cubic_extrema(&mut layout, start, control1, control2, end);
                current = Some(end);
            }
            PathCommand::Close => {
                if let (Some(start), Some(end)) = (subpath_start, current) {
                    include(&mut layout, start.0, start.1);
                    include(&mut layout, end.0, end.1);
                    current = Some(start);
                }
            }
        }
    }

    let layout = layout.map(|bounds| bounds.expand(stroke_width.max(0.0) * 0.5));
    SemanticBounds {
        layout,
        conservative,
    }
}

fn include_quadratic_extrema(
    bounds: &mut Option<Bounds2D64>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
) {
    for axis in 0..2 {
        let (a, b, c) = if axis == 0 {
            (p0.0, p1.0, p2.0)
        } else {
            (p0.1, p1.1, p2.1)
        };
        let denominator = a - 2.0 * b + c;
        if denominator.abs() <= f64::EPSILON {
            continue;
        }
        let t = (a - b) / denominator;
        if (0.0..1.0).contains(&t) {
            let point = quadratic_point(p0, p1, p2, t);
            if let Some(bounds) = bounds {
                bounds.include(point.0, point.1);
            }
        }
    }
}

fn include_cubic_extrema(
    bounds: &mut Option<Bounds2D64>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) {
    let mut roots = Vec::with_capacity(4);
    cubic_derivative_roots(p0.0, p1.0, p2.0, p3.0, &mut roots);
    cubic_derivative_roots(p0.1, p1.1, p2.1, p3.1, &mut roots);
    for t in roots {
        if (0.0..1.0).contains(&t) {
            let point = cubic_point(p0, p1, p2, p3, t);
            if let Some(bounds) = bounds {
                bounds.include(point.0, point.1);
            }
        }
    }
}

fn cubic_derivative_roots(p0: f64, p1: f64, p2: f64, p3: f64, roots: &mut Vec<f64>) {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    if a.abs() <= f64::EPSILON {
        if b.abs() > f64::EPSILON {
            roots.push(-c / b);
        }
        return;
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return;
    }
    let root = discriminant.sqrt();
    roots.push((-b + root) / (2.0 * a));
    if root > f64::EPSILON {
        roots.push((-b - root) / (2.0 * a));
    }
}

fn quadratic_point(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}

fn cubic_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
        u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticLoweringError {
    NonFiniteVector(SemanticVec3),
    CoordinateOutOfRange(SemanticVec3),
}

impl std::fmt::Display for SemanticLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteVector(value) => {
                write!(formatter, "non-finite semantic vector {value:?}")
            }
            Self::CoordinateOutOfRange(value) => {
                write!(
                    formatter,
                    "semantic vector cannot lower to f32 renderer coordinates: {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for SemanticLoweringError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3_authoring_keeps_z_while_2d_lowering_is_explicit() {
        let value = SemanticVec3::new(1.25, -2.5, 7.0);
        assert_eq!(value.lower_xy_f32().unwrap(), Vec2::new(1.25, -2.5));
        assert_eq!(value.z, 7.0);
    }

    #[test]
    fn style_opacity_layers_remain_independent() {
        let style = SemanticStyle {
            fill_opacity: 0.5,
            stroke_opacity: 0.25,
            object_opacity: 0.4,
            ..SemanticStyle::default()
        };
        assert!((style.effective_fill_opacity() - 0.2).abs() < 1e-12);
        assert!((style.effective_stroke_opacity() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn style_preserves_legacy_stroke_topology() {
        let semantic = SemanticStyle::from_legacy(Style {
            stroke_join: StrokeJoin::Bevel,
            stroke_cap: StrokeCap::Square,
            ..Style::default()
        });

        assert_eq!(semantic.stroke_join, StrokeJoin::Bevel);
        assert_eq!(semantic.stroke_cap, StrokeCap::Square);
    }

    #[test]
    fn painter_key_uses_z_then_stable_insertion_order() {
        assert!(
            SemanticPresentation {
                z_index: 2,
                insertion_order: 0,
            }
            .order_key()
                > SemanticPresentation {
                    z_index: 1,
                    insertion_order: 100,
                }
                .order_key()
        );
    }

    #[test]
    fn quadratic_layout_bounds_use_curve_extremum_not_control_hull() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0));
        let bounds = semantic_path_bounds(&path, 0.0);
        let layout = bounds.layout.unwrap();
        let conservative = bounds.conservative.unwrap();
        assert!((layout.max_y - 1.0).abs() < 1e-12);
        assert_eq!(conservative.max_y, 2.0);
    }

    #[test]
    fn stroke_expansion_is_explicit_in_both_bounds_classes() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(2.0, 0.0));
        let bounds = semantic_path_bounds(&path, 0.5);
        assert_eq!(bounds.layout.unwrap().min_y, -0.25);
        assert_eq!(bounds.conservative.unwrap().max_y, 0.25);
    }
}
