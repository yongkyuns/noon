use serde::{Deserialize, Serialize};

use crate::{
    Color, PathCommand, SemanticRotation3D, SemanticWorldTransform3D, StrokeCap, StrokeJoin, Style,
    Vec2, VectorPath,
};

/// High-precision authoring vector. Renderers explicitly lower the semantic
/// coordinates to their GPU representation when preparing affected objects.
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

    /// An explicit planar projection; the semantic value retains all three axes.
    pub const fn xy(self) -> (f64, f64) {
        (self.x, self.y)
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

/// A Manim VMobject sheen direction, preserving whether the source supplied a
/// 2D or 3D vector. The direction is authored metadata even while sheen paint
/// itself is unavailable to the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticSheenDirection {
    pub value: SemanticVec3,
    pub dimensions: u8,
}

impl SemanticSheenDirection {
    pub const DEFAULT: Self = Self {
        value: SemanticVec3::new(-1.0, 1.0, 0.0),
        dimensions: 3,
    };

    pub const fn new(value: SemanticVec3, dimensions: u8) -> Self {
        Self { value, dimensions }
    }

    pub fn is_valid(self) -> bool {
        self.value.is_finite()
            && matches!(self.dimensions, 2 | 3)
            && (self.dimensions == 3 || self.value.z == 0.0)
    }

    pub fn components(self) -> Vec<f64> {
        let components = [self.value.x, self.value.y, self.value.z];
        components[..self.dimensions as usize].to_vec()
    }
}

impl Default for SemanticSheenDirection {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticTransform2_5D {
    pub translation: SemanticVec3,
    pub scale: SemanticVec3,
    /// Rotation around the renderer-facing z axis in radians.
    pub rotation_z: f64,
}

impl SemanticTransform2_5D {
    /// Apply the authored transform to a full semantic point. The rotation is
    /// around z; scale and translation retain their third-axis components.
    pub fn transform_xyz(self, point: SemanticVec3) -> SemanticVec3 {
        let (x, y) = self.transform_xy(point.x, point.y);
        SemanticVec3::new(x, y, point.z * self.scale.z + self.translation.z)
    }

    /// Transform one local 2D point through the authored scale, rotation, and
    /// translation without lowering it to renderer precision.
    pub fn transform_xy(self, x: f64, y: f64) -> (f64, f64) {
        let x = x * self.scale.x;
        let y = y * self.scale.y;
        let (sine, cosine) = self.rotation_z.sin_cos();
        (
            x * cosine - y * sine + self.translation.x,
            x * sine + y * cosine + self.translation.y,
        )
    }
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

/// The single authored object transform. Planar rotation keeps its original
/// scalar angle (including unwrapped endpoints); spatial rotation is a unit
/// quaternion and cannot be silently projected into 2D.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticTransform {
    pub translation: SemanticVec3,
    pub scale: SemanticVec3,
    pub orientation: SemanticOrientation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticOrientation {
    Planar(f64),
    Spatial(SemanticRotation3D),
}

impl Default for SemanticTransform {
    fn default() -> Self {
        Self {
            translation: SemanticVec3::ZERO,
            scale: SemanticVec3::new(1.0, 1.0, 1.0),
            orientation: SemanticOrientation::Planar(0.0),
        }
    }
}

impl From<SemanticTransform2_5D> for SemanticTransform {
    fn from(value: SemanticTransform2_5D) -> Self {
        Self {
            translation: value.translation,
            scale: value.scale,
            orientation: SemanticOrientation::Planar(value.rotation_z),
        }
    }
}

impl From<SemanticWorldTransform3D> for SemanticTransform {
    fn from(value: SemanticWorldTransform3D) -> Self {
        Self {
            translation: value.translation,
            scale: value.scale,
            orientation: SemanticOrientation::Spatial(value.rotation),
        }
    }
}

impl SemanticTransform {
    pub fn planar_rotation(self) -> Option<f64> {
        match self.orientation {
            SemanticOrientation::Planar(value) => Some(value),
            SemanticOrientation::Spatial(_) => None,
        }
    }

    pub fn is_valid(self) -> bool {
        self.translation.is_finite()
            && self.scale.is_finite()
            && match self.orientation {
                SemanticOrientation::Planar(angle) => angle.is_finite(),
                SemanticOrientation::Spatial(rotation) => rotation.is_valid(),
            }
    }

