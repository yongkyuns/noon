//! Scalar point edits keep authored coordinates until explicit renderer lowering.
use super::*;

type Point = (f64, f64);
pub(crate) const LINE_SAMPLES: [f64; 4] = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];

/// Transient preparation inputs, never retained scene or runtime state.
#[derive(Clone, Copy)]
pub(crate) enum ScalarPathEdit<'a, P = Point> {
    Start {
        point: P,
        stride: i32,
    },
    Line {
        point: P,
        stride: i32,
        samples: &'a [f64],
    },
    CubicTo {
        controls: [P; 3],
        stride: i32,
    },
    Cubic([P; 4]),
    Cubics {
        curves: &'a [[P; 4]],
        replace: bool,
    },
    Corners {
        points: &'a [P],
        samples: &'a [f64],
    },
    AppendCorners {
        points: &'a [P],
        stride: i32,
        samples: &'a [f64],
    },
    Close {
        stride: i32,
        tolerance: f64,
        samples: &'a [f64],
    },
}

/// Coordinate promotion at the typed convenience boundary, never a stored projection.
pub(crate) trait ScalarPoint: Copy {
    fn into_xyz(self) -> SemanticVec3;
}
impl ScalarPoint for Point {
    fn into_xyz(self) -> SemanticVec3 {
        SemanticVec3::new(self.0, self.1, 0.0)
    }
}
impl ScalarPoint for SemanticVec3 {
    fn into_xyz(self) -> SemanticVec3 {
        self
    }
}

pub(super) fn prepare<P: ScalarPoint>(
    path: VectorPath,
    edit: ScalarPathEdit<'_, P>,
) -> Result<Option<VectorPath>, AuthoringError> {
    use ScalarPathEdit::*;
    let result = match edit {
        Start { point, stride } => start(path, point.into_xyz(), stride)?,
        Line {
            point,
            stride,
            samples,
        } => line(path, point.into_xyz(), stride, samples)?,
        CubicTo { controls, stride } => {
            let controls = controls.map(ScalarPoint::into_xyz);
            point_arrays::validate(&controls)?;
            return append_manim_cubic_points_with_stride(
                point_arrays::points(&path)?,
                controls,
                stride,
                VectorPath::from_manim_points_3d,
            )
            .map(Some);
        }
        Cubic(controls) => {
            let controls = controls.map(ScalarPoint::into_xyz);
            point_arrays::validate(&controls)?;
            let target = path.morph_target().cloned();
            let mut points = point_arrays::points(&path)?;
            points.extend(controls);
            let mut result = VectorPath::from_manim_points_3d(points);
            if let Some(target) = target {
                result = result.with_morph_target(target);
            }
            result
        }
        Cubics { curves, replace } => {
            let flattened = curves
                .iter()
                .flat_map(|curve| curve.iter().copied())
                .map(ScalarPoint::into_xyz)
                .collect::<Vec<_>>();
            point_arrays::validate(&flattened)?;
            if !replace && curves.is_empty() {
                return Ok(None);
            }
            let target = if replace {
                None
            } else {
                path.morph_target().cloned()
            };
            let mut points = if replace {
                Vec::new()
            } else {
                point_arrays::points(&path)?
            };
            points.extend(flattened);
            let mut result = VectorPath::from_manim_points_3d(points);
            if let Some(target) = target {
                result = result.with_morph_target(target);
            }
            result
        }
        Corners { points, samples } => corners(
            &points
                .iter()
                .copied()
                .map(ScalarPoint::into_xyz)
                .collect::<Vec<_>>(),
            samples,
        )?,
        AppendCorners {
            points,
            stride,
            samples,
        } => {
            if points.is_empty() {
                point_arrays::points(&path)?.last().ok_or_else(empty_path)?;
                return Ok(None);
            }
            append_corners(
                path,
                &points
                    .iter()
                    .copied()
                    .map(ScalarPoint::into_xyz)
                    .collect::<Vec<_>>(),
                stride,
                samples,
            )?
        }
        Close {
            stride,
            tolerance,
            samples,
        } => return close(path, stride, tolerance, samples),
    };
    finish(result).map(Some)
}

fn finish(path: VectorPath) -> Result<VectorPath, AuthoringError> {
    if !path.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(path)
}

