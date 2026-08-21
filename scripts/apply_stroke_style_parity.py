from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected fragment not found in {path}:\n{old[:900]}")
    file.write_text(text.replace(old, new, 1))


def replace_region(path: str, start: str, end: str, replacement: str) -> None:
    file = Path(path)
    text = file.read_text()
    start_index = text.find(start)
    if start_index < 0:
        if replacement.strip() in text:
            return
        raise SystemExit(f"start marker not found in {path}: {start}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise SystemExit(f"end marker not found in {path}: {end}")
    file.write_text(text[:start_index] + replacement + text[end_index:])


# ---------------------------------------------------------------------------
# noon-core: semantic stroke join/cap properties with backward-compatible IR.
# ---------------------------------------------------------------------------
replace_once(
    "crates/noon-core/src/lib.rs",
    '''#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]\npub struct Style {\n    pub fill: Option<Color>,\n    pub stroke: Option<Color>,\n    pub stroke_width: f32,\n    pub opacity: f32,\n}\n\nimpl Default for Style {\n    fn default() -> Self {\n        Self {\n            fill: Some(Color::WHITE),\n            stroke: None,\n            stroke_width: 1.0,\n            opacity: 1.0,\n        }\n    }\n}\n''',
    '''#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]\n#[serde(rename_all = "snake_case")]\npub enum StrokeJoin {\n    #[default]\n    Round,\n    Miter,\n    Bevel,\n}\n\n#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]\n#[serde(rename_all = "snake_case")]\npub enum StrokeCap {\n    #[default]\n    Round,\n    Butt,\n    Square,\n}\n\n#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]\npub struct Style {\n    pub fill: Option<Color>,\n    pub stroke: Option<Color>,\n    pub stroke_width: f32,\n    #[serde(default)]\n    pub stroke_join: StrokeJoin,\n    #[serde(default)]\n    pub stroke_cap: StrokeCap,\n    pub opacity: f32,\n}\n\nimpl Default for Style {\n    fn default() -> Self {\n        Self {\n            fill: Some(Color::WHITE),\n            stroke: None,\n            stroke_width: 1.0,\n            stroke_join: StrokeJoin::Round,\n            stroke_cap: StrokeCap::Round,\n            opacity: 1.0,\n        }\n    }\n}\n''',
)

# Fill existing workspace Style literals with explicit defaults. This is scoped
# to the new architecture crates; legacy Noon has an unrelated Style type.
def add_style_defaults(path: Path) -> None:
    text = path.read_text()
    out = []
    cursor = 0
    changed = False
    while True:
        index = text.find("Style {", cursor)
        if index < 0:
            out.append(text[cursor:])
            break
        out.append(text[cursor:index])
        # Struct declaration, not a literal.
        prefix = text[max(0, index - 24):index]
        if "struct " in prefix:
            out.append("Style {")
            cursor = index + len("Style {")
            continue
        brace = index + len("Style ")
        depth = 0
        end = None
        for pos in range(brace, len(text)):
            char = text[pos]
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    end = pos
                    break
        if end is None:
            raise SystemExit(f"unbalanced Style literal in {path}")
        block = text[index:end + 1]
        if "stroke_join:" in block or ":" not in block:
            out.append(block)
        else:
            indent_start = text.rfind("\n", index, end) + 1
            closing_indent = text[indent_start:end]
            # If the closing brace is not on its own indentation, derive it from
            # the line containing the brace.
            closing_line_start = text.rfind("\n", index, end) + 1
            closing_indent = text[closing_line_start:end]
            if closing_indent.strip():
                closing_indent = ""
            field_indent = closing_indent + "    "
            insertion = (
                f"\n{field_indent}stroke_join: noon_core::StrokeJoin::Round,"
                f"\n{field_indent}stroke_cap: noon_core::StrokeCap::Round,"
                f"\n{closing_indent}"
            )
            block = block[:-1].rstrip() + "," + insertion + "}"
            out.append(block)
            changed = True
        cursor = end + 1
    if changed:
        path.write_text("".join(out))


for rust_file in Path("crates").rglob("*.rs"):
    if rust_file.as_posix() == "crates/noon-core/src/lib.rs":
        continue
    add_style_defaults(rust_file)

# ---------------------------------------------------------------------------
# Geometry: shared Lyon/static style and fixed-topology morph joins/caps.
# ---------------------------------------------------------------------------
replace_once(
    "crates/noon-geometry/src/tessellation.rs",
    "use noon_core::{PathCommand, Rect, Vec2, VectorPath};",
    "use noon_core::{PathCommand, Rect, StrokeCap, StrokeJoin, Vec2, VectorPath};",
)
replace_once(
    "crates/noon-geometry/src/tessellation.rs",
    "const MORPH_MITER_LIMIT: f32 = 4.0;",
    "const MORPH_MITER_LIMIT: f32 = 4.0;\nconst ROUND_JOIN_SEGMENTS: usize = 8;\nconst ROUND_CAP_SEGMENTS: usize = 8;",
)

new_tessellate = r'''pub fn tessellate(path: &VectorPath, stroke_width: f32) -> Result<TessellatedPath, GeometryError> {
    tessellate_styled(
        path,
        stroke_width,
        StrokeJoin::Round,
        StrokeCap::Round,
    )
}

pub fn tessellate_styled(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
) -> Result<TessellatedPath, GeometryError> {
    if !stroke_width.is_finite() || stroke_width < 0.0 {
        return Err(GeometryError::InvalidStrokeWidth(stroke_width));
    }
    if let Some(target) = path.morph_target() {
        return tessellate_morph_path(path, target, stroke_width, stroke_join, stroke_cap);
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
                    .with_miter_limit(MORPH_MITER_LIMIT)
                    .with_line_cap(lyon_line_cap(stroke_cap))
                    .with_line_join(lyon_line_join(stroke_join)),
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

'''
replace_region(
    "crates/noon-geometry/src/tessellation.rs",
    "pub fn tessellate(",
    "fn tessellate_morph_path(",
    new_tessellate,
)

new_morph = r'''fn tessellate_morph_path(
    source: &VectorPath,
    target: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
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
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
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
            let next = if segment + 1 == point_count { 0 } else { segment + 1 };
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
            let previous = if index == 0 { point_count - 1 } else { index - 1 };
            let next = if index + 1 == point_count { 0 } else { index + 1 };
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
            let end_progress = ((global_point + point_count - 1) as f32 / progress_denominator)
                .clamp(0.0, 1.0);
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
    if (intersection.x - point.x).hypot(intersection.y - point.y)
        > half_width * MORPH_MITER_LIMIT
    {
        None
    } else {
        Some(intersection)
    }
}

fn round_cap_polygon(points: &[Vec2], start: bool, half_width: f32) -> LocalPolygon {
    let (center, tangent) = if start {
        (points[0], normalized(Vec2::new(points[1].x - points[0].x, points[1].y - points[0].y)))
    } else {
        let last = points.len() - 1;
        (
            points[last],
            normalized(Vec2::new(points[last].x - points[last - 1].x, points[last].y - points[last - 1].y)),
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

'''
replace_region(
    "crates/noon-geometry/src/tessellation.rs",
    "fn tessellate_morph_path(",
    "fn normalized(",
    new_morph,
)

# ---------------------------------------------------------------------------
# Renderer: include join/cap in path mesh cache identity and tessellation.
# ---------------------------------------------------------------------------
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    "use noon_core::{Color, GeometryRef, ObjectId, PathCommand, Style, Transform2D, VectorPath};",
    "use noon_core::{Color, GeometryRef, ObjectId, PathCommand, StrokeCap, StrokeJoin, Style, Transform2D, VectorPath};",
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''struct PathMeshKey {\n    path_hash: u64,\n    stroke_width_bits: u32,\n}\n\n#[derive(Clone, Debug)]\nstruct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    mesh: TessellatedPath,\n}\n''',
    '''struct PathMeshKey {\n    path_hash: u64,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n}\n\n#[derive(Clone, Debug)]\nstruct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    mesh: TessellatedPath,\n}\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    "self.cache_path_mesh(path, object.style.stroke_width)",
    "self.cache_path_mesh(path, object.style)",
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''                    && cache.path == *path\n                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()\n''',
    '''                    && cache.path == *path\n                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()\n                    && cache.stroke_join == object.style.stroke_join\n                    && cache.stroke_cap == object.style.stroke_cap\n''',
)
replace_region(
    "crates/noon-render-wgpu/src/lib.rs",
    "    fn cache_path_mesh(\n",
    "    pub fn cached_path_mesh_count",
    r'''    fn cache_path_mesh(
        &mut self,
        path: &VectorPath,
        style: Style,
    ) -> Result<(usize, bool), noon_geometry::GeometryError> {
        let stroke_width_bits = style.stroke_width.to_bits();
        let key = path_mesh_key(path, stroke_width_bits, style.stroke_join, style.stroke_cap);
        if let Some(candidates) = self.path_mesh_lookup.get(&key) {
            if let Some(index) = candidates.iter().copied().find(|&index| {
                let entry = &self.path_mesh_cache[index];
                entry.path == *path
                    && entry.stroke_width_bits == stroke_width_bits
                    && entry.stroke_join == style.stroke_join
                    && entry.stroke_cap == style.stroke_cap
            }) {
                return Ok((index, false));
            }
        }

        let mesh = noon_geometry::tessellate_styled(
            path,
            style.stroke_width,
            style.stroke_join,
            style.stroke_cap,
        )?;
        let index = self.path_mesh_cache.len();
        self.path_mesh_cache.push(CachedPathMesh {
            path: path.clone(),
            stroke_width_bits,
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            mesh,
        });
        self.path_mesh_lookup.entry(key).or_default().push(index);
        Ok((index, true))
    }

''',
)
replace_region(
    "crates/noon-render-wgpu/src/lib.rs",
    "fn path_mesh_key(",
    "fn hash_vector_path(",
    r'''fn path_mesh_key(
    path: &VectorPath,
    stroke_width_bits: u32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
) -> PathMeshKey {
    let mut hasher = DefaultHasher::new();
    hash_vector_path(path, &mut hasher);
    PathMeshKey {
        path_hash: hasher.finish(),
        stroke_width_bits,
        stroke_join,
        stroke_cap,
    }
}

''',
)

