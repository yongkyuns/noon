//! Shared Rust authoring semantics for common polygonal Manim-compatible shapes.
//!
//! These constructors lower directly to Noon's retained [`VectorPath`] geometry.
//! Frontends should adapt their syntax to these semantics instead of independently
//! rebuilding polygon vertex/order rules.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, ObjectSnapshot, Vec2, VectorPath, BLUE, TAU};

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryAuthoringError {
    NoRegularPolygramVertices,
    TooFewRegularPolygonVertices(usize),
    InvalidDensity(usize),
    IncompatibleStarDensity { points: usize, density: usize },
    InvalidRadius(f32),
    InvalidStartAngle(f32),
}

impl std::fmt::Display for GeometryAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoRegularPolygramVertices => {
                formatter.write_str("regular polygram requires at least 1 vertex")
            }
            Self::TooFewRegularPolygonVertices(value) => write!(
                formatter,
                "regular polygon requires at least 3 vertices, got {value}"
            ),
            Self::InvalidDensity(value) => {
                write!(
                    formatter,
                    "regular polygram density must be positive, got {value}"
                )
            }
            Self::IncompatibleStarDensity { points, density } => write!(
                formatter,
                "incompatible density {density} for number of star points {points}"
            ),
            Self::InvalidRadius(value) => write!(
                formatter,
                "polygon radius must be finite and positive, got {value}"
            ),
            Self::InvalidStartAngle(value) => {
                write!(formatter, "polygon start angle must be finite, got {value}")
            }
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
define_path_shape!(RegularPolygram);
define_path_shape!(RegularPolygon);
define_path_shape!(Star);
define_path_shape!(Triangle);

fn append_closed_group(mut path: VectorPath, vertices: &[Vec2]) -> VectorPath {
    let Some((&first, rest)) = vertices.split_first() else {
        return path;
    };
    path = path.move_to(first);
    for &vertex in rest {
        path = path.line_to(vertex);
    }
    path.close()
}

fn polygon_path(vertices: impl IntoIterator<Item = Vec2>) -> VectorPath {
    let vertices: Vec<_> = vertices.into_iter().collect();
    append_closed_group(VectorPath::new(), &vertices)
}

fn polygram_path(vertex_groups: &[Vec<Vec2>]) -> VectorPath {
    vertex_groups.iter().fold(VectorPath::new(), |path, group| {
        append_closed_group(path, group)
    })
}

fn manim_path_snapshot(path: VectorPath) -> ObjectSnapshot {
    // `Path` already owns the canonical VMobject stroke width/cap/join policy. Polygram
    // only differs in Manim's default BLUE color; keep the transparent fill opacity.
    let mut snapshot = Path::new(path).into_snapshot();
    let mut transparent_fill = BLUE;
    transparent_fill.alpha = 0.0;
    snapshot.style.fill = Some(transparent_fill);
    snapshot.style.stroke = Some(BLUE);
    snapshot
}

fn manim_polygon_snapshot(vertices: impl IntoIterator<Item = Vec2>) -> ObjectSnapshot {
    manim_path_snapshot(polygon_path(vertices))
}

