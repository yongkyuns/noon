//! Shared Manim-compatible Arrow observations over retained semantic components.
//!
//! These queries deliberately read the authoritative Arrow leaves rather than
//! reconstructing geometry in a frontend. They therefore remain correct after
//! ordinary shared family translation/rotation/scale operations and provide the
//! observation seam needed by later Arrow-specific dependent mutations.

use crate::{AuthoringError, ManimArrow, ManimLineEndpoints};

impl ManimArrow {
    /// Public Arrow endpoints in world space, including retained tips.
    pub fn manim_endpoints(&self) -> Result<ManimLineEndpoints, AuthoringError> {
        let start = match self.start_tip() {
            Some(tip) => tip.manim_line_endpoints()?.start,
            None => self.shaft().manim_line_endpoints()?.start,
        };
        // Arrow tip paths retain their public apex as the first path point.
        let end = self.end_tip().manim_line_endpoints()?.start;
        Ok(ManimLineEndpoints { start, end })
    }

    /// Equivalent to ManimCE `Line.get_vector()` projected into Noon's 2D scene.
    pub fn manim_vector(&self) -> Result<(f64, f64), AuthoringError> {
        let endpoints = self.manim_endpoints()?;
        Ok((
            endpoints.end.0 - endpoints.start.0,
            endpoints.end.1 - endpoints.start.1,
        ))
    }

    /// Equivalent to ManimCE `Line.get_length()` for the retained Arrow family.
    pub fn manim_length(&self) -> Result<f64, AuthoringError> {
        let (x, y) = self.manim_vector()?;
        Ok(x.hypot(y))
    }

    /// Equivalent to ManimCE `Line.get_unit_vector()`; a degenerate Arrow
    /// returns the zero vector, matching Manim's `normalize` fallback.
    pub fn manim_unit_vector(&self) -> Result<(f64, f64), AuthoringError> {
        let (x, y) = self.manim_vector()?;
        let length = x.hypot(y);
        if length == 0.0 {
            Ok((0.0, 0.0))
        } else {
            Ok((x / length, y / length))
        }
    }

    /// Equivalent to ManimCE `Line.get_angle()` in the XY plane.
    pub fn manim_angle(&self) -> Result<f64, AuthoringError> {
        let (x, y) = self.manim_vector()?;
        Ok(y.atan2(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManimArrowOptions, Scene};
    use std::rc::Rc;

    fn arrow(scene: &Scene) -> ManimArrow {
        let mut options = ManimArrowOptions::arrow(-2.0, -1.0, 2.0, 1.0).unwrap();
        options.set_buff(0.25).unwrap();
        ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap()
    }

    #[test]
    fn shared_queries_observe_public_tip_endpoints() {
        let scene = Scene::new();
        let arrow = arrow(&scene);
        let endpoints = arrow.manim_endpoints().unwrap();
        let dx = 4.0_f64;
        let dy = 2.0_f64;
        let raw_length = dx.hypot(dy);
        let expected_length = raw_length - 0.5;
        let direction = (dx / raw_length, dy / raw_length);

        assert!((endpoints.start.0 - (-2.0 + direction.0 * 0.25)).abs() < 1.0e-6);
        assert!((endpoints.start.1 - (-1.0 + direction.1 * 0.25)).abs() < 1.0e-6);
        assert!((endpoints.end.0 - (2.0 - direction.0 * 0.25)).abs() < 1.0e-6);
        assert!((endpoints.end.1 - (1.0 - direction.1 * 0.25)).abs() < 1.0e-6);
        assert!((arrow.manim_length().unwrap() - expected_length).abs() < 1.0e-6);
        assert!((arrow.manim_angle().unwrap() - dy.atan2(dx)).abs() < 1.0e-6);
        let unit = arrow.manim_unit_vector().unwrap();
        assert!((unit.0 - direction.0).abs() < 1.0e-6);
        assert!((unit.1 - direction.1).abs() < 1.0e-6);
    }

    #[test]
    fn queries_follow_shared_family_affine_edits_without_frontend_reconstruction() {
        let scene = Scene::new();
        let arrow = arrow(&scene);
        let before = arrow.manim_length().unwrap();

        arrow.family().scale(2.0, 2.0).unwrap();
        arrow
            .family()
            .rotate(std::f64::consts::FRAC_PI_2, crate::ManimRotationPivot::Center)
            .unwrap();

        assert!((arrow.manim_length().unwrap() - before * 2.0).abs() < 2.0e-6);
        let expected_angle = 2.0_f64.atan2(4.0) + std::f64::consts::FRAC_PI_2;
        assert!((arrow.manim_angle().unwrap() - expected_angle).abs() < 2.0e-6);
    }

    #[test]
    fn degenerate_arrow_unit_vector_matches_manim_zero_fallback() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(1.0, 2.0, 1.0, 2.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();

        assert_eq!(arrow.manim_unit_vector().unwrap(), (0.0, 0.0));
        assert_eq!(arrow.manim_angle().unwrap(), 0.0);
        assert_eq!(arrow.manim_length().unwrap(), 0.0);
    }
}
