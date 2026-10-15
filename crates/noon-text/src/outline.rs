//! Shared glyph geometry preparation. A run's retained recipe determines layout
//! bounds, lazy animation paths and atlas masks; projection never changes it.

use noon_core::{GlyphOutlineProfile, Vec2, VectorPath};
use skrifa::raw::TableProvider;
use skrifa::{
    instance::{Location, Size},
    outline::{
        pen::{OutlinePen, PathElement, PathStyle},
        DrawSettings, Hinting, OutlineGlyphFormat,
    },
    FontRef, GlyphId, MetadataProvider, OutlineGlyphCollection, Tag,
};
use swash::{
    scale::{ScaleContext, Scaler},
    zeno::{Command as ZenoCommand, PathData},
};

mod cairo_path;
mod contours;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphOutlineError;

impl std::fmt::Display for GlyphOutlineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid or unsupported OpenType glyph outline")
    }
}
impl std::error::Error for GlyphOutlineError {}

/// Reused across glyph preparation. This owns scratch storage, not a second font
/// resource cache; callers retain their existing bounded outline/image caches.
#[derive(Default)]
pub struct GlyphOutlineContext {
    swash: ScaleContext,
    memory: Vec<u8>,
    points: Vec<[i32; 2]>,
}

impl GlyphOutlineContext {
    fn coretext_scaler<'a>(
        &'a mut self,
        font: swash::FontRef<'a>,
        face_index: u32,
        size: f32,
        variations: &[([u8; 4], f32)],
    ) -> Result<GlyphOutlineScaler<'a>, GlyphOutlineError> {
        if !size.is_finite()
            || size <= 0.0
            || variations.iter().any(|(_, value)| !value.is_finite())
            || native_outline_profile(font.data, face_index)? != GlyphOutlineProfile::Cairo
        {
            return Err(GlyphOutlineError);
        }
        let font = FontRef::from_index(font.data, face_index).map_err(|_| GlyphOutlineError)?;
        if font.table_data(Tag::new(b"glyf")).is_none() {
            return Err(GlyphOutlineError);
        }
        let svg_size = coretext_layout_size(size)?;
        let units_per_em = font.head().map_err(|_| GlyphOutlineError)?.units_per_em();
        if units_per_em == 0 {
            return Err(GlyphOutlineError);
        }
        Ok(GlyphOutlineScaler(ScalerKind::CoreText(
            CoreTextGlyphScaler {
                outlines: OutlineGlyphCollection::new(&font),
                location: font.axes().location(
                    variations
                        .iter()
                        .map(|(tag, value)| (Tag::new(tag), *value)),
                ),
                scale: svg_size / f64::from(units_per_em),
                memory: &mut self.memory,
            },
        )))
    }

    pub fn font_scaler<'a>(
        &'a mut self,
        font: swash::FontRef<'a>,
        size: f32,
        variations: &[([u8; 4], f32)],
    ) -> Scaler<'a> {
        self.swash
            .builder(font)
            .size(size)
            .hint(false)
            .variations(variations.iter())
            .build()
    }

    pub fn scaler<'a>(
        &'a mut self,
        font: swash::FontRef<'a>,
        face_index: u32,
        size: f32,
        variations: &[([u8; 4], f32)],
        profile: GlyphOutlineProfile,
    ) -> Result<GlyphOutlineScaler<'a>, GlyphOutlineError> {
        if !size.is_finite() || size <= 0.0 || variations.iter().any(|(_, v)| !v.is_finite()) {
            return Err(GlyphOutlineError);
        }
        match profile {
            GlyphOutlineProfile::CairoCoreText => {
                self.coretext_scaler(font, face_index, size, variations)
            }
            GlyphOutlineProfile::Font => Ok(GlyphOutlineScaler(ScalerKind::Font(
                self.font_scaler(font, size, variations),
            ))),
            GlyphOutlineProfile::Cairo => {
                let font =
                    FontRef::from_index(font.data, face_index).map_err(|_| GlyphOutlineError)?;
                if font.table_data(Tag::new(b"fvar")).is_some() {
                    return Err(GlyphOutlineError);
                }
                let location = font.axes().location(
                    variations
                        .iter()
                        .map(|(tag, value)| (Tag::new(tag), *value)),
                );
                let svg_size = f64::from(size) / 3.6;
                // FreeType scales at at least one ppem. Cairo applies the remaining
                // signed affine before TrueType implied-point decomposition.
                let base_size = ((svg_size.max(1.0) * 64.0).round() / 64.0) as f32;
                if f64::from(base_size) > f64::from(i32::MAX) / 256.0 {
                    return Err(GlyphOutlineError);
                }
                let scale = (svg_size.min(1.0) * 65536.0).round() as i32;
                Ok(GlyphOutlineScaler(ScalerKind::Cairo(CairoGlyphScaler {
                    outlines: OutlineGlyphCollection::new(&font),
                    location,
                    base_size,
                    scale,
                    memory: &mut self.memory,
                    points: &mut self.points,
                })))
            }
        }
    }
}

