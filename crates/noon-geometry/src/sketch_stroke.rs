//! A deterministic foreground stroke derived from an ordinary vector contour.
use noon_core::{PathCommand, SketchStroke, Vec2, VectorPath};

const MAX_ROUGHNESS_SCALE: f32 = 1.5;
const DISPLACEMENT: f32 = 0.085;

/// A conservative local-space radial bound on the perturbation of each control point.
pub fn sketch_outset(stroke: SketchStroke) -> f32 {
    sketch_outset_for_extent(stroke, MAX_ROUGHNESS_SCALE)
}

pub fn sketch_outset_for_extent(stroke: SketchStroke, extent: f32) -> f32 {
    // Allow for both the displaced anchor and asymmetric bow controls.
    stroke.roughness() * extent * DISPLACEMENT * 3.0
}

pub fn sketch_outline(path: &VectorPath, stroke: SketchStroke) -> VectorPath {
    if stroke.roughness() == 0.0 || path.is_empty() {
        return path.clone();
    }
    let mut outline = VectorPath::new();
    for (index, contour) in path
        .commands()
        .chunk_by(|_, next| !matches!(next, PathCommand::MoveTo { .. }))
        .enumerate()
    {
        let Some((PathCommand::MoveTo { to: start }, segments)) = contour.split_first() else {
            return path.clone();
        };
        let bounds =
            VectorPath::conservative_command_bounds(contour).expect("contour has an anchor");
        let extent = bounds.max - bounds.min;
        let closed = matches!(segments.last(), Some(PathCommand::Close));
        let size = if closed {
            extent.x.min(extent.y)
        } else {
            extent.x.hypot(extent.y)
        };
        let amplitude = size.min(MAX_ROUGHNESS_SCALE) * stroke.roughness() * DISPLACEMENT;
        let mut random = SketchRandom(stroke.seed().wrapping_add(index as u32));
        outline = draw_pass(outline, *start, segments, closed, amplitude, &mut random);
        if amplitude != 0.0 {
            outline = draw_pass(outline, *start, segments, closed, -amplitude, &mut random);
        }
    }
    outline
}

/// A closed, filled tip with its apex and shaft attachment fixed.
pub fn sketch_arrow_tip([apex, first, second]: [Vec2; 3], stroke: SketchStroke) -> VectorPath {
    let axis = apex - (first * 0.5 + second * 0.5);
    let length = axis.x.hypot(axis.y);
    let roughness = stroke.roughness();
    let mut path = VectorPath::new().move_to(apex);
    if length == 0.0 || roughness == 0.0 {
        return path.line_to(first).line_to(second).close();
    }

    let mut random = SketchRandom(stroke.seed());
    let tilt = random.signed();
    let skew = axis * (roughness * (0.12 + 0.06 * tilt.abs()).copysign(tilt));
    let first = first + skew;
    let second = second - skew;
    let amplitude = length * (roughness * 0.16);
    path = bow_line(path, apex, first, amplitude, &mut random).line_to(second);
    bow_line(path, second, apex, -amplitude, &mut random).close()
}

fn draw_pass(
    mut path: VectorPath,
    start: Vec2,
    segments: &[PathCommand],
    closed: bool,
    amplitude: f32,
    random: &mut SketchRandom,
) -> VectorPath {
    let start_offset = if closed {
        random.point(amplitude * 0.45)
    } else {
        Vec2::ZERO
    };
    path = path.move_to(start + start_offset);
    let mut point = start;
    let mut offset = start_offset;
    let drawing_count = segments.len() - usize::from(closed);
    for (index, command) in segments.iter().enumerate() {
        let next_offset = |to, random: &mut SketchRandom| {
            if index + 1 == drawing_count {
                if !closed {
                    return Vec2::ZERO;
                }
                if to == start {
                    return start_offset;
                }
            }
            random.point(amplitude * 0.45)
        };
        path = match *command {
            PathCommand::MoveTo { .. } => unreachable!("contour contains only one move"),
            PathCommand::LineTo { to } => {
                let next = next_offset(to, random);
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
                let next = next_offset(to, random);
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
                let next = next_offset(to, random);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tessellate_styled_with_fill, tessellate_with_stroke_outline, PathSurface};
    use noon_core::{StrokeCap, StrokeJoin};

    #[test]
    fn sketch_mixed_contours_keep_open_endpoints_and_the_original_fill_mesh() {
        let start = Vec2::new(-4.0, 0.0);
        let end = Vec2::new(-2.0, 0.0);
        let path = VectorPath::new()
            .move_to(start)
            .cubic_to(Vec2::new(-3.5, 1.0), Vec2::new(-2.5, -1.0), end)
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(4.0, 0.0))
            .line_to(Vec2::new(4.0, 4.0))
            .line_to(Vec2::new(0.0, 4.0))
            .close()
            .move_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(1.0, 3.0))
            .line_to(Vec2::new(3.0, 3.0))
            .line_to(Vec2::new(3.0, 1.0))
            .close();
        let stroke = SketchStroke::new(1.1, 8).unwrap();
        let outline = sketch_outline(&path, stroke);
        assert_eq!(outline, sketch_outline(&path, stroke));
        for pass in outline.commands()[..4].as_chunks::<2>().0 {
            assert_eq!(pass[0], PathCommand::MoveTo { to: start });
            assert!(matches!(pass[1], PathCommand::CubicTo { to, .. } if to == end));
        }
        assert_eq!(
            outline
                .commands()
                .iter()
                .filter(|cmd| **cmd == PathCommand::Close)
                .count(),
            4
        );

        let fill =
            tessellate_styled_with_fill(&path, 0.0, StrokeJoin::Round, StrokeCap::Round, true)
                .unwrap();
        let sketch = tessellate_with_stroke_outline(
            &path,
            &outline,
            0.05,
            0.0,
            StrokeJoin::Round,
            StrokeCap::Round,
            true,
        )
        .unwrap();
        assert!(!fill.indices.is_empty());
        assert!(sketch.indices.starts_with(&fill.indices));
        assert!(sketch.vertices.len() > fill.vertices.len());
        for (expected, actual) in fill.vertices.iter().zip(&sketch.vertices) {
            assert_eq!(actual.surface, PathSurface::Fill);
            assert_eq!(actual.position, expected.position);
        }
    }
}
