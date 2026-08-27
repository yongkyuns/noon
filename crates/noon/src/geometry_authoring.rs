//! Shared Rust authoring semantics for common polygonal Manim-compatible shapes.
//!
//! These constructors lower directly to Noon's retained [`VectorPath`] geometry.
//! Frontends should adapt their syntax to these semantics instead of independently
//! rebuilding polygon vertex/order rules.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, GeometryRef, ObjectSnapshot, Vec2, VectorPath, BLUE, TAU};

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryAuthoringError {
    TooFewRegularPolygonVertices(usize),
    InvalidRadius(f32),
    InvalidStartAngle(f32),
}

impl std::fmt::Display for GeometryAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewRegularPolygonVertices(value) => write!(
                formatter,
                "regular polygon requires at least 3 vertices, got {value}"
            ),
            Self::InvalidRadius(value) => {
                write!(formatter, "regular polygon radius must be finite and positive, got {value}")
            }
            Self::InvalidStartAngle(value) => write!(
                formatter,
                "regular polygon start angle must be finite, got {value}"
            ),
        }
    }
}

impl std::error::Error for GeometryAuthoringError {}

macro_rules! define_path_shape {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(ObjectSnapshot);

        impl $name {
            pub fn color(mut self, color: Color) -> Self {
                self.0 = self.0.set_color(color);
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0 = self.0.shift(offset);
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0 = self.0.move_to(point);
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0 = self.0.scale_by(factor);
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0 = self.0.scale_xy(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0 = self.0.rotate_by(angle);
                self
            }

            pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
                self.0 = self.0.set_fill(color, opacity);
                self
            }

            pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
                self.0 = self.0.set_stroke(color, width);
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0 = self.0.set_opacity(opacity);
                self
            }

            pub fn snapshot(&self) -> &ObjectSnapshot {
                &self.0
            }
        }

        impl IntoSnapshot for $name {
            fn into_snapshot(self) -> ObjectSnapshot {
                self.0
            }
        }
    };
}

define_path_shape!(Polygon);
define_path_shape!(RegularPolygon);
define_path_shape!(Triangle);

fn polygon_path(vertices: impl IntoIterator<Item = Vec2>) -> VectorPath {
    let mut vertices = vertices.into_iter();
    let Some(first) = vertices.next() else {
        return VectorPath::new();
    };

    let mut path = VectorPath::new().move_to(first);
    for vertex in vertices {
        path = path.line_to(vertex);
    }
    path.close()
}

fn manim_polygon_snapshot(vertices: impl IntoIterator<Item = Vec2>) -> ObjectSnapshot {
    // `Path` already owns the canonical VMobject stroke width/cap/join policy. Polygon
    // only differs in Manim's default BLUE color; keep the transparent fill opacity.
    let path = polygon_path(vertices);
    let mut snapshot = Path::new(path).into_snapshot();
    let mut transparent_fill = BLUE;
    transparent_fill.alpha = 0.0;
    snapshot.style.fill = Some(transparent_fill);
    snapshot.style.stroke = Some(BLUE);
    snapshot
}

