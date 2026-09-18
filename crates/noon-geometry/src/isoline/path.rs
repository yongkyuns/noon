use noon_core::{Vec2, VectorPath};

use super::{IsolinePlan, IsolinePoint};

/// Failure while lowering an already planned contour into retained path geometry.
///
/// Planning deliberately retains f64 coordinates. The ordinary renderer path is
/// f32, so this separate conversion reports values that cannot be represented
/// without changing the planner's topology or callback behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsolinePathError {
    InvalidPoint {
        curve_index: usize,
        point_index: usize,
    },
}

impl std::fmt::Display for IsolinePathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPoint {
                curve_index,
                point_index,
            } => write!(
                formatter,
                "isoline point {point_index} in curve {curve_index} is not a finite renderable 2D point"
            ),
        }
    }
}

impl std::error::Error for IsolinePathError {}

impl IsolinePlan {
    /// Convert planned coordinate-space contours into one ordinary retained path.
    ///
    /// Each curve remains a distinct subpath in planner order. Explicit repeated
    /// endpoints become `Close`, so later shared anchor smoothing preserves a
    /// closed contour instead of adding a redundant closing segment.
    pub fn path(&self) -> Result<VectorPath, IsolinePathError> {
        let mut path = VectorPath::new();
        for (curve_index, curve) in self.curves.iter().enumerate() {
            let Some(&first) = curve.first() else {
                continue;
            };
            let first = renderable_point(first, curve_index, 0)?;
            path = path.move_to(first);
            let closed = curve.len() > 2 && curve.first() == curve.last();
            let end = if closed { curve.len() - 1 } else { curve.len() };
            for (point_index, &point) in curve[1..end].iter().enumerate() {
                path = path.line_to(renderable_point(point, curve_index, point_index + 1)?);
            }
            if closed {
                path = path.close();
            }
        }
        Ok(path)
    }
}

fn renderable_point(
    point: IsolinePoint,
    curve_index: usize,
    point_index: usize,
) -> Result<Vec2, IsolinePathError> {
    let point = Vec2::new(point.x as f32, point.y as f32);
    if point.x.is_finite() && point.y.is_finite() {
        Ok(point)
    } else {
        Err(IsolinePathError::InvalidPoint {
            curve_index,
            point_index,
        })
    }
}
