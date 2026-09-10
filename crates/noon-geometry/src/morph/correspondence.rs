//! Shared curve correspondence: flatten both endpoints at the same parameters.
use super::*;

pub(super) fn plan(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<MorphPlan, MorphError> {
    let (source, target) = crate::align_paths(source, target).map_err(MorphError::PathAlignment)?;
    let source = crate::partial::cubic_contours(&source);
    let target = crate::partial::cubic_contours(&target);
    if source.len() != target.len() {
        return Err(MorphError::ContourCountMismatch {
            source: source.len(),
            target: target.len(),
        });
    }
    let mut contours = Vec::with_capacity(source.len());
    for (index, (source, target)) in source.into_iter().zip(target).enumerate() {
        if source.closed != target.closed {
            return Err(MorphError::ClosureMismatch {
                contour: index,
                source_closed: source.closed,
                target_closed: target.closed,
            });
        }
        debug_assert_eq!(source.curves.len(), target.curves.len());
        let samples = options.samples_per_contour.div_ceil(source.curves.len());
        let minimum_depth = samples.clamp(1, 1 << 16).next_power_of_two().ilog2();
        let mut result = MorphContourPlan {
            source_points: Vec::new(),
            target_points: Vec::new(),
            closed: source.closed,
        };
        let endpoints = (
            source.curves.last().unwrap()[3],
            target.curves.last().unwrap()[3],
        );
        for (a, b) in source.curves.into_iter().zip(target.curves) {
            flatten_pair(
                a,
                b,
                options.flatten_tolerance,
                minimum_depth,
                0,
                &mut result,
            );
        }
        if !result.closed {
            // flatten_pair records each interval's start. Preserve the final
            // endpoint for open contours without duplicating a closed seam.
            result.source_points.push(endpoints.0);
            result.target_points.push(endpoints.1);
        }
        contours.push(result);
    }
    Ok(MorphPlan { contours })
}

fn split(p: [Vec2; 4]) -> ([Vec2; 4], [Vec2; 4]) {
    let a = (p[0] + p[1]) * 0.5;
    let b = (p[1] + p[2]) * 0.5;
    let c = (p[2] + p[3]) * 0.5;
    let d = (a + b) * 0.5;
    let e = (b + c) * 0.5;
    let m = (d + e) * 0.5;
    ([p[0], a, d, m], [m, e, c, p[3]])
}

fn flat(p: [Vec2; 4], tolerance: f32) -> bool {
    let chord = p[3] - p[0];
    let length = chord.length();
    if length <= f32::EPSILON {
        return (p[1] - p[0]).length().max((p[2] - p[0]).length()) <= tolerance;
    }
    [p[1], p[2]].into_iter().all(|point| {
        let delta = point - p[0];
        (chord.x * delta.y - chord.y * delta.x).abs() <= tolerance * length
    })
}

fn flatten_pair(
    a: [Vec2; 4],
    b: [Vec2; 4],
    tolerance: f32,
    minimum: u32,
    depth: u32,
    output: &mut MorphContourPlan,
) {
    if depth == 16 || (depth >= minimum && flat(a, tolerance) && flat(b, tolerance)) {
        output.source_points.push(a[0]);
        output.target_points.push(b[0]);
        return;
    }
    let (a0, a1) = split(a);
    let (b0, b1) = split(b);
    flatten_pair(a0, b0, tolerance, minimum, depth + 1, output);
    flatten_pair(a1, b1, tolerance, minimum, depth + 1, output);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_stretch_keeps_each_sample_at_the_corresponding_curve_parameter() {
        let source = VectorPath::new()
            .move_to(Vec2::new(0., 0.))
            .cubic_to(Vec2::new(0., 2.), Vec2::new(2., 2.), Vec2::new(2., 0.))
            .line_to(Vec2::new(0., 0.))
            .close();
        let affine = noon_core::Transform2D {
            scale: Vec2::new(3., 0.4),
            ..Default::default()
        };
        let target = source.transformed(affine);
        let plan = plan(&source, &target, MorphOptions::DEFAULT).unwrap();
        for (&a, &b) in plan.contours[0]
            .source_points
            .iter()
            .zip(&plan.contours[0].target_points)
        {
            assert!((affine.transform_point(a) - b).length() < 2e-6);
        }
        assert!(plan.contours[0].source_points.contains(&Vec2::new(2., 0.)));
    }
    #[test]
    fn aligned_curve_counts_preserve_open_endpoints_and_contour_breaks() {
        let source = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2., 0.))
            .move_to(Vec2::new(5., 0.))
            .line_to(Vec2::new(6., 0.));
        let target = crate::subdivide_path(&source, 3).unwrap();
        let plan = plan(&source, &target, MorphOptions::DEFAULT).unwrap();
        assert_eq!(plan.contours.len(), 2);
        for contour in plan.contours {
            assert!(!contour.closed);
            for (a, b) in contour.source_points.iter().zip(contour.target_points) {
                assert!((*a - b).length() < 2e-6);
            }
        }
    }
}
