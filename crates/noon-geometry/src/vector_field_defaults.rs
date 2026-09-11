use crate::{
    plan_static_arrow_vector_field, VectorFieldAxisRange, VectorFieldPoint, VectorFieldRanges2D,
};
use noon_core::{DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH};

/// Pinned ManimCE v0.21 default 2D ArrowVectorField ranges for Noon's default frame.
///
/// Manim derives omitted x/y ranges from `floor(-frame / 2)` and
/// `ceil(frame / 2)`, then inserts the ordinary 0.5 vector-field step. Keeping
/// this derivation in shared Rust prevents language frontends from owning frame
/// geometry or a second sampling contract.
pub fn manim_default_vector_field_ranges_2d() -> VectorFieldRanges2D {
    let frame_width = f64::from(DEFAULT_FRAME_WIDTH);
    let frame_height = f64::from(DEFAULT_FRAME_HEIGHT);
    VectorFieldRanges2D::new(
        VectorFieldAxisRange::with_default_step(
            (-frame_width / 2.0).floor(),
            (frame_width / 2.0).ceil(),
        ),
        VectorFieldAxisRange::with_default_step(
            (-frame_height / 2.0).floor(),
            (frame_height / 2.0).ceil(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_frame_ranges_match_pinned_manim_grid() {
        let ranges = manim_default_vector_field_ranges_2d();
        assert_eq!(ranges.x, VectorFieldAxisRange::new(-8.0, 8.0, 0.5));
        assert_eq!(ranges.y, VectorFieldAxisRange::new(-4.0, 4.0, 0.5));

        let plan = plan_static_arrow_vector_field(|_| VectorFieldPoint::ZERO, ranges).unwrap();
        assert_eq!(plan.x_samples, 33);
        assert_eq!(plan.y_samples, 17);
        assert_eq!(plan.samples.len(), 561);
        assert_eq!(plan.samples.first().unwrap().point, VectorFieldPoint::new(-8.0, -4.0));
        assert_eq!(plan.samples.last().unwrap().point, VectorFieldPoint::new(8.0, 4.0));
    }
}
