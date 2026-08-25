use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};
use noon_core::{PathCommand, Rect, StrokeCap, StrokeJoin, Vec2, VectorPath};

const PATH_TESSELLATION_TOLERANCE: f32 = 0.002;
const MORPH_MITER_LIMIT: f32 = 4.0;
const ROUND_JOIN_SEGMENTS: usize = 8;
const ROUND_CAP_SEGMENTS: usize = 8;

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
    // Cached centerline measure used to place a procedural Create reveal head.
    // It is built only when geometry is tessellated, never per animation frame.
    reveal_points: Vec<RevealPoint>,
}

impl TessellatedPath {
    pub fn revealed_stroke_length(&self, reveal: f32) -> f32 {
        if !reveal.is_finite() {
            return 0.0;
        }
        self.stroke_length * reveal.clamp(0.0, 1.0)
    }

    /// Returns the local-space centerline position for normalized path progress.
    ///
    /// The lookup is O(log N) over a centerline measure cached with the mesh, so
    /// repeated Create frames do not flatten or tessellate the path again.
    pub fn reveal_head_position(&self, reveal: f32) -> Option<Vec2> {
        let first = *self.reveal_points.first()?;
        let last = *self.reveal_points.last()?;
        let reveal = if reveal.is_finite() {
            reveal.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let upper = self
            .reveal_points
            .partition_point(|point| point.progress < reveal);
        if upper == 0 {
            return Some(first.position);
        }
        if upper >= self.reveal_points.len() {
            return Some(last.position);
        }
        let left = self.reveal_points[upper - 1];
        let right = self.reveal_points[upper];
        let span = right.progress - left.progress;
        if span <= f32::EPSILON {
            return Some(right.position);
        }
        let t = ((reveal - left.progress) / span).clamp(0.0, 1.0);
        Some(Vec2::new(
            left.position.x + (right.position.x - left.position.x) * t,
            left.position.y + (right.position.y - left.position.y) * t,
        ))
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
struct RevealPoint {
    progress: f32,
    position: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CubicRevealCurve {
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TessellationVertex {
    position: Vec2,
    surface: PathSurface,
    path_distance: f32,
    path_progress: f32,
}

pub fn tessellate(path: &VectorPath, stroke_width: f32) -> Result<TessellatedPath, GeometryError> {
    tessellate_styled(path, stroke_width, StrokeJoin::Round, StrokeCap::Round)
}

pub fn tessellate_styled(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
) -> Result<TessellatedPath, GeometryError> {
    // Preserve the historical helper contract: static paths include their fill
    // surface, while morph paths were stroke-only before fill became an explicit
    // renderer/style decision. Production rendering uses the explicit variant.
    let fill_enabled = path.morph_target().is_none();
    tessellate_styled_with_fill(path, stroke_width, stroke_join, stroke_cap, fill_enabled)
}

pub fn tessellate_styled_with_fill(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
) -> Result<TessellatedPath, GeometryError> {
    tessellate_styled_with_fill_impl(
        path,
        stroke_width,
        stroke_join,
        stroke_cap,
        fill_enabled,
        false,
    )
}

/// Tessellate a morph while retaining source/target contour point order.
///
/// This is used by the Manim compatibility path, whose Transform contract is
/// index-based rather than Noon's native minimum-distance closed-contour matching.
pub fn tessellate_styled_with_fill_preserving_morph_order(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
) -> Result<TessellatedPath, GeometryError> {
    tessellate_styled_with_fill_impl(
        path,
        stroke_width,
        stroke_join,
        stroke_cap,
        fill_enabled,
        true,
    )
}

fn tessellate_styled_with_fill_impl(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
    preserve_morph_order: bool,
) -> Result<TessellatedPath, GeometryError> {
    if !stroke_width.is_finite() || stroke_width < 0.0 {
        return Err(GeometryError::InvalidStrokeWidth(stroke_width));
    }
    if let Some(target) = path.morph_target() {
        return tessellate_morph_path(
            path,
            target,
            stroke_width,
            stroke_join,
            stroke_cap,
            fill_enabled,
            preserve_morph_order,
        );
    }
    let reveal_points = build_reveal_points(path)?;
    let fill_path = build_lyon_path(path)?;
    let stroke_path = build_lyon_path_with_manim_progress(path)?;
    let mut buffers = VertexBuffers::new();

    if fill_enabled {
        FillTessellator::new()
            .tessellate_path(
                &fill_path,
                &FillOptions::default().with_tolerance(PATH_TESSELLATION_TOLERANCE),
                &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| {
                    TessellationVertex {
                        position: vec2(vertex.position().x, vertex.position().y),
                        surface: PathSurface::Fill,
                        path_distance: 0.0,
                        path_progress: 1.0,
                    }
                }),
            )
            .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
    }

    if stroke_width > 0.0 {
        StrokeTessellator::new()
            .tessellate_path(
                &stroke_path,
                &StrokeOptions::default()
                    .with_tolerance(PATH_TESSELLATION_TOLERANCE)
                    .with_line_width(stroke_width)
                    .with_miter_limit(MORPH_MITER_LIMIT)
                    .with_line_cap(lyon_line_cap(stroke_cap))
                    .with_line_join(lyon_line_join(stroke_join)),
                &mut BuffersBuilder::new(&mut buffers, |mut vertex: StrokeVertex<'_, '_>| {
                    let path_progress = vertex
                        .interpolated_attributes()
                        .first()
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    TessellationVertex {
                        position: vec2(vertex.position().x, vertex.position().y),
                        surface: PathSurface::Stroke,
                        // Keep advancement as an independent physical-length metric,
                        // but drive Create from Manim's curve-index + local-t
                        // parameter carried as an interpolated endpoint attribute.
                        path_distance: vertex.advancement(),
                        path_progress,
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
                MeshVertex {
                    position: vertex.position,
                    target_position: vertex.position,
                    surface: vertex.surface,
                    path_distance: vertex.path_distance,
                    path_progress: vertex.path_progress,
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
        reveal_points,
    })
}

fn build_reveal_points(path: &VectorPath) -> Result<Vec<RevealPoint>, GeometryError> {
    let curve_count = count_manim_curves(path)?;
    let mut points = Vec::new();
    let mut current = None;
    let mut contour_start = None;
    let mut curve_index = 0_usize;

    let progress = |index: usize| -> f32 {
        if curve_count == 0 {
            0.0
        } else {
            index as f32 / curve_count as f32
        }
    };

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                ensure_finite_point(to)?;
                points.push(RevealPoint {
                    progress: progress(curve_index),
                    position: to,
                });
                current = Some(to);
                contour_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                ensure_finite_point(to)?;
                let from = current.ok_or(GeometryError::DrawingBeforeMove)?;
                append_reveal_segment(
                    &mut points,
                    progress(curve_index),
                    progress(curve_index + 1),
                    from,
                    to,
                );
                curve_index += 1;
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                ensure_finite_point(control)?;
                ensure_finite_point(to)?;
                let from = current.ok_or(GeometryError::DrawingBeforeMove)?;
                flatten_quadratic_reveal(
                    &mut points,
                    progress(curve_index),
                    progress(curve_index + 1),
                    from,
                    control,
                    to,
                    0,
                );
                curve_index += 1;
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                ensure_finite_point(control1)?;
                ensure_finite_point(control2)?;
                ensure_finite_point(to)?;
                let from = current.ok_or(GeometryError::DrawingBeforeMove)?;
                flatten_cubic_reveal(
                    &mut points,
                    progress(curve_index),
                    progress(curve_index + 1),
                    CubicRevealCurve {
                        start: from,
                        control1,
                        control2,
                        end: to,
                    },
                    0,
                );
                curve_index += 1;
                current = Some(to);
            }
            PathCommand::Close => {
                let from = current.ok_or(GeometryError::CloseBeforeMove)?;
                let to = contour_start.ok_or(GeometryError::CloseBeforeMove)?;
                if (from.x - to.x).hypot(from.y - to.y) > f32::EPSILON {
                    append_reveal_segment(
                        &mut points,
                        progress(curve_index),
                        progress(curve_index + 1),
                        from,
                        to,
                    );
                    curve_index += 1;
                }
                current = Some(to);
            }
        }
    }
    Ok(points)
}

fn ensure_finite_point(point: Vec2) -> Result<(), GeometryError> {
    if point.x.is_finite() && point.y.is_finite() {
        Ok(())
    } else {
        Err(GeometryError::NonFinitePoint)
    }
}

fn append_reveal_segment(
    points: &mut Vec<RevealPoint>,
    start_progress: f32,
    end_progress: f32,
    from: Vec2,
    to: Vec2,
) {
    if points.is_empty() {
        points.push(RevealPoint {
            progress: start_progress,
            position: from,
        });
    }
    if (to.x - from.x).hypot(to.y - from.y) > 0.0 {
        points.push(RevealPoint {
            progress: end_progress,
            position: to,
        });
    }
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn point_line_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let denominator = (dx * dx + dy * dy).sqrt();
    if denominator <= f32::EPSILON {
        let px = point.x - start.x;
        let py = point.y - start.y;
        return (px * px + py * py).sqrt();
    }
    ((dx * (start.y - point.y) - (start.x - point.x) * dy).abs()) / denominator
}

fn flatten_quadratic_reveal(
    points: &mut Vec<RevealPoint>,
    start_progress: f32,
    end_progress: f32,
    start: Vec2,
    control: Vec2,
    end: Vec2,
    depth: u8,
) {
    if depth >= 16 || point_line_distance(control, start, end) <= PATH_TESSELLATION_TOLERANCE {
        append_reveal_segment(points, start_progress, end_progress, start, end);
        return;
    }
    let start_control = midpoint(start, control);
    let control_end = midpoint(control, end);
    let center = midpoint(start_control, control_end);
    let mid_progress = (start_progress + end_progress) * 0.5;
    flatten_quadratic_reveal(
        points,
        start_progress,
        mid_progress,
        start,
        start_control,
        center,
        depth + 1,
    );
    flatten_quadratic_reveal(
        points,
        mid_progress,
        end_progress,
        center,
        control_end,
        end,
        depth + 1,
    );
}

fn flatten_cubic_reveal(
    points: &mut Vec<RevealPoint>,
    start_progress: f32,
    end_progress: f32,
    curve: CubicRevealCurve,
    depth: u8,
) {
    let CubicRevealCurve {
        start,
        control1,
        control2,
        end,
    } = curve;
    let flatness =
        point_line_distance(control1, start, end).max(point_line_distance(control2, start, end));
    if depth >= 16 || flatness <= PATH_TESSELLATION_TOLERANCE {
        append_reveal_segment(points, start_progress, end_progress, start, end);
        return;
    }
    let a = midpoint(start, control1);
    let b = midpoint(control1, control2);
    let c = midpoint(control2, end);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let center = midpoint(d, e);
    let mid_progress = (start_progress + end_progress) * 0.5;
    flatten_cubic_reveal(
        points,
        start_progress,
        mid_progress,
        CubicRevealCurve {
            start,
            control1: a,
            control2: d,
            end: center,
        },
        depth + 1,
    );
    flatten_cubic_reveal(
        points,
        mid_progress,
        end_progress,
        CubicRevealCurve {
            start: center,
            control1: e,
            control2: c,
            end,
        },
        depth + 1,
    );
}

fn lyon_line_join(join: StrokeJoin) -> LineJoin {
    match join {
        StrokeJoin::Round => LineJoin::Round,
        StrokeJoin::Miter => LineJoin::Miter,
        StrokeJoin::Bevel => LineJoin::Bevel,
    }
}

fn lyon_line_cap(cap: StrokeCap) -> LineCap {
    match cap {
        StrokeCap::Round => LineCap::Round,
        StrokeCap::Butt => LineCap::Butt,
        StrokeCap::Square => LineCap::Square,
    }
}

fn tessellate_morph_path(
    source: &VectorPath,
    target: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
    fill_enabled: bool,
    preserve_morph_order: bool,
) -> Result<TessellatedPath, GeometryError> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let reveal_points = build_reveal_points(source)?;

    if fill_enabled {
        let fill = if preserve_morph_order {
            crate::plan_filled_morph_preserving_order(source, target, crate::MorphOptions::DEFAULT)
        } else {
            crate::plan_filled_morph(source, target, crate::MorphOptions::DEFAULT)
        }
        .map_err(|error| {
            GeometryError::Tessellation(format!("filled morph planning failed: {error}"))
        })?;
        let vertex_start = u32::try_from(vertices.len()).map_err(|_| {
            GeometryError::Tessellation("filled morph vertex count overflow".into())
        })?;
        for (source_point, target_point) in fill
            .contour
            .source_points
            .iter()
            .zip(&fill.contour.target_points)
        {
            vertices.push(MeshVertex {
                position: *source_point,
                target_position: *target_point,
                surface: PathSurface::Fill,
                path_distance: 0.0,
                path_progress: 1.0,
            });
        }
        vertices.push(MeshVertex {
            position: fill.source_center,
            target_position: fill.target_center,
            surface: PathSurface::Fill,
            path_distance: 0.0,
            path_progress: 1.0,
        });
        indices.extend(fill.indices.iter().map(|index| {
            index
                .checked_add(vertex_start)
                .expect("filled morph index overflow validated by vertex count")
        }));
    }

    if stroke_width == 0.0 {
        let bounds = morph_mesh_bounds(&vertices);
        return Ok(TessellatedPath {
            vertices,
            indices,
            bounds,
            stroke_length: 0.0,
            morphing: true,
            reveal_points,
        });
    }

    let plan = if preserve_morph_order {
        crate::plan_morph_preserving_order(source, target, crate::MorphOptions::DEFAULT)
    } else {
        crate::plan_morph(source, target, crate::MorphOptions::DEFAULT)
    }
    .map_err(|error| GeometryError::Tessellation(format!("morph planning failed: {error}")))?;
    let total_points = plan.point_count();
    let mut global_point = 0_usize;
    let progress_denominator = total_points.saturating_sub(1).max(1) as f32;
    let half_width = stroke_width * 0.5;
    let mut stroke_length = 0.0_f32;

    for contour in &plan.contours {
        let point_count = contour.source_points.len();
        let segment_count = if contour.closed {
            point_count
        } else {
            point_count - 1
        };
        stroke_length += polyline_length(&contour.source_points, contour.closed);

        // Independent segment quads establish exact butt faces. Join/cap
        // primitives then fill only the area outside those faces. This keeps a
        // fixed topology even when a morph changes turn direction.
        for segment in 0..segment_count {
            let next = if segment + 1 == point_count {
                0
            } else {
                segment + 1
            };
            let source_quad = segment_quad(
                &contour.source_points,
                segment,
                next,
                contour.closed,
                stroke_cap,
                half_width,
            );
            let target_quad = segment_quad(
                &contour.target_points,
                segment,
                next,
                contour.closed,
                stroke_cap,
                half_width,
            );
            let start_progress = (global_point + segment) as f32 / progress_denominator;
            let end_progress = (global_point + next) as f32 / progress_denominator;
            add_paired_polygon(
                &mut vertices,
                &mut indices,
                &source_quad,
                &target_quad,
                &[0, 1, 2, 1, 3, 2],
                &[start_progress, start_progress, end_progress, end_progress],
            )?;
        }

        let join_range: Box<dyn Iterator<Item = usize>> = if contour.closed {
            Box::new(0..point_count)
        } else {
            Box::new(1..point_count - 1)
        };
        for index in join_range {
            let previous = if index == 0 {
                point_count - 1
            } else {
                index - 1
            };
            let next = if index + 1 == point_count {
                0
            } else {
                index + 1
            };
            let progress = (global_point + index) as f32 / progress_denominator;
            for side in [StrokeSide::Left, StrokeSide::Right] {
                let source_join = join_polygon(
                    contour.source_points[previous],
                    contour.source_points[index],
                    contour.source_points[next],
                    half_width,
                    stroke_join,
                    side,
                );
                let target_join = join_polygon(
                    contour.target_points[previous],
                    contour.target_points[index],
                    contour.target_points[next],
                    half_width,
                    stroke_join,
                    side,
                );
                debug_assert_eq!(source_join.points.len(), target_join.points.len());
                debug_assert_eq!(source_join.indices, target_join.indices);
                let progress_values = vec![progress; source_join.points.len()];
                add_paired_polygon(
                    &mut vertices,
                    &mut indices,
                    &source_join.points,
                    &target_join.points,
                    &source_join.indices,
                    &progress_values,
                )?;
            }
        }

        if !contour.closed && stroke_cap == StrokeCap::Round {
            let source_start = round_cap_polygon(&contour.source_points, true, half_width);
            let target_start = round_cap_polygon(&contour.target_points, true, half_width);
            let start_progress = (global_point as f32 / progress_denominator).clamp(0.0, 1.0);
            add_paired_polygon(
                &mut vertices,
                &mut indices,
                &source_start.points,
                &target_start.points,
                &source_start.indices,
                &vec![start_progress; source_start.points.len()],
            )?;

            let source_end = round_cap_polygon(&contour.source_points, false, half_width);
            let target_end = round_cap_polygon(&contour.target_points, false, half_width);
            let end_progress =
                ((global_point + point_count - 1) as f32 / progress_denominator).clamp(0.0, 1.0);
            add_paired_polygon(
                &mut vertices,
                &mut indices,
                &source_end.points,
                &target_end.points,
                &source_end.indices,
                &vec![end_progress; source_end.points.len()],
            )?;
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
        reveal_points,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StrokeSide {
    Left,
    Right,
}

#[derive(Clone, Debug)]
struct LocalPolygon {
    points: Vec<Vec2>,
    indices: Vec<u32>,
}

fn segment_quad(
    points: &[Vec2],
    segment: usize,
    next: usize,
    closed: bool,
    cap: StrokeCap,
    half_width: f32,
) -> [Vec2; 4] {
    let mut start = points[segment];
    let mut end = points[next];
    let tangent = normalized(Vec2::new(end.x - start.x, end.y - start.y));
    if !closed && cap == StrokeCap::Square {
        if segment == 0 {
            start.x -= tangent.x * half_width;
            start.y -= tangent.y * half_width;
        }
        if next + 1 == points.len() {
            end.x += tangent.x * half_width;
            end.y += tangent.y * half_width;
        }
    }
    let normal = Vec2::new(-tangent.y * half_width, tangent.x * half_width);
    [
        Vec2::new(start.x + normal.x, start.y + normal.y),
        Vec2::new(start.x - normal.x, start.y - normal.y),
        Vec2::new(end.x + normal.x, end.y + normal.y),
        Vec2::new(end.x - normal.x, end.y - normal.y),
    ]
}

fn join_polygon(
    previous: Vec2,
    point: Vec2,
    next: Vec2,
    half_width: f32,
    join: StrokeJoin,
    side: StrokeSide,
) -> LocalPolygon {
    let incoming = normalized(Vec2::new(point.x - previous.x, point.y - previous.y));
    let outgoing = normalized(Vec2::new(next.x - point.x, next.y - point.y));
    let turn = cross(incoming, outgoing);
    let side_is_outer = match side {
        StrokeSide::Left => turn < -1.0e-6,
        StrokeSide::Right => turn > 1.0e-6,
    };
    if !side_is_outer {
        return collapsed_join(point, join);
    }

    let sign = if side == StrokeSide::Left { 1.0 } else { -1.0 };
    let incoming_normal = Vec2::new(-incoming.y * sign, incoming.x * sign);
    let outgoing_normal = Vec2::new(-outgoing.y * sign, outgoing.x * sign);
    let outer_in = Vec2::new(
        point.x + incoming_normal.x * half_width,
        point.y + incoming_normal.y * half_width,
    );
    let outer_out = Vec2::new(
        point.x + outgoing_normal.x * half_width,
        point.y + outgoing_normal.y * half_width,
    );

    match join {
        StrokeJoin::Bevel => LocalPolygon {
            points: vec![point, outer_in, outer_out],
            indices: vec![0, 1, 2],
        },
        StrokeJoin::Miter => {
            let miter = miter_point(
                point,
                incoming,
                outgoing,
                incoming_normal,
                outgoing_normal,
                half_width,
            )
            .unwrap_or(outer_out);
            LocalPolygon {
                points: vec![point, outer_in, miter, outer_out],
                indices: vec![0, 1, 2, 0, 2, 3],
            }
        }
        StrokeJoin::Round => {
            let start_angle = incoming_normal.y.atan2(incoming_normal.x);
            let end_angle = outgoing_normal.y.atan2(outgoing_normal.x);
            let mut delta = normalized_angle(end_angle - start_angle);
            // The active side is the outside of the signed turn, so the short
            // sweep must follow the same sign as that outer arc.
            if side == StrokeSide::Left && delta > 0.0 {
                delta -= std::f32::consts::TAU;
            } else if side == StrokeSide::Right && delta < 0.0 {
                delta += std::f32::consts::TAU;
            }
            let mut points = Vec::with_capacity(ROUND_JOIN_SEGMENTS + 2);
            points.push(point);
            for step in 0..=ROUND_JOIN_SEGMENTS {
                let angle = start_angle + delta * step as f32 / ROUND_JOIN_SEGMENTS as f32;
                points.push(Vec2::new(
                    point.x + angle.cos() * half_width,
                    point.y + angle.sin() * half_width,
                ));
            }
            LocalPolygon {
                points,
                indices: fan_indices(ROUND_JOIN_SEGMENTS),
            }
        }
    }
}

fn collapsed_join(point: Vec2, join: StrokeJoin) -> LocalPolygon {
    let point_count = match join {
        StrokeJoin::Bevel => 3,
        StrokeJoin::Miter => 4,
        StrokeJoin::Round => ROUND_JOIN_SEGMENTS + 2,
    };
    let indices = match join {
        StrokeJoin::Bevel => vec![0, 1, 2],
        StrokeJoin::Miter => vec![0, 1, 2, 0, 2, 3],
        StrokeJoin::Round => fan_indices(ROUND_JOIN_SEGMENTS),
    };
    LocalPolygon {
        points: vec![point; point_count],
        indices,
    }
}

fn miter_point(
    point: Vec2,
    incoming: Vec2,
    outgoing: Vec2,
    incoming_normal: Vec2,
    outgoing_normal: Vec2,
    half_width: f32,
) -> Option<Vec2> {
    let a = Vec2::new(
        point.x + incoming_normal.x * half_width,
        point.y + incoming_normal.y * half_width,
    );
    let b = Vec2::new(
        point.x + outgoing_normal.x * half_width,
        point.y + outgoing_normal.y * half_width,
    );
    let denominator = cross(incoming, outgoing);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let difference = Vec2::new(b.x - a.x, b.y - a.y);
    let distance = cross(difference, outgoing) / denominator;
    let intersection = Vec2::new(a.x + incoming.x * distance, a.y + incoming.y * distance);
    if (intersection.x - point.x).hypot(intersection.y - point.y) > half_width * MORPH_MITER_LIMIT {
        None
    } else {
        Some(intersection)
    }
}

fn round_cap_polygon(points: &[Vec2], start: bool, half_width: f32) -> LocalPolygon {
    let (center, tangent) = if start {
        (
            points[0],
            normalized(Vec2::new(
                points[1].x - points[0].x,
                points[1].y - points[0].y,
            )),
        )
    } else {
        let last = points.len() - 1;
        (
            points[last],
            normalized(Vec2::new(
                points[last].x - points[last - 1].x,
                points[last].y - points[last - 1].y,
            )),
        )
    };
    let outward_angle = tangent.y.atan2(tangent.x) + if start { std::f32::consts::PI } else { 0.0 };
    let arc_start = outward_angle - std::f32::consts::FRAC_PI_2;
    let mut result = Vec::with_capacity(ROUND_CAP_SEGMENTS + 2);
    result.push(center);
    for step in 0..=ROUND_CAP_SEGMENTS {
        let angle = arc_start + std::f32::consts::PI * step as f32 / ROUND_CAP_SEGMENTS as f32;
        result.push(Vec2::new(
            center.x + angle.cos() * half_width,
            center.y + angle.sin() * half_width,
        ));
    }
    LocalPolygon {
        points: result,
        indices: fan_indices(ROUND_CAP_SEGMENTS),
    }
}

fn fan_indices(segment_count: usize) -> Vec<u32> {
    let mut indices = Vec::with_capacity(segment_count * 3);
    for segment in 0..segment_count {
        indices.extend_from_slice(&[0, (segment + 1) as u32, (segment + 2) as u32]);
    }
    indices
}

fn add_paired_polygon(
    vertices: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    source: &[Vec2],
    target: &[Vec2],
    local_indices: &[u32],
    progress: &[f32],
) -> Result<(), GeometryError> {
    debug_assert_eq!(source.len(), target.len());
    debug_assert_eq!(source.len(), progress.len());
    let start = u32::try_from(vertices.len())
        .map_err(|_| GeometryError::Tessellation("morph vertex count exceeds u32".into()))?;
    for ((position, target_position), path_progress) in
        source.iter().zip(target).zip(progress.iter().copied())
    {
        vertices.push(MeshVertex {
            position: *position,
            target_position: *target_position,
            surface: PathSurface::Stroke,
            path_distance: path_progress,
            path_progress,
        });
    }
    for index in local_indices {
        indices.push(
            start
                .checked_add(*index)
                .ok_or_else(|| GeometryError::Tessellation("morph index exceeds u32".into()))?,
        );
    }
    Ok(())
}

fn cross(left: Vec2, right: Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

fn normalized_angle(mut angle: f32) -> f32 {
    while angle <= -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    while angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    }
    angle
}

#[cfg(test)]
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
            test_segment_normal(point, next, half_width)
        } else if !closed && index + 1 == points.len() {
            test_segment_normal(previous, point, half_width)
        } else {
            test_miter_offset(previous, point, next, half_width)
        };
        result.push((
            Vec2::new(point.x + offset.x, point.y + offset.y),
            Vec2::new(point.x - offset.x, point.y - offset.y),
        ));
    }
    result
}

#[cfg(test)]
fn test_segment_normal(from: Vec2, to: Vec2, half_width: f32) -> Vec2 {
    let tangent = normalized(Vec2::new(to.x - from.x, to.y - from.y));
    Vec2::new(-tangent.y * half_width, tangent.x * half_width)
}

#[cfg(test)]
fn test_miter_offset(previous: Vec2, point: Vec2, next: Vec2, half_width: f32) -> Vec2 {
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

fn count_manim_curves(path: &VectorPath) -> Result<usize, GeometryError> {
    let mut count = 0_usize;
    let mut active = false;
    let mut current = None;
    let mut contour_start = None;
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                active = true;
                current = Some(to);
                contour_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                count += 1;
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                count += 1;
                current = Some(to);
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
                count += 1;
                current = Some(to);
            }
            PathCommand::Close => {
                require_active(active)?;
                let from = current.ok_or(GeometryError::CloseBeforeMove)?;
                let to = contour_start.ok_or(GeometryError::CloseBeforeMove)?;
                if (from.x - to.x).hypot(from.y - to.y) > f32::EPSILON {
                    count += 1;
                }
                active = false;
                current = Some(to);
            }
        }
    }
    Ok(count)
}

fn build_lyon_path_with_manim_progress(path: &VectorPath) -> Result<Path, GeometryError> {
    let curve_count = count_manim_curves(path)?;
    let mut builder = Path::builder_with_attributes(1);
    let mut active = false;
    let mut current = None;
    let mut contour_start = None;
    let mut curve_index = 0_usize;
    let progress = |index: usize| -> f32 {
        if curve_count == 0 {
            0.0
        } else {
            index as f32 / curve_count as f32
        }
    };

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    builder.end(false);
                }
                builder.begin(point(to.x, to.y), &[progress(curve_index)]);
                active = true;
                current = Some(to);
                contour_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                curve_index += 1;
                builder.line_to(point(to.x, to.y), &[progress(curve_index)]);
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                curve_index += 1;
                builder.quadratic_bezier_to(
                    point(control.x, control.y),
                    point(to.x, to.y),
                    &[progress(curve_index)],
                );
                current = Some(to);
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
                curve_index += 1;
                builder.cubic_bezier_to(
                    point(control1.x, control1.y),
                    point(control2.x, control2.y),
                    point(to.x, to.y),
                    &[progress(curve_index)],
                );
                current = Some(to);
            }
            PathCommand::Close => {
                require_active(active)?;
                let from = current.ok_or(GeometryError::CloseBeforeMove)?;
                let to = contour_start.ok_or(GeometryError::CloseBeforeMove)?;
                // A native lyon close interpolates back to the first endpoint's
                // attribute, which would make reveal progress run backwards. Emit
                // Manim's closing curve explicitly with the next global progress.
                if (from.x - to.x).hypot(from.y - to.y) > f32::EPSILON {
                    curve_index += 1;
                    builder.line_to(point(to.x, to.y), &[progress(curve_index)]);
                }
                builder.end(false);
                active = false;
                current = Some(to);
            }
        }
    }
    if active {
        builder.end(false);
    }
    Ok(builder.build())
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
    fn multiple_contours_use_one_global_manim_curve_parameter() {
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
            .all(|progress| *progress >= 0.5 - 1e-5));
        assert!(second_contour_progresses
            .iter()
            .any(|progress| (*progress - 1.0).abs() < 1e-5));
    }

    #[test]
    fn reveal_head_uses_curve_count_not_arc_length() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(100.0, 0.0))
            .line_to(Vec2::new(100.0, 1.0));
        let mesh = tessellate(&path, 0.2).expect("valid path");
        let halfway = mesh.reveal_head_position(0.5).expect("reveal head");
        assert!((halfway.x - 100.0).abs() < 1e-5);
        assert!(halfway.y.abs() < 1e-5);

        let three_quarters = mesh.reveal_head_position(0.75).expect("reveal head");
        assert!((three_quarters.x - 100.0).abs() < 1e-5);
        assert!((three_quarters.y - 0.5).abs() < 1e-5);
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
