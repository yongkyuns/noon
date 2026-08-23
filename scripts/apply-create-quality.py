from pathlib import Path
import re


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


def regex_once(text, pattern, replacement, label):
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"{label}: expected one regex match, found {count}")
    return updated


# Geometry: improve generic curve quality and cache a cheap centerline measure for reveal heads.
path = Path("crates/noon-geometry/src/tessellation.rs")
text = path.read_text()
text = replace_once(
    text,
    "const PATH_TESSELLATION_TOLERANCE: f32 = 0.01;",
    "const PATH_TESSELLATION_TOLERANCE: f32 = 0.002;",
    "path tolerance",
)
text = replace_once(
    text,
    "    /// True when vertices contain distinct source/target morph endpoints.\n    pub morphing: bool,\n}",
    "    /// True when vertices contain distinct source/target morph endpoints.\n    pub morphing: bool,\n    // Cached centerline measure used to place a procedural Create reveal head.\n    // It is built only when geometry is tessellated, never per animation frame.\n    reveal_points: Vec<RevealPoint>,\n}",
    "reveal points field",
)
text = replace_once(
    text,
    "    pub fn revealed_stroke_length(&self, reveal: f32) -> f32 {\n        if !reveal.is_finite() {\n            return 0.0;\n        }\n        self.stroke_length * reveal.clamp(0.0, 1.0)\n    }\n}",
    "    pub fn revealed_stroke_length(&self, reveal: f32) -> f32 {\n        if !reveal.is_finite() {\n            return 0.0;\n        }\n        self.stroke_length * reveal.clamp(0.0, 1.0)\n    }\n\n    /// Returns the local-space centerline position for normalized path progress.\n    ///\n    /// The lookup is O(log N) over a centerline measure cached with the mesh, so\n    /// repeated Create frames do not flatten or tessellate the path again.\n    pub fn reveal_head_position(&self, reveal: f32) -> Option<Vec2> {\n        let first = *self.reveal_points.first()?;\n        let last = *self.reveal_points.last()?;\n        let reveal = if reveal.is_finite() {\n            reveal.clamp(0.0, 1.0)\n        } else {\n            0.0\n        };\n        if last.distance <= 0.0 {\n            return Some(first.position);\n        }\n        let target = last.distance * reveal;\n        let upper = self\n            .reveal_points\n            .partition_point(|point| point.distance < target);\n        if upper == 0 {\n            return Some(first.position);\n        }\n        if upper >= self.reveal_points.len() {\n            return Some(last.position);\n        }\n        let left = self.reveal_points[upper - 1];\n        let right = self.reveal_points[upper];\n        let span = right.distance - left.distance;\n        if span <= f32::EPSILON {\n            return Some(right.position);\n        }\n        let t = ((target - left.distance) / span).clamp(0.0, 1.0);\n        Some(Vec2::new(\n            left.position.x + (right.position.x - left.position.x) * t,\n            left.position.y + (right.position.y - left.position.y) * t,\n        ))\n    }\n}",
    "reveal head lookup",
)
text = replace_once(
    text,
    "#[derive(Clone, Copy, Debug, PartialEq)]\nstruct TessellationVertex {",
    "#[derive(Clone, Copy, Debug, PartialEq)]\nstruct RevealPoint {\n    distance: f32,\n    position: Vec2,\n}\n\n#[derive(Clone, Copy, Debug, PartialEq)]\nstruct TessellationVertex {",
    "RevealPoint type",
)
text = replace_once(
    text,
    "    let path = build_lyon_path(path)?;\n    let mut buffers = VertexBuffers::new();",
    "    let reveal_points = build_reveal_points(path)?;\n    let path = build_lyon_path(path)?;\n    let mut buffers = VertexBuffers::new();",
    "static reveal measure",
)
text = replace_once(
    text,
    "        stroke_length,\n        morphing: false,\n    })",
    "        stroke_length,\n        morphing: false,\n        reveal_points,\n    })",
    "static mesh reveal points",
)
text = replace_once(
    text,
    "    let mut vertices = Vec::new();\n    let mut indices = Vec::new();",
    "    let mut vertices = Vec::new();\n    let mut indices = Vec::new();\n    let reveal_points = build_reveal_points(source)?;",
    "morph reveal measure",
)
text = text.replace(
    "            morphing: true,\n        });",
    "            morphing: true,\n            reveal_points,\n        });",
    1,
)
text = text.replace(
    "        morphing: true,\n    })",
    "        morphing: true,\n        reveal_points,\n    })",
    1,
)
measure_helpers = r'''
fn build_reveal_points(path: &VectorPath) -> Result<Vec<RevealPoint>, GeometryError> {
    let mut points = Vec::new();
    let mut current = None;
    let mut contour_start = None;
    let mut distance = 0.0_f32;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                ensure_finite_point(to)?;
                points.push(RevealPoint {
                    distance,
                    position: to,
                });
                current = Some(to);
                contour_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                ensure_finite_point(to)?;
                let from = current.ok_or(GeometryError::DrawingBeforeMove)?;
                append_reveal_segment(&mut points, &mut distance, from, to);
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                ensure_finite_point(control)?;
                ensure_finite_point(to)?;
                let from = current.ok_or(GeometryError::DrawingBeforeMove)?;
                flatten_quadratic_reveal(
                    &mut points,
                    &mut distance,
                    from,
                    control,
                    to,
                    0,
                );
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
                    &mut distance,
                    from,
                    control1,
                    control2,
                    to,
                    0,
                );
                current = Some(to);
            }
            PathCommand::Close => {
                let from = current.ok_or(GeometryError::CloseBeforeMove)?;
                let to = contour_start.ok_or(GeometryError::CloseBeforeMove)?;
                append_reveal_segment(&mut points, &mut distance, from, to);
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
    distance: &mut f32,
    from: Vec2,
    to: Vec2,
) {
    if points.is_empty() {
        points.push(RevealPoint {
            distance: *distance,
            position: from,
        });
    }
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length > 0.0 {
        *distance += length;
        points.push(RevealPoint {
            distance: *distance,
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
    distance: &mut f32,
    start: Vec2,
    control: Vec2,
    end: Vec2,
    depth: u8,
) {
    if depth >= 16 || point_line_distance(control, start, end) <= PATH_TESSELLATION_TOLERANCE {
        append_reveal_segment(points, distance, start, end);
        return;
    }
    let start_control = midpoint(start, control);
    let control_end = midpoint(control, end);
    let center = midpoint(start_control, control_end);
    flatten_quadratic_reveal(
        points,
        distance,
        start,
        start_control,
        center,
        depth + 1,
    );
    flatten_quadratic_reveal(
        points,
        distance,
        center,
        control_end,
        end,
        depth + 1,
    );
}

fn flatten_cubic_reveal(
    points: &mut Vec<RevealPoint>,
    distance: &mut f32,
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
    depth: u8,
) {
    let flatness = point_line_distance(control1, start, end)
        .max(point_line_distance(control2, start, end));
    if depth >= 16 || flatness <= PATH_TESSELLATION_TOLERANCE {
        append_reveal_segment(points, distance, start, end);
        return;
    }
    let a = midpoint(start, control1);
    let b = midpoint(control1, control2);
    let c = midpoint(control2, end);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let center = midpoint(d, e);
    flatten_cubic_reveal(points, distance, start, a, d, center, depth + 1);
    flatten_cubic_reveal(points, distance, center, e, c, end, depth + 1);
}

'''
text = replace_once(text, "fn lyon_line_join(join: StrokeJoin) -> LineJoin {", measure_helpers + "fn lyon_line_join(join: StrokeJoin) -> LineJoin {", "measure helpers")
path.write_text(text)


