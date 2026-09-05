#[cfg(test)]
use noon::Mobject;

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
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut line =
            Mobject::manim_line(std::rc::Rc::clone(&authoring_store), 1.0, 2.0, 5.0, 4.0).unwrap();
        line.shift(0.75, -1.25).unwrap();
        line.rotate(0.37).unwrap();

        let center = line.center().unwrap();
        let width = line.width().unwrap();
        let height = line.height().unwrap();

        line.manim_scale(1.75, 1.75).unwrap();

        let scaled_center = line.center().unwrap();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);
        assert_close(line.width().unwrap(), width * 1.75);
        assert_close(line.height().unwrap(), height * 1.75);
    }

    #[test]
    fn manim_scale_preserves_center_for_non_uniform_scale() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut line =
            Mobject::manim_line(std::rc::Rc::clone(&authoring_store), -2.0, 1.0, 3.0, 5.0).unwrap();
        line.shift(-0.5, 0.25).unwrap();
        line.rotate(-0.2).unwrap();
        let center = line.center().unwrap();

        line.manim_scale(2.0, 0.5).unwrap();

        let scaled_center = line.center().unwrap();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);
    }

    #[test]
    fn native_relative_scale_keeps_origin_space_contract() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut line =
            Mobject::manim_line(std::rc::Rc::clone(&authoring_store), 1.0, 2.0, 5.0, 4.0).unwrap();
        let center = line.center().unwrap();

        line.scale(2.0, 2.0).unwrap();

        let scaled_center = line.center().unwrap();
        assert!((scaled_center.0 - center.0).abs() > EPSILON);
        assert!((scaled_center.1 - center.1).abs() > EPSILON);
    }
}
