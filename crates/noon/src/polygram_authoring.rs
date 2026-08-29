//! Shared Rust authoring semantics and queries for Manim-compatible polygrams.
//!
//! `Polygram` represents one or more independently closed straight-edged vertex
//! groups as a single retained [`VectorPath`]. The query methods below are also
//! exposed on Noon's existing polygon/polygram family so frontends can delegate
//! `get_vertices()` and `get_vertex_groups()` to shared Rust semantics.

use crate::legacy::{IntoSnapshot, Path};
use crate::{Polygon, RegularPolygon, RegularPolygram, Star, Triangle};
use noon_core::{Color, GeometryRef, ObjectSnapshot, PathCommand, Vec2, VectorPath, BLUE};

#[derive(Clone, Debug, PartialEq)]
pub enum PolygramAuthoringError {
    EmptyVertexGroup(usize),
}

impl std::fmt::Display for PolygramAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertexGroup(index) => {
                write!(
                    formatter,
                    "polygram vertex group {index} must contain at least one vertex"
                )
            }
        }
    }
}

impl std::error::Error for PolygramAuthoringError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Polygram(ObjectSnapshot);

impl Polygram {
    /// Construct a Manim-compatible polygram from independently closed vertex groups.
    pub fn new<I, G>(vertex_groups: I) -> Result<Self, PolygramAuthoringError>
    where
        I: IntoIterator<Item = G>,
        G: IntoIterator<Item = Vec2>,
    {
        Self::with_color(vertex_groups, BLUE)
    }

    pub fn with_color<I, G>(vertex_groups: I, color: Color) -> Result<Self, PolygramAuthoringError>
    where
        I: IntoIterator<Item = G>,
        G: IntoIterator<Item = Vec2>,
    {
        let mut path = VectorPath::new();
        for (index, group) in vertex_groups.into_iter().enumerate() {
            let vertices: Vec<_> = group.into_iter().collect();
            let Some((&first, rest)) = vertices.split_first() else {
                return Err(PolygramAuthoringError::EmptyVertexGroup(index));
            };
            path = path.move_to(first);
            for &vertex in rest {
                path = path.line_to(vertex);
            }
            path = path.close();
        }

        let mut snapshot = Path::new(path).into_snapshot();
        let mut transparent_fill = color;
        transparent_fill.alpha = 0.0;
        snapshot.style.fill = Some(transparent_fill);
        snapshot.style.stroke = Some(color);
        Ok(Self(snapshot))
    }

