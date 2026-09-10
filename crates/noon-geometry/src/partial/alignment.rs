//! Shared shape-preserving subdivision for corresponding path contours.
use super::*;

/// Match contour and curve counts without changing either path's visible shape.
/// Missing contours become null curves at the last endpoint. Unfinished anchors
/// become null curves; an entirely empty path uses the origin. Explicit retained
/// contour breaks and each operand's closed joins remain intact.
///
/// The result is temporary preparation data. Persistent callers publish both
/// replacements in one transaction; transform preparation can consume it directly.
pub fn align_paths(
    left: &VectorPath,
    right: &VectorPath,
) -> Result<(VectorPath, VectorPath), PathProportionError> {
    if !left.is_finite() || !right.is_finite() {
        return Err(PathProportionError::InvalidMetric);
    }
    if left.commands().is_empty() && right.commands().is_empty() {
        return Ok((left.clone(), right.clone()));
    }
    let mut left_contours = contours(left);
    let mut right_contours = contours(right);
    let point_count = |path: &VectorPath| {
        collect_curves(path).len() * 4
            + usize::from(matches!(
                path.commands().last(),
                Some(PathCommand::MoveTo { .. })
            ))
    };
    if point_count(left) == point_count(right)
        && left_contours.len() == right_contours.len()
        && left_contours
            .iter()
            .zip(&right_contours)
            .all(|(a, b)| a.len() == b.len())
    {
        return Ok((left.clone(), right.clone()));
    }
    let count = left_contours.len().max(right_contours.len());
    pad_contours(&mut left_contours, count);
    pad_contours(&mut right_contours, count);
    let mut results = [VectorPath::new(), VectorPath::new()];
    for (a, b) in left_contours.iter_mut().zip(&mut right_contours) {
        trim_null_tail(a);
        trim_null_tail(b);
        let count = a.len().max(b.len());
        for (slot, curves) in results.iter_mut().zip([a, b]) {
            let mut path = VectorPath::new().move_to(curves[0].from);
            for curve in curves.iter() {
                path = append_curve(path, *curve);
            }
            if curves.last().is_some_and(|c| c.closes_contour) {
                path = path.close();
            }
            let path = subdivide_path(&path, count - curves.len())?;
            let mut combined = std::mem::take(slot).move_to(curves[0].from);
            for curve in collect_curves(&path) {
                combined = append_curve(combined, curve);
                if curve.closes_contour {
                    combined = combined.close();
                }
            }
            *slot = combined;
        }
    }
    let [left, right] = results;
    Ok((left, right))
}

fn null_curve(point: Vec2, subpath: usize) -> Curve {
    Curve {
        from: point,
        to: point,
        kind: CurveKind::Line,
        subpath,
        closes_contour: false,
    }
}
fn contours(path: &VectorPath) -> Vec<Vec<Curve>> {
    let mut contours: Vec<Vec<Curve>> = Vec::new();
    for curve in collect_curves(path) {
        if contours
            .last()
            .is_none_or(|c| c[0].subpath != curve.subpath)
        {
            contours.push(Vec::new());
        }
        contours.last_mut().unwrap().push(curve);
    }
    if let Some(PathCommand::MoveTo { to }) = path.commands().last() {
        // An unfinished path is semantically a new contour, even if coincident.
        contours.push(vec![null_curve(*to, usize::MAX)]);
    }
    if contours.is_empty() {
        contours.push(vec![null_curve(
            path.endpoints().map_or(Vec2::ZERO, |(_, p)| p),
            0,
        )]);
    }
    contours
}
fn pad_contours(contours: &mut Vec<Vec<Curve>>, count: usize) {
    let endpoint = contours.last().unwrap().last().unwrap().to;
    while contours.len() < count {
        contours.push(vec![null_curve(endpoint, contours.len())]);
    }
}
fn trim_null_tail(curves: &mut Vec<Curve>) {
    while curves.len() > 1 {
        let last = *curves.last().unwrap();
        let point = curves[curves.len() - 2].to;
        let near = |p: Vec2| (p - point).length() <= 1e-6;
        let null = near(last.from)
            && near(last.to)
            && match last.kind {
                CurveKind::Line | CurveKind::Close => true,
                CurveKind::Quadratic { control } => near(control),
                CurveKind::Cubic { control1, control2 } => near(control1) && near(control2),
            };
        if !null {
            break;
        }
        curves.pop();
        if last.closes_contour {
            curves.last_mut().unwrap().closes_contour = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lines_and_cubics_align_without_changing_trace() {
        let line = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3., 0.));
        let curve = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(1., 2.),
            Vec2::new(2., 2.),
            Vec2::new(3., 0.),
        );
        let split = subdivide_path(&curve, 3).unwrap();
        let (a, b) = align_paths(&line, &split).unwrap();
        assert_eq!(collect_curves(&a).len(), 4);
        assert_eq!(collect_curves(&b).len(), 4);
        assert!(collect_curves(&a)
            .iter()
            .all(|c| c.from.y == 0. && c.to.y == 0.));
        assert_eq!(b, split);
        assert_eq!(a.endpoints(), line.endpoints());
    }
    #[test]
    fn missing_contours_and_empty_paths_get_null_curves_at_endpoint() {
        let a = VectorPath::new()
            .move_to(Vec2::new(2., 1.))
            .line_to(Vec2::new(4., 1.));
        let b = a
            .clone()
            .move_to(Vec2::new(-1., 0.))
            .line_to(Vec2::new(-2., 1.));
        let (aligned, other) = align_paths(&a, &b).unwrap();
        assert_eq!(contours(&aligned).len(), 2);
        assert_eq!(contours(&other).len(), 2);
        assert_eq!(contours(&aligned)[1][0].from, Vec2::new(4., 1.));
        let (single, _) = align_paths(&VectorPath::new(), &a).unwrap();
        assert_eq!(collect_curves(&single).len(), 1);
        let (empty, _) = align_paths(&VectorPath::new(), &b).unwrap();
        assert!(collect_curves(&empty)
            .iter()
            .all(|c| c.from == Vec2::ZERO && c.to == Vec2::ZERO));
    }
    #[test]
    fn closures_and_explicit_coincident_breaks_survive() {
        let square =
            crate::canonical_outline_path(&noon_core::GeometryRef::rectangle(2., 2.)).unwrap();
        let split = subdivide_path(&square, 3).unwrap();
        let (a, b) = align_paths(&square, &split).unwrap();
        assert_eq!(a.commands().last(), Some(&PathCommand::Close));
        assert_eq!(b.commands().last(), Some(&PathCommand::Close));
        let p = Vec2::new(2., 0.);
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(p)
            .move_to(p)
            .line_to(Vec2::new(3., 0.));
        assert_eq!(contours(&align_paths(&path, &square).unwrap().0).len(), 2);
    }
}
