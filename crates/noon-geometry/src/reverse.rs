//! Reversal of retained contours; no semantic or execution state is owned here.
use noon_core::{PathCommand, Vec2, VectorPath};

/// Reverse curve direction and subpath order, preserving each contour's closure.
/// Work and temporary storage are linear in the selected path only.
pub fn reverse_path(path: &VectorPath) -> VectorPath {
    struct Contour {
        start: Vec2,
        curves: Vec<(Vec2, PathCommand)>,
        closed: bool,
    }
    let mut contours: Vec<Contour> = Vec::new();
    let mut current = Vec2::ZERO;
    for &command in path.commands() {
        if let PathCommand::MoveTo { to } = command {
            contours.push(Contour {
                start: to,
                curves: Vec::new(),
                closed: false,
            });
            current = to;
            continue;
        }
        let Some(Contour {
            start,
            curves,
            closed,
        }) = contours.last_mut()
        else {
            continue;
        };
        match command {
            PathCommand::Close => {
                if current != *start {
                    curves.push((current, PathCommand::LineTo { to: *start }));
                }
                current = *start;
                *closed = true;
            }
            PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => {
                curves.push((current, command));
                current = to;
            }
            PathCommand::MoveTo { .. } => unreachable!(),
        }
    }
    let mut reversed = VectorPath::new();
    for Contour {
        start,
        curves,
        closed,
    } in contours.into_iter().rev()
    {
        let end = curves.last().map_or(start, |(_, command)| match *command {
            PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => to,
            _ => unreachable!(),
        });
        reversed = reversed.move_to(end);
        for (from, command) in curves.into_iter().rev() {
            reversed = match command {
                PathCommand::LineTo { .. } => reversed.line_to(from),
                PathCommand::QuadraticTo { control, .. } => reversed.quadratic_to(control, from),
                PathCommand::CubicTo {
                    control1, control2, ..
                } => reversed.cubic_to(control2, control1, from),
                _ => unreachable!(),
            };
        }
        if closed {
            reversed = reversed.close();
        }
    }
    if let Some(target) = path.morph_target() {
        reversed = reversed.with_morph_target(reverse_path(target));
    }
    reversed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversal_preserves_curves_breaks_and_singletons() {
        let p = VectorPath::new()
            .move_to(Vec2::new(0., 0.))
            .quadratic_to(Vec2::new(1., 2.), Vec2::new(2., 0.))
            .cubic_to(Vec2::new(3., -1.), Vec2::new(4., 1.), Vec2::new(5., 0.))
            .move_to(Vec2::new(10., 10.));
        let reversed = reverse_path(&p);
        assert_eq!(
            reversed.endpoints(),
            Some((Vec2::new(10., 10.), Vec2::ZERO))
        );
        assert_eq!(reverse_path(&reversed), p);
    }
    #[test]
    fn implicit_closing_edge_remains_a_curve_and_a_join() {
        let p = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2., 0.))
            .line_to(Vec2::new(1., 2.))
            .close();
        let r = reverse_path(&p);
        assert_eq!(r.endpoints(), p.endpoints());
        assert_eq!(r.commands().last(), Some(&PathCommand::Close));
        assert_eq!(
            r.commands()[1],
            PathCommand::LineTo {
                to: Vec2::new(1., 2.)
            }
        );
        assert_eq!(
            crate::PathProportionPlan::new(&p)
                .unwrap()
                .arc_length(None)
                .unwrap(),
            crate::PathProportionPlan::new(&r)
                .unwrap()
                .arc_length(None)
                .unwrap()
        );
    }
}