    pub fn color(mut self, color: Color) -> Self {
        let fill_alpha = self.0.style.fill.map_or(0.0, |fill| fill.alpha);
        let mut fill = color;
        fill.alpha = fill_alpha;
        self.0.style.fill = Some(fill);
        self.0.style.stroke = Some(color);
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

    pub fn get_vertices(&self) -> Vec<Vec2> {
        polygram_vertices(&self.0)
    }

    pub fn get_vertex_groups(&self) -> Vec<Vec<Vec2>> {
        polygram_vertex_groups(&self.0)
    }
}

impl IntoSnapshot for Polygram {
    fn into_snapshot(self) -> ObjectSnapshot {
        self.0
    }
}

/// Return transformed world-space vertex groups for any retained polygon/polygram snapshot.
///
/// This is the shared frontend query boundary: adapters should call this instead of
/// reconstructing retained path commands or transform math in their host language.
pub fn polygram_vertex_groups(snapshot: &ObjectSnapshot) -> Vec<Vec<Vec2>> {
    let GeometryRef::VectorPath(path) = &snapshot.geometry else {
        return Vec::new();
    };

    let transform_point = |point: Vec2| snapshot.transform.transform_point(point);
    let mut groups = Vec::new();
    let mut current = Vec::new();

    for command in path.commands().iter().copied() {
        match command {
            PathCommand::MoveTo { to } => {
                if !current.is_empty() {
                    groups.push(std::mem::take(&mut current));
                }
                current.push(transform_point(to));
            }
            PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => current.push(transform_point(to)),
            PathCommand::Close => {
                if !current.is_empty() {
                    groups.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Return transformed world-space vertices flattened in retained group order.
pub fn polygram_vertices(snapshot: &ObjectSnapshot) -> Vec<Vec2> {
    polygram_vertex_groups(snapshot)
        .into_iter()
        .flatten()
        .collect()
}

macro_rules! impl_polygram_queries {
    ($($shape:ty),+ $(,)?) => {
        $(
            impl $shape {
                pub fn get_vertices(&self) -> Vec<Vec2> {
                    polygram_vertices(self.snapshot())
                }

                pub fn get_vertex_groups(&self) -> Vec<Vec<Vec2>> {
                    polygram_vertex_groups(self.snapshot())
                }
            }
        )+
    };
}

impl_polygram_queries!(Polygon, RegularPolygram, RegularPolygon, Star, Triangle);

#[cfg(test)]
mod tests {
    use noon_core::{PathCommand, StrokeWidthMode};

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected retained VectorPath geometry, got {other:?}"),
        }
    }

    #[test]
    fn polygram_preserves_independent_closed_vertex_groups() {
        let polygram = Polygram::new([
            vec![
                Vec2::new(0.0, 2.0),
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
            ],
            vec![
                Vec2::new(0.0, -2.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
        ])
        .expect("valid polygram");
        let commands = commands(polygram.snapshot());
        assert_eq!(commands.len(), 8);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
                .count(),
            2
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, PathCommand::Close))
                .count(),
            2
        );
    }

    #[test]
    fn polygram_uses_manim_blue_vmobject_style() {
        let polygram = Polygram::new([vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ]])
        .expect("valid polygram");
        assert_eq!(polygram.snapshot().style.stroke, Some(BLUE));
        assert_eq!(
            polygram.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
        assert_eq!(
            polygram.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn vertex_group_queries_follow_world_transform() {
        let polygram = Polygram::new([vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, 1.0),
        ]])
        .expect("valid polygram")
        .scale_xy(Vec2::new(2.0, 3.0))
        .shift(Vec2::new(1.0, -2.0));
        assert_eq!(
            polygram.get_vertices(),
            vec![
                Vec2::new(1.0, -2.0),
                Vec2::new(5.0, -2.0),
                Vec2::new(1.0, 1.0),
            ]
        );
        assert_eq!(polygram.get_vertex_groups().len(), 1);
    }

    #[test]
    fn shared_query_functions_match_shape_methods() {
        let polygon = Polygon::new([
            Vec2::new(-1.0, -1.0),
            Vec2::new(2.0, -1.0),
            Vec2::new(0.0, 3.0),
        ])
        .shift(Vec2::new(4.0, 2.0));
        assert_eq!(
            polygram_vertices(polygon.snapshot()),
            polygon.get_vertices()
        );
        assert_eq!(
            polygram_vertex_groups(polygon.snapshot()),
            polygon.get_vertex_groups()
        );
    }

    #[test]
    fn existing_polygon_exposes_shared_vertex_queries() {
        let polygon = Polygon::new([
            Vec2::new(-1.0, -1.0),
            Vec2::new(2.0, -1.0),
            Vec2::new(0.0, 3.0),
        ])
        .shift(Vec2::new(4.0, 2.0));
        assert_eq!(
            polygon.get_vertices(),
            vec![
                Vec2::new(3.0, 1.0),
                Vec2::new(6.0, 1.0),
                Vec2::new(4.0, 5.0),
            ]
        );
        assert_eq!(polygon.get_vertex_groups(), vec![polygon.get_vertices()]);
    }

    #[test]
    fn disconnected_regular_polygram_reports_separate_groups() {
        let polygram = RegularPolygram::with_options(6, 2, 1.0, None).expect("valid polygram");
        let groups = polygram.get_vertex_groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 3);
        assert_eq!(groups[1].len(), 3);
        assert_eq!(polygram.get_vertices().len(), 6);
    }

    #[test]
    fn empty_vertex_group_is_rejected_with_group_index() {
        assert_eq!(
            Polygram::new([Vec::<Vec2>::new()]),
            Err(PolygramAuthoringError::EmptyVertexGroup(0))
        );
    }
}