# Renderer: Circle Create remains analytic; path Create gets a cached procedural round head.
path = Path("crates/noon-render-wgpu/src/reveal.rs")
text = path.read_text()
text = text.replace("    Circle(u32),\n", "")
text = replace_once(
    text,
    "        GeometryRef::Circle { radius } => Some(AnalyticRevealKey::Circle(radius.to_bits())),\n        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(",
    "        GeometryRef::Circle { .. } => None,\n        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(",
    "circle analytic reveal key",
)
path.write_text(text)

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()
text = replace_once(
    text,
    "    Color, GeometryRef, ObjectId, PathCommand, StrokeCap, StrokeJoin, Style, Transform2D,\n    VectorPath,",
    "    Color, GeometryRef, ObjectId, PathCommand, StrokeCap, StrokeJoin, Style, Transform2D, Vec2,\n    VectorPath,",
    "Vec2 import",
)
text = replace_once(
    text,
    "        analytic_reveal: Option<AnalyticRevealKey>,\n    },",
    "        analytic_reveal: Option<AnalyticRevealKey>,\n        reveal_head: Option<usize>,\n    },",
    "path slot reveal head",
)
text = text.replace("let packed = pack_circle(object);", "let packed = pack_circle(object, frame.reveal(object_index));")
text = replace_once(
    text,
    "                PreparedSlot::Path { index, .. } => {\n                    let packed = pack_path(\n                        object,\n                        frame.reveal(object_index),\n                        frame.morph(object_index),\n                    );\n                    instances_repacked += 1;\n                    if self.paths[index] != packed {\n                        self.paths[index] = packed;\n                        push_dirty_range(&mut self.path_dirty_ranges, index);\n                    }\n                }",
    "                PreparedSlot::Path {\n                    index,\n                    batch,\n                    reveal_head,\n                    ..\n                } => {\n                    let reveal = frame.reveal(object_index);\n                    let packed = pack_path(object, reveal, frame.morph(object_index));\n                    instances_repacked += 1;\n                    if self.paths[index] != packed {\n                        self.paths[index] = packed;\n                        push_dirty_range(&mut self.path_dirty_ranges, index);\n                    }\n                    if let Some(head_index) = reveal_head {\n                        let cache_index = self.path_batch_cache_indices[batch];\n                        let packed_head = pack_path_reveal_head(\n                            object,\n                            &self.path_mesh_cache[cache_index].mesh,\n                            reveal,\n                        );\n                        instances_repacked += 1;\n                        if self.lines[head_index] != packed_head {\n                            self.lines[head_index] = packed_head;\n                            push_dirty_range(&mut self.line_dirty_ranges, head_index);\n                        }\n                    }\n                }",
    "incremental path reveal head",
)
text = replace_once(
    text,
    "                let index = path_groups[batch].instances.len();\n                path_groups[batch].ids.push(object.id);\n                path_groups[batch].instances.push(pack_path(\n                    object,\n                    frame.reveal(object_index),\n                    frame.morph(object_index),\n                ));\n                self.slots.push(PreparedSlot::Path {\n                    index,\n                    batch,\n                    analytic_reveal: temporary_reveal.as_ref().map(|(key, _)| *key),\n                });",
    "                let reveal = frame.reveal(object_index);\n                let index = path_groups[batch].instances.len();\n                path_groups[batch].ids.push(object.id);\n                path_groups[batch].instances.push(pack_path(\n                    object,\n                    reveal,\n                    frame.morph(object_index),\n                ));\n                let reveal_head = if should_create_path_reveal_head(object, reveal) {\n                    let head_index = self.lines.len();\n                    self.line_ids.push(object.id);\n                    self.lines.push(pack_path_reveal_head(\n                        object,\n                        &self.path_mesh_cache[cache_index].mesh,\n                        reveal,\n                    ));\n                    Some(head_index)\n                } else {\n                    None\n                };\n                self.slots.push(PreparedSlot::Path {\n                    index,\n                    batch,\n                    analytic_reveal: temporary_reveal.as_ref().map(|(key, _)| *key),\n                    reveal_head,\n                });",
    "rebuild path reveal head",
)
text = replace_once(
    text,
    "                    self.circles.push(pack_circle(object));",
    "                    self.circles\n                        .push(pack_circle(object, frame.reveal(object_index)));",
    "rebuild circle reveal",
)
text = replace_once(
    text,
    "                analytic_reveal,\n            } => {",
    "                analytic_reveal,\n                reveal_head,\n            } => {",
    "slot match reveal head binding",
)
text = replace_once(
    text,
    "                self.path_ids.get(*index) == Some(&object.id)\n                    && geometry_matches\n                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()",
    "                let reveal_head_available = reveal_head.is_some()\n                    || !should_create_path_reveal_head(object, frame.reveal(object_index));\n                self.path_ids.get(*index) == Some(&object.id)\n                    && geometry_matches\n                    && reveal_head_available\n                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()",
    "slot match reveal head availability",
)
text = replace_once(
    text,
    "fn pack_circle(object: &FrameObjectState) -> CircleInstance {",
    "fn pack_circle(object: &FrameObjectState, reveal: f32) -> CircleInstance {",
    "pack circle signature",
)
text = replace_once(
    text,
    "        radius: *radius,\n        padding: [0.0; 3],",
    "        radius: *radius,\n        padding: [reveal.clamp(0.0, 1.0), 0.0, 0.0],",
    "circle reveal packing",
)
head_helpers = r'''
fn should_create_path_reveal_head(object: &FrameObjectState, reveal: f32) -> bool {
    reveal < 1.0
        && object.style.stroke_cap == StrokeCap::Round
        && object.style.stroke_width > 0.0
        && (object.style.stroke.is_some() || object.style.fill.is_some())
}

fn pack_path_reveal_head(
    object: &FrameObjectState,
    mesh: &TessellatedPath,
    reveal: f32,
) -> LineInstance {
    let reveal = reveal.clamp(0.0, 1.0);
    let point = mesh.reveal_head_position(reveal).unwrap_or(Vec2::ZERO);
    let mut transform: PackedTransform = object.transform.into();
    transform.padding = 1.0;
    let mut style = pack_style(object);
    style.fill = [0.0; 4];
    style.fill_enabled = 0;
    if let Some(color) = object.style.stroke.or(object.style.fill) {
        style.stroke = [color.red, color.green, color.blue, color.alpha];
        style.stroke_enabled = 1;
    } else {
        style.stroke = [0.0; 4];
        style.stroke_enabled = 0;
    }
    let active = reveal > 0.0 && reveal < 1.0;
    style.opacity *= f32::from(active);
    if object.style.stroke.is_none() {
        style.opacity *= 1.0 - smoothstep(0.75, 1.0, reveal);
    }
    LineInstance {
        transform,
        style,
        start: [point.x, point.y],
        end: [point.x, point.y],
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return f32::from(value >= edge1);
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

'''
text = replace_once(text, "fn pack_path(object: &FrameObjectState, reveal: f32, morph: f32) -> PathInstance {", head_helpers + "fn pack_path(object: &FrameObjectState, reveal: f32, morph: f32) -> PathInstance {", "path head helpers")