# ---------------------------------------------------------------------------
# Compiler/runtime: path join/cap are topology choices, not interpolated scalars.
# ---------------------------------------------------------------------------
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''        if from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits() {\n            return Err(TransformCompileFailure::RequiresRetessellation);\n        }\n''',
    '''        if from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits()\n            || from.style.stroke_join != to.style.stroke_join\n            || from.style.stroke_cap != to.style.stroke_cap\n        {\n            return Err(TransformCompileFailure::RequiresRetessellation);\n        }\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''        stroke_width: lerp(from.stroke_width, to.stroke_width, progress),\n        opacity: lerp(from.opacity, to.opacity, progress),\n''',
    '''        stroke_width: lerp(from.stroke_width, to.stroke_width, progress),\n        stroke_join: if progress >= 1.0 { to.stroke_join } else { from.stroke_join },\n        stroke_cap: if progress >= 1.0 { to.stroke_cap } else { from.stroke_cap },\n        opacity: lerp(from.opacity, to.opacity, progress),\n''',
)

# ---------------------------------------------------------------------------
# Python authoring: expose validated round/miter/bevel and round/butt/square.
# ---------------------------------------------------------------------------
replace_once(
    "web/python/noon.py",
    '''def _unit_interval(name: str, value: Any) -> float:\n    result = _finite_number(name, value)\n    if not 0.0 <= result <= 1.0:\n        raise ValueError(f"{name} must be between 0 and 1")\n    return result\n\n\n''',
    '''def _unit_interval(name: str, value: Any) -> float:\n    result = _finite_number(name, value)\n    if not 0.0 <= result <= 1.0:\n        raise ValueError(f"{name} must be between 0 and 1")\n    return result\n\n\ndef _stroke_join(value: Any) -> str:\n    if not isinstance(value, str):\n        raise TypeError("stroke_join must be a string")\n    if value not in {"round", "miter", "bevel"}:\n        raise ValueError("stroke_join must be round, miter, or bevel")\n    return value\n\n\ndef _stroke_cap(value: Any) -> str:\n    if not isinstance(value, str):\n        raise TypeError("stroke_cap must be a string")\n    if value not in {"round", "butt", "square"}:\n        raise ValueError("stroke_cap must be round, butt, or square")\n    return value\n\n\n''',
)
replace_once(
    "web/python/noon.py",
    '''    stroke_width: float = 1.0,\n    opacity: float = 1.0,\n) -> Mobject:\n''',
    '''    stroke_width: float = 1.0,\n    stroke_join: str = "round",\n    stroke_cap: str = "round",\n    opacity: float = 1.0,\n) -> Mobject:\n''',
)
replace_once(
    "web/python/noon.py",
    '''            "stroke_width": width,\n            "opacity": _finite_number("opacity", opacity),\n''',
    '''            "stroke_width": width,\n            "stroke_join": _stroke_join(stroke_join),\n            "stroke_cap": _stroke_cap(stroke_cap),\n            "opacity": _finite_number("opacity", opacity),\n''',
)
# Add public Scene constructor parameters for all four primitive/path methods.
text_path = Path("web/python/noon.py")
text = text_path.read_text()
needle = '''        stroke_width: float = 1.0,\n        opacity: float = 1.0,\n        key: str | None = None,\n'''
text = text.replace(
    needle,
    '''        stroke_width: float = 1.0,\n        stroke_join: str = "round",\n        stroke_cap: str = "round",\n        opacity: float = 1.0,\n        key: str | None = None,\n''',
    2,
)
needle_line = '''        stroke_width: float = 0.1,\n        opacity: float = 1.0,\n        key: str | None = None,\n'''
text = text.replace(
    needle_line,
    '''        stroke_width: float = 0.1,\n        stroke_join: str = "round",\n        stroke_cap: str = "round",\n        opacity: float = 1.0,\n        key: str | None = None,\n''',
    2,
)
# Thread the new args through each Scene._add_object call.
text = text.replace(
    '''            stroke_width=stroke_width,\n            opacity=opacity,\n            key=key,\n''',
    '''            stroke_width=stroke_width,\n            stroke_join=stroke_join,\n            stroke_cap=stroke_cap,\n            opacity=opacity,\n            key=key,\n''',
    4,
)
# Internal _add_object signature.
text = text.replace(
    '''        stroke_width: float,\n        opacity: float,\n        key: str | None,\n''',
    '''        stroke_width: float,\n        stroke_join: str,\n        stroke_cap: str,\n        opacity: float,\n        key: str | None,\n''',
    1,
)
# Internal style dict (the first occurrence was already changed in _make_mobject).
old_style = '''                    "stroke_width": width,\n                    "opacity": _finite_number("opacity", opacity),\n'''
new_style = '''                    "stroke_width": width,\n                    "stroke_join": _stroke_join(stroke_join),\n                    "stroke_cap": _stroke_cap(stroke_cap),\n                    "opacity": _finite_number("opacity", opacity),\n'''
if old_style not in text:
    raise SystemExit("Scene._add_object style fragment not found")
text = text.replace(old_style, new_style, 1)
# PatchBatch style support.
text = text.replace(
    '''        stroke_width: float,\n        opacity: float = 1.0,\n    ) -> PatchBatch:\n''',
    '''        stroke_width: float,\n        stroke_join: str = "round",\n        stroke_cap: str = "round",\n        opacity: float = 1.0,\n    ) -> PatchBatch:\n''',
    1,
)
text = text.replace(
    '''                        "stroke_width": _finite_number(\n                            "stroke_width", stroke_width\n                        ),\n                        "opacity": _finite_number("opacity", opacity),\n''',
    '''                        "stroke_width": _finite_number(\n                            "stroke_width", stroke_width\n                        ),\n                        "stroke_join": _stroke_join(stroke_join),\n                        "stroke_cap": _stroke_cap(stroke_cap),\n                        "opacity": _finite_number("opacity", opacity),\n''',
    1,
)
text_path.write_text(text)

# ---------------------------------------------------------------------------
# Correctness/regression tests.
# ---------------------------------------------------------------------------
Path("crates/noon-geometry/tests/stroke_style_parity.rs").write_text(r'''use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{tessellate_styled, TessellatedPath};

