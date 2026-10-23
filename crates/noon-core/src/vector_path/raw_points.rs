//! Full semantic point storage and explicit planar drawing projection.
use super::*;

/// Authored contour topology in canonical semantic precision. A straight edge
/// retains cubic controls for CE queries and a line recipe for renderer lowering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticPathSegment {
    Line([SemanticVec3; 2]),
    Cubic([SemanticVec3; 4]),
}

impl SemanticPathSegment {
    fn controls(self) -> [SemanticVec3; 4] {
        match self {
            Self::Line([start, end]) => VectorPath::manim_line_controls(start, end),
            Self::Cubic(points) => points,
        }
    }
}

impl VectorPath {
    /// Preserve a zero-row caller array without inventing XYZ dimensions.
    /// Native typed XYZ constructors keep their ordinary [0, 3] empty shape.
    pub fn from_empty_point_array(
        shape: crate::PointArrayShape,
    ) -> Result<Self, crate::PointArrayShapeError> {
        if shape.rows() != 0 {
            return Err(crate::PointArrayShapeError::ExpectedZeroRows);
        }
        let mut result = Self::new();
        if shape.dimensions() != [0, 3] {
            result.manim_points = Some(Vec::new());
            result.empty_point_array_shape = Some(Box::new(shape));
        }
        Ok(result)
    }

    pub fn empty_point_array_shape(&self) -> Option<&crate::PointArrayShape> {
        self.empty_point_array_shape.as_deref()
    }

    /// Retain a complete raw point sequence, including an unfinished cubic tail.
    pub fn from_manim_points(points: Vec<Vec2>) -> Self {
        Self::from_manim_points_3d(points.into_iter().map(SemanticVec3::from_vec2).collect())
    }

    /// Planar authoring convenience over the same three-axis semantic storage.
    pub fn from_manim_points_f64(points: Vec<(f64, f64)>) -> Self {
        Self::from_manim_points_3d(
            points
                .into_iter()
                .map(|(x, y)| SemanticVec3::new(x, y, 0.0))
                .collect(),
        )
    }

    /// Retain every f64 coordinate. Drawable commands are the explicitly lowered
    /// XY prefix; a planar camera does not erase the semantic Z coordinates.
    pub fn from_manim_points_3d(points: Vec<SemanticVec3>) -> Self {
        Self::from_manim_points_drawing(points, false, false)
    }

    /// Retain complete high-precision cubic contours and explicitly lower drawing.
    /// Contour closure remains part of the path topology, including after transforms.
    pub fn from_semantic_contours(
        contours: impl IntoIterator<Item = (Vec<SemanticPathSegment>, bool)>,
    ) -> Self {
        let lower = |point: SemanticVec3| Vec2::new(point.x as f32, point.y as f32);
        let mut path = Self::new();
        let mut points = Vec::new();
        for (mut curves, closed) in contours {
            if curves.is_empty() {
                continue;
            }
            // Use the existing canonical line controls for an implicit close.
            // The final Close command is only the explicitly lowered join recipe.
            if closed {
                let first = curves[0].controls()[0];
                let last = curves.last().expect("nonempty contour").controls()[3];
                if first != last {
                    curves.push(SemanticPathSegment::Line([last, first]));
                }
            }
            let mut previous = None;
            for segment in curves {
                let curve = segment.controls();
                if previous != Some(curve[0]) {
                    path = path.move_to(lower(curve[0]));
                }
                path = match segment {
                    SemanticPathSegment::Line(_) => path.line_to(lower(curve[3])),
                    SemanticPathSegment::Cubic(_) => {
                        path.cubic_to(lower(curve[1]), lower(curve[2]), lower(curve[3]))
                    }
                };
                previous = Some(curve[3]);
                points.extend(curve);
            }
            if closed {
                path = path.close();
            }
        }
        if !points.is_empty() {
            path.manim_points = Some(points);
        }
        path
    }

    /// Rebuild the same authored contour/line/closure recipe from transformed
    /// canonical controls; rounded drawable coordinates never become input.
    pub(super) fn from_transformed_manim_points(
        points: Vec<SemanticVec3>,
        recipe: &[PathCommand],
    ) -> Self {
        let lower = |point: SemanticVec3| Vec2::new(point.x as f32, point.y as f32);
        let mut path = Self::new();
        let mut curve = 0;
        for command in recipe {
            path = match command {
                PathCommand::MoveTo { .. } => {
                    let Some(&point) = points.get(curve * 4) else {
                        continue;
                    };
                    path.move_to(lower(point))
                }
                PathCommand::LineTo { .. } | PathCommand::CubicTo { .. } => {
                    let Some(points) = points.get(curve * 4..curve * 4 + 4) else {
                        continue;
                    };
                    curve += 1;
                    if matches!(command, PathCommand::LineTo { .. }) {
                        path.line_to(lower(points[3]))
                    } else {
                        path.cubic_to(lower(points[1]), lower(points[2]), lower(points[3]))
                    }
                }
                PathCommand::Close => path.close(),
                // External decoded data may contain an inconsistent old recipe.
                // The canonical controls remain authoritative in that case too.
                PathCommand::QuadraticTo { .. } => {
                    return Self::from_manim_points_drawing(points, false, false);
                }
            };
        }
        path.manim_points = Some(points);
        path
    }

