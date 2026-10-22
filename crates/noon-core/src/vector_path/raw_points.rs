//! Full semantic point storage and explicit planar drawing projection.
use super::*;

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
            let (start, end) = (pair[0], pair[1]);
            for t in [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0] {
                raw.push(SemanticVec3::new(
                    (1.0 - t) * start.x + t * end.x,
                    (1.0 - t) * start.y + t * end.y,
                    (1.0 - t) * start.z + t * end.z,
                ));
            }
        }
        Self::from_manim_points_drawing(raw, true, corners.first() == corners.last())
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
