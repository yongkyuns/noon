//! Cairo fixed-path construction in the retained layout coordinate space.
use noon_core::{Vec2, VectorPath};
use skrifa::outline::pen::PathElement;
type Point = [i32; 2];
#[derive(Clone, Copy)]
enum FixedCommand {
    Move(Point),
    Line(Point),
    Cubic(Point, Point, Point),
    Close,
}
impl FixedCommand {
    fn end(self) -> Point {
        match self {
            Self::Move(p) | Self::Line(p) | Self::Cubic(_, _, p) => p,
            Self::Close => unreachable!("closed contour cannot precede an emitted line"),
        }
    }
}
#[derive(Default)]
struct Builder {
    commands: Vec<FixedCommand>,
    current: Option<Point>,
    start: Point,
    pending_move: bool,
}
impl Builder {
    fn apply_move(&mut self) {
        if self.pending_move {
            self.commands
                .push(FixedCommand::Move(self.current.unwrap()));
            self.pending_move = false;
        }
    }
    fn previous_end(&self) -> Point {
        self.commands[self.commands.len() - 2].end()
    }
    fn move_to(&mut self, p: Point) {
        self.current = Some(p);
        self.start = p;
        self.pending_move = true;
    }
    fn line_to(&mut self, p: Point) {
        let Some(current) = self.current else {
            self.move_to(p);
            return;
        };
        self.apply_move();
        if !matches!(self.commands.last(), Some(FixedCommand::Move(_))) && p == current {
            return;
        }
        if matches!(self.commands.last(), Some(FixedCommand::Line(_))) {
            let previous = self.previous_end();
            let a = [
                i64::from(current[0]) - i64::from(previous[0]),
                i64::from(current[1]) - i64::from(previous[1]),
            ];
            let b = [
                i64::from(p[0]) - i64::from(current[0]),
                i64::from(p[1]) - i64::from(current[1]),
            ];
            if previous == current
                || (i128::from(a[0]) * i128::from(b[1]) == i128::from(a[1]) * i128::from(b[0])
                    && i128::from(a[0]) * i128::from(b[0]) + i128::from(a[1]) * i128::from(b[1])
                        >= 0)
            {
                self.commands.pop();
            }
        }
        self.commands.push(FixedCommand::Line(p));
        self.current = Some(p);
    }
    fn curve_to(&mut self, a: Point, b: Point, p: Point) {
        if self.current == Some(p) && p == a && a == b {
            self.line_to(p);
            return;
        }
        if self.current.is_none() {
            self.move_to(a);
        }
        self.apply_move();
        if matches!(self.commands.last(), Some(FixedCommand::Line(_)))
            && self.previous_end() == self.current.unwrap()
        {
            self.commands.pop();
        }
        self.commands.push(FixedCommand::Cubic(a, b, p));
        self.current = Some(p);
    }
    fn close(&mut self) {
        if self.current.is_some() {
            self.line_to(self.start);
            if matches!(self.commands.last(), Some(FixedCommand::Line(_))) {
                self.commands.pop();
            }
            self.commands.push(FixedCommand::Close);
            self.pending_move = true;
        }
    }
}
fn point(x: f32, y: f32) -> Point {
    [
        (f64::from(x) * 256.0).round() as i32,
        (f64::from(y) * 256.0).round() as i32,
    ]
}
fn native(p: Point) -> Vec2 {
    Vec2::new(
        (f64::from(p[0]) * 3.6 / 256.0) as f32,
        (-f64::from(p[1]) * 3.6 / 256.0) as f32,
    )
}
pub fn native_path(commands: &[PathElement]) -> VectorPath {
    path_on_grid(commands, None)
}

pub fn coretext_path(commands: &[PathElement], scale: f64) -> VectorPath {
    path_on_grid(commands, Some(scale))
}