# Update renderer invariants for analytic Circle Create and cached path heads.
text = regex_once(
    text,
    r"    #\[test\]\n    fn path_reveal_changes_only_dirty_the_instance_record\(\) \{.*?\n    \}\n\n(?=    #\[test\]\n    fn analytic_reveal_uses_cached_path_until_completion_then_returns_to_fast_path)",
    '''    #[test]\n    fn path_reveal_reuses_cached_geometry_and_moves_only_instance_and_head() {\n        let mut state = object(7, GeometryRef::path(curved_path()));\n        state.style.fill = None;\n        state.style.stroke = Some(Color::WHITE);\n        state.style.stroke_width = 0.2;\n        let mut frame = frame(vec![state]);\n        frame.reveals[0] = 0.2;\n        let mut preparer = FramePreparer::new();\n        let cold = preparer.prepare(&frame);\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n        assert_eq!(cold.paths.len(), 1);\n        assert_eq!(cold.lines.len(), 1);\n        assert_eq!(cold.lines[0].start, cold.lines[0].end);\n        let head_before = cold.lines[0].start;\n\n        frame.reveals[0] = 0.35;\n        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));\n\n        assert_eq!(prepared.stats.geometry_cache_misses, 0);\n        assert_eq!(prepared.stats.instances_repacked, 2);\n        assert_eq!(prepared.stats.dirty_instance_count, 2);\n        assert!(!prepared.path_geometry_dirty);\n        assert_eq!(prepared.path_dirty_ranges, &[0..1]);\n        assert_eq!(prepared.line_dirty_ranges, &[0..1]);\n        assert_eq!(prepared.paths[0].path_params[0], 0.35);\n        assert_ne!(prepared.lines[0].start, head_before);\n        assert_eq!(prepared.lines[0].start, prepared.lines[0].end);\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n    }\n\n''',
    "path reveal test",
)
text = regex_once(
    text,
    r"    #\[test\]\n    fn analytic_reveal_uses_cached_path_until_completion_then_returns_to_fast_path\(\) \{.*?\n    \}\n\n(?=    #\[test\]\n    fn closed_analytic_create_uses_paths_while_line_reveal_stays_analytic)",
    '''    #[test]\n    fn circle_create_stays_on_the_analytic_fast_path() {\n        let mut state = object(7, GeometryRef::circle(1.25));\n        state.style.fill = Some(Color::WHITE);\n        state.style.stroke = Some(Color::BLACK);\n        state.style.stroke_width = 0.08;\n        let mut frame = frame(vec![state]);\n        frame.reveals[0] = 0.25;\n        let mut preparer = FramePreparer::new();\n\n        let cold = preparer.prepare(&frame);\n        assert_eq!(cold.circles.len(), 1);\n        assert!(cold.paths.is_empty());\n        assert_eq!(cold.circles[0].padding[0], 0.25);\n        assert_eq!(cold.stats.geometry_cache_misses, 0);\n\n        frame.reveals[0] = 0.6;\n        let steady = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));\n        assert_eq!(steady.circles.len(), 1);\n        assert!(steady.paths.is_empty());\n        assert_eq!(steady.circles[0].padding[0], 0.6);\n        assert_eq!(steady.stats.geometry_cache_misses, 0);\n        assert_eq!(steady.stats.instances_repacked, 1);\n        assert!(!steady.path_geometry_dirty);\n\n        frame.reveals[0] = 1.0;\n        let complete = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));\n        assert_eq!(complete.circles.len(), 1);\n        assert!(complete.paths.is_empty());\n        assert_eq!(complete.circles[0].padding[0], 1.0);\n        assert_eq!(complete.stats.instance_count, 1);\n    }\n\n''',
    "circle analytic test",
)
text = regex_once(
    text,
    r"    #\[test\]\n    fn closed_analytic_create_uses_paths_while_line_reveal_stays_analytic\(\) \{.*?\n    \}\n\n(?=    #\[test\]\n    fn path_morph_changes_only_dirty_the_instance_record)",
    '''    #[test]\n    fn circle_and_line_create_stay_analytic_while_rectangle_uses_a_path() {\n        let mut circle = object(1, GeometryRef::circle(1.0));\n        let mut rectangle = object(2, GeometryRef::rectangle(2.0, 1.0));\n        let mut line = object(\n            3,\n            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),\n        );\n        for state in [&mut circle, &mut rectangle, &mut line] {\n            state.style.fill = None;\n            state.style.stroke = Some(Color::WHITE);\n            state.style.stroke_width = 0.05;\n        }\n        let mut frame = frame(vec![circle, rectangle, line]);\n        frame.reveals.fill(0.5);\n        let mut preparer = FramePreparer::new();\n\n        let prepared = preparer.prepare(&frame);\n        assert_eq!(prepared.circles.len(), 1);\n        assert_eq!(prepared.circles[0].padding[0], 0.5);\n        assert!(prepared.rectangles.is_empty());\n        assert_eq!(prepared.lines.len(), 2);\n        assert_eq!(prepared.lines[1].transform.padding, 0.5);\n        assert_eq!(prepared.paths.len(), 1);\n        assert_eq!(prepared.stats.instance_count, 4);\n        assert_eq!(prepared.stats.unsupported_count, 0);\n        assert_eq!(prepared.stats.geometry_cache_misses, 1);\n\n        frame.reveals[2] = 0.8;\n        let advanced = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2]));\n        assert_eq!(advanced.lines.len(), 2);\n        assert_eq!(advanced.lines[1].transform.padding, 0.8);\n        assert_eq!(advanced.stats.geometry_cache_misses, 0);\n        assert_eq!(advanced.stats.instances_repacked, 1);\n        assert_eq!(advanced.line_dirty_ranges, &[1..2]);\n        assert!(!advanced.path_geometry_dirty);\n    }\n\n''',
    "mixed create fast path test",
)
performance_test = '''\n    #[test]\n    fn two_thousand_revealed_paths_share_one_mesh_without_per_frame_tessellation() {\n        const OBJECT_COUNT: usize = 2_000;\n        let geometry = GeometryRef::path(\n            VectorPath::new()\n                .move_to(Vec2::new(-2.4, -1.0))\n                .cubic_to(\n                    Vec2::new(-1.2, -2.0),\n                    Vec2::new(1.2, 0.0),\n                    Vec2::new(2.4, -1.0),\n                ),\n        );\n        let objects = (0..OBJECT_COUNT)\n            .map(|index| {\n                let mut state = object(index as u64, geometry.clone());\n                state.style.fill = None;\n                state.style.stroke = Some(Color::WHITE);\n                state.style.stroke_width = 0.05;\n                state\n            })\n            .collect();\n        let mut frame = frame(objects);\n        frame.reveals.fill(0.25);\n        let mut preparer = FramePreparer::new();\n\n        let cold = preparer.prepare(&frame);\n        assert_eq!(cold.stats.geometry_cache_misses, 1);\n        assert_eq!(cold.paths.len(), OBJECT_COUNT);\n        assert_eq!(cold.lines.len(), OBJECT_COUNT);\n        assert_eq!(cold.path_batches.len(), 1);\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n\n        frame.reveals.fill(0.65);\n        let changes = FrameChanges::objects((0..OBJECT_COUNT).collect());\n        let steady = preparer.prepare_incremental(&frame, &changes);\n        assert_eq!(steady.stats.geometry_cache_misses, 0);\n        assert_eq!(steady.stats.instances_repacked, OBJECT_COUNT * 2);\n        assert!(!steady.path_geometry_dirty);\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n    }\n'''
text = replace_once(text, "    #[test]\n    fn one_hundred_thousand_circles_form_one_batch() {", performance_test + "\n    #[test]\n    fn one_hundred_thousand_circles_form_one_batch() {", "path performance regression")
path.write_text(text)