fn open_corner() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-2.0, 0.0))
        .line_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(0.0, 2.0))
}

fn bounds(mesh: &TessellatedPath, target: bool) -> (Vec2, Vec2) {
    let mut points = mesh.vertices.iter().filter(|vertex| {
        matches!(vertex.surface, noon_geometry::PathSurface::Stroke)
    }).map(|vertex| if target { vertex.target_position } else { vertex.position });
    let first = points.next().expect("stroke mesh must contain vertices");
    let mut min = first;
    let mut max = first;
    for point in points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    (min, max)
}

fn assert_bounds_close(left: (Vec2, Vec2), right: (Vec2, Vec2), tolerance: f32) {
    for (actual, expected) in [
        (left.0.x, right.0.x),
        (left.0.y, right.0.y),
        (left.1.x, right.1.x),
        (left.1.y, right.1.y),
    ] {
        assert!((actual - expected).abs() <= tolerance, "{actual} != {expected}");
    }
}

#[test]
fn static_and_identity_morph_have_matching_endpoint_bounds_for_all_styles() {
    let source = open_corner();
    for join in [StrokeJoin::Round, StrokeJoin::Miter, StrokeJoin::Bevel] {
        for cap in [StrokeCap::Round, StrokeCap::Butt, StrokeCap::Square] {
            let static_mesh = tessellate_styled(&source, 0.4, join, cap).unwrap();
            let morph = source.clone().with_morph_target(source.clone());
            let morph_mesh = tessellate_styled(&morph, 0.4, join, cap).unwrap();
            assert_bounds_close(bounds(&static_mesh, false), bounds(&morph_mesh, false), 1.0e-4);
            assert_bounds_close(bounds(&static_mesh, false), bounds(&morph_mesh, true), 1.0e-4);
        }
    }
}

