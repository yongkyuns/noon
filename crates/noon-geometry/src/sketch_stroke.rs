//! A deterministic foreground stroke derived from an ordinary vector contour.
use noon_core::{PathCommand, SketchStroke, Vec2, VectorPath};

const MAX_ROUGHNESS_SCALE: f32 = 1.5;
const DISPLACEMENT: f32 = 0.085;

/// A conservative local-space radial bound on the perturbation of each control point.
pub fn sketch_outset(stroke: SketchStroke) -> f32 {
    // Allow for both the displaced anchor and asymmetric bow controls.
    stroke.roughness() * MAX_ROUGHNESS_SCALE * DISPLACEMENT * 3.0
}

pub fn sketch_outline(path: &VectorPath, stroke: SketchStroke) -> VectorPath {
    let Some(bounds) = path.conservative_bounds() else {
        return path.clone();
    };
    let extent = bounds.max - bounds.min;
    let closed = path
        .commands()
        .iter()
        .any(|command| matches!(command, PathCommand::Close));
    let size = if closed {
        extent.x.min(extent.y)
    } else {
        extent.x.hypot(extent.y)
    };
    let amplitude = size.min(MAX_ROUGHNESS_SCALE) * stroke.roughness() * DISPLACEMENT;
    if amplitude == 0.0 {
        return path.clone();
    }
    let mut random = SketchRandom(stroke.seed());
    let first = draw_pass(path, VectorPath::new(), closed, amplitude, &mut random);
    draw_pass(path, first, closed, -amplitude, &mut random)
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
            PathCommand::QuadraticTo { control, to } => {
                let next = if closed {
                    random.point(amplitude * 0.45)
                } else {
                    Vec2::ZERO
                };
                let result = path.quadratic_to(control + random.point(amplitude), to + next);
                point = to;
                offset = next;
                result
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