fn validate_samples(samples: &[f64]) -> Result<(), AuthoringError> {
    if samples.iter().any(|t| !t.is_finite()) {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(())
}

// CE's interpolate uses two weighted terms, not a renderer-precision delta.
fn interpolate(start: SemanticVec3, end: SemanticVec3, t: f64) -> SemanticVec3 {
    SemanticVec3::new(
        (1.0 - t) * start.x + t * end.x,
        (1.0 - t) * start.y + t * end.y,
        (1.0 - t) * start.z + t * end.z,
    )
}

fn empty_path() -> AuthoringError {
    AuthoringError::PathQuery(noon_geometry::PathProportionError::EmptyPath)
}

fn start(path: VectorPath, point: SemanticVec3, stride: i32) -> Result<VectorPath, AuthoringError> {
    point_arrays::validate(&[point])?;
    if stride == 0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: "n_points_per_cubic_curve".to_owned(),
            value: 0.0,
        });
    }
    let mut points = point_arrays::points(&path)?;
    if stride > 0 {
        let stride = stride as usize;
        let remainder = points.len() % stride;
        if remainder != 0 {
            let anchor = points[((points.len() - 1) / stride) * stride];
            points.extend(std::iter::repeat_n(anchor, stride - remainder));
        }
    }
    points.push(point);
    finish(VectorPath::from_manim_points_3d(points))
}

fn line(
    path: VectorPath,
    point: SemanticVec3,
    stride: i32,
    samples: &[f64],
) -> Result<VectorPath, AuthoringError> {
    point_arrays::validate(&[point])?;
    validate_samples(samples)?;
    let target = path.morph_target().cloned();
    let mut points = point_arrays::points(&path)?;
    let start = points.last().copied().ok_or_else(empty_path)?;
    if !manim_remainder_is_one(points.len(), stride) {
        points.push(start);
    }
    points.extend(
        samples
            .iter()
            .skip(1)
            .map(|&t| interpolate(start, point, t)),
    );
    let mut result = VectorPath::from_manim_points_3d(points);
    if let Some(target) = target {
        result = result.with_morph_target(line(target, point, stride, samples)?);
    }
    finish(result)
}

fn corners(points: &[SemanticVec3], samples: &[f64]) -> Result<VectorPath, AuthoringError> {
    point_arrays::validate(points)?;
    validate_samples(samples)?;
    if samples == LINE_SAMPLES {
        return finish(VectorPath::from_manim_corners_3d(points));
    }
    let raw = points
        .windows(2)
        .flat_map(|pair| {
            samples
                .iter()
                .map(move |&t| interpolate(pair[0], pair[1], t))
        })
        .collect();
    finish(VectorPath::from_manim_points_3d(raw))
}

fn append_corners(
    path: VectorPath,
    corners: &[SemanticVec3],
    stride: i32,
    samples: &[f64],
) -> Result<VectorPath, AuthoringError> {
    let mut points = point_arrays::points(&path)?;
    let mut start = points.last().copied().ok_or_else(empty_path)?;
    if corners.is_empty() {
        return Ok(path);
    }
    point_arrays::validate(corners)?;
    validate_samples(samples)?;
    if stride <= 0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: "n_points_per_cubic_curve".to_owned(),
            value: f64::from(stride),
        });
    }
    if manim_remainder_is_one(points.len(), stride) {
        points.pop();
    }
    for &end in corners {
        for index in 0..stride as usize {
            // Preserve the existing deterministic fill for unsampled mutable
            // stride slots; the Python adapter guards CE's unstable profiles.
            points.push(
                samples
                    .get(index)
                    .map_or(end, |&t| interpolate(start, end, t)),
            );
        }
        start = end;
    }
    let mut result = VectorPath::from_manim_points_3d(points);
    if let Some(target) = path.morph_target().cloned() {
        result = result.with_morph_target(target);
    }
    finish(result)
}

fn close(
    path: VectorPath,
    stride: i32,
    tolerance: f64,
    samples: &[f64],
) -> Result<Option<VectorPath>, AuthoringError> {
    let target = path.morph_target().cloned();
    let points = point_arrays::points(&path)?;
    let first = points.first().copied().ok_or_else(empty_path)?;
    let last = *points.last().expect("the nonempty raw array was checked");
    if crate::path_queries::manim_points_allclose(
        &[first.x, first.y, first.z],
        &[last.x, last.y, last.z],
        tolerance,
    ) {
        return Ok(None);
    }
    let coordinates = points
        .iter()
        .flat_map(|p| [p.x, p.y, p.z])
        .collect::<Vec<_>>();
    let ranges = crate::path_queries::manim_subpath_ranges(
        &coordinates,
        points.len(),
        3,
        3,
        f64::from(stride),
        tolerance,
        true,
    )?;
    let start_index = ranges.get(ranges.len().saturating_sub(2)).copied().ok_or(
        AuthoringError::NoSubpathForClose {
            point_count: points.len(),
            stride,
        },
    )?;
    // The ordinary line operation shares interpolation, prefix handling and
    // validation. Closing each morph target uses its own last subpath anchor.
    let anchor = points[start_index];
    let mut result = line(
        VectorPath::from_manim_points_3d(points),
        anchor,
        stride,
        samples,
    )?;
    if let Some(target) = target {
        result = result.with_morph_target(
            close(target.clone(), stride, tolerance, samples)?.unwrap_or(target),
        );
    }
    finish(result).map(Some)
}

#[cfg(all(test, feature = "diagnostics"))]
mod tests;

#[cfg(test)]
mod xyz_tests;