#[test]
fn open_caps_match_theoretical_extents() {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    let half_width = 0.25;
    for (cap, expected_x) in [
        (StrokeCap::Butt, 1.0),
        (StrokeCap::Round, 1.0 + half_width),
        (StrokeCap::Square, 1.0 + half_width),
    ] {
        let morph = path.clone().with_morph_target(path.clone());
        let mesh = tessellate_styled(&morph, half_width * 2.0, StrokeJoin::Round, cap).unwrap();
        let (min, max) = bounds(&mesh, false);
        assert!((min.x + expected_x).abs() < 1.0e-5);
        assert!((max.x - expected_x).abs() < 1.0e-5);
        assert!((min.y + half_width).abs() < 1.0e-5);
        assert!((max.y - half_width).abs() < 1.0e-5);
    }
}

#[test]
fn right_angle_miter_reaches_closed_form_intersection() {
    let path = open_corner();
    let morph = path.clone().with_morph_target(path);
    let mesh = tessellate_styled(&morph, 0.4, StrokeJoin::Miter, StrokeCap::Butt).unwrap();
    // A left turn has its outer miter at (+h, -h) about the corner (0,0).
    assert!(mesh.vertices.iter().any(|vertex| {
        (vertex.position.x - 0.2).abs() < 1.0e-6
            && (vertex.position.y + 0.2).abs() < 1.0e-6
    }));
}

