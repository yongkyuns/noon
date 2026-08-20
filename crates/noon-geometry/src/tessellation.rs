use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};
use noon_core::{PathCommand, Rect, Vec2, VectorPath};

const PATH_TESSELLATION_TOLERANCE: f32 = 0.01;
const MORPH_MITER_LIMIT: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSurface {
    Fill,
    Stroke,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: Vec2,
    pub target_position: Vec2,
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
    /// True when vertices contain distinct source/target morph endpoints.
    pub morphing: bool,
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
    if let Some(target) = path.morph_target() {
        return tessellate_morph_path(path, target, stroke_width);
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
                    target_position: vertex.position,
                    surface: vertex.surface,
                    path_distance: vertex.path_distance,
                    path_progress,
                }
            } else {
                MeshVertex {
                    position: vertex.position,
                    target_position: vertex.position,
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
        morphing: false,
    })
}

fn tessellate_morph_path(
    source: &VectorPath,
    target: &VectorPath,
    stroke_width: f32,
) -> Result<TessellatedPath, GeometryError> {
    if stroke_width == 0.0 {
        return Ok(TessellatedPath {
            morphing: true,
            ..TessellatedPath::default()
        });
    }
    let plan = crate::plan_morph(source, target, crate::MorphOptions::DEFAULT)
        .map_err(|error| GeometryError::Tessellation(format!("morph planning failed: {error}")))?;
    let total_points = plan.point_count();
    let mut vertices = Vec::with_capacity(total_points.saturating_mul(2));
    let mut indices = Vec::new();
    let mut global_point = 0_usize;
    let progress_denominator = total_points.saturating_sub(1).max(1) as f32;
    let mut stroke_length = 0.0_f32;

    for contour in &plan.contours {
        let source_edges = stroke_edges(&contour.source_points, contour.closed, stroke_width * 0.5);
        let target_edges = stroke_edges(&contour.target_points, contour.closed, stroke_width * 0.5);
        let vertex_start = u32::try_from(vertices.len())
            .map_err(|_| GeometryError::Tessellation("morph vertex count exceeds u32".into()))?;
        stroke_length += polyline_length(&contour.source_points, contour.closed);

        for (index, ((source_left, source_right), (target_left, target_right))) in
            source_edges.into_iter().zip(target_edges).enumerate()
        {
            let progress = (global_point + index) as f32 / progress_denominator;
            for (position, target_position) in
                [(source_left, target_left), (source_right, target_right)]
            {
                vertices.push(MeshVertex {
                    position,
                    target_position,
                    surface: PathSurface::Stroke,
                    path_distance: progress,
                    path_progress: progress,
                });
            }
        }

        let point_count = contour.source_points.len();
        let segment_count = if contour.closed {
            point_count
        } else {
            point_count - 1
        };
        for segment in 0..segment_count {
            let next = if segment + 1 == point_count {
                0
            } else {
                segment + 1
            };
            let a = vertex_start + u32::try_from(segment * 2).unwrap();
            let b = a + 1;
            let c = vertex_start + u32::try_from(next * 2).unwrap();
            let d = c + 1;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
        global_point += point_count;
    }

    let bounds = morph_mesh_bounds(&vertices);
    Ok(TessellatedPath {
        vertices,
        indices,
        bounds,
        stroke_length,
        morphing: true,
    })
}

fn stroke_edges(points: &[Vec2], closed: bool, half_width: f32) -> Vec<(Vec2, Vec2)> {
    let mut result = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        let point = points[index];
        let previous = if index > 0 {
            points[index - 1]
        } else if closed {
            points[points.len() - 1]
        } else {
            point
        };
        let next = if index + 1 < points.len() {
            points[index + 1]
        } else if closed {
            points[0]
        } else {
            point
        };

        let offset = if !closed && index == 0 {
            segment_normal(point, next, half_width)
        } else if !closed && index + 1 == points.len() {
            segment_normal(previous, point, half_width)
        } else {
            miter_offset(previous, point, next, half_width)
        };
        result.push((
            Vec2::new(point.x + offset.x, point.y + offset.y),
            Vec2::new(point.x - offset.x, point.y - offset.y),
        ));
    }
    result
}

fn segment_normal(from: Vec2, to: Vec2, half_width: f32) -> Vec2 {
    let tangent = normalized(Vec2::new(to.x - from.x, to.y - from.y));
    Vec2::new(-tangent.y * half_width, tangent.x * half_width)
}

