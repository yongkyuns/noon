//! Deterministic compilation of renderer-independent Noon vector paths.

#![forbid(unsafe_code)]

use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers, VertexSource,
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
    /// Global distance along the ordered path contours for stroke vertices.
    /// Fill vertices use the total stroke length so they can remain hidden
    /// until a reveal reaches its endpoint.
    pub path_distance: f32,
    /// `path_distance / stroke_length` in the inclusive range `[0, 1]`.
    pub path_progress: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TessellatedPath {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub bounds: Option<Rect>,
    /// Sum of the flattened lengths of all ordered stroke contours.
    pub stroke_length: f32,
}

impl TessellatedPath {
    /// Returns the amount of stroke arc length visible at a normalized reveal.
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
            Self::InvalidStrokeWidth(width) => {
                write!(
                    formatter,
                    "path stroke width must be finite and non-negative: {width}"
                )
            }
            Self::Tessellation(message) => write!(formatter, "path tessellation failed: {message}"),
        }
    }
}

impl std::error::Error for GeometryError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TessellationVertex {
    position: Vec2,
    surface: PathSurface,
    contour: usize,
    local_distance: f32,
}

#[derive(Debug)]
struct BuiltPath {
    path: Path,
    endpoint_contours: Vec<usize>,
    contour_count: usize,
}

pub fn tessellate(path: &VectorPath, stroke_width: f32) -> Result<TessellatedPath, GeometryError> {
    if !stroke_width.is_finite() || stroke_width < 0.0 {
        return Err(GeometryError::InvalidStrokeWidth(stroke_width));
    }
    let built = build_lyon_path(path)?;
    let mut buffers = VertexBuffers::new();

    FillTessellator::new()
        .tessellate_path(
            &built.path,
            &FillOptions::default().with_tolerance(PATH_TESSELLATION_TOLERANCE),
            &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| TessellationVertex {
                position: vec2(vertex.position().x, vertex.position().y),
                surface: PathSurface::Fill,
                contour: usize::MAX,
                local_distance: 0.0,
            }),
        )
        .map_err(|error| GeometryError::Tessellation(error.to_string()))?;

    if stroke_width > 0.0 {
        let endpoint_contours = &built.endpoint_contours;
        StrokeTessellator::new()
            .tessellate_path(
                &built.path,
                &StrokeOptions::default()
                    .with_tolerance(PATH_TESSELLATION_TOLERANCE)
                    .with_line_width(stroke_width)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
                &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
                    TessellationVertex {
                        position: vec2(vertex.position().x, vertex.position().y),
                        surface: PathSurface::Stroke,
                        contour: source_contour(vertex.source(), endpoint_contours),
                        local_distance: vertex.advancement(),
                    }
                }),
            )
            .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
    }

    let mut contour_lengths = vec![0.0_f32; built.contour_count];
    for vertex in &buffers.vertices {
        if vertex.surface == PathSurface::Stroke {
            contour_lengths[vertex.contour] =
                contour_lengths[vertex.contour].max(vertex.local_distance);
        }
    }
    let mut contour_offsets = vec![0.0_f32; built.contour_count];
    let mut stroke_length = 0.0_f32;
    for (index, length) in contour_lengths.iter().copied().enumerate() {
        contour_offsets[index] = stroke_length;
        stroke_length += length;
    }

    let vertices: Vec<MeshVertex> = buffers
        .vertices
        .into_iter()
        .map(|vertex| {
            if vertex.surface == PathSurface::Stroke {
                let path_distance = contour_offsets[vertex.contour] + vertex.local_distance;
                let path_progress = if stroke_length > 0.0 {
                    (path_distance / stroke_length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                MeshVertex {
                    position: vertex.position,
                    surface: vertex.surface,
                    path_distance,
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

fn build_lyon_path(path: &VectorPath) -> Result<BuiltPath, GeometryError> {
    let mut builder = Path::builder();
    let mut active = false;
    let mut current_contour = 0_usize;
    let mut endpoint_contours = Vec::new();

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    builder.end(false);
                    current_contour += 1;
                }
                let id = builder.begin(point(to.x, to.y));
                record_endpoint_contour(&mut endpoint_contours, id.to_usize(), current_contour);
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                let id = builder.line_to(point(to.x, to.y));
                record_endpoint_contour(&mut endpoint_contours, id.to_usize(), current_contour);
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                let id =
                    builder.quadratic_bezier_to(point(control.x, control.y), point(to.x, to.y));
                record_endpoint_contour(&mut endpoint_contours, id.to_usize(), current_contour);
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
                let id = builder.cubic_bezier_to(
                    point(control1.x, control1.y),
                    point(control2.x, control2.y),
                    point(to.x, to.y),
                );
                record_endpoint_contour(&mut endpoint_contours, id.to_usize(), current_contour);
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                builder.end(true);
                active = false;
                current_contour += 1;
            }
        }
    }
    if active {
        builder.end(false);
        current_contour += 1;
    }

    Ok(BuiltPath {
        path: builder.build(),
        endpoint_contours,
        contour_count: current_contour,
    })
}

fn record_endpoint_contour(endpoint_contours: &mut Vec<usize>, endpoint: usize, contour: usize) {
    if endpoint == endpoint_contours.len() {
        endpoint_contours.push(contour);
    } else if endpoint < endpoint_contours.len() {
        endpoint_contours[endpoint] = contour;
    } else {
        endpoint_contours.resize(endpoint + 1, contour);
        endpoint_contours[endpoint] = contour;
    }
}

fn source_contour(source: VertexSource, endpoint_contours: &[usize]) -> usize {
    let endpoint = match source {
        VertexSource::Endpoint { id } => id,
        VertexSource::Edge { from, .. } => from,
    };
    endpoint_contours[endpoint.to_usize()]
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
