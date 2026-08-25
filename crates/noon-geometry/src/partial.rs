use noon_core::{PathCommand, Vec2, VectorPath};

#[derive(Clone, Copy, Debug)]
enum CurveKind {
    Line,
    Quadratic { control: Vec2 },
    Cubic { control1: Vec2, control2: Vec2 },
    Close,
}

#[derive(Clone, Copy, Debug)]
struct Curve {
    from: Vec2,
    to: Vec2,
    kind: CurveKind,
    subpath: usize,
}

/// Return the portion of a vector path whose global Bezier parameter lies in `[a, b]`.
///
/// ManimCE's `VMobject.pointwise_become_partial` divides `[0, 1]` uniformly by
/// Bezier curve count and then uses the local Bezier parameter inside the two
/// boundary curves. It does *not* use arc length. This function mirrors that
/// contract while preserving explicit Noon subpath breaks.
pub fn pointwise_partial_path(path: &VectorPath, a: f32, b: f32) -> VectorPath {
    let curves = collect_curves(path);
    if curves.is_empty() {
        return VectorPath::new();
    }

    let a = a.clamp(0.0, 1.0);
    let b = b.clamp(a, 1.0);
    let (lower_index, lower_t) = integer_interpolate(curves.len(), a);
    let (upper_index, upper_t) = integer_interpolate(curves.len(), b);

    if b <= a {
        let point = curve_point(curves[lower_index], lower_t);
        return VectorPath::new().move_to(point);
    }

    let mut result = VectorPath::new();
    let mut active_subpath = None;
    for index in lower_index..=upper_index {
        let curve = curves[index];
        let t0 = if index == lower_index { lower_t } else { 0.0 };
        let t1 = if index == upper_index { upper_t } else { 1.0 };
        if t1 <= t0 {
            continue;
        }

        let partial = partial_curve(curve, t0, t1);
        if active_subpath != Some(curve.subpath) {
            result = result.move_to(partial.from);
            active_subpath = Some(curve.subpath);
        }

        result = match partial.kind {
            CurveKind::Line | CurveKind::Close => result.line_to(partial.to),
            CurveKind::Quadratic { control } => result.quadratic_to(control, partial.to),
            CurveKind::Cubic { control1, control2 } => {
                result.cubic_to(control1, control2, partial.to)
            }
        };
    }
    result
}

fn integer_interpolate(curve_count: usize, alpha: f32) -> (usize, f32) {
    debug_assert!(curve_count > 0);
    if alpha >= 1.0 {
        return (curve_count - 1, 1.0);
    }
    let scaled = alpha.max(0.0) * curve_count as f32;
    let index = (scaled.floor() as usize).min(curve_count - 1);
    (index, scaled - index as f32)
}

fn collect_curves(path: &VectorPath) -> Vec<Curve> {
    let mut curves = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    let mut subpath = 0usize;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                current = Some(to);
                subpath_start = Some(to);
                if !curves.is_empty() {
                    subpath += 1;
                }
            }
            PathCommand::LineTo { to } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Line,
                        subpath,
                    });
                }
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Quadratic { control },
                        subpath,
                    });
                }
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Cubic { control1, control2 },
                        subpath,
                    });
                }
                current = Some(to);
            }
            PathCommand::Close => {
                if let (Some(from), Some(to)) = (current, subpath_start) {
                    if (from - to).length() > f32::EPSILON {
                        curves.push(Curve {
                            from,
                            to,
                            kind: CurveKind::Close,
                            subpath,
                        });
                    }
                    current = Some(to);
                }
            }
        }
    }
    curves
}

fn curve_point(curve: Curve, t: f32) -> Vec2 {
    match curve.kind {
        CurveKind::Line | CurveKind::Close => lerp(curve.from, curve.to, t),
        CurveKind::Quadratic { control } => {
            let p01 = lerp(curve.from, control, t);
            let p12 = lerp(control, curve.to, t);
            lerp(p01, p12, t)
        }
        CurveKind::Cubic { control1, control2 } => {
            let p01 = lerp(curve.from, control1, t);
            let p12 = lerp(control1, control2, t);
            let p23 = lerp(control2, curve.to, t);
            let p012 = lerp(p01, p12, t);
            let p123 = lerp(p12, p23, t);
            lerp(p012, p123, t)
        }
    }
}