/// Manim supplies Pango point sizes. Pango quantizes those before resolving the
/// 96-DPI Cairo size on its own 1024-unit grid; combining the two steps loses
/// a layout unit at some authored sizes.
pub(crate) fn coretext_layout_size(size: f32) -> Result<f64, GlyphOutlineError> {
    let point_units = (f64::from(size) / 4.8 * 1024.0).round();
    let size = (point_units * (4.0 / 3.0)).round() / 1024.0;
    if !size.is_finite() || size > f64::from(i32::MAX) / 256.0 {
        return Err(GlyphOutlineError);
    }
    Ok(size)
}

pub(crate) fn supports_coretext_metrics(data: &[u8], face_index: u32) -> bool {
    let Ok(font) = FontRef::from_index(data, face_index) else {
        return false;
    };
    font.table_data(Tag::new(b"glyf")).is_some()
        && native_outline_profile(data, face_index) == Ok(GlyphOutlineProfile::Cairo)
}

/// Intrinsic color/bitmap and variable fonts keep their natural resource recipe.
/// The Cairo profile currently covers static monochrome TrueType/CFF faces.
pub fn native_outline_profile(
    data: &[u8],
    face_index: u32,
) -> Result<GlyphOutlineProfile, GlyphOutlineError> {
    let font = FontRef::from_index(data, face_index).map_err(|_| GlyphOutlineError)?;
    let natural = [b"COLR", b"CBDT", b"sbix", b"SVG ", b"VARC", b"fvar"]
        .iter()
        .any(|tag| font.table_data(Tag::new(tag)).is_some());
    Ok(if natural {
        GlyphOutlineProfile::Font
    } else {
        GlyphOutlineProfile::Cairo
    })
}

pub struct GlyphOutlineScaler<'a>(ScalerKind<'a>);

enum ScalerKind<'a> {
    Font(Scaler<'a>),
    Cairo(CairoGlyphScaler<'a>),
    CoreText(CoreTextGlyphScaler<'a>),
}

impl GlyphOutlineScaler<'_> {
    pub fn outline(&mut self, glyph_id: u16) -> Result<VectorPath, GlyphOutlineError> {
        match &mut self.0 {
            ScalerKind::Font(scaler) => Ok(scaler
                .scale_outline(glyph_id)
                .map(|outline| outline_commands_to_path(outline.path().commands()))
                .unwrap_or_default()),
            ScalerKind::Cairo(scaler) => scaler.outline(glyph_id),
            ScalerKind::CoreText(scaler) => scaler.outline(glyph_id),
        }
    }
}

struct CoreTextGlyphScaler<'a> {
    outlines: OutlineGlyphCollection<'a>,
    location: Location,
    scale: f64,
    memory: &'a mut Vec<u8>,
}
impl CoreTextGlyphScaler<'_> {
    fn outline(&mut self, glyph_id: u16) -> Result<VectorPath, GlyphOutlineError> {
        let Some(outline) = self.outlines.get(GlyphId::new(u32::from(glyph_id))) else {
            return Ok(VectorPath::new());
        };
        self.memory
            .resize(outline.draw_memory_size(Hinting::None), 0);
        let mut pen = Collector::default();
        outline
            .draw(
                DrawSettings::unhinted(Size::unscaled(), &self.location)
                    .with_memory(Some(self.memory.as_mut_slice()))
                    .with_path_style(PathStyle::FreeType),
                &mut pen,
            )
            .map_err(|_| GlyphOutlineError)?;
        if pen.0.iter().any(|command| {
            !transformed_fixed_coordinates_fit(*command, [256.0 * self.scale, -256.0 * self.scale])
        }) {
            return Err(GlyphOutlineError);
        }
        Ok(cairo_path::coretext_path(&pen.0, self.scale))
    }
}

struct CairoGlyphScaler<'a> {
    outlines: OutlineGlyphCollection<'a>,
    location: Location,
    base_size: f32,
    scale: i32,
    memory: &'a mut Vec<u8>,
    points: &'a mut Vec<[i32; 2]>,
}