# Analytic circle reveal: exact SDF circle with angular progress and round moving cap.
path = Path("crates/noon-render-wgpu/src/analytic.wgsl")
text = path.read_text()
text = replace_once(
    text,
    "    let stroke_padding = stroke_half_width(input.metrics, input.flags);\n    let local = input.unit * (vec2<f32>(radius + stroke_padding) + padding);",
    "    let stroke_padding = stroke_half_width(input.metrics, input.flags);\n    let reveal = clamp(input.geometry.y, 0.0, 1.0);\n    let derive_creation_stroke = reveal < 1.0 && input.flags.x != 0u && input.flags.y == 0u;\n    let creation_padding = select(\n        stroke_padding,\n        max(input.metrics.x, 0.0) * 0.5,\n        derive_creation_stroke,\n    );\n    let local = input.unit * (vec2<f32>(radius + creation_padding) + padding);",
    "circle proxy create padding",
)
circle_fragment = r'''@fragment
fn fs_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    let radius = max(abs(input.geometry.x), 0.000001);
    let reveal = clamp(input.geometry.y, 0.0, 1.0);
    let signed_distance = length(input.local) - radius;
    let stroke_width = max(input.metrics.x, 0.0);
    let fill_enabled = input.flags.x > 0.5;
    let stroke_enabled = input.flags.y > 0.5;

    if reveal >= 1.0 {
        return styled_shape_color(
            input.fill,
            input.stroke,
            input.metrics.y,
            fill_enabled,
            stroke_enabled,
            signed_distance,
            stroke_width,
        );
    }
    if reveal <= 0.0 {
        return vec4<f32>(0.0);
    }

    // Circle Create remains entirely analytic. The SDF gives an exact circle;
    // angular progress reveals its outline and an analytic disk supplies the
    // moving round head, so there is no faceted temporary mesh or endpoint pop.
    let tau = 6.283185307179586;
    var angle = atan2(input.local.y, input.local.x);
    if angle < 0.0 {
        angle += tau;
    }
    let progress = angle / tau;
    let progress_edge = max(fwidth(progress), 0.00001);
    let body_reveal = 1.0 - smoothstep(reveal, reveal + progress_edge, progress);
    let half_stroke_width = stroke_width * 0.5;
    let outer_coverage = inside_coverage(signed_distance - half_stroke_width);
    let inner_coverage = outside_coverage(signed_distance + half_stroke_width);
    let ring_coverage = outer_coverage * inner_coverage;

    let head_angle = reveal * tau;
    let head_center = radius * vec2<f32>(cos(head_angle), sin(head_angle));
    let start_center = vec2<f32>(radius, 0.0);
    let head_cap = inside_coverage(length(input.local - head_center) - half_stroke_width);
    let start_cap = inside_coverage(length(input.local - start_center) - half_stroke_width);
    let has_creation_stroke = stroke_width > 0.0 && (stroke_enabled || fill_enabled);
    let stroke_coverage = select(
        0.0,
        max(ring_coverage * body_reveal, max(head_cap, start_cap)),
        has_creation_stroke,
    );

    let fill_alpha = smoothstep(0.0, 1.0, reveal);
    let fill_layer = select(
        vec4<f32>(0.0),
        covered_color(input.fill, input.metrics.y * fill_alpha, inside_coverage(signed_distance)),
        fill_enabled,
    );
    let derive_creation_stroke = fill_enabled && !stroke_enabled;
    let creation_outline_alpha = select(
        1.0,
        1.0 - smoothstep(0.75, 1.0, reveal),
        derive_creation_stroke,
    );
    let stroke_color = select(input.stroke, input.fill, derive_creation_stroke);
    let stroke_layer = covered_color(
        stroke_color,
        input.metrics.y * creation_outline_alpha,
        stroke_coverage,
    );
    return stroke_layer + fill_layer * (1.0 - stroke_layer.a);
}
'''
text = regex_once(text, r"@fragment\nfn fs_circle\(input: VertexOutput\) -> @location\(0\) vec4<f32> \{.*?\n\}\n(?=\n@fragment\nfn fs_rectangle)", circle_fragment, "analytic circle fragment")
path.write_text(text)