fn regular_polygon_vertices(
    num_vertices: usize,
    radius: f32,
    start_angle: Option<f32>,
) -> Result<(Vec<Vec2>, f32), GeometryAuthoringError> {
    if num_vertices < 3 {
        return Err(GeometryAuthoringError::TooFewRegularPolygonVertices(
            num_vertices,
        ));
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err(GeometryAuthoringError::InvalidRadius(radius));
    }

    let start_angle = start_angle.unwrap_or_else(|| {
        if num_vertices % 2 == 0 {
            0.0
        } else {
            TAU / 4.0
        }
    });
    if !start_angle.is_finite() {
        return Err(GeometryAuthoringError::InvalidStartAngle(start_angle));
    }

    let step = TAU / num_vertices as f32;
    let vertices = (0..num_vertices)
        .map(|index| {
            let angle = start_angle + index as f32 * step;
            Vec2::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect();
    Ok((vertices, start_angle))
}

impl Polygon {
    /// Build one closed straight-edged loop in the authored vertex order.
    pub fn new(vertices: impl IntoIterator<Item = Vec2>) -> Self {
        Self(manim_polygon_snapshot(vertices))
    }
}

impl RegularPolygon {
    /// Build a Manim-compatible regular polygon using radius 1 and Manim's default
    /// orientation: even-sided polygons start on +X; odd-sided polygons start on +Y.
    pub fn new(num_vertices: usize) -> Result<Self, GeometryAuthoringError> {
        Self::with_options(num_vertices, 1.0, None)
    }

    pub fn with_options(
        num_vertices: usize,
        radius: f32,
        start_angle: Option<f32>,
    ) -> Result<Self, GeometryAuthoringError> {
        let (vertices, _) = regular_polygon_vertices(num_vertices, radius, start_angle)?;
        Ok(Self(manim_polygon_snapshot(vertices)))
    }
}

impl Default for RegularPolygon {
    fn default() -> Self {
        let (vertices, _) = regular_polygon_vertices(6, 1.0, None)
            .expect("the built-in RegularPolygon default is valid");
        Self(manim_polygon_snapshot(vertices))
    }
}

impl Triangle {
    pub fn new() -> Self {
        let (vertices, _) = regular_polygon_vertices(3, 1.0, None)
            .expect("the built-in Triangle definition is valid");
        Self(manim_polygon_snapshot(vertices))
    }
}

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode};

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected vector path geometry, got {other:?}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn move_point(command: PathCommand) -> Vec2 {
        match command {
            PathCommand::MoveTo { to } => to,
            other => panic!("expected MoveTo, got {other:?}"),
        }
    }

    fn line_point(command: PathCommand) -> Vec2 {
        match command {
            PathCommand::LineTo { to } => to,
            other => panic!("expected LineTo, got {other:?}"),
        }
    }

    #[test]
    fn polygon_preserves_vertex_order_and_closes_once() {
        let polygon = Polygon::new([
            Vec2::new(-2.0, -1.0),
            Vec2::new(3.0, -0.5),
            Vec2::new(1.0, 4.0),
        ]);
        let commands = commands(polygon.snapshot());
        assert_eq!(commands.len(), 4);
        assert_eq!(move_point(commands[0]), Vec2::new(-2.0, -1.0));
        assert_eq!(line_point(commands[1]), Vec2::new(3.0, -0.5));
        assert_eq!(line_point(commands[2]), Vec2::new(1.0, 4.0));
        assert_eq!(commands[3], PathCommand::Close);
    }

    #[test]
    fn polygon_uses_manim_vmobject_style_with_blue_default() {
        let polygon = Polygon::new([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ]);
        let style = polygon.snapshot().style;
        assert_eq!(style.stroke, Some(BLUE));
        assert_eq!(style.fill.map(|color| color.alpha), Some(0.0));
        assert_eq!(style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
        assert_eq!(style.stroke_join, StrokeJoin::Miter);
        assert_eq!(style.stroke_cap, StrokeCap::Butt);
    }

    #[test]
    fn regular_polygon_matches_manim_default_even_orientation() {
        let polygon = RegularPolygon::new(6).unwrap();
        let commands = commands(polygon.snapshot());
        let first = move_point(commands[0]);
        let second = line_point(commands[1]);
        assert_close(first.x, 1.0);
        assert_close(first.y, 0.0);
        assert_close(second.x, 0.5);
        assert_close(second.y, 3.0_f32.sqrt() * 0.5);
        assert_eq!(commands.last(), Some(&PathCommand::Close));
    }

    #[test]
    fn regular_polygon_matches_manim_default_odd_orientation() {
        let polygon = RegularPolygon::new(5).unwrap();
        let first = move_point(commands(polygon.snapshot())[0]);
        assert_close(first.x, 0.0);
        assert_close(first.y, 1.0);
    }

    #[test]
    fn regular_polygon_honors_radius_and_start_angle() {
        let polygon = RegularPolygon::with_options(4, 2.5, Some(TAU / 8.0)).unwrap();
        let first = move_point(commands(polygon.snapshot())[0]);
        assert_close(first.x, 2.5 * (TAU / 8.0).cos());
        assert_close(first.y, 2.5 * (TAU / 8.0).sin());
    }

    #[test]
    fn triangle_is_the_regular_three_vertex_specialization() {
        let triangle = Triangle::new();
        let regular = RegularPolygon::new(3).unwrap();
        assert_eq!(triangle.snapshot(), regular.snapshot());
    }

    #[test]
    fn regular_polygon_rejects_invalid_parameters() {
        assert_eq!(
            RegularPolygon::new(2),
            Err(GeometryAuthoringError::TooFewRegularPolygonVertices(2))
        );
        assert_eq!(
            RegularPolygon::with_options(4, 0.0, None),
            Err(GeometryAuthoringError::InvalidRadius(0.0))
        );
        assert!(matches!(
            RegularPolygon::with_options(4, 1.0, Some(f32::NAN)),
            Err(GeometryAuthoringError::InvalidStartAngle(value)) if value.is_nan()
        ));
    }
}
