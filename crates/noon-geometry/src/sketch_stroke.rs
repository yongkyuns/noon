//! A deterministic foreground stroke derived from an ordinary vector contour.
//! The line and Bézier jitter and PRNG are adapted from RoughJS 4.6.4 (MIT).

use noon_core::{PathCommand, SketchStroke, Vec2, VectorPath};

const MAX_ROUGHNESS_SCALE: f32 = 1.5;
const DISPLACEMENT: f32 = 0.045;

/// A conservative local-space radial bound on the perturbation of each control point.
pub fn sketch_outset(stroke: SketchStroke) -> f32 {
    sketch_outset_for_extent(stroke, MAX_ROUGHNESS_SCALE)
}

pub fn sketch_outset_for_extent(stroke: SketchStroke, extent: f32) -> f32 {
    // Maximum radial jitter is sqrt(2) times the offset. The bounded
    // RoughJS line bow adds at most 1.15 times the offset.
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
        let size = if matches!(segments.last(), Some(PathCommand::Close)) {
            extent.x.min(extent.y)
        } else {
            extent.x.hypot(extent.y)
        };
        // Map RoughJS's default two-pixel offset to the contour's local units.
        let offset = size.min(MAX_ROUGHNESS_SCALE) * DISPLACEMENT;
        let mut random = SketchRandom::new(stroke.seed().wrapping_add(index as u32));
        let roughness = stroke.roughness();
        outline = draw_pass(
            outline,
            *start,
            segments,
            offset,
            roughness,
            false,
            &mut random,
        );
        if offset > 0.0 {
            outline = draw_pass(
                outline,
                *start,
                segments,
                offset,
                roughness,
                true,
                &mut random,
            );
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

    let mut random = SketchRandom::new(stroke.seed());
    let tilt = random.signed();
    let skew = axis * (roughness * (0.12 + 0.06 * tilt.abs()).copysign(tilt));
    let first = first + skew;
    let second = second - skew;
    let offset = length * 0.16;
    path = rough_line(path, apex, first, offset, roughness, false, &mut random).line_to(second);
    rough_line(path, second, apex, offset, roughness, false, &mut random).close()
}

fn draw_pass(
    mut path: VectorPath,
    start: Vec2,
    segments: &[PathCommand],
    offset: f32,
    roughness: f32,
    overlay: bool,
    random: &mut SketchRandom,
) -> VectorPath {
    // Keep each pass connected at the authored vertices for joins and reveals.
    let curve_jitter = offset * roughness * if overlay { 1.15 } else { 1.0 };
    path = path.move_to(start);
    let mut point = start;
    for &command in segments {
        let (next_path, end) = match command {
            PathCommand::MoveTo { .. } => unreachable!("contour contains only one move"),
            PathCommand::LineTo { to } => (
                rough_line(path, point, to, offset, roughness, overlay, random),
                to,
            ),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => (
                path.cubic_to(
                    control1 + random.point(curve_jitter),
                    control2 + random.point(curve_jitter),
                    to,
                ),
                to,
            ),
            PathCommand::Close => {
                if point != start {
                    path = rough_line(path, point, start, offset, roughness, overlay, random);
                }
                (path.close(), start)
            }
            PathCommand::QuadraticTo { control, to } => (
                path.cubic_to(
                    point + (control - point) * (2.0 / 3.0) + random.point(curve_jitter),
                    to + (control - to) * (2.0 / 3.0) + random.point(curve_jitter),
                    to,
                ),
                to,
            ),
        };
        path = next_path;
        point = end;
    }
    path
}

fn rough_line(
    path: VectorPath,
    from: Vec2,
    to: Vec2,
    offset: f32,
    roughness: f32,
    overlay: bool,
    random: &mut SketchRandom,
) -> VectorPath {
    let delta = to - from;
    let length = delta.x.hypot(delta.y);
    if length == 0.0 || offset == 0.0 {
        return path.line_to(to);
    }
    let drawing_length = length / (offset * 0.5);
    let gain = if drawing_length < 200.0 {
        1.0
    } else if drawing_length > 500.0 {
        0.4
    } else {
        1.233_334 - 0.001_666_8 * drawing_length
    };
    let roughness = roughness * gain;
    let jitter = offset.min(length * 0.1) * roughness * if overlay { 0.5 } else { 1.0 };
    let diverge = 0.3 + random.signed() * 0.1;
    // Bound the bow for very long world-space paths so spatial culling can use
    // the same conservative envelope for any contour.
    let bow_scale = 0.01 * roughness * (500.0 / drawing_length).min(1.0);
    let bow = Vec2::new(delta.y * random.signed(), -delta.x * random.signed()) * bow_scale;
    path.cubic_to(
        from + delta * diverge + bow + random.point(jitter),
        from + delta * (2.0 * diverge) + bow + random.point(jitter),
        to,
    )
}

struct SketchRandom(u32);

impl SketchRandom {
    fn new(seed: u32) -> Self {
        // RoughJS uses entropy at seed zero. Noon needs repeatable geometry for
        // seek and export, including when authors use the default seed.
        const SEED_MIX: u32 = 0x61c8_8647;
        let folded = (seed & 0x7fff_ffff) ^ ((seed >> 31) * SEED_MIX);
        Self(if folded == 0 { 0x2c92_77b5 } else { folded })
    }

    fn signed(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(48_271);
        (self.0 & 0x7fff_ffff) as f32 / 1_073_741_824.0 - 1.0
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
    fn sketch_single_segments_match_roughjs_4_6_4() {
        let start = Vec2::ZERO;
        let end = Vec2::new(2.0, 1.0);
        let line = VectorPath::new().move_to(start).line_to(end);
        let cubic = VectorPath::new().move_to(start).cubic_to(
            Vec2::new(0.3, 1.2),
            Vec2::new(1.4, 0.2),
            end,
        );
        let default = sketch_outline(&line, SketchStroke::new(1.3, 0).unwrap());
        assert_ne!(
            default,
            sketch_outline(&line, SketchStroke::new(1.3, 1).unwrap())
        );
        assert_ne!(
            default,
            sketch_outline(&line, SketchStroke::new(1.3, 1 << 31).unwrap())
        );
        let stroke = SketchStroke::new(1.3, 17).unwrap();
        // RoughJS line() and path(), with preserveVertices and coordinates
        // converted at 1.5 * 0.045 / 2 local units per drawing unit.
        for (path, expected) in [
            (
                line,
                [
                    [
                        Vec2::new(0.336_148_6, 0.231_439_8),
                        Vec2::new(0.735_893_7, 0.404_659_1),
                    ],
                    [
                        Vec2::new(0.409_423_1, 0.270_912_7),
                        Vec2::new(0.891_623_8, 0.438_180_2),
                    ],
                ],
            ),
            (
                cubic,
                [
                    [
                        Vec2::new(0.212_317_1, 1.190_444),
                        Vec2::new(1.351_313, 0.137_411_4),
                    ],
                    [
                        Vec2::new(0.319_478_2, 1.127_554),
                        Vec2::new(1.388_592, 0.123_200_4),
                    ],
                ],
            ),
        ] {
            let outline = sketch_outline(&path, stroke);
            assert_eq!(outline.commands().len(), 4);
            for (pass, [first, second]) in
                outline.commands().as_chunks::<2>().0.iter().zip(expected)
            {
                let [PathCommand::MoveTo { to: from }, PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                }] = *pass
                else {
                    panic!("expected two anchored cubic strokes");
                };
                assert_eq!((from, to), (start, end));
                assert!((control1 - first).length() < 1e-6);
                assert!((control2 - second).length() < 1e-6);
            }
        }
    }

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
