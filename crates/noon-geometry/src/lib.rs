//! Deterministic compilation of renderer-independent Noon vector paths.

#![forbid(unsafe_code)]

use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};
use noon_core::{PathCommand, Rect, Vec2, VectorPath};

const PATH_TESSELLATION_TOLERANCE: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSurface {
    Fill,
    Stroke,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: Vec2,
    pub surface: PathSurface,
    /// Global distance along the ordered path for stroke vertices.
    /// Fill vertices use the total stroke length so they remain hidden until
    /// a reveal reaches its endpoint.
    pub path_distance: f32,
    /// `path_distance / stroke_length` in the inclusive range `[0, 1]`.
    pub path_progress: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TessellatedPath {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub bounds: Option<Rect>,
    /// Flattened length of all ordered stroke contours.
    pub stroke_length: f32,
}

impl TessellatedPath {
    pub fn revealed_stroke_length(&self, reveal: f32) -> f32 {
        if !reveal.is_finite() {
            return 0.0;
        }
        self.stroke_length * reveal.clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryError {
    DrawingBeforeMove,
    CloseBeforeMove,
    NonFinitePoint,
    InvalidStrokeWidth(f32),
    Tessellation(String),
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DrawingBeforeMove => formatter.write_str("path drawing command precedes move_to"),
            Self::CloseBeforeMove => formatter.write_str("path close command precedes move_to"),
            Self::NonFinitePoint => formatter.write_str("path contains a non-finite point"),
            Self::InvalidStrokeWidth(width) => write!(
                formatter,
                "path stroke width must be finite and non-negative: {width}"
            ),
            Self::Tessellation(message) => write!(formatter, "path tessellation failed: {message}"),
        }
    }
}

impl std::error::Error for GeometryError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TessellationVertex {
    position: Vec2,
    surface: PathSurface,
    path_distance: f32,
}

pub fn tessellate(path: &VectorPath, stroke_width: f32) -> Result<TessellatedPath, GeometryError> {
    if !stroke_width.is_finite() || stroke_width < 0.0 {
        return Err(GeometryError::InvalidStrokeWidth(stroke_width));
    }
    let path = build_lyon_path(path)?;
    let mut buffers = VertexBuffers::new();

    FillTessellator::new()
        .tessellate_path(
            &path,
            &FillOptions::default().with_tolerance(PATH_TESSELLATION_TOLERANCE),
            &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| TessellationVertex {
                position: vec2(vertex.position().x, vertex.position().y),
                surface: PathSurface::Fill,
                path_distance: 0.0,
            }),
        )
        .map_err(|error| GeometryError::Tessellation(error.to_string()))?;

    if stroke_width > 0.0 {
        StrokeTessellator::new()
            .tessellate_path(
                &path,
                &StrokeOptions::default()
                    .with_tolerance(PATH_TESSELLATION_TOLERANCE)
                    .with_line_width(stroke_width)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
                &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
                    TessellationVertex {
                        position: vec2(vertex.position().x, vertex.position().y),
                        surface: PathSurface::Stroke,
                        // Lyon defines advancement as how far along the complete
                        // input path the stroke vertex is. It is already global
                        // across subpaths, so adding contour offsets would
                        // double-count later contours.
                        path_distance: vertex.advancement(),
                    }
                }),
            )
            .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
    }

    let stroke_length = buffers
        .vertices
        .iter()
        .filter(|vertex| vertex.surface == PathSurface::Stroke)
        .map(|vertex| vertex.path_distance)
        .fold(0.0_f32, f32::max);

    let vertices: Vec<MeshVertex> = buffers
        .vertices
        .into_iter()
        .map(|vertex| {
            if vertex.surface == PathSurface::Stroke {
                let path_progress = if stroke_length > 0.0 {
                    (vertex.path_distance / stroke_length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                MeshVertex {
                    position: vertex.position,
                    surface: vertex.surface,
                    path_distance: vertex.path_distance,
                    path_progress,
                }
            } else {
                MeshVertex {
                    position: vertex.position,
                    surface: vertex.surface,
                    path_distance: stroke_length,
                    path_progress: 1.0,
                }
            }
        })
        .collect();

    let bounds = mesh_bounds(&vertices);
    Ok(TessellatedPath {
        vertices,
        indices: buffers.indices,
        bounds,
        stroke_length,
    })
}

fn build_lyon_path(path: &VectorPath) -> Result<Path, GeometryError> {
    let mut builder = Path::builder();
    let mut active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    builder.end(false);
                }
                builder.begin(point(to.x, to.y));
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                builder.line_to(point(to.x, to.y));
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                builder.quadratic_bezier_to(point(control.x, control.y), point(to.x, to.y));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                require_active(active)?;
                finite(control1)?;
                finite(control2)?;
                finite(to)?;
                builder.cubic_bezier_to(
                    point(control1.x, control1.y),
                    point(control2.x, control2.y),
                    point(to.x, to.y),
                );
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                builder.end(true);
                active = false;
            }
        }
    }
    if active {
        builder.end(false);
    }

    Ok(builder.build())
}

