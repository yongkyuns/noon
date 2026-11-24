//! Deterministic authoring of sketch outlines as ordinary retained paths.
use crate::{
    AuthoringError, ManimGeometryOptions, DEFAULT_ARROW_TIP_LENGTH, DEFAULT_ARROW_TIP_LENGTH_RATIO,
};
use noon_core::{PathCommand, Vec2, VectorPath};
use noon_geometry::{arrow_tip_vertices, ArrowGeometry};

/// The planar primitives supported by the hand-drawn path constructor.
#[derive(Clone, Copy, Debug)]
pub enum HandDrawnShape {
    Line { start: (f64, f64), end: (f64, f64) },
    Arrow { start: (f64, f64), end: (f64, f64) },
    Rectangle { width: f64, height: f64 },
    Circle { radius: f64 },
}

impl ManimGeometryOptions {
    /// Create a path with seeded, double-stroked contours. Roughness is in
    /// `0..=2`; zero produces a single clean outline. Closed shapes can use any
    /// ordinary fill, including [`crate::HatchPaint`].
    pub fn hand_drawn(
        shape: HandDrawnShape,
        roughness: f64,
        seed: u32,
    ) -> Result<Self, AuthoringError> {
        if !(0.0..=2.0).contains(&roughness) {
            return Err(AuthoringError::InvalidHandDrawnOptions(
                "hand-drawn roughness must be between 0 and 2",
            ));
        }
        let (outline, size, closed) = shape.outline()?;
        let path = if roughness == 0.0 || size == 0.0 {
            outline
        } else {
            // Cap the local offset for long lines and scale it down on small shapes.
            let amplitude = size.min(1.5) * roughness as f32 * 0.085;
            let mut random = SketchRandom(seed);
            let first = draw_pass(&outline, VectorPath::new(), closed, amplitude, &mut random);
            draw_pass(&outline, first, closed, -amplitude, &mut random)
        };
        let mut options = Self::path(path)?;
        options.set_stroke_cap("round")?;
        options.set_stroke_join("round")?;
        Ok(options)
    }
}

impl HandDrawnShape {
    fn outline(self) -> Result<(VectorPath, f32, bool), AuthoringError> {
        // Use the same input checks and canonical contours as the regular shapes.
        Ok(match self {
            Self::Rectangle { width, height } => (
                ManimGeometryOptions::rectangle(width, height)?.hand_drawn_outline(),
                width.min(height) as f32,
                true,
            ),
            Self::Circle { radius } => (
                ManimGeometryOptions::circle(radius)?.hand_drawn_outline(),
                radius as f32,
                true,
            ),
            Self::Line { start, end } | Self::Arrow { start, end } => {
                let mut path = ManimGeometryOptions::line(start.0, start.1, end.0, end.1)?
                    .hand_drawn_outline();
                let arrow = ArrowGeometry::between(
                    start,
                    end,
                    0.0,
                    DEFAULT_ARROW_TIP_LENGTH,
                    DEFAULT_ARROW_TIP_LENGTH_RATIO,
                    false,
                );
                if matches!(self, Self::Arrow { .. }) && arrow.visible_length > 0.0 {
                    let [apex, left, right] =
                        arrow_tip_vertices(end, arrow.direction, arrow.tip_length)
                            .map(|(x, y)| Vec2::new(x as f32, y as f32));
                    path = path.move_to(left).line_to(apex).line_to(right);
                }
                (path, arrow.visible_length as f32, false)
            }
        })
    }
}

fn draw_pass(
    outline: &VectorPath,
    mut path: VectorPath,
    closed: bool,
    amplitude: f32,
    random: &mut SketchRandom,
) -> VectorPath {
    let mut start = Vec2::ZERO;
    let mut start_offset = Vec2::ZERO;
    let mut point = Vec2::ZERO;
    let mut offset = Vec2::ZERO;
    for command in outline.commands() {
        path = match *command {
            PathCommand::MoveTo { to } => {
                start = to;
                point = to;
                start_offset = if closed {
                    random.point(amplitude * 0.45)
                } else {
                    Vec2::ZERO
                };
                offset = start_offset;
                path.move_to(to + offset)
            }
            PathCommand::LineTo { to } => {
                let next = if closed {
                    random.point(amplitude * 0.45)
                } else {
                    Vec2::ZERO
                };
                let result = bow_line(path, point + offset, to + next, amplitude, random);
                point = to;
                offset = next;
                result
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let next = if to == start {
                    start_offset
                } else {
                    random.point(amplitude * 0.45)
                };
                let result = path.cubic_to(
                    control1 + offset + random.point(amplitude),
                    control2 + next + random.point(amplitude),
                    to + next,
                );
                point = to;
                offset = next;
                result
            }
            PathCommand::Close => {
                if point != start {
                    path = bow_line(
                        path,
                        point + offset,
                        start + start_offset,
                        amplitude,
                        random,
                    );
                }
                path.close()
            }
            PathCommand::QuadraticTo { .. } => {
                unreachable!("primitive outlines use lines or cubics")
            }
        };
    }
    path
}

fn bow_line(
    path: VectorPath,
    from: Vec2,
    to: Vec2,
    amplitude: f32,
    random: &mut SketchRandom,
) -> VectorPath {
    let delta = to - from;
    let length = delta.x.hypot(delta.y);
    if length == 0.0 {
        return path.line_to(to);
    }
    let normal = Vec2::new(-delta.y / length, delta.x / length);
    let amplitude = amplitude.clamp(-length * 0.18, length * 0.18);
    let bow = amplitude * (0.4 + 0.6 * random.signed());
    let skew = amplitude * (0.35 + 0.35 * random.signed());
    path.cubic_to(
        from + delta * (1.0 / 3.0) + normal * (bow + skew),
        to - delta * (1.0 / 3.0) + normal * (bow - skew),
        to,
    )
}

struct SketchRandom(u32);

impl SketchRandom {
    fn signed(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }

    fn point(&mut self, amplitude: f32) -> Vec2 {
        Vec2::new(self.signed(), self.signed()) * amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_fixes_both_strokes_for_every_supported_shape() {
        for shape in [
            HandDrawnShape::Line {
                start: (0.0, 0.0),
                end: (2.0, 2.0),
            },
            HandDrawnShape::Arrow {
                start: (0.0, 0.0),
                end: (2.0, 2.0),
            },
            HandDrawnShape::Rectangle {
                width: 2.0,
                height: 1.0,
            },
            HandDrawnShape::Circle { radius: 1.0 },
        ] {
            let path = |roughness, seed| {
                ManimGeometryOptions::hand_drawn(shape, roughness, seed)
                    .unwrap()
                    .hand_drawn_outline()
            };
            let sketch = path(1.0, 0);
            assert!(sketch.is_finite() && sketch.has_drawable_segments());
            assert_eq!(sketch, path(1.0, 0));
            assert_ne!(sketch, path(1.0, 1));
            let clean = path(0.0, 0);
            assert_eq!(clean, shape.outline().unwrap().0);
            let subpaths = |path: &VectorPath| {
                path.commands()
                    .iter()
                    .filter(|cmd| matches!(cmd, PathCommand::MoveTo { .. }))
                    .count()
            };
            assert_eq!(subpaths(&sketch), 2 * subpaths(&clean));
            assert!(ManimGeometryOptions::hand_drawn(shape, f64::NAN, 0).is_err());
        }
    }
}
