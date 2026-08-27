//! Shared query semantics for Manim-compatible straight lines.
//!
//! These helpers read Noon's retained line snapshot after affine transforms so
//! Python/JS adapters do not need to reconstruct endpoint/vector math independently.

use crate::legacy::Line;
use noon_core::{GeometryRef, ObjectSnapshot, Vec2};

/// Return the transformed start point of a retained straight-line snapshot.
pub fn line_start_from_snapshot(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    let GeometryRef::Line { start, .. } = &snapshot.geometry else {
        return None;
    };
    Some(snapshot.transform.transform_point(*start))
}

/// Return the transformed end point of a retained straight-line snapshot.
pub fn line_end_from_snapshot(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    let GeometryRef::Line { end, .. } = &snapshot.geometry else {
        return None;
    };
    Some(snapshot.transform.transform_point(*end))
}

/// Return the transformed chord from start to end.
pub fn line_vector_from_snapshot(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    Some(line_end_from_snapshot(snapshot)? - line_start_from_snapshot(snapshot)?)
}

/// Return the transformed line length.
pub fn line_length_from_snapshot(snapshot: &ObjectSnapshot) -> Option<f32> {
    Some(line_vector_from_snapshot(snapshot)?.length())
}

/// Return the transformed line angle in the xy plane, matching Manim `angle_of_vector`.
pub fn line_angle_from_snapshot(snapshot: &ObjectSnapshot) -> Option<f32> {
    let vector = line_vector_from_snapshot(snapshot)?;
    Some(vector.y.atan2(vector.x))
}

/// Return the transformed unit direction. A zero-length line yields the zero vector,
/// matching Manim's `normalize` default fallback.
pub fn line_unit_vector_from_snapshot(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    let vector = line_vector_from_snapshot(snapshot)?;
    Some(vector.normalized().unwrap_or(Vec2::ZERO))
}

/// Project a world-space point onto the infinite transformed line.
///
/// A zero-length line projects every point to its start, matching Manim's zero-vector
/// normalization fallback.
pub fn line_projection_from_snapshot(snapshot: &ObjectSnapshot, point: Vec2) -> Option<Vec2> {
    let start = line_start_from_snapshot(snapshot)?;
    let unit = line_unit_vector_from_snapshot(snapshot)?;
    Some(start + unit * (point - start).dot(unit))
}

impl Line {
    pub fn get_start(&self) -> Vec2 {
        line_start_from_snapshot(self.snapshot()).expect("Line retains straight-line geometry")
    }

    pub fn get_end(&self) -> Vec2 {
        line_end_from_snapshot(self.snapshot()).expect("Line retains straight-line geometry")
    }

    pub fn get_vector(&self) -> Vec2 {
        line_vector_from_snapshot(self.snapshot()).expect("Line retains straight-line geometry")
    }

    pub fn get_unit_vector(&self) -> Vec2 {
        line_unit_vector_from_snapshot(self.snapshot())
            .expect("Line retains straight-line geometry")
    }

    pub fn get_length(&self) -> f32 {
        line_length_from_snapshot(self.snapshot()).expect("Line retains straight-line geometry")
    }

    pub fn get_angle(&self) -> f32 {
        line_angle_from_snapshot(self.snapshot()).expect("Line retains straight-line geometry")
    }

    pub fn get_slope(&self) -> f32 {
        self.get_angle().tan()
    }

    pub fn get_projection(&self, point: Vec2) -> Vec2 {
        line_projection_from_snapshot(self.snapshot(), point)
            .expect("Line retains straight-line geometry")
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use crate::legacy::IntoSnapshot;
    use noon_core::{GeometryRef, RIGHT};

    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn line_queries_follow_retained_affine_transform() {
        let line = Line::new(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0))
            .scale_xy(Vec2::new(2.0, 3.0))
            .rotate(PI / 2.0)
            .shift(Vec2::new(4.0, -2.0));

        assert_vec_close(line.get_start(), Vec2::new(4.0, -4.0));
        assert_vec_close(line.get_end(), Vec2::new(4.0, 0.0));
        assert_vec_close(line.get_vector(), Vec2::new(0.0, 4.0));
        assert_vec_close(line.get_unit_vector(), Vec2::new(0.0, 1.0));
        assert_close(line.get_length(), 4.0);
        assert_close(line.get_angle(), PI / 2.0);
    }

    #[test]
    fn projection_and_slope_match_transformed_line() {
        let line = Line::new(Vec2::ZERO, Vec2::new(2.0, 2.0)).shift(RIGHT);
        let projection = line.get_projection(Vec2::new(3.0, 1.0));
        assert_vec_close(projection, Vec2::new(2.0, 1.0));
        assert_close(line.get_slope(), 1.0);
    }

    #[test]
    fn zero_length_line_has_manim_zero_unit_vector_and_start_projection() {
        let point = Vec2::new(2.0, -3.0);
        let line = Line::new(point, point).shift(Vec2::new(1.0, 2.0));
        let start = Vec2::new(3.0, -1.0);
        assert_vec_close(line.get_start(), start);
        assert_vec_close(line.get_end(), start);
        assert_vec_close(line.get_unit_vector(), Vec2::ZERO);
        assert_close(line.get_length(), 0.0);
        assert_close(line.get_angle(), 0.0);
        assert_vec_close(line.get_projection(Vec2::new(9.0, 9.0)), start);
    }

    #[test]
    fn snapshot_helpers_reject_non_line_geometry() {
        let snapshot = crate::legacy::Circle::default().into_snapshot();
        assert_eq!(line_start_from_snapshot(&snapshot), None);
        assert_eq!(line_end_from_snapshot(&snapshot), None);
        assert_eq!(line_vector_from_snapshot(&snapshot), None);
        assert_eq!(line_length_from_snapshot(&snapshot), None);
        assert_eq!(line_angle_from_snapshot(&snapshot), None);
        assert_eq!(line_unit_vector_from_snapshot(&snapshot), None);
        assert_eq!(line_projection_from_snapshot(&snapshot, Vec2::ZERO), None);

        assert!(matches!(snapshot.geometry, GeometryRef::Circle { .. }));
    }
}
