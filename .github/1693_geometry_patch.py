from pathlib import Path
import hashlib


def replace_once(source, old, new):
    assert source.count(old) == 1, (old[:100], source.count(old))
    return source.replace(old, new, 1)


p = Path('crates/noon-geometry/src/smoothing.rs')
s = p.read_text()
start = s.index('fn manim_signed_is_closed(')
end = s.index('/// Solve the natural/open', start)
s = s[:start] + '''fn manim_signed_is_closed(anchors: &[Vec2]) -> bool {
    let (Some(start), Some(end)) = (anchors.first(), anchors.last()) else {
        return false;
    };
    manim_signed_endpoint_closure(
        [f64::from(start.x), f64::from(start.y)],
        [f64::from(end.x), f64::from(end.y)],
    )
}

fn manim_signed_endpoint_closure(start: [f64; 2], end: [f64; 2]) -> bool {
    (0..2).all(|axis| (end[axis] - start[axis]).abs() <= 1.0e-8 + 1.0e-5 * start[axis])
}

/// Smooth a coordinate-space polyline without narrowing its anchors or handles.
///
/// Closed contours repeat their first anchor. Boundary selection is performed in
/// this source space, before any affine mapping into renderable scene units.
/// The ordinary retained-path smoother uses the same f64 spline solver below.
pub fn smooth_curve_handles(
    anchors: &[[f64; 2]],
    boundary: SplineBoundary,
) -> Result<Vec<[[f64; 2]; 2]>, PathProportionError> {
    if anchors.iter().flatten().any(|value| !value.is_finite()) {
        return Err(PathProportionError::InvalidMetric);
    }
    let (Some(first), Some(last)) = (anchors.first(), anchors.last()) else {
        return Ok(Vec::new());
    };
    if anchors.len() < 2 {
        return Ok(Vec::new());
    }
    let closed = match boundary {
        SplineBoundary::ExactClosure => first == last,
        SplineBoundary::ManimSignedClosure => manim_signed_endpoint_closure(*first, *last),
    };
    let handles = smooth_handles_f64(anchors, closed);
    if handles.iter().flatten().flatten().any(|value| !value.is_finite()) {
        return Err(PathProportionError::InvalidMetric);
    }
    Ok(handles)
}

''' + s[end:]
start = s.index('fn smooth_handles(')
end = s.index('#[cfg(test)]', start)
old = s[start:end]
solver = replace_once(old,
    'fn smooth_handles(anchors: &[Vec2], closed: bool) -> Vec<[Vec2; 2]> {',
    'fn smooth_handles_f64(anchors: &[[f64; 2]], closed: bool) -> Vec<[[f64; 2]; 2]> {')
one_start = solver.index('    if count == 1 {')
one_end = solver.index('    let mut upper', one_start)
solver = solver[:one_start] + '''    if count == 1 {
        let mut handles = [[0.0; 2]; 2];
        for axis in 0..2 {
            let delta = anchors[1][axis] - anchors[0][axis];
            handles[0][axis] = anchors[0][axis] + delta / 3.0;
            handles[1][axis] = anchors[0][axis] + delta * (2.0 / 3.0);
        }
        return vec![handles];
    }
''' + solver[one_end:]
solver = replace_once(solver, 'let mut result = vec![[Vec2::ZERO; 2]; count];', 'let mut result = vec![[[0.0; 2]; 2]; count];')
solver = replace_once(solver, '''        let coordinate = |index: usize| -> f64 {
            if axis == 0 {
                f64::from(anchors[index].x)
            } else {
                f64::from(anchors[index].y)
            }
        };''', '''        let coordinate = |index: usize| anchors[index][axis];''')
solver = replace_once(solver, '''            if axis == 0 {
                result[index][0].x = first[index] as f32;
                result[index][1].x = second as f32;
            } else {
                result[index][0].y = first[index] as f32;
                result[index][1].y = second as f32;
            }''', '''            result[index][0][axis] = first[index];
            result[index][1][axis] = second;''')
wrapper = '''fn smooth_handles(anchors: &[Vec2], closed: bool) -> Vec<[Vec2; 2]> {
    // Preserve the existing two-anchor f32 path exactly for ordinary callers.
    if anchors.len() == 2 {
        return vec![[
            anchors[0] + (anchors[1] - anchors[0]) / 3.,
            anchors[0] + (anchors[1] - anchors[0]) * (2. / 3.),
        ]];
    }
    let anchors = anchors
        .iter()
        .map(|point| [f64::from(point.x), f64::from(point.y)])
        .collect::<Vec<_>>();
    smooth_handles_f64(&anchors, closed)
        .into_iter()
        .map(|[first, second]| {
            [
                Vec2::new(first[0] as f32, first[1] as f32),
                Vec2::new(second[0] as f32, second[1] as f32),
            ]
        })
        .collect()
}

'''
s = s[:start] + wrapper + solver + s[end:]
p.write_text(s)
p = Path('crates/noon-geometry/src/lib.rs')
s = p.read_text()
s = replace_once(s, 'change_path_anchor_mode, change_path_anchor_mode_with_boundary, SplineBoundary,',
    'change_path_anchor_mode, change_path_anchor_mode_with_boundary, smooth_curve_handles, SplineBoundary,')
