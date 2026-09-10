//! Filled-region operations produce ordinary immutable paths, never renderer state.
use crate::{flatten::flatten_path, GeometryError};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::single::SingleFloatOverlay,
};
use noon_core::{Vec2, VectorPath};

/// World/content-space chord tolerance used when converting curves to polygons.
/// This is an authoring approximation, independent of camera and frame rate.
pub const BOOLEAN_FLATTEN_TOLERANCE: f32 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOperation {
    Union,
    Intersection,
    Difference,
    Exclusion,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BooleanPathError {
    OperandCount {
        operation: BooleanOperation,
        count: usize,
    },
    Geometry(GeometryError),
}
impl std::fmt::Display for BooleanPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OperandCount { operation, count } => write!(
                f,
                "{operation:?} requires {} operands, got {count}",
                if matches!(
                    operation,
                    BooleanOperation::Difference | BooleanOperation::Exclusion
                ) {
                    "exactly two"
                } else {
                    "at least two"
                }
            ),
            Self::Geometry(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for BooleanPathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            _ => None,
        }
    }
}
impl From<GeometryError> for BooleanPathError {
    fn from(value: GeometryError) -> Self {
        Self::Geometry(value)
    }
}

/// Apply a filled-region operation with non-zero winding. Open contours are
/// implicitly closed; contours with fewer than three distinct vertices have no
/// area. Curves are flattened once, at a fixed 0.001-unit chord tolerance. Output
/// outer contours and holes have opposite winding and explicit closure.
///
/// Work is proportional to the selected input segments and their intersections;
/// no scene scan, semantic mutation, or per-frame computation is performed here.
pub fn boolean_paths(
    operation: BooleanOperation,
    paths: &[VectorPath],
) -> Result<VectorPath, BooleanPathError> {
    let count = paths.len();
    if count < 2
        || (matches!(
            operation,
            BooleanOperation::Difference | BooleanOperation::Exclusion
        ) && count != 2)
    {
        return Err(BooleanPathError::OperandCount { operation, count });
    }
    // Validate every operand before starting overlay, including after an empty
    // intersection. Invalid input must never be hidden by an early result.
    let contours = paths
        .iter()
        .map(|path| {
            flatten_path(path, BOOLEAN_FLATTEN_TOLERANCE)?
                .into_iter()
                .filter(|c| c.points.len() >= 3)
                .map(|c| {
                    c.points
                        .into_iter()
                        .map(|p| {
                            if !p.x.is_finite() || !p.y.is_finite() {
                                return Err(GeometryError::NonFinitePoint);
                            }
                            Ok([f64::from(p.x), f64::from(p.y)])
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, GeometryError>>()?;
    let rule = match operation {
        BooleanOperation::Union => OverlayRule::Union,
        BooleanOperation::Intersection => OverlayRule::Intersect,
        BooleanOperation::Difference => OverlayRule::Difference,
        BooleanOperation::Exclusion => OverlayRule::Xor,
    };
    let mut operands = contours.into_iter();
    let mut result = operands.next().expect("validated operand count");
    for operand in operands {
        result = result
            .overlay_as::<i64>(&operand, rule, FillRule::NonZero)
            .into_iter()
            .flatten()
            .collect();
    }
    let mut path = VectorPath::new();
    for contour in result {
        let Some(first) = contour.first() else {
            continue;
        };
        path = path.move_to(Vec2::new(first[0] as f32, first[1] as f32));
        for point in contour.iter().skip(1) {
            path = path.line_to(Vec2::new(point[0] as f32, point[1] as f32));
        }
        path = path.close();
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32) -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(x, y))
            .line_to(Vec2::new(x + w, y))
            .line_to(Vec2::new(x + w, y + h))
            .line_to(Vec2::new(x, y + h))
            .close()
    }
    fn area(path: &VectorPath) -> f64 {
        flatten_path(path, BOOLEAN_FLATTEN_TOLERANCE)
            .unwrap()
            .iter()
            .map(|c| {
                c.points
                    .iter()
                    .zip(c.points.iter().cycle().skip(1))
                    .take(c.points.len())
                    .map(|(a, b)| f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x))
                    .sum::<f64>()
                    * 0.5
            })
            .sum::<f64>()
            .abs()
    }
    #[test]
    fn overlapping_rectangles_cover_all_operations() {
        let paths = [rect(0., 0., 2., 2.), rect(1., 0., 2., 2.)];
        for (op, expected) in [
            (BooleanOperation::Union, 6.),
            (BooleanOperation::Intersection, 2.),
            (BooleanOperation::Difference, 2.),
            (BooleanOperation::Exclusion, 4.),
        ] {
            let result = boolean_paths(op, &paths).unwrap();
            assert!((area(&result) - expected).abs() < 1e-5);
            assert_eq!(result, boolean_paths(op, &paths).unwrap());
            assert!(flatten_path(&result, 0.001)
                .unwrap()
                .iter()
                .all(|c| c.closed));
        }
    }
    #[test]
    fn holes_have_opposite_winding_and_survive_later_union() {
        let hole = boolean_paths(
            BooleanOperation::Difference,
            &[rect(-2., -2., 4., 4.), rect(-1., -1., 2., 2.)],
        )
        .unwrap();
        assert!((area(&hole) - 12.).abs() < 1e-5);
        assert_eq!(flatten_path(&hole, 0.001).unwrap().len(), 2);
        let result = boolean_paths(
            BooleanOperation::Union,
            &[hole, rect(4., 0., 1., 1.), rect(6., 0., 1., 1.)],
        )
        .unwrap();
        assert!((area(&result) - 14.).abs() < 1e-5);
    }
    #[test]
    fn empty_disjoint_identical_and_touching_regions() {
        let r = rect(0., 0., 1., 1.);
        for op in [BooleanOperation::Difference, BooleanOperation::Exclusion] {
            assert!(boolean_paths(op, &[r.clone(), r.clone()])
                .unwrap()
                .commands()
                .is_empty());
        }
        for other in [
            VectorPath::new(),
            rect(2., 0., 1., 1.),
            rect(1., 0., 1., 1.),
        ] {
            assert!(
                boolean_paths(BooleanOperation::Intersection, &[r.clone(), other])
                    .unwrap()
                    .commands()
                    .is_empty()
            );
        }
        assert!(
            (area(&boolean_paths(BooleanOperation::Union, &[r, VectorPath::new()]).unwrap()) - 1.)
                .abs()
                < 1e-5
        );
    }
    #[test]
    fn nonzero_winding_cleans_self_intersections_and_closes_open_regions() {
        let bowtie = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2., 2.))
            .line_to(Vec2::new(0., 2.))
            .line_to(Vec2::new(2., 0.));
        let result = boolean_paths(BooleanOperation::Union, &[bowtie, VectorPath::new()]).unwrap();
        assert!((area(&result) - 2.).abs() < 1e-5);
        let square = rect(0., 0., 2., 2.);
        let result = boolean_paths(
            BooleanOperation::Union,
            &[square.clone(), crate::reverse_path(&square)],
        )
        .unwrap();
        assert!((area(&result) - 4.).abs() < 1e-5);
    }

    #[test]
    fn invalid_inputs_do_not_get_hidden_by_empty_intersection() {
        assert!(matches!(
            boolean_paths(BooleanOperation::Union, &[]),
            Err(BooleanPathError::OperandCount { .. })
        ));
        let invalid = VectorPath::new().move_to(Vec2::new(f32::NAN, 0.));
        assert!(boolean_paths(
            BooleanOperation::Intersection,
            &[VectorPath::new(), invalid]
        )
        .is_err());
    }
    #[test]
    fn cubic_circle_intersection_retains_area_within_flatten_tolerance() {
        let circle = crate::canonical_outline_path(&noon_core::GeometryRef::circle(1.)).unwrap();
        let result = boolean_paths(
            BooleanOperation::Intersection,
            &[circle, rect(-2., -2., 4., 4.)],
        )
        .unwrap();
        assert!((area(&result) - std::f64::consts::PI).abs() < 0.01);
    }
}