fn regular_vertices(
    num_vertices: usize,
    radius: f32,
    start_angle: Option<f32>,
) -> Result<(Vec<Vec2>, f32), GeometryAuthoringError> {
    if num_vertices == 0 {
        return Err(GeometryAuthoringError::NoRegularPolygramVertices);
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err(GeometryAuthoringError::InvalidRadius(radius));
    }

    let start_angle = start_angle.unwrap_or_else(|| {
        if num_vertices.is_multiple_of(2) {
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
    regular_vertices(num_vertices, radius, start_angle)
}

fn greatest_common_divisor(mut lhs: usize, mut rhs: usize) -> usize {
    while rhs != 0 {
        let remainder = lhs % rhs;
        lhs = rhs;
        rhs = remainder;
    }
    lhs
}

fn regular_polygram_groups(
    num_vertices: usize,
    density: usize,
    radius: f32,
    start_angle: Option<f32>,
) -> Result<Vec<Vec<Vec2>>, GeometryAuthoringError> {
    if num_vertices == 0 {
        return Err(GeometryAuthoringError::NoRegularPolygramVertices);
    }
    if density == 0 {
        return Err(GeometryAuthoringError::InvalidDensity(density));
    }

    // Manim reduces the Schlaefli symbol by gcd first. A {6/2} therefore becomes
    // two separately closed {3/1} triangles rather than one self-crossing loop.
    let num_groups = greatest_common_divisor(num_vertices, density);
    let reduced_vertices = num_vertices / num_groups;
    let reduced_density = density / num_groups;

    let build_group = |angle: Option<f32>| -> Result<(Vec<Vec2>, f32), GeometryAuthoringError> {
        let (regular, resolved_angle) = regular_vertices(reduced_vertices, radius, angle)?;
        let mut group = Vec::with_capacity(reduced_vertices);
        let mut index = 0;
        loop {
            group.push(regular[index]);
            index = (index + reduced_density) % reduced_vertices;
            if index == 0 {
                break;
            }
        }
        Ok((group, resolved_angle))
    };

    let (first_group, resolved_start) = build_group(start_angle)?;
    let mut groups = vec![first_group];
    for index in 1..num_groups {
        let angle =
            resolved_start + (index as f32 / num_groups as f32) * TAU / reduced_vertices as f32;
        groups.push(build_group(Some(angle))?.0);
    }
    Ok(groups)
}

impl Polygon {
    /// Build one closed straight-edged loop in the authored vertex order.
    pub fn new(vertices: impl IntoIterator<Item = Vec2>) -> Self {
        Self(manim_polygon_snapshot(vertices))
    }
}

impl RegularPolygram {
    /// Build a Manim-compatible regular polygram with the default density of two.
    pub fn new(num_vertices: usize) -> Result<Self, GeometryAuthoringError> {
        Self::with_options(num_vertices, 2, 1.0, None)
    }

    pub fn with_options(
        num_vertices: usize,
        density: usize,
        radius: f32,
        start_angle: Option<f32>,
    ) -> Result<Self, GeometryAuthoringError> {
        let groups = regular_polygram_groups(num_vertices, density, radius, start_angle)?;
        Ok(Self(manim_path_snapshot(polygram_path(&groups))))
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

impl Star {
    /// Build Manim's default five-point star. The inner radius is derived from the
    /// corresponding density-two regular polygram.
    pub fn new() -> Self {
        Self::with_options(5, 1.0, None, 2, Some(TAU / 4.0))
            .expect("the built-in Star default is valid")
    }

    pub fn with_options(
        points: usize,
        outer_radius: f32,
        inner_radius: Option<f32>,
        density: usize,
        start_angle: Option<f32>,
    ) -> Result<Self, GeometryAuthoringError> {
        if points < 3 {
            return Err(GeometryAuthoringError::TooFewRegularPolygonVertices(points));
        }
        if !outer_radius.is_finite() || outer_radius <= 0.0 {
            return Err(GeometryAuthoringError::InvalidRadius(outer_radius));
        }

        let inner_angle = TAU / (2.0 * points as f32);
        let inner_radius = match inner_radius {
            Some(radius) => {
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(GeometryAuthoringError::InvalidRadius(radius));
                }
                radius
            }
            None => {
                if density == 0 || density as f32 >= points as f32 / 2.0 {
                    return Err(GeometryAuthoringError::IncompatibleStarDensity {
                        points,
                        density,
                    });
                }
                let outer_angle = TAU * density as f32 / points as f32;
                let inverse_x =
                    1.0 - inner_angle.tan() * ((outer_angle.cos() - 1.0) / outer_angle.sin());
                outer_radius / (inner_angle.cos() * inverse_x)
            }
        };

        let (outer_vertices, resolved_start) =
            regular_polygon_vertices(points, outer_radius, start_angle)?;
        let (inner_vertices, _) =
            regular_polygon_vertices(points, inner_radius, Some(resolved_start + inner_angle))?;

        let mut vertices = Vec::with_capacity(points * 2);
        for (outer, inner) in outer_vertices.into_iter().zip(inner_vertices) {
            vertices.push(outer);
            vertices.push(inner);
        }
        Ok(Self(manim_polygon_snapshot(vertices)))
    }
}

impl Default for Star {
    fn default() -> Self {
        Self::new()
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
    use noon_core::{GeometryRef, PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode};

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
    fn regular_polygram_pentagram_uses_density_hop_order() {
        let polygram = RegularPolygram::with_options(5, 2, 1.0, None).unwrap();
        let commands = commands(polygram.snapshot());
        assert_eq!(commands.len(), 6);
        let (regular, _) = regular_vertices(5, 1.0, None).unwrap();
        assert_eq!(move_point(commands[0]), regular[0]);
        assert_eq!(line_point(commands[1]), regular[2]);
        assert_eq!(line_point(commands[2]), regular[4]);
        assert_eq!(line_point(commands[3]), regular[1]);
        assert_eq!(line_point(commands[4]), regular[3]);
        assert_eq!(commands[5], PathCommand::Close);
    }

    #[test]
    fn regular_polygram_reduces_hexagram_into_two_triangle_subpaths() {
        let polygram = RegularPolygram::with_options(6, 2, 1.0, None).unwrap();
        let commands = commands(polygram.snapshot());
        assert_eq!(commands.len(), 8);
        let first = move_point(commands[0]);
        let second = move_point(commands[4]);
        assert_close(first.x, 0.0);
        assert_close(first.y, 1.0);
        assert_close(second.x, -(3.0_f32.sqrt() * 0.5));
        assert_close(second.y, 0.5);
        assert_eq!(commands[3], PathCommand::Close);
        assert_eq!(commands[7], PathCommand::Close);
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
    fn star_interleaves_outer_and_inner_vertices() {
        let star = Star::new();
        let commands = commands(star.snapshot());
        assert_eq!(commands.len(), 11);
        let outer = move_point(commands[0]);
        let inner = line_point(commands[1]);
        assert_close(outer.x, 0.0);
        assert_close(outer.y, 1.0);
        let inner_angle = TAU / 10.0;
        let expected_angle = TAU / 4.0 + inner_angle;
        let expected_radius = {
            let outer_angle = TAU * 2.0 / 5.0;
            let inverse_x =
                1.0 - inner_angle.tan() * ((outer_angle.cos() - 1.0) / outer_angle.sin());
            1.0 / (inner_angle.cos() * inverse_x)
        };
        assert_close(inner.x, expected_radius * expected_angle.cos());
        assert_close(inner.y, expected_radius * expected_angle.sin());
        assert_eq!(commands[10], PathCommand::Close);
    }

    #[test]
    fn star_explicit_inner_radius_ignores_density_compatibility() {
        let star = Star::with_options(5, 2.0, Some(0.75), 99, Some(0.0)).unwrap();
        let commands = commands(star.snapshot());
        let outer = move_point(commands[0]);
        let inner = line_point(commands[1]);
        assert_close(outer.x, 2.0);
        assert_close(outer.y, 0.0);
        assert_close(inner.x, 0.75 * (TAU / 10.0).cos());
        assert_close(inner.y, 0.75 * (TAU / 10.0).sin());
    }

    #[test]
    fn triangle_is_the_regular_three_vertex_specialization() {
        let triangle = Triangle::new();
        let regular = RegularPolygon::new(3).unwrap();
        assert_eq!(triangle.snapshot(), regular.snapshot());
    }

    #[test]
    fn polygon_family_rejects_invalid_parameters() {
        assert_eq!(
            RegularPolygon::new(2),
            Err(GeometryAuthoringError::TooFewRegularPolygonVertices(2))
        );
        assert_eq!(
            RegularPolygram::with_options(5, 0, 1.0, None),
            Err(GeometryAuthoringError::InvalidDensity(0))
        );
        assert_eq!(
            Star::with_options(5, 1.0, None, 3, None),
            Err(GeometryAuthoringError::IncompatibleStarDensity {
                points: 5,
                density: 3,
            })
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
