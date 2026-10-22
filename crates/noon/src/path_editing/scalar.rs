//! Scalar point edits keep authored coordinates until explicit renderer lowering.
use super::*;

type Point = (f64, f64);
pub(crate) const LINE_SAMPLES: [f64; 4] = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];

/// Transient preparation inputs, never retained scene or runtime state.
#[derive(Clone, Copy)]
pub(crate) enum ScalarPathEdit<'a> {
    Start {
        point: Point,
        stride: i32,
    },
    Line {
        point: Point,
        stride: i32,
        samples: &'a [f64],
    },
    CubicTo {
        controls: [Point; 3],
        stride: i32,
    },
    Cubic([Point; 4]),
    Cubics {
        curves: &'a [[Point; 4]],
        replace: bool,
    },
    Corners {
        points: &'a [Point],
        samples: &'a [f64],
    },
    AppendCorners {
        points: &'a [Point],
        stride: i32,
        samples: &'a [f64],
    },
    Close {
        stride: i32,
        tolerance: f64,
        samples: &'a [f64],
    },
}

pub(super) fn prepare(
    path: VectorPath,
    edit: ScalarPathEdit<'_>,
) -> Result<Option<VectorPath>, AuthoringError> {
    use ScalarPathEdit::*;
    let result = match edit {
        Start { point, stride } => start(path, point, stride)?,
        Line {
            point,
            stride,
            samples,
        } => line(path, point, stride, samples)?,
        CubicTo { controls, stride } => {
            validate_raw_points(&controls)?;
            return append_manim_cubic_points_with_stride(
                manim_points_f64_for_path(&path)?,
                controls,
                stride,
                VectorPath::from_manim_points_f64,
            )
            .map(Some);
        }
        Cubic(controls) => {
            validate_raw_points(&controls)?;
            let target = path.morph_target().cloned();
            let mut points = manim_points_f64_for_path(&path)?;
            points.extend(controls);
            let mut result = VectorPath::from_manim_points_f64(points);
            if let Some(target) = target {
                result = result.with_morph_target(target);
            }
            result
        }
        Cubics { curves, replace } => {
            validate_raw_points(curves.as_flattened())?;
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
                manim_points_f64_for_path(&path)?
            };
            points.extend_from_slice(curves.as_flattened());
            let mut result = VectorPath::from_manim_points_f64(points);
            if let Some(target) = target {
                result = result.with_morph_target(target);
            }
            result
        }
        Corners { points, samples } => corners(points, samples)?,
        AppendCorners {
            points,
            stride,
            samples,
        } => append_corners(path, points, stride, samples)?,
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
fn interpolate(start: Point, end: Point, t: f64) -> Point {
    (
        (1.0 - t) * start.0 + t * end.0,
        (1.0 - t) * start.1 + t * end.1,
    )
}

fn empty_path() -> AuthoringError {
    AuthoringError::PathQuery(noon_geometry::PathProportionError::EmptyPath)
}

fn start(path: VectorPath, point: Point, stride: i32) -> Result<VectorPath, AuthoringError> {
    validate_raw_points(&[point])?;
    if stride == 0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: "n_points_per_cubic_curve".to_owned(),
            value: 0.0,
        });
    }
    let mut points = manim_points_f64_for_path(&path)?;
    if stride > 0 {
        let stride = stride as usize;
        let remainder = points.len() % stride;
        if remainder != 0 {
            let anchor = points[((points.len() - 1) / stride) * stride];
            points.extend(std::iter::repeat_n(anchor, stride - remainder));
        }
    }
    points.push(point);
    finish(VectorPath::from_manim_points_f64(points))
}

fn line(
    path: VectorPath,
    point: Point,
    stride: i32,
    samples: &[f64],
) -> Result<VectorPath, AuthoringError> {
    validate_raw_points(&[point])?;
    validate_samples(samples)?;
    let target = path.morph_target().cloned();
    let mut points = manim_points_f64_for_path(&path)?;
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
    let mut result = VectorPath::from_manim_points_f64(points);
    if let Some(target) = target {
        result = result.with_morph_target(line(target, point, stride, samples)?);
    }
    finish(result)
}

fn corners(points: &[Point], samples: &[f64]) -> Result<VectorPath, AuthoringError> {
    validate_raw_points(points)?;
    validate_samples(samples)?;
    let raw = points
        .windows(2)
        .flat_map(|pair| {
            samples
                .iter()
                .map(move |&t| interpolate(pair[0], pair[1], t))
        })
        .collect();
    finish(VectorPath::from_manim_points_f64(raw))
}

fn append_corners(
    path: VectorPath,
    corners: &[Point],
    stride: i32,
    samples: &[f64],
) -> Result<VectorPath, AuthoringError> {
    let mut points = manim_points_f64_for_path(&path)?;
    let mut start = points.last().copied().ok_or_else(empty_path)?;
    if corners.is_empty() {
        return Ok(path);
    }
    validate_raw_points(corners)?;
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
    finish(VectorPath::from_manim_points_f64(points))
}

fn close(
    path: VectorPath,
    stride: i32,
    tolerance: f64,
    samples: &[f64],
) -> Result<Option<VectorPath>, AuthoringError> {
    let target = path.morph_target().cloned();
    let points = manim_points_f64_for_path(&path)?;
    let first = points.first().copied().ok_or_else(empty_path)?;
    let last = *points.last().expect("the nonempty raw array was checked");
    if crate::path_queries::manim_points_2d_close([first.0, first.1], [last.0, last.1], tolerance) {
        return Ok(None);
    }
    let coordinates = points.iter().flat_map(|&(x, y)| [x, y]).collect::<Vec<_>>();
    let ranges = crate::path_queries::manim_subpath_ranges(
        &coordinates,
        points.len(),
        2,
        2,
        f64::from(stride),
        tolerance,
        true,
    )?;
    let start_index = ranges
        .get(ranges.len().saturating_sub(2))
        .copied()
        .ok_or_else(empty_path)?;
    // The ordinary line operation shares interpolation, prefix handling and
    // validation. Closing each morph target uses its own last subpath anchor.
    let anchor = points[start_index];
    let mut result = line(
        VectorPath::from_manim_points_f64(points),
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
