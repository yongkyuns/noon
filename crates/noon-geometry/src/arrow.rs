//! Shared Arrow endpoint geometry for authored and effective runtime content.

/// Geometry in semantic precision, independent of resource admission and paint.
#[derive(Clone, Copy, Debug)]
pub struct ArrowGeometry {
    pub visible_start: (f64, f64),
    pub visible_end: (f64, f64),
    pub direction: (f64, f64),
    pub visible_length: f64,
    pub tip_length: f64,
    pub shaft_start: (f64, f64),
    pub shaft_end: (f64, f64),
    stroke_cap_length: f64,
}

impl ArrowGeometry {
    /// Inputs are validated by semantic admission. Short lines retain their
    /// endpoints when applying both buffers would reverse their direction.
    pub fn between(
        start: (f64, f64),
        end: (f64, f64),
        buff: f64,
        tip_length: f64,
        max_tip_length_to_length_ratio: f64,
        start_tip: bool,
    ) -> Self {
        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let length = dx.hypot(dy);
        let direction = if length == 0.0 {
            (1.0, 0.0)
        } else {
            (dx / length, dy / length)
        };
        let (visible_start, visible_end) = if buff > 0.0 && length >= 2.0 * buff && length > 0.0 {
            (
                (start.0 + direction.0 * buff, start.1 + direction.1 * buff),
                (end.0 - direction.0 * buff, end.1 - direction.1 * buff),
            )
        } else {
            (start, end)
        };
        let visible_length =
            (visible_end.0 - visible_start.0).hypot(visible_end.1 - visible_start.1);
        let tip_length = tip_length.min(max_tip_length_to_length_ratio * visible_length);
        let shaft_end = (
            visible_end.0 - direction.0 * tip_length,
            visible_end.1 - direction.1 * tip_length,
        );
        let shaft_start = if start_tip {
            (
                visible_start.0 + direction.0 * tip_length,
                visible_start.1 + direction.1 * tip_length,
            )
        } else {
            visible_start
        };
        // Manim caps the stroke after the end tip and before an optional start tip.
        let stroke_cap_length =
            (shaft_end.0 - visible_start.0).hypot(shaft_end.1 - visible_start.1);
        Self {
            visible_start,
            visible_end,
            direction,
            visible_length,
            tip_length,
            shaft_start,
            shaft_end,
            stroke_cap_length,
        }
    }

    pub fn stroke_width(self, initial: f64, max_ratio: f64) -> f64 {
        initial.min(max_ratio * self.stroke_cap_length)
    }
}

/// Apex-first triangular tip vertices, shared by static and dependent arrows.
pub fn arrow_tip_vertices(apex: (f64, f64), direction: (f64, f64), length: f64) -> [(f64, f64); 3] {
    let base = (apex.0 - direction.0 * length, apex.1 - direction.1 * length);
    let half_width = length * 0.5;
    let perpendicular = (-direction.1, direction.0);
    [
        apex,
        (
            base.0 + perpendicular.0 * half_width,
            base.1 + perpendicular.1 * half_width,
        ),
        (
            base.0 - perpendicular.0 * half_width,
            base.1 - perpendicular.1 * half_width,
        ),
    ]
}
