//! Quadratic contours over the font provider's fully decoded/varied points.
//! No font parsing, composite expansion, variation processing, or private library source.
use skrifa::{
    outline::pen::OutlinePen,
    raw::{
        tables::glyf::{PointCoord, PointFlags},
        types::F26Dot6,
    },
};
type Point = [i32; 2];
fn midpoint(a: Point, b: Point) -> Point {
    std::array::from_fn(|axis| {
        F26Dot6::from_bits(a[axis])
            .midpoint(F26Dot6::from_bits(b[axis]))
            .to_bits()
    })
}
fn xy(p: Point) -> (f32, f32) {
    (p[0] as f32 / 64.0, p[1] as f32 / 64.0)
}
pub fn quadratic_outline(
    points: &[Point],
    flags: &[PointFlags],
    ends: &[u16],
    pen: &mut impl OutlinePen,
) -> Result<(), &'static str> {
    if points.len() != flags.len() {
        return Err("point/flag lengths differ");
    }
    let mut first = 0;
    for &last in ends {
        let last = usize::from(last);
        if last < first || last >= points.len() {
            return Err("invalid contour end");
        }
        let p = &points[first..=last];
        let f = &flags[first..=last];
        if f.iter().any(|f| f.is_off_curve_cubic()) {
            return Err("unsupported cubic glyf contour");
        }
        let (start, begin, end) = if f[0].is_on_curve() {
            (p[0], 1, p.len())
        } else if f[p.len() - 1].is_on_curve() {
            (p[p.len() - 1], 0, p.len() - 1)
        } else {
            (midpoint(p[p.len() - 1], p[0]), 0, p.len())
        };
        let (x, y) = xy(start);
        pen.move_to(x, y);
        let mut control = None;
        for i in begin..end {
            if f[i].is_on_curve() {
                let (x, y) = xy(p[i]);
                if let Some(c) = control.take() {
                    let (cx, cy) = xy(c);
                    pen.quad_to(cx, cy, x, y);
                } else {
                    pen.line_to(x, y);
                }
            } else {
                if let Some(c) = control.replace(p[i]) {
                    let (cx, cy) = xy(c);
                    let (x, y) = xy(midpoint(c, p[i]));
                    pen.quad_to(cx, cy, x, y);
                }
            }
        }
        if let Some(c) = control {
            let (cx, cy) = xy(c);
            let (x, y) = xy(start);
            pen.quad_to(cx, cy, x, y);
        }
        pen.close();
        first = last + 1;
    }
    if first != points.len() {
        return Err("unassigned contour points");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Collector;
    use super::*;
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{
            pen::{PathElement, PathStyle},
            OutlineGlyphCollection, OutlineGlyphFormat,
        },
        raw::{tables::glyf::PointFlags, types::GlyphId, TableProvider},
        FontRef,
    };
    #[test]
    fn negative_odd_midpoints_follow_public_fixed_point_contract() {
        let mut pen = Collector::default();
        quadratic_outline(
            &[[-1, -1], [-2, -2]],
            &[PointFlags::off_curve_quad(); 2],
            &[1],
            &mut pen,
        )
        .unwrap();
        assert_eq!(
            pen.0[0],
            PathElement::MoveTo {
                x: -1.0 / 64.0,
                y: -1.0 / 64.0
            }
        );
        assert_eq!(
            pen.0[1],
            PathElement::QuadTo {
                cx0: -1.0 / 64.0,
                cy0: -1.0 / 64.0,
                x: -1.0 / 64.0,
                y: -1.0 / 64.0
            }
        );
        assert_eq!(pen.0.last(), Some(&PathElement::Close));
    }
    #[test]
    fn malformed_and_unsupported_inputs_reject() {
        let p = [[0, 0], [1, 1]];
        let f = [PointFlags::on_curve(); 2];
        for (flags, ends) in [
            (&f[..1], &[1][..]),
            (&f[..], &[2][..]),
            (&f[..], &[1, 0][..]),
            (&f[..], &[0][..]),
        ] {
            assert!(quadratic_outline(&p, flags, ends, &mut Collector::default()).is_err());
        }
        assert!(quadratic_outline(
            &p,
            &[PointFlags::off_curve_cubic(); 2],
            &[1],
            &mut Collector::default()
        )
        .is_err());
    }
    #[test]
    fn bundled_glyf_contours_match_the_public_font_provider() {
        let mut count = 0;
        for bytes in typst_assets::fonts() {
            let font = FontRef::from_index(bytes, 0).unwrap();
            let outlines = OutlineGlyphCollection::new(&font);
            if outlines.format() != Some(OutlineGlyphFormat::Glyf) {
                continue;
            }
            for size in [1.0, 17.3] {
                for id in 0..font.maxp().unwrap().num_glyphs() {
                    let glyph = outlines.get(GlyphId::new(u32::from(id))).unwrap();
                    glyph
                        .with_scaled_glyf_outline(
                            Size::new(size),
                            LocationRef::default(),
                            None,
                            |decoded| {
                                let mut expected = Collector::default();
                                decoded.to_path(PathStyle::FreeType, &mut expected)?;
                                let points: Vec<_> = decoded
                                    .points
                                    .iter()
                                    .map(|p| [p.x.to_bits(), p.y.to_bits()])
                                    .collect();
                                let mut actual = Collector::default();
                                quadratic_outline(
                                    &points,
                                    decoded.flags,
                                    decoded.contours,
                                    &mut actual,
                                )
                                .unwrap();
                                assert_eq!(actual.0, expected.0, "glyph {id}, size {size}");
                                count += 1;
                                Ok(())
                            },
                        )
                        .unwrap();
                }
            }
        }
        assert!(count > 1_000, "bundled composite and empty glyph coverage");
    }
}