# Browser regression: specifically guard the wave's final round cap.
path = Path("scripts/browser-smoke.mjs")
text = path.read_text()
needle = '''      console.log(\n        `✓ ${example.name}: line endpoint continuous near completion (${lineEndpointDiff} changed pixels)`,\n      );\n'''
wave_check = needle + '''\n      const waveBeforePath = path.join(\n        artifactDir,\n        artifactName(index, example.name, "wave-end-before"),\n      );\n      const waveEndPath = path.join(\n        artifactDir,\n        artifactName(index, example.name, "wave-end-final"),\n      );\n      const waveBefore = await renderAndCapture(page, latestEnd - 0.001, waveBeforePath);\n      const waveEnd = await renderAndCapture(page, latestEnd, waveEndPath);\n      const waveEndpointDiff = differingPixelCount(\n        waveBefore.screenshot,\n        waveEnd.screenshot,\n        { minX: 0.58, maxX: 0.84, minY: 0.55, maxY: 0.96 },\n      );\n      assert.ok(\n        waveEndpointDiff <= 20,\n        `${example.name}: wave endpoint jumped across ${waveEndpointDiff} pixels at completion`,\n      );\n      console.log(\n        `✓ ${example.name}: wave endpoint continuous at completion (${waveEndpointDiff} changed pixels)`,\n      );\n'''
text = replace_once(text, needle, wave_check, "wave browser regression")
path.write_text(text)