p.write_text(s)

p = Path('crates/noon/src/implicit_plotting.rs')
s = p.read_text().replace('PathCommand::QuadTo', 'PathCommand::QuadraticTo')
s = replace_once(s, 'change_path_anchor_mode_with_boundary, plan_isoline, validate_isoline_request, AxesFrame,',
    'change_path_anchor_mode_with_boundary, plan_isoline, smooth_curve_handles, validate_isoline_request, AxesFrame,')
start = s.index('    // Map the planner\'s f64 coordinate points')
end = s.index('\nfn map_isoline_point(', start)
s = s[:start] + '''    // Keep both spline boundary selection and handle preparation in the
    // planner's f64 coordinate space. Map only completed anchors/handles, then
    // narrow once at the retained-path boundary. A translation/reflection must
    // not change Manim's signed source-space closure decision.
    let mut path = VectorPath::new();
    for curve in &plan.curves {
        let Some(first) = curve.first().copied() else {
            continue;
        };
        path = path.move_to(map_isoline_point(frame, first)?);
        let closed = curve.len() > 2 && curve.first() == curve.last();
        if options.use_smoothing {
            let anchors = curve.iter().map(|point| [point.x, point.y]).collect::<Vec<_>>();
            let handles = smooth_curve_handles(&anchors, SplineBoundary::ManimSignedClosure)
                .map_err(|_| PlotAuthoringError::from(PlotPreparationError::SmoothingFailed))?;
            for ([first, second], end) in handles.into_iter().zip(&curve[1..]) {
                path = path.cubic_to(
                    map_isoline_point(frame, noon_geometry::IsolinePoint::new(first[0], first[1]))?,
                    map_isoline_point(frame, noon_geometry::IsolinePoint::new(second[0], second[1]))?,
                    map_isoline_point(frame, *end)?,
                );
            }
        } else {
            let end = if closed { curve.len() - 1 } else { curve.len() };
            for point in &curve[1..end] {
                path = path.line_to(map_isoline_point(frame, *point)?);
            }
        }
        if closed {
            path = path.close();
        }
    }
    Ok(path)
}
''' + s[end:]
end = s.rfind('\n}')
s = s[:end] + '''
    #[test]
    fn axes_smoothing_boundary_is_selected_before_frame_mapping() {
        let base = AxesFrame::centered([1.0, 3.0, 1.0], [1.0, 3.0, 1.0], 4.0, 4.0).unwrap();
        let translate = |point: [f64; 2]| [point[0] + 10.0, point[1] + 10.0];
        let translated = AxesFrame::new(
            noon_geometry::NumberLineFrame::new(base.x().range(), translate(base.x().start()), translate(base.x().end())).unwrap(),
            noon_geometry::NumberLineFrame::new(base.y().range(), translate(base.y().start()), translate(base.y().end())).unwrap(),
        );
        let options = ImplicitPlotOptions {
            bounds: IsolineBounds::new(
                noon_geometry::IsolinePoint::new(1.0, 1.0),
                noon_geometry::IsolinePoint::new(3.0, 3.0),
            ),
            contour: IsolineOptions { min_depth: 2, max_quads: 64, tolerance: None },
            max_leaves: 100,
            use_smoothing: true,
        };
        let circle = |x: f64, y: f64| (x - 2.0).powi(2) + (y - 2.0).powi(2) - 0.36;
        let original = prepare_axes_implicit_path(base, &options, circle).unwrap();
        let shifted = prepare_axes_implicit_path(translated, &options, circle).unwrap();
        assert_eq!(original.commands().len(), shifted.commands().len());
        let near = |left: Vec2, right: Vec2| {
            assert!((right.x - 10.0 - left.x).abs() < 2.0e-5, "{left:?} {right:?}");
            assert!((right.y - 10.0 - left.y).abs() < 2.0e-5, "{left:?} {right:?}");
        };
        let mut cubics = 0;
        for (left, right) in original.commands().iter().zip(shifted.commands()) {
            match (*left, *right) {
                (PathCommand::MoveTo { to: a }, PathCommand::MoveTo { to: b }) => near(a, b),
                (PathCommand::CubicTo { control1: a1, control2: a2, to: a }, PathCommand::CubicTo { control1: b1, control2: b2, to: b }) => {
                    near(a1, b1);
                    near(a2, b2);
                    near(a, b);
                    cubics += 1;
                }
                (PathCommand::Close, PathCommand::Close) => {}
                _ => panic!("unexpected contour command pair: {left:?} {right:?}"),
            }
        }
        assert!(cubics > 3);
    }
''' + s[end:]
p.write_text(s)
print('Prepared shared f64 spline solver and source-space boundary regression')