    pub(super) fn from_manim_points_drawing(
        points: Vec<SemanticVec3>,
        line_segments: bool,
        closed: bool,
    ) -> Self {
        if points.is_empty() {
            return Self::new();
        }
        let lower = |point: SemanticVec3| Vec2::new(point.x as f32, point.y as f32);
        let mut path = Self::new();
        let mut previous_end = None;
        for curve in points.as_chunks::<4>().0 {
            let [start, control1, control2, end] = *curve;
            if previous_end != Some(start) {
                path = path.move_to(lower(start));
            }
            path = if line_segments {
                path.line_to(lower(end))
            } else {
                path.cubic_to(lower(control1), lower(control2), lower(end))
            };
            previous_end = Some(end);
        }
        if let Some(&start) = points.get(points.len() / 4 * 4) {
            path = path.move_to(lower(start));
        }
        if closed {
            path = path.close();
        }
        path.manim_points = Some(points);
        path
    }

    /// CE's canonical controls and the compact line/closure drawing use one
    /// semantic array. The two-dimensional convenience supplies zero Z.
    pub fn from_manim_corners_f64(corners: &[(f64, f64)]) -> Self {
        let corners = corners
            .iter()
            .map(|&(x, y)| SemanticVec3::new(x, y, 0.0))
            .collect::<Vec<_>>();
        Self::from_manim_corners_3d(&corners)
    }

    pub fn from_manim_corners_3d(corners: &[SemanticVec3]) -> Self {
        let mut raw = Vec::with_capacity(corners.len().saturating_sub(1) * 4);
        for pair in corners.windows(2) {
            raw.extend(Self::manim_line_controls(pair[0], pair[1]));
        }
        Self::from_manim_points_drawing(raw, true, corners.first() == corners.last())
    }

    /// CE's complete cubic controls for a straight segment, without allocation.
    pub fn manim_line_controls(start: SemanticVec3, end: SemanticVec3) -> [SemanticVec3; 4] {
        [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].map(|t| {
            SemanticVec3::new(
                (1.0 - t) * start.x + t * end.x,
                (1.0 - t) * start.y + t * end.y,
                (1.0 - t) * start.z + t * end.z,
            )
        })
    }

    /// The sole authored array, including all axes and incomplete trailing points.
    pub fn manim_points(&self) -> Option<&[SemanticVec3]> {
        self.manim_points.as_deref()
    }

    /// A derived XY projection for explicitly planar geometry algorithms.
    /// This allocates no retained state and does not alter the semantic array.
    pub fn manim_points_xy(&self) -> Option<Vec<(f64, f64)>> {
        self.manim_points()
            .map(|points| points.iter().copied().map(SemanticVec3::xy).collect())
    }
}

#[cfg(test)]
mod empty_shape_tests {
    use super::*;
    use crate::{PointArrayShape, PointArrayShapeError};

    #[test]
    fn empty_point_array_shape_is_retained_by_copy_and_transform() {
        for dimensions in [vec![0], vec![0, 2], vec![0, 3], vec![0, 2, 3]] {
            let shape = PointArrayShape::new(dimensions.clone()).unwrap();
            let path = VectorPath::from_empty_point_array(shape.clone()).unwrap();
            assert!(path.is_finite());
            assert!(path.commands().is_empty());
            let transformed = path.transformed_semantic(SemanticTransform2_5D {
                translation: SemanticVec3::new(2., -1., 4.),
                ..Default::default()
            });
            assert_eq!(transformed, path);
            assert_eq!(
                path.empty_point_array_shape().cloned().unwrap_or_default(),
                shape
            );
            assert_eq!(
                path.clone().move_to(Vec2::ZERO).empty_point_array_shape(),
                None
            );
        }
        assert_eq!(
            VectorPath::from_empty_point_array(PointArrayShape::xyz(1).unwrap()),
            Err(PointArrayShapeError::ExpectedZeroRows)
        );
    }
}

#[cfg(test)]
mod tests;