impl CairoGlyphScaler<'_> {
    fn outline(&mut self, glyph_id: u16) -> Result<VectorPath, GlyphOutlineError> {
        let Some(outline) = self.outlines.get(GlyphId::new(u32::from(glyph_id))) else {
            return Ok(VectorPath::new()); // Legitimate bitmap-only or empty glyph.
        };
        self.memory
            .resize(outline.draw_memory_size(Hinting::None), 0);
        let mut pen = Collector::default();
        match outline.format() {
            OutlineGlyphFormat::Glyf => {
                outline
                    .with_scaled_glyf_outline(
                        Size::new(self.base_size),
                        &self.location,
                        Some(self.memory.as_mut_slice()),
                        |decoded| {
                            self.points.clear();
                            self.points.extend(decoded.points.iter().map(|p| {
                                [
                                    mulfix(p.x.to_bits(), self.scale),
                                    mulfix(p.y.to_bits(), -self.scale),
                                ]
                            }));
                            contours::quadratic_outline(
                                self.points,
                                decoded.flags,
                                decoded.contours,
                                &mut pen,
                            )
                            .map_err(|_| skrifa::outline::DrawError::NoSources)
                        },
                    )
                    .map_err(|_| GlyphOutlineError)?;
            }
            OutlineGlyphFormat::Cff | OutlineGlyphFormat::Cff2 => {
                outline
                    .draw(
                        DrawSettings::unhinted(Size::new(self.base_size), &self.location)
                            .with_path_style(PathStyle::FreeType),
                        &mut pen,
                    )
                    .map_err(|_| GlyphOutlineError)?;
                if pen
                    .0
                    .iter()
                    .any(|command| !fixed_coordinates_fit(*command, 64.0))
                {
                    return Err(GlyphOutlineError);
                }
                for command in &mut pen.0 {
                    transform_cff(command, self.scale);
                }
            }
            _ => return Err(GlyphOutlineError),
        }
        if pen
            .0
            .iter()
            .any(|command| !fixed_coordinates_fit(*command, 256.0))
        {
            return Err(GlyphOutlineError);
        }
        Ok(cairo_path::native_path(&pen.0))
    }
}

// Signed 16.16 multiplication with FreeType's nearest/away-from-zero tie rule.
fn mulfix(a: i32, b: i32) -> i32 {
    let product = i64::from(a) * i64::from(b);
    let rounded = (product.abs() + 0x8000) >> 16;
    (if product < 0 { -rounded } else { rounded }) as i32
}

fn transform_cff(command: &mut PathElement, scale: i32) {
    let xy = |x: &mut f32, y: &mut f32| {
        *x = mulfix((*x * 64.0).round() as i32, scale) as f32 / 64.0;
        *y = mulfix((*y * 64.0).round() as i32, -scale) as f32 / 64.0;
    };
    match command {
        PathElement::MoveTo { x, y } | PathElement::LineTo { x, y } => xy(x, y),
        PathElement::QuadTo { cx0, cy0, x, y } => {
            xy(cx0, cy0);
            xy(x, y);
        }
        PathElement::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        } => {
            xy(cx0, cy0);
            xy(cx1, cy1);
            xy(x, y);
        }
        PathElement::Close => {}
    }
}

#[derive(Default)]
struct Collector(Vec<PathElement>);
impl OutlinePen for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(PathElement::MoveTo { x, y });
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(PathElement::LineTo { x, y });
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(PathElement::QuadTo { cx0, cy0, x, y });
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(PathElement::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        });
    }
    fn close(&mut self) {
        self.0.push(PathElement::Close);
    }
}

/// Adapt a shared vector resource to the existing CPU coverage/stroke engine.
pub fn path_commands(path: &VectorPath) -> impl Iterator<Item = swash::zeno::Command> + Clone + '_ {
    use noon_core::PathCommand;
    use swash::zeno::{Command, Point};
    path.commands().iter().map(|command| match *command {
        PathCommand::MoveTo { to } => Command::MoveTo(Point::new(to.x, to.y)),
        PathCommand::LineTo { to } => Command::LineTo(Point::new(to.x, to.y)),
        PathCommand::QuadraticTo { control, to } => {
            Command::QuadTo(Point::new(control.x, control.y), Point::new(to.x, to.y))
        }
        PathCommand::CubicTo {
            control1,
            control2,
            to,
        } => Command::CurveTo(
            Point::new(control1.x, control1.y),
            Point::new(control2.x, control2.y),
            Point::new(to.x, to.y),
        ),
        PathCommand::Close => Command::Close,
    })
}