fn partial_curve(curve: Curve, a: f32, b: f32) -> Curve {
    match curve.kind {
        CurveKind::Line | CurveKind::Close => Curve {
            from: lerp(curve.from, curve.to, a),
            to: lerp(curve.from, curve.to, b),
            kind: curve.kind,
            subpath: curve.subpath,
        },
        CurveKind::Quadratic { control } => {
            let points = partial_quadratic([curve.from, control, curve.to], a, b);
            Curve {
                from: points[0],
                to: points[2],
                kind: CurveKind::Quadratic { control: points[1] },
                subpath: curve.subpath,
            }
        }
        CurveKind::Cubic { control1, control2 } => {
            let points = partial_cubic([curve.from, control1, control2, curve.to], a, b);
            Curve {
                from: points[0],
                to: points[3],
                kind: CurveKind::Cubic {
                    control1: points[1],
                    control2: points[2],
                },
                subpath: curve.subpath,
            }
        }
    }
}

fn partial_quadratic(points: [Vec2; 3], a: f32, b: f32) -> [Vec2; 3] {
    let (_, right) = split_quadratic(points, a);
    if a >= 1.0 {
        return [points[2]; 3];
    }
    let local_b = ((b - a) / (1.0 - a)).clamp(0.0, 1.0);
    split_quadratic(right, local_b).0
}

fn split_quadratic(points: [Vec2; 3], t: f32) -> ([Vec2; 3], [Vec2; 3]) {
    let p01 = lerp(points[0], points[1], t);
    let p12 = lerp(points[1], points[2], t);
    let p012 = lerp(p01, p12, t);
    ([points[0], p01, p012], [p012, p12, points[2]])
}

fn partial_cubic(points: [Vec2; 4], a: f32, b: f32) -> [Vec2; 4] {
    let (_, right) = split_cubic(points, a);
    if a >= 1.0 {
        return [points[3]; 4];
    }
    let local_b = ((b - a) / (1.0 - a)).clamp(0.0, 1.0);
    split_cubic(right, local_b).0
}

fn split_cubic(points: [Vec2; 4], t: f32) -> ([Vec2; 4], [Vec2; 4]) {
    let p01 = lerp(points[0], points[1], t);
    let p12 = lerp(points[1], points[2], t);
    let p23 = lerp(points[2], points[3], t);
    let p012 = lerp(p01, p12, t);
    let p123 = lerp(p12, p23, t);
    let p0123 = lerp(p012, p123, t);
    (
        [points[0], p01, p012, p0123],
        [p0123, p123, p23, points[3]],
    )
}

fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec2(actual: Vec2, expected: Vec2) {
        assert!((actual.x - expected.x).abs() < 1e-5, "x: {actual:?} != {expected:?}");
        assert!((actual.y - expected.y).abs() < 1e-5, "y: {actual:?} != {expected:?}");
    }

    #[test]
    fn line_half_is_first_half() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        assert_eq!(partial.commands().len(), 2);
        match partial.commands()[0] {
            PathCommand::MoveTo { to } => assert_vec2(to, Vec2::new(-1.0, 0.0)),
            other => panic!("unexpected command: {other:?}"),
        }
        match partial.commands()[1] {
            PathCommand::LineTo { to } => assert_vec2(to, Vec2::ZERO),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn curve_count_not_arc_length_selects_boundary_curve() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(100.0, 0.0))
            .line_to(Vec2::new(100.0, 1.0));
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        match partial.commands().last().unwrap() {
            PathCommand::LineTo { to } => assert_vec2(*to, Vec2::new(100.0, 0.0)),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn zero_width_partial_collapses_to_one_point() {
        let path = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
        let partial = pointwise_partial_path(&path, 0.25, 0.25);
        assert_eq!(partial.commands().len(), 1);
    }

    #[test]
    fn cubic_partial_end_matches_original_curve() {
        let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::ONE,
        );
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        match partial.commands().last().unwrap() {
            PathCommand::CubicTo { to, .. } => assert_vec2(*to, Vec2::new(0.5, 0.75)),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