    /// Return the exact 2.5D projection only for planar orientation.
    pub fn as_planar(self) -> Option<SemanticTransform2_5D> {
        let SemanticOrientation::Planar(rotation_z) = self.orientation else {
            return None;
        };
        Some(SemanticTransform2_5D {
            translation: self.translation,
            scale: self.scale,
            rotation_z,
        })
    }

    pub fn world_transform(self) -> Option<SemanticWorldTransform3D> {
        if !self.is_valid() {
            return None;
        }
        let rotation = match self.orientation {
            SemanticOrientation::Planar(angle) => {
                SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), angle)?
            }
            SemanticOrientation::Spatial(rotation) => rotation,
        };
        Some(SemanticWorldTransform3D {
            translation: self.translation,
            rotation,
            scale: self.scale,
        })
    }

    /// Transform an authored local z=0 point and return its actual xy result.
    pub fn transform_xy(self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !self.is_valid() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        if matches!(self.orientation, SemanticOrientation::Planar(_)) {
            // Preserve the existing 2D scalar path exactly: this keeps authored
            // unwrapped angle behavior and avoids constructing/normalizing a
            // quaternion for every point in a layout or bounds query.
            let planar = self.as_planar()?;
            let result = planar.transform_xy(x, y);
            return (result.0.is_finite() && result.1.is_finite()).then_some(result);
        }
        let point = self
            .world_transform()?
            .transform_point(SemanticVec3::new(x, y, 0.0))?;
        Some((point.x, point.y))
    }
}

/// Paint representation is intentionally extensible before gradients/text enter
/// the public API. The renderer may specialize solid colors aggressively.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPaint {
    Solid(Color),
    /// Continuous local paint; opacity remains on the owning semantic style.
    LinearGradient(std::sync::Arc<crate::LinearGradient>),
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
    /// Solid stroke painted behind fill and foreground stroke.
    #[serde(default)]
    pub background_stroke: Option<SemanticPaint>,
    #[serde(default = "default_semantic_opacity")]
    pub background_stroke_opacity: f64,
    #[serde(default)]
    pub background_stroke_width: f64,
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
            background_stroke: Some(SemanticPaint::Solid(Color::BLACK)),
            background_stroke_opacity: 1.0,
            background_stroke_width: 0.0,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: StrokeJoin::Round,
            stroke_cap: StrokeCap::Round,
            object_opacity: 1.0,
        }
    }
}

impl SemanticStyle {
    /// Capture a lowered effective fill, retaining authored precision exactly
    /// when its value still lowers to the same paint and alpha.
    pub fn capture_effective_fill(
        &self,
        fill: Option<Color>,
        gradient: Option<&std::sync::Arc<crate::LinearGradient>>,
    ) -> (Option<SemanticPaint>, f64) {
        if let Some(gradient) = gradient {
            let alpha = fill.map_or(0.0, |color| color.alpha);
            let unchanged = matches!(&self.fill, Some(SemanticPaint::LinearGradient(authored))
                if authored == gradient && self.fill_opacity as f32 == alpha);
            return (
                Some(SemanticPaint::LinearGradient(gradient.clone())),
                if unchanged {
                    self.fill_opacity
                } else {
                    f64::from(alpha)
                },
            );
        }
        let lowered = match &self.fill {
            Some(SemanticPaint::Solid(color)) => {
                let mut color = *color;
                color.alpha = (f64::from(color.alpha) * self.fill_opacity) as f32;
                Some(color)
            }
            _ => None,
        };
        if lowered == fill && !matches!(self.fill, Some(SemanticPaint::LinearGradient(_))) {
            (self.fill.clone(), self.fill_opacity)
        } else {
            (fill.map(SemanticPaint::Solid), 1.0)
        }
    }
    /// Validate all numeric paint and style fields before publishing authored state.
    pub fn is_finite(&self) -> bool {
        let paint = |paint: Option<&SemanticPaint>| match paint {
            Some(SemanticPaint::Solid(color)) => {
                color.red.is_finite()
                    && color.green.is_finite()
                    && color.blue.is_finite()
                    && color.alpha.is_finite()
            }
            Some(SemanticPaint::LinearGradient(_) | SemanticPaint::Resource(_)) | None => true,
        };
        paint(self.fill.as_ref())
            && paint(self.stroke.as_ref())
            && paint(self.background_stroke.as_ref())
            && self.fill_opacity.is_finite()
            && self.stroke_opacity.is_finite()
            && self.stroke_width.is_finite()
            && self.background_stroke_opacity.is_finite()
            && self.background_stroke_width.is_finite()
            && self.object_opacity.is_finite()
    }

