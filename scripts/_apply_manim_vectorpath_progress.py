from pathlib import Path

path = Path('crates/noon-geometry/src/tessellation.rs')
text = path.read_text()

def replace_once(old: str, new: str) -> None:
    global text
    if text.count(old) != 1:
        raise SystemExit(f'expected one match, got {text.count(old)} for:\n{old[:160]}')
    text = text.replace(old, new, 1)

replace_once(
'''struct RevealPoint {
    distance: f32,
    position: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TessellationVertex {
    position: Vec2,
    surface: PathSurface,
    path_distance: f32,
}
''',
'''struct RevealPoint {
    progress: f32,
    position: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TessellationVertex {
    position: Vec2,
    surface: PathSurface,
    path_distance: f32,
    path_progress: f32,
}
''')

replace_once(
'''        if last.distance <= 0.0 {
            return Some(first.position);
        }
        let target = last.distance * reveal;
        let upper = self
            .reveal_points
            .partition_point(|point| point.distance < target);
''',
'''        let upper = self
            .reveal_points
            .partition_point(|point| point.progress < reveal);
''')
replace_once(
'''        let span = right.distance - left.distance;
        if span <= f32::EPSILON {
            return Some(right.position);
        }
        let t = ((target - left.distance) / span).clamp(0.0, 1.0);
''',
'''        let span = right.progress - left.progress;
        if span <= f32::EPSILON {
            return Some(right.position);
        }
        let t = ((reveal - left.progress) / span).clamp(0.0, 1.0);
''')

replace_once(
'''    let reveal_points = build_reveal_points(path)?;
    let path = build_lyon_path(path)?;
    let mut buffers = VertexBuffers::new();
''',
'''    let reveal_points = build_reveal_points(path)?;
    let fill_path = build_lyon_path(path)?;
    let stroke_path = build_lyon_path_with_manim_progress(path)?;
    let mut buffers = VertexBuffers::new();
''')
replace_once('&path,\n                &FillOptions::default()', '&fill_path,\n                &FillOptions::default()')
replace_once(
'''                        surface: PathSurface::Fill,
                        path_distance: 0.0,
''',
'''                        surface: PathSurface::Fill,
                        path_distance: 0.0,
                        path_progress: 1.0,
''')
replace_once('&path,\n                &StrokeOptions::default()', '&stroke_path,\n                &StrokeOptions::default()')
replace_once(
'''                &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
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
''',
'''                &mut BuffersBuilder::new(&mut buffers, |mut vertex: StrokeVertex<'_, '_>| {
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
''')
replace_once(
'''            if vertex.surface == PathSurface::Stroke {
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
''',
'''            if vertex.surface == PathSurface::Stroke {
                MeshVertex {
                    position: vertex.position,
                    target_position: vertex.position,
                    surface: vertex.surface,
                    path_distance: vertex.path_distance,
                    path_progress: vertex.path_progress,
                }
''')

start = text.index('fn build_reveal_points(path: &VectorPath)')
end = text.index('\nfn ensure_finite_point', start)
text = text[:start] + '''fn build_reveal_points(path: &VectorPath) -> Result<Vec<RevealPoint>, GeometryError> {
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
                    from,
                    control1,
                    control2,
                    to,
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
''' + text[end:]

start = text.index('fn append_reveal_segment(')
end = text.index('\nfn midpoint', start)
text = text[:start] + '''fn append_reveal_segment(
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
''' + text[end:]

start = text.index('fn flatten_quadratic_reveal(')
end = text.index('\nfn lyon_line_join', start)
text = text[:start] + '''fn flatten_quadratic_reveal(
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
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
    depth: u8,
) {
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
        start,
        a,
        d,
        center,
        depth + 1,
    );
    flatten_cubic_reveal(
        points,
        mid_progress,
        end_progress,
        center,
        e,
        c,
        end,
        depth + 1,
    );
}
''' + text[end:]

marker = 'fn build_lyon_path(path: &VectorPath) -> Result<Path, GeometryError> {'
insert = r'''fn count_manim_curves(path: &VectorPath) -> Result<usize, GeometryError> {
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
            PathCommand::CubicTo { control1, control2, to } => {
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
        if curve_count == 0 { 0.0 } else { index as f32 / curve_count as f32 }
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
            PathCommand::CubicTo { control1, control2, to } => {
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

'''
if marker not in text:
    raise SystemExit('build_lyon_path marker missing')
text = text.replace(marker, insert + marker, 1)

old_test = '''    #[test]
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
'''
new_test = '''    #[test]
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
'''
replace_once(old_test, new_test)

path.write_text(text)
