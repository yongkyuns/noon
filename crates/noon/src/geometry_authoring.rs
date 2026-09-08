//! The retained path used by the shared Manim Triangle constructor.
use noon_core::{Vec2, VectorPath, TAU};

pub(crate) fn manim_triangle_path() -> VectorPath {
    let vertices: Vec<_> = (0..3)
        .map(|index| {
            let angle = TAU / 4.0 + index as f32 * (TAU / 3.0);
            Vec2::new(angle.cos(), angle.sin())
        })
        .collect();
    VectorPath::new()
        .move_to(vertices[0])
        .line_to(vertices[1])
        .line_to(vertices[2])
        .close()
}
