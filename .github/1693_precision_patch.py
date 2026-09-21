from pathlib import Path
import hashlib

path = Path('crates/noon/src/implicit_plotting.rs')
raw = path.read_bytes()
sha = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
assert sha == 'b045785244551ebd6db8e8ec5d7f280f7d4ad209', sha
source = raw.decode()
start = source.index('    #[test]\n    fn axes_mapping_preserves_small_span_at_large_coordinate_offset()')
assert source[start:].rstrip().endswith('}\n}'), 'unexpected test tail'
source = source[:start] + '''    fn assert_mapped_vertical_contour(x_range: [f64; 3], y_range: [f64; 3]) {
        // Use the same clamped-origin construction as real Axes. A manually
        // positioned y axis at x=0 is not coherent with a positive-only x range.
        let centered = AxesFrame::centered(x_range, y_range, 10.0, 4.0).unwrap();
        let root_x = x_range[0] + (x_range[1] - x_range[0]) * 0.375;
        let center_y = y_range[0] + (y_range[1] - y_range[0]) * 0.5;
        let expected_x = -1.25;
        let mapped_center = centered.coords_to_point(root_x, center_y).unwrap();
        assert!((mapped_center[0] - expected_x).abs() < 1.0e-6);
        assert!(mapped_center[1].abs() < 1.0e-6);

        for transformed in [false, true] {
            let project = |point: [f64; 2]| {
                if transformed {
                    // Reflection, quarter-turn, nonuniform scale and translation.
                    [3.0 - 1.5 * point[1], -2.0 - 0.75 * point[0]]
                } else {
                    point
                }
            };
            let frame = AxesFrame::new(
                noon_geometry::NumberLineFrame::new(
                    x_range,
                    project(centered.x().start()),
                    project(centered.x().end()),
                )
                .unwrap(),
                noon_geometry::NumberLineFrame::new(
                    y_range,
                    project(centered.y().start()),
                    project(centered.y().end()),
                )
                .unwrap(),
            );
            for use_smoothing in [false, true] {
                let options = ImplicitPlotOptions {
                    bounds: IsolineBounds::new(
                        noon_geometry::IsolinePoint::new(x_range[0], y_range[0]),
                        noon_geometry::IsolinePoint::new(x_range[1], y_range[1]),
                    ),
                    contour: IsolineOptions {
                        min_depth: 3,
                        max_quads: 256,
                        tolerance: None,
                    },
                    use_smoothing,
                    max_leaves: 1_000,
                };
                let path = prepare_axes_implicit_path(frame, &options, |x, y| {
                    assert!((x_range[0]..=x_range[1]).contains(&x));
                    assert!((y_range[0]..=y_range[1]).contains(&y));
                    (x - root_x) / (x_range[1] - x_range[0])
                })
                .unwrap();
                let points = path
                    .commands()
                    .iter()
                    .filter_map(|command| match command {
                        PathCommand::MoveTo { to }
                        | PathCommand::LineTo { to }
                        | PathCommand::QuadTo { to, .. }
                        | PathCommand::CubicTo { to, .. } => Some(*to),
                        PathCommand::Close => None,
                    })
                    .collect::<Vec<_>>();
                assert!(points.len() >= 2, "nonempty vertical contour: {path:?}");
                let expected = project([expected_x, 0.0]);
                let fixed = usize::from(transformed);
                let value = |point: &Vec2| {
                    if fixed == 0 {
                        f64::from(point.x)
                    } else {
                        f64::from(point.y)
                    }
                };
                assert!(
                    points.iter().all(|p| (value(p) - expected[fixed]).abs() < 0.025),
                    "mapped contour drift: range={x_range:?} transformed={transformed} smooth={use_smoothing} points={points:?}"
                );
                let varying = |point: &Vec2| {
                    if transformed { f64::from(point.x) } else { f64::from(point.y) }
                };
                let min = points.iter().map(varying).fold(f64::INFINITY, f64::min);
                let max = points.iter().map(varying).fold(f64::NEG_INFINITY, f64::max);
                let expected_min = if transformed { 0.0 } else { -2.0 };
                let expected_max = if transformed { 6.0 } else { 2.0 };
                assert!((min - expected_min).abs() < 0.025, "{min}");
                assert!((max - expected_max).abs() < 0.025, "{max}");
            }
        }
    }

    #[test]
    fn axes_mapping_preserves_small_spans_at_large_positive_and_negative_offsets() {
        for start in [1_000_000_000.0, -1_000_000_010.0] {
            assert_mapped_vertical_contour([start, start + 10.0, 1.0], [-1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn axes_mapping_preserves_tiny_domains_and_values_outside_f32_range() {
        for extent in [1.0e-60, 1.0e45] {
            assert_mapped_vertical_contour(
                [-extent, extent, extent],
                [-extent, extent, extent],
            );
        }
    }
}
'''
path.write_text(source)
print('Replaced inconsistent frame with centered-axis precision regressions')