// Reject before fixed conversion rather than saturating coordinates into another
// shape. The 24.8 Cairo path representation has a finite coordinate domain.
fn fixed_coordinates_fit(command: PathElement, units: f64) -> bool {
    transformed_fixed_coordinates_fit(command, [units; 2])
}

fn transformed_fixed_coordinates_fit(command: PathElement, units: [f64; 2]) -> bool {
    let xy = |x: f32, y: f32| {
        [x, y].into_iter().zip(units).all(|(value, scale)| {
            let fixed = (f64::from(value) * scale).round();
            value.is_finite() && fixed >= f64::from(i32::MIN) && fixed <= f64::from(i32::MAX)
        })
    };
    match command {
        PathElement::MoveTo { x, y } | PathElement::LineTo { x, y } => xy(x, y),
        PathElement::QuadTo { cx0, cy0, x, y } => xy(cx0, cy0) && xy(x, y),
        PathElement::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        } => xy(cx0, cy0) && xy(cx1, cy1) && xy(x, y),
        PathElement::Close => true,
    }
}

/// Convert Swash/Zeno geometry without changing curve degree or coordinates.
#[doc(hidden)]
pub fn outline_commands_to_path(commands: impl Iterator<Item = ZenoCommand>) -> VectorPath {
    let mut path = VectorPath::new();
    for command in commands {
        path = match command {
            ZenoCommand::MoveTo(p) => path.move_to(Vec2::new(p.x, p.y)),
            ZenoCommand::LineTo(p) => path.line_to(Vec2::new(p.x, p.y)),
            ZenoCommand::QuadTo(c, p) => {
                path.quadratic_to(Vec2::new(c.x, c.y), Vec2::new(p.x, p.y))
            }
            ZenoCommand::CurveTo(a, b, p) => path.cubic_to(
                Vec2::new(a.x, a.y),
                Vec2::new(b.x, b.y),
                Vec2::new(p.x, p.y),
            ),
            ZenoCommand::Close => path.close(),
        };
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coretext_settings_are_bounded_before_outline_preparation() {
        let bytes = typst_assets::fonts()
            .find(|data| data.starts_with(&[0, 1, 0, 0]))
            .unwrap();
        let font = swash::FontRef::from_index(bytes, 0).unwrap();
        let mut context = GlyphOutlineContext::default();
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(context
                .scaler(font, 0, size, &[], GlyphOutlineProfile::CairoCoreText)
                .is_err());
        }
        assert!(context
            .scaler(
                font,
                u32::MAX,
                42.0,
                &[],
                GlyphOutlineProfile::CairoCoreText
            )
            .is_err());
        assert!(context
            .scaler(
                font,
                0,
                42.0,
                &[(*b"wght", f32::NAN)],
                GlyphOutlineProfile::CairoCoreText
            )
            .is_err());
        assert_eq!(coretext_layout_size(48.0).unwrap(), 13653.0 / 1024.0);
        // Independent Pango/Cairo observations at Manim's 67-point text size.
        assert_eq!(coretext_layout_size(67.0).unwrap(), 19057.0 / 1024.0);
        assert!(!transformed_fixed_coordinates_fit(
            PathElement::MoveTo {
                x: 0.0,
                y: -8_388_608.0
            },
            [256.0, -256.0]
        ));
        assert!(transformed_fixed_coordinates_fit(
            PathElement::MoveTo {
                x: -8_388_608.0,
                y: 0.0
            },
            [256.0, -256.0]
        ));
    }

    #[test]
    fn fixed_coordinate_overflow_and_invalid_settings_reject() {
        let bytes = typst_assets::fonts().next().unwrap();
        let font = swash::FontRef::from_index(bytes, 0).unwrap();
        let mut context = GlyphOutlineContext::default();
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(context
                .scaler(font, 0, size, &[], GlyphOutlineProfile::Cairo)
                .is_err());
        }
        assert!(context
            .scaler(
                font,
                0,
                42.0,
                &[(*b"wght", f32::NAN)],
                GlyphOutlineProfile::Cairo
            )
            .is_err());
        assert!(context
            .scaler(font, u32::MAX, 42.0, &[], GlyphOutlineProfile::Cairo)
            .is_err());
        for value in [f32::NAN, f32::INFINITY, 8_388_608.0] {
            assert!(!fixed_coordinates_fit(
                PathElement::MoveTo { x: value, y: 0.0 },
                256.0
            ));
        }
        assert!(fixed_coordinates_fit(
            PathElement::MoveTo {
                x: -8_388_608.0,
                y: 0.0
            },
            256.0
        ));
    }
}
