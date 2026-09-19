//! Shared curve correspondence: flatten both endpoints at the same parameters.
use super::*;

pub(super) fn plan(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<MorphPlan, MorphError> {
    Ok(plan_with_authored_progress(source, target, options)?.plan)
}

/// Ordered morph samples together with their authored global curve parameters.
/// The progress rows belong to preparation, not a frame, and let retained meshes
/// preserve VMobject partial-path semantics regardless of adaptive sample counts.
pub(crate) struct AuthoredProgressMorphPlan {
    pub plan: MorphPlan,
    pub contours: Vec<AuthoredContourProgress>,
}

pub(crate) struct AuthoredContourProgress {
    pub point_progress: Vec<f32>,
    pub end_progress: f32,
}

pub(crate) fn plan_with_authored_progress(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<AuthoredProgressMorphPlan, MorphError> {
    let pairs = aligned_contours(source, target)?;
    let total_curves = pairs
        .iter()
        .map(|(source, _)| source.curves.len())
        .sum::<usize>()
        .max(1);
    let mut contours = Vec::with_capacity(pairs.len());
    let mut progress_contours = Vec::with_capacity(pairs.len());
    let mut global_curve = 0_usize;
    for (source, target) in pairs {
        let samples = options.samples_per_contour.div_ceil(source.curves.len());
        let minimum_depth = samples.clamp(1, 1 << 16).next_power_of_two().ilog2();
        let mut result = MorphContourPlan {
            source_points: Vec::new(),
            target_points: Vec::new(),
            closed: source.closed,
        };
        let mut point_progress = Vec::new();
        let endpoints = (
            source.curves.last().unwrap()[3],
            target.curves.last().unwrap()[3],
        );
        let curve_count = source.curves.len();
        for (curve, (a, b)) in source.curves.into_iter().zip(target.curves).enumerate() {
            let start_progress = (global_curve + curve) as f32 / total_curves as f32;
            let end_progress = (global_curve + curve + 1) as f32 / total_curves as f32;
            flatten_pair(
                a,
                b,
                options.flatten_tolerance,
                minimum_depth,
                0,
                &mut result,
                &mut point_progress,
                start_progress,
                end_progress,
            );
        }
        let end_progress = (global_curve + curve_count) as f32 / total_curves as f32;
        if !result.closed {
            // flatten_pair records each interval's start. Preserve the final
            // endpoint for open contours without duplicating a closed seam.
            result.source_points.push(endpoints.0);
            result.target_points.push(endpoints.1);
            point_progress.push(end_progress);
        }
        debug_assert_eq!(result.source_points.len(), point_progress.len());
        contours.push(result);
        progress_contours.push(AuthoredContourProgress {
            point_progress,
            end_progress,
        });
        global_curve += curve_count;
    }
    Ok(AuthoredProgressMorphPlan {
        plan: MorphPlan { contours },
        contours: progress_contours,
    })
}

fn aligned_contours(
    source: &VectorPath,
    target: &VectorPath,
) -> Result<Vec<(crate::partial::CubicContour, crate::partial::CubicContour)>, MorphError> {
    let (source, target) = crate::align_paths(source, target).map_err(MorphError::PathAlignment)?;
    let source = crate::partial::cubic_contours(&source);
    let target = crate::partial::cubic_contours(&target);
    if source.len() != target.len() {
        return Err(MorphError::ContourCountMismatch {
            source: source.len(),
            target: target.len(),
        });
    }
    source
        .into_iter()
        .zip(target)
        .enumerate()
        .map(|(index, (source, target))| {
            if source.closed != target.closed {
                return Err(MorphError::ClosureMismatch {
                    contour: index,
                    source_closed: source.closed,
                    target_closed: target.closed,
                });
            }
            debug_assert_eq!(source.curves.len(), target.curves.len());
            Ok((source, target))
        })
        .collect()
}

/// Prepared canonical cubic correspondence for a path whose fill topology can change.
///
/// Preparation is independent of playback time. Sampling preserves the same point,
/// contour and closure ordering used by ordinary path transforms; no endpoint fan
/// triangulation is assumed. The renderer can tessellate the current filled path
/// without mutating or retransmitting its immutable execution resource.
#[derive(Clone, Debug)]
pub struct PreparedPathInterpolation {
    contours: Vec<PreparedContourInterpolation>,
}

#[derive(Clone, Debug)]
struct PreparedContourInterpolation {
    curves: Vec<([Vec2; 4], [Vec2; 4])>,
    closed: bool,
}

impl PreparedPathInterpolation {
    pub fn new(source: &VectorPath, target: &VectorPath) -> Result<Self, MorphError> {
        if !source.is_finite() || !target.is_finite() {
            return Err(GeometryError::NonFinitePoint.into());
        }
        let contours = aligned_contours(source, target)?
            .into_iter()
            .map(|(source, target)| PreparedContourInterpolation {
                curves: source.curves.into_iter().zip(target.curves).collect(),
                closed: source.closed,
            })
            .collect();
        Ok(Self { contours })
    }

    pub fn interpolate(&self, progress: f32) -> Result<VectorPath, MorphError> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(MorphError::PathAlignment(
                crate::PathProportionError::InvalidProportion(progress),
            ));
        }
        let mut path = VectorPath::new();
        for contour in &self.contours {
            for (index, (a, b)) in contour.curves.iter().enumerate() {
                let p: [Vec2; 4] =
                    std::array::from_fn(|index| a[index] * (1.0 - progress) + b[index] * progress);
                if index == 0 {
                    path = path.move_to(p[0]);
                }
                path = path.cubic_to(p[1], p[2], p[3]);
            }
            if contour.closed {
                path = path.close();
            }
        }
        if !path.is_finite() {
            return Err(GeometryError::NonFinitePoint.into());
        }
        Ok(path)
    }
}

pub(super) fn interpolate(
    source: &VectorPath,
    target: &VectorPath,
    progress: f32,
) -> Result<VectorPath, MorphError> {
    PreparedPathInterpolation::new(source, target)?.interpolate(progress)
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
    point_progress: &mut Vec<f32>,
    start_progress: f32,
    end_progress: f32,
) {
    if depth == 16 || (depth >= minimum && flat(a, tolerance) && flat(b, tolerance)) {
        output.source_points.push(a[0]);
        output.target_points.push(b[0]);
        point_progress.push(start_progress);
        return;
    }
    let (a0, a1) = split(a);
    let (b0, b1) = split(b);
    let middle_progress = (start_progress + end_progress) * 0.5;
    flatten_pair(
        a0,
        b0,
        tolerance,
        minimum,
        depth + 1,
        output,
        point_progress,
        start_progress,
        middle_progress,
    );
    flatten_pair(
        a1,
        b1,
        tolerance,
        minimum,
        depth + 1,
        output,
        point_progress,
        middle_progress,
        end_progress,
    );
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