fn finite(value: Vec2) -> Result<(), GeometryError> {
    if value.x.is_finite() && value.y.is_finite() {
        Ok(())
    } else {
        Err(GeometryError::NonFinitePoint)
    }
}

fn require_active(active: bool) -> Result<(), GeometryError> {
    if active {
        Ok(())
    } else {
        Err(GeometryError::DrawingBeforeMove)
    }
}

fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn mesh_bounds(vertices: &[MeshVertex]) -> Option<Rect> {
    let first = vertices.first()?.position;
    let mut min = first;
    let mut max = first;
    for vertex in &vertices[1..] {
        min.x = min.x.min(vertex.position.x);
        min.y = min.y.min(vertex.position.y);
        max.x = max.x.max(vertex.position.x);
        max.y = max.y.max(vertex.position.y);
    }
    Some(Rect::new(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curved_shape() -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0))
            .cubic_to(
                Vec2::new(1.5, -1.0),
                Vec2::new(-1.5, -1.0),
                Vec2::new(-1.0, 0.0),
            )
            .close()
    }

    #[test]
    fn tessellation_is_deterministic_and_structurally_valid() {
        let first = tessellate(&curved_shape(), 0.1).expect("valid path");
        let second = tessellate(&curved_shape(), 0.1).expect("valid path");

        assert_eq!(first, second);
        assert!(!first.vertices.is_empty());
        assert!(!first.indices.is_empty());
        assert!(first
            .indices
            .iter()
            .all(|index| (*index as usize) < first.vertices.len()));
        assert!(first.vertices.iter().all(|vertex| {
            vertex.position.x.is_finite()
                && vertex.position.y.is_finite()
                && vertex.path_distance.is_finite()
                && vertex.path_progress.is_finite()
        }));
        assert!(first
            .vertices
            .iter()
            .any(|vertex| vertex.surface == PathSurface::Fill));
        assert!(first
            .vertices
            .iter()
            .any(|vertex| vertex.surface == PathSurface::Stroke));
        assert!(first.stroke_length > 0.0);
    }

    #[test]
    fn bounds_contain_every_generated_vertex() {
        let mesh = tessellate(&curved_shape(), 0.2).expect("valid path");
        let bounds = mesh.bounds.expect("non-empty mesh has bounds");
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.x >= bounds.min.x
                && vertex.position.x <= bounds.max.x
                && vertex.position.y >= bounds.min.y
                && vertex.position.y <= bounds.max.y
        }));
    }

    #[test]
    fn stroke_arc_length_metadata_has_exact_endpoints_and_monotonic_reveal() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(3.0, 4.0));
        let mesh = tessellate(&path, 0.2).expect("valid path");

        assert!((mesh.stroke_length - 5.0).abs() < 1e-5);
        let stroke: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.surface == PathSurface::Stroke)
            .collect();
        assert!(stroke.iter().any(|vertex| vertex.path_progress == 0.0));
        assert!(stroke.iter().any(|vertex| vertex.path_progress == 1.0));
        assert!(stroke.iter().all(|vertex| {
            (0.0..=mesh.stroke_length).contains(&vertex.path_distance)
                && (0.0..=1.0).contains(&vertex.path_progress)
        }));

        let reveals = [0.0, 0.1, 0.25, 0.5, 0.75, 1.0];
        let lengths: Vec<f32> = reveals
            .into_iter()
            .map(|reveal| mesh.revealed_stroke_length(reveal))
            .collect();
        assert_eq!(lengths[0], 0.0);
        assert_eq!(lengths[lengths.len() - 1], mesh.stroke_length);
        assert!(lengths.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn multiple_contours_use_one_global_ordered_arc_length() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0))
            .move_to(Vec2::new(10.0, 0.0))
            .line_to(Vec2::new(10.0, 4.0));
        let mesh = tessellate(&path, 0.2).expect("valid path");

        assert!((mesh.stroke_length - 7.0).abs() < 1e-5);
        let second_contour_progresses: Vec<f32> = mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.surface == PathSurface::Stroke && vertex.position.x > 9.0)
            .map(|vertex| vertex.path_progress)
            .collect();
        assert!(!second_contour_progresses.is_empty());
        assert!(second_contour_progresses
            .iter()
            .all(|progress| *progress >= 3.0 / 7.0 - 1e-5));
        assert!(second_contour_progresses
            .iter()
            .any(|progress| (*progress - 1.0).abs() < 1e-5));
    }

    #[test]
    fn malformed_and_degenerate_paths_are_handled_intentionally() {
        let malformed = VectorPath::new().line_to(Vec2::new(1.0, 0.0));
        assert_eq!(
            tessellate(&malformed, 0.1),
            Err(GeometryError::DrawingBeforeMove)
        );

        let empty = tessellate(&VectorPath::new(), 0.1).expect("empty path is valid");
        assert!(empty.vertices.is_empty());
        assert!(empty.indices.is_empty());
        assert_eq!(empty.bounds, None);
        assert_eq!(empty.stroke_length, 0.0);
    }
}