fn miter_offset(previous: Vec2, point: Vec2, next: Vec2, half_width: f32) -> Vec2 {
    let incoming = normalized(Vec2::new(point.x - previous.x, point.y - previous.y));
    let outgoing = normalized(Vec2::new(next.x - point.x, next.y - point.y));
    let incoming_normal = Vec2::new(-incoming.y, incoming.x);
    let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
    let summed = Vec2::new(
        incoming_normal.x + outgoing_normal.x,
        incoming_normal.y + outgoing_normal.y,
    );
    let summed_length = summed.x.hypot(summed.y);
    if summed_length <= f32::EPSILON {
        return Vec2::new(
            outgoing_normal.x * half_width,
            outgoing_normal.y * half_width,
        );
    }

    let miter = Vec2::new(summed.x / summed_length, summed.y / summed_length);
    let alignment = (miter.x * outgoing_normal.x + miter.y * outgoing_normal.y).abs();
    if alignment <= f32::EPSILON {
        return Vec2::new(
            outgoing_normal.x * half_width,
            outgoing_normal.y * half_width,
        );
    }

    let length = (half_width / alignment).min(half_width * MORPH_MITER_LIMIT);
    Vec2::new(miter.x * length, miter.y * length)
}

fn normalized(vector: Vec2) -> Vec2 {
    let length = vector.x.hypot(vector.y);
    if length <= f32::EPSILON {
        Vec2::new(1.0, 0.0)
    } else {
        Vec2::new(vector.x / length, vector.y / length)
    }
}

fn polyline_length(points: &[Vec2], closed: bool) -> f32 {
    let mut length = points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum::<f32>();
    if closed {
        let first = points[0];
        let last = points[points.len() - 1];
        length += (first.x - last.x).hypot(first.y - last.y);
    }
    length
}

fn morph_mesh_bounds(vertices: &[MeshVertex]) -> Option<Rect> {
    let first = vertices.first()?;
    let mut min = Vec2::new(
        first.position.x.min(first.target_position.x),
        first.position.y.min(first.target_position.y),
    );
    let mut max = Vec2::new(
        first.position.x.max(first.target_position.x),
        first.position.y.max(first.target_position.y),
    );
    for vertex in &vertices[1..] {
        for point in [vertex.position, vertex.target_position] {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
    }
    Some(Rect::new(min, max))
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
    fn morph_closed_stroke_seam_preserves_constant_width_at_sharp_join() {
        let half_width = 0.1;
        let points = vec![
            Vec2::new(0.0, 2.0),
            Vec2::new(0.47, 0.65),
            Vec2::new(1.9, 0.62),
            Vec2::new(0.76, -0.25),
            Vec2::new(1.18, -1.62),
            Vec2::new(0.0, -0.82),
            Vec2::new(-1.18, -1.62),
            Vec2::new(-0.76, -0.25),
            Vec2::new(-1.9, 0.62),
            Vec2::new(-0.47, 0.65),
        ];
        let edges = stroke_edges(&points, true, half_width);
        let point = points[0];
        let previous = points[points.len() - 1];
        let next = points[1];
        let incoming = normalized(Vec2::new(point.x - previous.x, point.y - previous.y));
        let outgoing = normalized(Vec2::new(next.x - point.x, next.y - point.y));
        let incoming_normal = Vec2::new(-incoming.y, incoming.x);
        let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
        let (left, right) = edges[0];
        let outer = if left.y > right.y { left } else { right };
        let offset = Vec2::new(outer.x - point.x, outer.y - point.y);

        let incoming_distance = (offset.x * incoming_normal.x + offset.y * incoming_normal.y).abs();
        let outgoing_distance = (offset.x * outgoing_normal.x + offset.y * outgoing_normal.y).abs();
        assert!((incoming_distance - half_width).abs() < 1e-5);
        assert!((outgoing_distance - half_width).abs() < 1e-5);
        assert!(outer.y > point.y + half_width * 2.0);
    }

    #[test]
    fn morph_tessellation_has_fixed_dual_position_topology() {
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .close();
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, -1.4))
            .line_to(Vec2::new(1.4, 0.0))
            .line_to(Vec2::new(0.0, 1.4))
            .line_to(Vec2::new(-1.4, 0.0))
            .close();
        let mesh = tessellate(&source.with_morph_target(target), 0.1).expect("valid morph");

        assert!(mesh.morphing);
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|vertex| vertex.surface == PathSurface::Stroke));
        assert!(mesh
            .vertices
            .iter()
            .any(|vertex| vertex.position != vertex.target_position));
        assert!(mesh
            .indices
            .iter()
            .all(|index| (*index as usize) < mesh.vertices.len()));
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