fn path_on_grid(commands: &[PathElement], coretext_scale: Option<f64>) -> VectorPath {
    // Cairo's double-to-24.8 conversion rounds ties to even. Its magic-number
    // conversion also preserves the exact near-tie behavior of native Cairo.
    const FIXED_MAGIC: f64 = (1_u64 << (52 - 8)) as f64 * 1.5;
    let coretext_point = |x: f64, y: f64| -> Point {
        [
            ((x + FIXED_MAGIC).to_bits() as u32) as i32,
            ((y + FIXED_MAGIC).to_bits() as u32) as i32,
        ]
    };
    let point = |x: f32, y: f32| -> Point {
        if let Some(scale) = coretext_scale {
            coretext_point(f64::from(x) * scale, -f64::from(y) * scale)
        } else {
            point(x, y)
        }
    };
    let mut b = Builder::default();
    for command in commands {
        match *command {
            PathElement::MoveTo { x, y } => b.move_to(point(x, y)),
            PathElement::LineTo { x, y } => b.line_to(point(x, y)),
            PathElement::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => b.curve_to(point(cx0, cy0), point(cx1, cy1), point(x, y)),
            PathElement::QuadTo { cx0, cy0, x, y } => {
                if let Some(scale) = coretext_scale {
                    // Quartz converts a floating quadratic after reading Cairo's
                    // already-quantized current point; quantizing its control first
                    // would instead produce the FreeType recipe below.
                    let current = b.current.expect("quadratic must have a current point");
                    let (cx, cy) = (f64::from(cx0) * scale, -f64::from(cy0) * scale);
                    let (ex, ey) = (f64::from(x) * scale, -f64::from(y) * scale);
                    b.curve_to(
                        coretext_point(
                            (f64::from(current[0]) / 256.0 + 2.0 * cx) / 3.0,
                            (f64::from(current[1]) / 256.0 + 2.0 * cy) / 3.0,
                        ),
                        coretext_point((2.0 * cx + ex) / 3.0, (2.0 * cy + ey) / 3.0),
                        coretext_point(ex, ey),
                    );
                    continue;
                }
                let current = b.current.expect("quadratic must have a current point");
                let control = point(cx0, cy0);
                let end = point(x, y);
                let first = std::array::from_fn(|i| {
                    (f64::from(current[i])
                        + (2.0 / 3.0) * (f64::from(control[i]) - f64::from(current[i])))
                    .trunc() as i32
                });
                let second = std::array::from_fn(|i| {
                    (f64::from(end[i]) + (2.0 / 3.0) * (f64::from(control[i]) - f64::from(end[i])))
                        .trunc() as i32
                });
                b.curve_to(first, second, end);
            }
            PathElement::Close => {
                // CoreGraphics omits contours containing only a move and close.
                if coretext_scale.is_some() && b.pending_move {
                    b.current = None;
                    b.pending_move = false;
                } else {
                    b.close();
                }
            }
        }
    }
    let mut path = VectorPath::new();
    for command in b.commands {
        path = match command {
            FixedCommand::Move(p) => path.move_to(native(p)),
            FixedCommand::Line(p) => path.line_to(native(p)),
            FixedCommand::Cubic(a, b, p) => path.cubic_to(native(a), native(b), native(p)),
            FixedCommand::Close => path.close(),
        };
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{Vec2, VectorPath};
    use skrifa::outline::pen::PathElement::{self, Close, CurveTo, LineTo, MoveTo, QuadTo};
    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new((f64::from(x) * 3.6) as f32, (-f64::from(y) * 3.6) as f32)
    }
    #[test]
    fn duplicate_and_forward_lines_compact_but_backtracking_and_turns_remain() {
        let cases: Vec<(Vec<PathElement>, VectorPath)> = vec![
            (
                vec![
                    MoveTo { x: 0., y: 0. },
                    LineTo { x: 0., y: 0. },
                    LineTo { x: 1., y: 0. },
                ],
                VectorPath::new().move_to(v(0., 0.)).line_to(v(1., 0.)),
            ),
            (
                vec![
                    MoveTo { x: 0., y: 0. },
                    LineTo { x: 1., y: 0. },
                    LineTo { x: 2., y: 0. },
                ],
                VectorPath::new().move_to(v(0., 0.)).line_to(v(2., 0.)),
            ),
            (
                vec![
                    MoveTo { x: 0., y: 0. },
                    LineTo { x: 1., y: 0. },
                    LineTo { x: 0., y: 0. },
                ],
                VectorPath::new()
                    .move_to(v(0., 0.))
                    .line_to(v(1., 0.))
                    .line_to(v(0., 0.)),
            ),
            (
                vec![
                    MoveTo { x: 0., y: 0. },
                    LineTo { x: 1., y: 0. },
                    LineTo { x: 2., y: 1. },
                ],
                VectorPath::new()
                    .move_to(v(0., 0.))
                    .line_to(v(1., 0.))
                    .line_to(v(2., 1.)),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(native_path(&input), expected);
        }
    }
    #[test]
    fn duplicate_curve_end_and_close_line_do_not_add_commands() {
        let curve = CurveTo {
            cx0: 1.,
            cy0: 0.,
            cx1: 1.,
            cy1: 1.,
            x: 0.,
            y: 1.,
        };
        let expected =
            VectorPath::new()
                .move_to(v(0., 0.))
                .cubic_to(v(1., 0.), v(1., 1.), v(0., 1.));
        assert_eq!(
            native_path(&[MoveTo { x: 0., y: 0. }, curve, LineTo { x: 0., y: 1. }]),
            expected
        );
        assert_eq!(
            native_path(&[
                MoveTo { x: 0., y: 0. },
                curve,
                LineTo { x: 0., y: 0. },
                Close
            ]),
            expected.close()
        );
    }
    #[test]
    fn pending_moves_and_contour_boundaries_are_independent() {
        assert_eq!(native_path(&[MoveTo { x: 0., y: 0. }]), VectorPath::new());
        assert_eq!(
            native_path(&[
                MoveTo { x: 0., y: 0. },
                MoveTo { x: 1., y: 1. },
                LineTo { x: 2., y: 2. }
            ]),
            VectorPath::new().move_to(v(1., 1.)).line_to(v(2., 2.))
        );
        let input = [
            MoveTo { x: 0., y: 0. },
            LineTo { x: 1., y: 0. },
            Close,
            MoveTo { x: 2., y: 2. },
            LineTo { x: 3., y: 2. },
            Close,
        ];
        assert_eq!(
            native_path(&input),
            VectorPath::new()
                .move_to(v(0., 0.))
                .line_to(v(1., 0.))
                .close()
                .move_to(v(2., 2.))
                .line_to(v(3., 2.))
                .close()
        );
    }
    #[test]
    fn negative_quadratic_cubic_controls_truncate_on_the_fixed_grid() {
        let input = [
            MoveTo {
                x: -1. / 64.,
                y: -1. / 64.,
            },
            QuadTo {
                cx0: 0.,
                cy0: 0.,
                x: 1. / 64.,
                y: 1. / 64.,
            },
        ];
        let expected = VectorPath::new()
            .move_to(Vec2::new(-0.05625, 0.05625))
            .cubic_to(
                Vec2::new(-0.0140625, 0.0140625),
                Vec2::new(0.0140625, -0.0140625),
                Vec2::new(0.05625, -0.05625),
            );
        assert_eq!(native_path(&input), expected);
    }

    #[test]
    fn valid_fixed_coordinates_can_span_more_than_a_signed_integer() {
        let path = native_path(&[
            MoveTo {
                x: -8_388_607.0,
                y: 0.0,
            },
            QuadTo {
                cx0: 8_388_607.0,
                cy0: 1.0,
                x: -8_388_607.0,
                y: 2.0,
            },
        ]);
        assert_eq!(path.commands().len(), 2);
        let bounds = path.conservative_bounds().unwrap();
        assert!(bounds.min.x.is_finite() && bounds.max.x.is_finite());
    }
}
