use crate::FrontendMobjectHandle;

impl FrontendMobjectHandle {
    /// Apply relative Manim scaling while preserving the mobject's world-space center.
    ///
    /// Noon's lower-level `scale` operation intentionally remains origin-space. Manim's
    /// `Mobject.scale` instead uses the current mobject center as its default pivot, so
    /// off-origin geometry needs a compensating translation after the relative scale.
    /// Keeping that pivot rule here gives every frontend one shared semantic operation
    /// without baking Manim behavior into the native transform primitive.
    pub fn manim_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let center = self.center();
        self.scale(x, y)?;
        let scaled_center = self.center();
        self.shift(center.0 - scaled_center.0, center.1 - scaled_center.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1.0e-6;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn manim_scale_preserves_center_for_off_origin_rotated_geometry() {
        let mut line = FrontendMobjectHandle::manim_line(1.0, 2.0, 5.0, 4.0).unwrap();
        line.shift(0.75, -1.25).unwrap();
        line.rotate(0.37).unwrap();

        let center = line.center();
        let width = line.width();
        let height = line.height();

        line.manim_scale(1.75, 1.75).unwrap();

        let scaled_center = line.center();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);
        assert_close(line.width(), width * 1.75);
        assert_close(line.height(), height * 1.75);
    }

    #[test]
    fn manim_scale_preserves_center_for_non_uniform_scale() {
        let mut line = FrontendMobjectHandle::manim_line(-2.0, 1.0, 3.0, 5.0).unwrap();
        line.shift(-0.5, 0.25).unwrap();
        line.rotate(-0.2).unwrap();
        let center = line.center();

        line.manim_scale(2.0, 0.5).unwrap();

        let scaled_center = line.center();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);
    }

    #[test]
    fn native_relative_scale_keeps_origin_space_contract() {
        let mut line = FrontendMobjectHandle::manim_line(1.0, 2.0, 5.0, 4.0).unwrap();
        let center = line.center();

        line.scale(2.0, 2.0).unwrap();

        let scaled_center = line.center();
        assert!((scaled_center.0 - center.0).abs() > EPSILON);
        assert!((scaled_center.1 - center.1).abs() > EPSILON);
    }
}