#[test]
fn round_join_and_cap_topology_is_fixed_when_turn_direction_changes() {
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 1.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, -1.0));
    let mesh = tessellate_styled(
        &source.with_morph_target(target),
        0.2,
        StrokeJoin::Round,
        StrokeCap::Round,
    )
    .unwrap();
    assert!(mesh.morphing);
    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
}
''')

Path("crates/noon-render-wgpu/tests/stroke_style_cache.rs").write_text(r'''use noon_core::{GeometryRef, ObjectDefinition, ObjectId, StrokeCap, StrokeJoin, Style, Vec2, VectorPath};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn frame(styles: &[Style]) -> FrameState {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 1.0));
    let objects = styles
        .iter()
        .enumerate()
        .map(|(index, style)| FrameObjectState::from_definition(&ObjectDefinition {
            id: ObjectId::new(index as u64),
            geometry: GeometryRef::path(path.clone()),
            transform: Default::default(),
            style: *style,
        }))
        .collect();
    FrameState {
        time: 0.0,
        objects,
        reveals: vec![1.0; styles.len()],
        morphs: vec![0.0; styles.len()],
        render_geometries: vec![None; styles.len()],
    }
}

fn style(join: StrokeJoin, cap: StrokeCap) -> Style {
    Style {
        fill: None,
        stroke: Some(noon_core::Color::WHITE),
        stroke_width: 0.2,
        stroke_join: join,
        stroke_cap: cap,
        opacity: 1.0,
    }
}