    /// Compatibility adapter. Legacy `Style::opacity` becomes overall object
    /// opacity; existing color alpha remains part of each solid paint.
    pub fn from_compact(style: Style) -> Self {
        Self {
            fill: style.fill.map(SemanticPaint::Solid),
            fill_opacity: 1.0,
            stroke: style.stroke.map(SemanticPaint::Solid),
            stroke_opacity: 1.0,
            stroke_width: style.stroke_width as f64,
            background_stroke: style.background_stroke.map(SemanticPaint::Solid),
            background_stroke_opacity: 1.0,
            background_stroke_width: style.background_stroke_width as f64,
            stroke_width_mode: style.stroke_width_mode,
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            object_opacity: style.opacity as f64,
        }
    }

    /// Default paint profile owned by a Manim VMobject family root.
    ///
    /// Unrendered roots retain a full vector style, exposed through VMobject
    /// getters and `family=False` mutations. Materializing root content moves
    /// that style into its ordinary object payload. Stroke widths use scene units, where
    /// the compatibility layer maps Manim's four-pixel default to `0.04`.
    pub fn manim_family_root(color: crate::Color) -> Self {
        Self {
            fill: Some(SemanticPaint::Solid(color)),
            fill_opacity: 0.0,
            stroke: Some(SemanticPaint::Solid(color)),
            stroke_opacity: 1.0,
            stroke_width: 0.04,
            background_stroke: Some(SemanticPaint::Solid(Color::BLACK)),
            background_stroke_opacity: 1.0,
            background_stroke_width: 0.0,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: StrokeJoin::Round,
            stroke_cap: StrokeCap::Auto,
            object_opacity: 1.0,
        }
    }

    /// Set the authored family color without changing channel enablement or opacity.
    pub fn set_family_color(&mut self, color: crate::Color) {
        for paint in [&mut self.fill, &mut self.stroke] {
            if paint.is_some() {
                *paint = Some(SemanticPaint::Solid(color));
            }
        }
    }

    /// Return Manim VMobject's root color selection from its fill/stroke profile.
    pub fn family_color(&self) -> Option<crate::Color> {
        let paint = if self.fill_opacity == 0.0 {
            self.stroke.as_ref()
        } else {
            self.fill.as_ref()
        }?;
        match paint {
            SemanticPaint::Solid(color) => Some(*color),
            SemanticPaint::LinearGradient(_) | SemanticPaint::Resource(_) => None,
        }
    }

    pub fn effective_fill_opacity(&self) -> f64 {
        self.fill_opacity * self.object_opacity
    }

    pub fn effective_stroke_opacity(&self) -> f64 {
        self.stroke_opacity * self.object_opacity
    }
}

fn default_semantic_opacity() -> f64 {
    1.0
}

/// Painter metadata is independent from transform hierarchy and style.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticPresentation {
    /// Drawing eligibility is independent from authored paint and query geometry.
    #[serde(
        default = "default_draw_enabled",
        skip_serializing_if = "is_draw_enabled_default"
    )]
    pub draw_enabled: bool,
    pub z_index: f64,
    /// Stable insertion/painter tie-break assigned by the semantic store.
    pub insertion_order: u64,
}

fn default_draw_enabled() -> bool {
    true
}
fn is_draw_enabled_default(value: &bool) -> bool {
    *value
}

impl Default for SemanticPresentation {
    fn default() -> Self {
        Self {
            draw_enabled: true,
            z_index: 0.0,
            insertion_order: 0,
        }
    }
}