#[test]
fn path_cache_key_includes_join_and_cap_policy() {
    let mut preparer = FramePreparer::new();
    let frame = frame(&[
        style(StrokeJoin::Round, StrokeCap::Round),
        style(StrokeJoin::Miter, StrokeCap::Round),
        style(StrokeJoin::Round, StrokeCap::Butt),
        style(StrokeJoin::Round, StrokeCap::Round),
    ]);
    let prepared = preparer.prepare(&frame);
    assert_eq!(prepared.stats.geometry_cache_misses, 3);
    assert_eq!(preparer.cached_path_mesh_count(), 3);
}
''')

# Add Python serialization/validation coverage without rewriting the test module.
test_path = Path("web/python/test_noon.py")
test_text = test_path.read_text()
marker = '''    def test_path_reveal_serializes_as_normalized_scalar_track(self) -> None:\n'''
addition = '''    def test_stroke_join_and_cap_are_semantic_and_validated(self) -> None:\n        scene = Scene()\n        scene.path(\n            VectorPath().move_to((-1.0, 0.0)).line_to((1.0, 0.0)),\n            fill=None,\n            stroke=Color(1.0, 1.0, 1.0),\n            stroke_width=0.2,\n            stroke_join="bevel",\n            stroke_cap="square",\n        )\n        style = scene.to_document()["objects"][0]["style"]\n        self.assertEqual(style["stroke_join"], "bevel")\n        self.assertEqual(style["stroke_cap"], "square")\n\n        with self.assertRaises(ValueError):\n            Scene().path(VectorPath().move_to((0.0, 0.0)).line_to((1.0, 0.0)), stroke_join="sharp")\n        with self.assertRaises(ValueError):\n            Scene().path(VectorPath().move_to((0.0, 0.0)).line_to((1.0, 0.0)), stroke_cap="triangle")\n\n'''
if addition not in test_text:
    if marker not in test_text:
        raise SystemExit("Python stroke style test insertion marker not found")
    test_text = test_text.replace(marker, addition + marker, 1)
test_path.write_text(test_text)

# Compiler regression: geometry-changing paths must keep topology policy stable.
compiler_test = Path("crates/noon-compile/tests/generic_transform.rs")
compiler_text = compiler_test.read_text()
append = r'''

#[test]
fn path_transform_rejects_join_or_cap_topology_changes() {
    let source_path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    let target_path = VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0));
    for change_join in [true, false] {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(source_path.clone()));
        let mut from = ObjectSnapshot::from(scene.object(object).unwrap());
        from.style.fill = None;
        from.style.stroke = Some(Color::WHITE);
        from.style.stroke_width = 0.1;
        let mut to = from.clone();
        to.geometry = GeometryRef::path(target_path.clone());
        if change_join {
            to.style.stroke_join = noon_core::StrokeJoin::Bevel;
        } else {
            to.style.stroke_cap = noon_core::StrokeCap::Butt;
        }
        scene
            .animate_transform(object, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
            .unwrap();
        assert!(matches!(
            CompiledScene::compile(&scene),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }
}
'''
if "fn path_transform_rejects_join_or_cap_topology_changes" not in compiler_text:
    compiler_text += append
compiler_test.write_text(compiler_text)