/// Family identity remains stable when its root acquires ordinary content.
/// Unrendered roots retain only their paint/painter state; after materialization
/// the node's normal `SemanticObjectState` is the sole payload/style authority.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)] // Keep the common unrendered family allocation-free.
pub enum SemanticFamilyState {
    Unrendered(SemanticFamilyPresentation),
    ContentOwning { fallback_color: crate::Color },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticFamilyPresentation {
    pub presentation: SemanticPresentation,
    pub own_color: crate::Color,
    pub own_style: SemanticStyle,
    pub own_sheen_direction: SemanticSheenDirection,
}

impl Default for SemanticFamilyState {
    fn default() -> Self {
        Self::Unrendered(SemanticFamilyPresentation::default())
    }
}

impl Default for SemanticFamilyPresentation {
    fn default() -> Self {
        Self {
            presentation: SemanticPresentation::default(),
            own_color: crate::Color::WHITE,
            own_style: SemanticStyle::manim_family_root(crate::Color::WHITE),
            own_sheen_direction: SemanticSheenDirection::DEFAULT,
        }
    }
}

impl SemanticFamilyPresentation {
    pub(crate) fn with_content(
        &self,
        content: super::SemanticObjectContent,
    ) -> super::SemanticObjectState {
        let mut state = super::SemanticObjectState::new(content);
        state.style = self.own_style.clone();
        state.sheen_direction = self.own_sheen_direction;
        state.set_draw_enabled(self.presentation.draw_enabled);
        state.set_z_index(self.presentation.z_index);
        state.assign_insertion_order(self.presentation.insertion_order);
        state
    }
}

impl SemanticPresentation {
    pub const fn order_key(self) -> (f64, u64) {
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
        .semantic_control_bounds()
        .map(|bounds| bounds.expand(stroke_width.max(0.0) * 0.5));
    if let Some(points) = path.manim_points() {
        let mut layout: Option<Bounds2D64> = None;
        let include = |bounds: &mut Option<Bounds2D64>, point: SemanticVec3| {
            if let Some(bounds) = bounds {
                bounds.include(point.x, point.y);
            } else {
                *bounds = Some(Bounds2D64::point(point.x, point.y));
            }
        };
        for curve in points.as_chunks::<4>().0 {
            include(&mut layout, curve[0]);
            include(&mut layout, curve[3]);
            include_cubic_extrema(
                &mut layout,
                curve[0].xy(),
                curve[1].xy(),
                curve[2].xy(),
                curve[3].xy(),
            );
        }
        if let Some(&tail) = points.get(points.len() / 4 * 4) {
            include(&mut layout, tail);
        }
        return SemanticBounds {
            layout: layout.map(|bounds| bounds.expand(stroke_width.max(0.0) * 0.5)),
            conservative,
        };
    }

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
        let semantic = SemanticStyle::from_compact(Style {
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
                draw_enabled: true,
                z_index: 2.0,
                insertion_order: 0,
            }
            .order_key()
                > SemanticPresentation {
                    draw_enabled: true,
                    z_index: 1.0,
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

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn planar_transform_preserves_unwrapped_angle_and_2d_mapping() {
        let transform = SemanticTransform {
            translation: SemanticVec3::new(2.0, -3.0, 4.0),
            scale: SemanticVec3::new(2.0, 3.0, 1.0),
            orientation: SemanticOrientation::Planar(std::f64::consts::TAU + 0.25),
        };
        let planar = transform.as_planar().expect("planar transform");
        assert_eq!(planar.rotation_z, std::f64::consts::TAU + 0.25);
        let expected = planar.transform_xy(1.0, 0.0);
        let actual = transform.transform_xy(1.0, 0.0).expect("transformed point");
        assert_eq!(
            actual, expected,
            "planar fast path must be bit-for-bit compatible"
        );
        let (x, y) = actual;
        assert!((x - (2.0 + 2.0 * 0.25f64.cos())).abs() < 1.0e-12);
        assert!((y - (-3.0 + 2.0 * 0.25f64.sin())).abs() < 1.0e-12);

        let overflowing = SemanticTransform {
            translation: SemanticVec3::new(f64::MAX, 0.0, 0.0),
            ..transform
        };
        assert_eq!(overflowing.transform_xy(f64::MAX, 0.0), None);
        assert_eq!(transform.transform_xy(f64::INFINITY, 0.0), None);
    }

    #[test]
    fn spatial_transform_cannot_be_projected_to_planar_and_xy_uses_3d_orientation() {
        let rotation = SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(1.0, 0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
        )
        .expect("valid rotation");
        let transform = SemanticTransform {
            translation: SemanticVec3::new(1.0, 2.0, 3.0),
            scale: SemanticVec3::new(1.0, 1.0, 1.0),
            orientation: SemanticOrientation::Spatial(rotation),
        };
        assert!(transform.as_planar().is_none());
        let (x, y) = transform.transform_xy(0.0, 1.0).expect("transformed point");
        assert!((x - 1.0).abs() < 1.0e-12);
        assert!((y - 2.0).abs() < 1.0e-12);
    }
}
