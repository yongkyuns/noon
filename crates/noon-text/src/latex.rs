//! Bounded DVI normalization for the real LaTeX backend.
//!
//! This is deliberately a DVI reader, not a renderer: it retains glyph placement
//! and rule geometry in Noon's shared text resources. Font programs are supplied
//! by the backend host, so this module neither shells out to TeX nor guesses a
//! system font.

use std::{collections::BTreeMap, fmt, sync::Arc};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, GeometryResourceArena, GlyphRun, PositionedGlyph,
    Rect, TextAffineTransform, TextClusterIdentity, TextDirection, TextLayoutArtifact, TextPart,
    TextRenderItem, TextResource, TextSourceKind, TextSourceSpan, TextVectorItem, TextVectorStyle,
    Vec2, VectorPath,
};
use swash::{FontRef, GlyphId};

pub const LATEX_DVI_BACKEND_VERSION: &str = "dvi-v2-tfm-v1";
pub const DEFAULT_MAX_DVI_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MAX_COMMANDS: usize = 200_000;
const DEFAULT_MAX_FONTS: usize = 256;
const DEFAULT_MAX_GLYPHS: usize = 100_000;
const DEFAULT_MAX_STACK: usize = 1_024;

/// Exact immutable resources selected by the LaTeX host for one DVI font name.
#[derive(Clone, Debug)]
pub struct DviFontResource {
    pub dvi_name: Arc<str>,
    pub family: Arc<str>,
    pub face_key: Arc<str>,
    pub face_index: u32,
    pub tfm: Arc<[u8]>,
    pub ttf: Arc<[u8]>,
    /// TeX's 8-bit character code to Unicode mapping for this exact TTF. This
    /// is explicit because Computer Modern math fonts do not use Unicode at
    /// codes 0..31 and 127.
    pub codepoints: Arc<[u32]>,
}

impl DviFontResource {
    /// Build the pinned BaKoMa resource used by the LaTeX host.
    ///
    /// The TeX math encodings are resolved against the supplied TTF's actual
    /// Unicode cmap. In particular, code 10 prefers U+00AD and falls back to
    /// U+00AC for BaKoMa files carrying the upstream soft-hyphen alias.
    pub fn bakoma(
        dvi_name: impl Into<Arc<str>>,
        backend_identity: impl AsRef<str>,
        tfm: impl Into<Arc<[u8]>>,
        ttf: impl Into<Arc<[u8]>>,
    ) -> Result<Self, LatexDviError> {
        let dvi_name = dvi_name.into();
        let ttf = ttf.into();
        let font = FontRef::from_index(&ttf, 0)
            .ok_or_else(|| LatexDviError::InvalidOpenType(dvi_name.clone()))?;
        let codepoints = bakoma_codepoints(&font);
        Ok(Self {
            family: dvi_name.clone(),
            face_key: Arc::from(format!("{}:{}", backend_identity.as_ref(), dvi_name)),
            dvi_name,
            face_index: 0,
            tfm: tfm.into(),
            ttf,
            codepoints,
        })
    }
}

fn bakoma_codepoints(font: &FontRef<'_>) -> Arc<[u32]> {
    let mut codepoints = Vec::with_capacity(256);
    for code in 0..=255_u8 {
        let candidates: &[u32] = match code {
            0..=9 => &[161 + code as u32],
            10 => &[173, 172],
            11..=19 => &[173 + (code - 10) as u32],
            20 => &[8729],
            21..=32 => &[184 + (code - 21) as u32],
            127 => &[196],
            _ => &[code as u32],
        };
        let selected = candidates
            .iter()
            .copied()
            .find(|candidate| {
                font.charmap()
                    .map(char::from_u32(*candidate).unwrap_or('\0'))
                    != 0
            })
            .unwrap_or(candidates[0]);
        codepoints.push(selected);
    }
    codepoints.into()
}

/// Explicit limits defend the backend boundary from hostile DVI payloads.
#[derive(Clone, Copy, Debug)]
pub struct DviLimits {
    pub max_bytes: usize,
    pub max_commands: usize,
    pub max_fonts: usize,
    pub max_glyphs: usize,
    pub max_stack: usize,
}

impl Default for DviLimits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_DVI_BYTES,
            max_commands: DEFAULT_MAX_COMMANDS,
            max_fonts: DEFAULT_MAX_FONTS,
            max_glyphs: DEFAULT_MAX_GLYPHS,
            max_stack: DEFAULT_MAX_STACK,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LatexDviArtifact {
    pub resource: TextResource,
    pub fonts: FontResourceArena,
    pub geometries: GeometryResourceArena,
}

/// Return font names declared by a DVI without loading font payloads. Hosts use
/// this first pass to resolve only the exact TFM/TTF resources subsequently
/// supplied to [`normalize_dvi`].
pub fn required_dvi_fonts(dvi: &[u8], limits: DviLimits) -> Result<Vec<Arc<str>>, LatexDviError> {
    if dvi.len() > limits.max_bytes {
        return Err(LatexDviError::Limit("DVI bytes"));
    }
    let mut r = Reader::new(dvi);
    if r.u8()? != 247 {
        return Err(LatexDviError::MissingPreamble);
    }
    if r.u8()? != 2 {
        return Err(LatexDviError::UnsupportedVersion(0));
    }
    r.skip(12)?;
    let comment = r.u8()? as usize;
    r.skip(comment)?;
    let mut out = Vec::new();
    let mut commands = 0;
    while !r.is_end() {
        commands += 1;
        if commands > limits.max_commands {
            return Err(LatexDviError::Limit("DVI commands"));
        }
        let op = r.u8()?;
        match op {
            0..=127 | 138 | 140..=142 | 147 | 152 | 161 | 166 | 171..=234 => {}
            128..=131
            | 133..=136
            | 143..=146
            | 148..=151
            | 153..=156
            | 157..=160
            | 162..=165
            | 167..=170
            | 235..=238 => {
                r.skip(
                    (match op {
                        128..=131 => op - 127,
                        133..=136 => op - 132,
                        143..=146 => op - 142,
                        148..=151 => op - 147,
                        153..=156 => op - 152,
                        157..=160 => op - 156,
                        162..=165 => op - 161,
                        167..=170 => op - 166,
                        235..=238 => op - 234,
                        _ => 0,
                    }) as usize,
                )?;
            }
            132 | 137 => r.skip(8)?,
            139 => r.skip(44)?,
            239..=242 => {
                let n = r.unsigned((op - 238) as usize)? as usize;
                r.skip(n)?;
            }
            243..=246 => {
                r.skip((op - 242) as usize)?;
                r.skip(12)?;
                let a = r.u8()? as usize;
                let l = r.u8()? as usize;
                let name = std::str::from_utf8(r.bytes(a + l)?)
                    .map_err(|_| LatexDviError::InvalidDvi("non UTF-8 font name"))?;
                if !out
                    .iter()
                    .any(|existing: &Arc<str>| existing.as_ref() == name)
                {
                    out.push(Arc::from(name));
                }
                if out.len() > limits.max_fonts {
                    return Err(LatexDviError::Limit("DVI fonts"));
                }
            }
            248 => r.skip(28)?,
            249 => {
                r.skip(5)?;
                while !r.is_end() {
                    if r.u8()? != 223 {
                        return Err(LatexDviError::InvalidDvi("invalid post_post padding"));
                    }
                }
            }
            _ => return Err(LatexDviError::InvalidDvi("unsupported opcode")),
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LatexDviError {
    Limit(&'static str),
    Truncated,
    UnsupportedVersion(u8),
    MissingPreamble,
    InvalidDvi(&'static str),
    IntegerOverflow,
    UnknownFont(u32),
    MissingFont(Arc<str>),
    InvalidTfm(Arc<str>),
    FontChecksumMismatch(Arc<str>),
    InvalidOpenType(Arc<str>),
    MissingGlyph { font: Arc<str>, code: u32 },
    UnknownSemanticSpecial(Arc<str>),
    MultipleNonemptyPages,
}

impl fmt::Display for LatexDviError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LatexDviError {}

/// Decode a single nonempty DVI page into retained glyph/rule resources.
///
/// `source` is the original TeX source. DVI `noon:source:start:end[:key]` and
/// `noon:part:start:end[:key]` specials attach source identities to subsequent
/// glyphs/rules and declared logical parts respectively.
pub fn normalize_dvi(
    source: &str,
    kind: TextSourceKind,
    backend_identity: TextLayoutArtifact,
    dvi: &[u8],
    resources: &[DviFontResource],
    limits: DviLimits,
) -> Result<LatexDviArtifact, LatexDviError> {
    if !matches!(kind, TextSourceKind::Tex | TextSourceKind::MathTex) {
        return Err(LatexDviError::InvalidDvi(
            "LaTeX DVI requires Tex or MathTex source kind",
        ));
    }
    if dvi.len() > limits.max_bytes {
        return Err(LatexDviError::Limit("DVI bytes"));
    }
    let source_len =
        u32::try_from(source.len()).map_err(|_| LatexDviError::Limit("source bytes"))?;
    let mut reader = Reader::new(dvi);
    if reader.u8()? != 247 {
        return Err(LatexDviError::MissingPreamble);
    }
    let version = reader.u8()?;
    if version != 2 {
        return Err(LatexDviError::UnsupportedVersion(version));
    }
    let num = reader.u32()?;
    let den = reader.u32()?;
    let mag = reader.u32()?;
    if den == 0 {
        return Err(LatexDviError::InvalidDvi("zero denominator"));
    }
    let comment_len = reader.u8()? as usize;
    reader.skip(comment_len)?;
    let point_scale = (num as f64 / den as f64) * (mag as f64 / 1000.0) * (72.27 / 254_000.0);
    if !point_scale.is_finite() || point_scale <= 0.0 {
        return Err(LatexDviError::InvalidDvi("invalid scale"));
    }

    let mut supplied = BTreeMap::new();
    for item in resources {
        supplied.insert(item.dvi_name.as_ref(), item);
    }
    let mut definitions: BTreeMap<u32, ActiveFont<'_>> = BTreeMap::new();
    let mut fonts = FontResourceArena::new();
    let mut geometries = GeometryResourceArena::new();
    let mut runs = Vec::new();
    let mut vectors = Vec::new();
    let mut order = Vec::new();
    let mut parts = Vec::new();
    let mut vector_bounds: Option<Rect> = None;
    let mut page_nonempty = false;
    let mut seen_nonempty_page = false;
    let mut in_page = false;
    let mut state = State::default();
    let mut stack = Vec::new();
    let mut commands = 0usize;
    let mut glyph_count = 0u32;
    let mut current_span = TextSourceSpan::new(0, source_len);
    let mut current_key = None;
    let mut current_fill = None;
    let mut color_stack = Vec::new();
    let mut declared_fonts = 0usize;
    let mut saw_post = false;
    let mut saw_post_post = false;

    while !reader.is_end() {
        commands += 1;
        if commands > limits.max_commands {
            return Err(LatexDviError::Limit("DVI commands"));
        }
        let op = reader.u8()?;
        match op {
            0..=127 => {
                require_page(in_page)?;
                page_nonempty = true;
                set_char(
                    op as u32,
                    true,
                    &mut state,
                    &definitions,
                    point_scale,
                    &mut fonts,
                    &mut runs,
                    &mut order,
                    &mut glyph_count,
                    limits.max_glyphs,
                    current_span,
                    current_key.clone(),
                    current_fill,
                )?;
            }
            128..=131 => {
                require_page(in_page)?;
                page_nonempty = true;
                let code = reader.unsigned((op - 127) as usize)?;
                set_char(
                    code,
                    true,
                    &mut state,
                    &definitions,
                    point_scale,
                    &mut fonts,
                    &mut runs,
                    &mut order,
                    &mut glyph_count,
                    limits.max_glyphs,
                    current_span,
                    current_key.clone(),
                    current_fill,
                )?;
            }
            132 | 137 => {
                require_page(in_page)?;
                let height = reader.signed(4)?;
                let width = reader.signed(4)?;
                if height > 0 && width > 0 {
                    page_nonempty = true;
                    let bounds = push_rule(
                        height,
                        width,
                        state.h,
                        state.v,
                        point_scale,
                        &mut geometries,
                        &mut vectors,
                        &mut order,
                        current_span,
                        current_key.clone(),
                        current_fill,
                    );
                    vector_bounds = Some(vector_bounds.map_or(bounds, |old| old.union(bounds)));
                }
                if op == 132 {
                    state.h = add(state.h, width)?;
                }
            }
            133..=136 => {
                require_page(in_page)?;
                page_nonempty = true;
                let code = reader.unsigned((op - 132) as usize)?;
                set_char(
                    code,
                    false,
                    &mut state,
                    &definitions,
                    point_scale,
                    &mut fonts,
                    &mut runs,
                    &mut order,
                    &mut glyph_count,
                    limits.max_glyphs,
                    current_span,
                    current_key.clone(),
                    current_fill,
                )?;
            }
            138 => {}
            139 => {
                if in_page {
                    return Err(LatexDviError::InvalidDvi("nested page"));
                }
                for _ in 0..10 {
                    reader.signed(4)?;
                }
                reader.signed(4)?;
                in_page = true;
                page_nonempty = false;
                state = State::default();
                stack.clear();
            }
            140 => {
                if !in_page || !stack.is_empty() {
                    return Err(LatexDviError::InvalidDvi("unbalanced page"));
                }
                if page_nonempty {
                    if seen_nonempty_page {
                        return Err(LatexDviError::MultipleNonemptyPages);
                    }
                    seen_nonempty_page = true;
                }
                in_page = false;
            }
            141 => {
                if stack.len() >= limits.max_stack {
                    return Err(LatexDviError::Limit("DVI stack"));
                }
                stack.push(state);
            }
            142 => {
                let font = state.font;
                state = stack
                    .pop()
                    .ok_or(LatexDviError::InvalidDvi("stack underflow"))?;
                state.font = font;
            }
            143..=146 => state.h = add(state.h, reader.signed((op - 142) as usize)?)?,
            147 => state.h = add(state.h, state.w)?,
            148..=151 => {
                state.w = reader.signed((op - 147) as usize)?;
                state.h = add(state.h, state.w)?;
            }
            152 => state.h = add(state.h, state.x)?,
            153..=156 => {
                state.x = reader.signed((op - 152) as usize)?;
                state.h = add(state.h, state.x)?;
            }
            157..=160 => state.v = add(state.v, reader.signed((op - 156) as usize)?)?,
            161 => state.v = add(state.v, state.y)?,
            162..=165 => {
                state.y = reader.signed((op - 161) as usize)?;
                state.v = add(state.v, state.y)?;
            }
            166 => state.v = add(state.v, state.z)?,
            167..=170 => {
                state.z = reader.signed((op - 166) as usize)?;
                state.v = add(state.v, state.z)?;
            }
            171..=234 => state.font = Some((op - 171) as u32),
            235..=238 => state.font = Some(reader.unsigned((op - 234) as usize)?),
            239..=242 => {
                let length = reader.unsigned((op - 238) as usize)? as usize;
                let bytes = reader.bytes(length)?;
                parse_special(
                    bytes,
                    source_len,
                    &mut current_span,
                    &mut current_key,
                    &mut parts,
                    glyph_count,
                    vectors.len() as u32,
                    &mut current_fill,
                    &mut color_stack,
                )?;
            }
            243..=246 => {
                let id = reader.unsigned((op - 242) as usize)?;
                let checksum = reader.u32()?;
                let scale = reader.u32()?;
                let design = reader.u32()?;
                let a = reader.u8()? as usize;
                let l = reader.u8()? as usize;
                let name = std::str::from_utf8(reader.bytes(a + l)?)
                    .map_err(|_| LatexDviError::InvalidDvi("non UTF-8 font name"))?;
                let supplied_font = supplied
                    .get(name)
                    .ok_or_else(|| LatexDviError::MissingFont(Arc::from(name)))?;
                if supplied_font.face_index != 0 {
                    return Err(LatexDviError::InvalidOpenType(Arc::from(name)));
                }
                let tfm = Tfm::parse(&supplied_font.tfm)
                    .map_err(|_| LatexDviError::InvalidTfm(Arc::from(name)))?;
                if checksum != 0 && tfm.checksum != 0 && checksum != tfm.checksum {
                    return Err(LatexDviError::FontChecksumMismatch(Arc::from(name)));
                }
                let face = FontFaceIdentity {
                    family: supplied_font.family.clone(),
                    face_key: supplied_font.face_key.clone(),
                    face_index: supplied_font.face_index,
                    variation_key: Arc::from(""),
                };
                FontRef::from_index(&supplied_font.ttf, supplied_font.face_index as usize)
                    .ok_or_else(|| LatexDviError::InvalidOpenType(Arc::from(name)))?;
                definitions.insert(
                    id,
                    ActiveFont {
                        name: Arc::from(name),
                        checksum,
                        scale,
                        design,
                        resource: supplied_font,
                        tfm,
                        face,
                    },
                );
                declared_fonts += 1;
                if declared_fonts > limits.max_fonts {
                    return Err(LatexDviError::Limit("DVI fonts"));
                }
            }
            248..=249 => {
                // postamble framing: only structurally skip, it cannot paint.
                if op == 248 {
                    if in_page {
                        return Err(LatexDviError::InvalidDvi("postamble in page"));
                    }
                    reader.signed(4)?;
                    reader.u32()?;
                    reader.u32()?;
                    reader.u32()?;
                    reader.u32()?;
                    reader.u32()?;
                    reader.u16()?;
                    reader.u16()?;
                    saw_post = true;
                } else {
                    if !saw_post {
                        return Err(LatexDviError::InvalidDvi("post_post without post"));
                    }
                    reader.u32()?;
                    let v = reader.u8()?;
                    if v != 2 {
                        return Err(LatexDviError::UnsupportedVersion(v));
                    }
                    while !reader.is_end() {
                        if reader.u8()? != 223 {
                            return Err(LatexDviError::InvalidDvi("invalid post_post padding"));
                        }
                    }
                    saw_post_post = true;
                    break;
                }
            }
            _ => return Err(LatexDviError::InvalidDvi("unsupported opcode")),
        }
    }
    if in_page || !saw_post || !saw_post_post {
        return Err(LatexDviError::Truncated);
    }
    if let Some(last) = parts.last_mut() {
        last.cluster_count = glyph_count
            .checked_sub(last.first_cluster)
            .ok_or(LatexDviError::InvalidDvi("part cluster range"))?;
        last.vector_count = (vectors.len() as u32)
            .checked_sub(last.first_vector)
            .ok_or(LatexDviError::InvalidDvi("part vector range"))?;
    } else {
        parts.push(TextPart {
            source_span: TextSourceSpan::new(0, source_len),
            first_cluster: 0,
            cluster_count: glyph_count,
            first_vector: 0,
            vector_count: vectors.len() as u32,
            semantic_key: None,
        });
    }
    let bounds =
        resource_bounds(&runs, vector_bounds).unwrap_or_else(|| Rect::new(Vec2::ZERO, Vec2::ZERO));
    let resource = TextResource {
        source: Arc::from(source),
        kind,
        runs: runs.into(),
        vector_items: vectors.into(),
        render_items: order.into(),
        parts: parts.into(),
        bounds,
        baseline: 0.0,
        layout_artifact: Some(backend_identity),
    };
    resource
        .validate()
        .map_err(|_| LatexDviError::InvalidDvi("invalid normalized resource"))?;
    Ok(LatexDviArtifact {
        resource,
        fonts,
        geometries,
    })
}

#[derive(Clone, Copy, Default)]
struct State {
    h: i64,
    v: i64,
    w: i64,
    x: i64,
    y: i64,
    z: i64,
    font: Option<u32>,
}
fn add(left: i64, right: i64) -> Result<i64, LatexDviError> {
    left.checked_add(right)
        .ok_or(LatexDviError::IntegerOverflow)
}
fn require_page(in_page: bool) -> Result<(), LatexDviError> {
    in_page
        .then_some(())
        .ok_or(LatexDviError::InvalidDvi("paint outside page"))
}
struct ActiveFont<'a> {
    name: Arc<str>,
    #[allow(dead_code)]
    checksum: u32,
    scale: u32,
    #[allow(dead_code)]
    design: u32,
    resource: &'a DviFontResource,
    tfm: Tfm,
    face: FontFaceIdentity,
}

// DVI opcode helpers receive explicit decoder state and bounded output sinks.
// Keep these borrows local instead of introducing another mutable parser owner.
#[allow(clippy::too_many_arguments)]
fn set_char(
    font_code: u32,
    advance: bool,
    state: &mut State,
    fonts_by_id: &BTreeMap<u32, ActiveFont<'_>>,
    scale: f64,
    fonts: &mut FontResourceArena,
    runs: &mut Vec<GlyphRun>,
    order: &mut Vec<TextRenderItem>,
    glyph_count: &mut u32,
    max_glyphs: usize,
    span: TextSourceSpan,
    key: Option<Arc<str>>,
    fill: Option<Color>,
) -> Result<(), LatexDviError> {
    let id = state
        .font
        .ok_or(LatexDviError::InvalidDvi("character without font"))?;
    let font = fonts_by_id.get(&id).ok_or(LatexDviError::UnknownFont(id))?;
    let width = font
        .tfm
        .width(font_code)
        .ok_or_else(|| LatexDviError::MissingGlyph {
            font: font.name.clone(),
            code: font_code,
        })?;
    let movement = fixed_mul(width, font.scale as i64)?;
    let unicode = *font
        .resource
        .codepoints
        .get(font_code as usize)
        .ok_or_else(|| LatexDviError::MissingGlyph {
            font: font.name.clone(),
            code: font_code,
        })?;
    let character = char::from_u32(unicode).ok_or_else(|| LatexDviError::MissingGlyph {
        font: font.name.clone(),
        code: font_code,
    })?;
    let font_ref = FontRef::from_index(&font.resource.ttf, font.resource.face_index as usize)
        .ok_or_else(|| LatexDviError::InvalidOpenType(font.name.clone()))?;
    let glyph_id: GlyphId = font_ref.charmap().map(character);
    if glyph_id == 0 {
        return Err(LatexDviError::MissingGlyph {
            font: font.name.clone(),
            code: font_code,
        });
    }
    if (*glyph_count as usize) >= max_glyphs {
        return Err(LatexDviError::Limit("DVI glyphs"));
    }
    fonts
        .intern_face(&font.face, font.resource.ttf.clone())
        .map_err(|_| LatexDviError::InvalidDvi("conflicting font resource"))?;
    let origin = Vec2::new(
        (state.h as f64 * scale) as f32,
        -(state.v as f64 * scale) as f32,
    );
    let advance_x = (movement as f64 * scale) as f32;
    let font_size = (font.scale as f64 * scale) as f32;
    let bounds = glyph_bounds(
        font.resource.ttf.as_ref(),
        glyph_id,
        font_size,
        origin,
        advance_x,
    )
    .ok_or_else(|| LatexDviError::InvalidOpenType(font.name.clone()))?;
    let ordinal = *glyph_count;
    *glyph_count = glyph_count
        .checked_add(1)
        .ok_or(LatexDviError::Limit("glyph identity"))?;
    let run_index = u32::try_from(runs.len()).map_err(|_| LatexDviError::Limit("glyph runs"))?;
    runs.push(GlyphRun {
        font: font.face.clone(),
        variations: Arc::from([]),
        font_size,
        direction: TextDirection::LeftToRight,
        fill,
        stroke: None,
        transform: TextAffineTransform::IDENTITY,
        glyphs: Arc::from([PositionedGlyph {
            glyph_id: glyph_id as u32,
            cluster: TextClusterIdentity {
                source_span: span,
                cluster_ordinal: ordinal,
                semantic_key: key,
            },
            origin,
            advance: Vec2::new(advance_x, 0.0),
            bounds,
        }]),
    });
    order.push(TextRenderItem::GlyphRun(run_index));
    if advance {
        state.h = add(state.h, movement)?;
    }
    Ok(())
}
fn fixed_mul(fix: i32, scale: i64) -> Result<i64, LatexDviError> {
    let value = (fix as i128)
        .checked_mul(scale as i128)
        .ok_or(LatexDviError::IntegerOverflow)?;
    i64::try_from(value >> 20).map_err(|_| LatexDviError::IntegerOverflow)
}
/// Reads `head`/`loca`/`glyf` table boxes directly, retaining lazy outlines for
/// the renderer. CFF-only fonts are deliberately unsupported by this DVI path.
fn glyph_bounds(ttf: &[u8], glyph: GlyphId, size: f32, origin: Vec2, advance: f32) -> Option<Rect> {
    fn table(bytes: &[u8], tag: &[u8; 4]) -> Option<usize> {
        let count = u16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?) as usize;
        for i in 0..count {
            let p = 12 + i * 16;
            if bytes.get(p..p + 4)? == tag {
                return Some(
                    u32::from_be_bytes(bytes.get(p + 8..p + 12)?.try_into().ok()?) as usize,
                );
            }
        }
        None
    }
    let head = table(ttf, b"head")?;
    let loca = table(ttf, b"loca")?;
    let glyf = table(ttf, b"glyf")?;
    let upem = u16::from_be_bytes(ttf.get(head + 18..head + 20)?.try_into().ok()?) as f32;
    let format = i16::from_be_bytes(ttf.get(head + 50..head + 52)?.try_into().ok()?);
    let id = glyph as usize;
    let loc = |n: usize| -> Option<usize> {
        match format {
            0 => Some(
                u16::from_be_bytes(ttf.get(loca + n * 2..loca + n * 2 + 2)?.try_into().ok()?)
                    as usize
                    * 2,
            ),
            1 => Some(
                u32::from_be_bytes(ttf.get(loca + n * 4..loca + n * 4 + 4)?.try_into().ok()?)
                    as usize,
            ),
            _ => None,
        }
    };
    let start = loc(id)?;
    if start == loc(id + 1)? {
        return Some(Rect::new(origin, origin + Vec2::new(advance, 0.0)));
    }
    let p = glyf + start;
    let read = |at: usize| -> Option<i16> {
        Some(i16::from_be_bytes(
            ttf.get(p + at..p + at + 2)?.try_into().ok()?,
        ))
    };
    let factor = size / upem;
    Some(Rect::new(
        origin + Vec2::new(read(2)? as f32 * factor, read(4)? as f32 * factor),
        origin + Vec2::new(read(6)? as f32 * factor, read(8)? as f32 * factor),
    ))
}
// DVI opcode helpers receive explicit decoder state and bounded output sinks.
// Keep these borrows local instead of introducing another mutable parser owner.
#[allow(clippy::too_many_arguments)]
fn push_rule(
    height: i64,
    width: i64,
    h: i64,
    v: i64,
    scale: f64,
    geometries: &mut GeometryResourceArena,
    vectors: &mut Vec<TextVectorItem>,
    order: &mut Vec<TextRenderItem>,
    span: TextSourceSpan,
    key: Option<Arc<str>>,
    fill: Option<Color>,
) -> Rect {
    let x = h as f64 * scale;
    let base = -(v as f64 * scale);
    let w = width as f64 * scale;
    let height = height as f64 * scale;
    let bounds = Rect::new(
        Vec2::new(x as f32, base as f32),
        Vec2::new((x + w) as f32, (base + height) as f32),
    );
    let path = VectorPath::new()
        .move_to(Vec2::new(x as f32, base as f32))
        .line_to(Vec2::new((x + w) as f32, base as f32))
        .line_to(Vec2::new((x + w) as f32, (base + height) as f32))
        .line_to(Vec2::new(x as f32, (base + height) as f32))
        .close();
    let index = vectors.len() as u32;
    vectors.push(TextVectorItem {
        geometry: geometries.insert_path(path),
        transform: TextAffineTransform::IDENTITY,
        style: TextVectorStyle {
            fill,
            ..TextVectorStyle::default()
        },
        source_span: Some(span),
        semantic_key: key,
    });
    order.push(TextRenderItem::Vector(index));
    bounds
}
fn resource_bounds(runs: &[GlyphRun], vector_bounds: Option<Rect>) -> Option<Rect> {
    runs.iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.bounds))
        .fold(vector_bounds, |out, b| {
            Some(out.map_or(b, |old| old.union(b)))
        })
}

// DVI opcode helpers receive explicit decoder state and bounded output sinks.
// Keep these borrows local instead of introducing another mutable parser owner.
#[allow(clippy::too_many_arguments)]
fn parse_special(
    bytes: &[u8],
    source_len: u32,
    span: &mut TextSourceSpan,
    key: &mut Option<Arc<str>>,
    parts: &mut Vec<TextPart>,
    clusters: u32,
    vectors: u32,
    fill: &mut Option<Color>,
    color_stack: &mut Vec<Option<Color>>,
) -> Result<(), LatexDviError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| LatexDviError::UnknownSemanticSpecial(Arc::from("non UTF-8")))?
        .trim();
    if let Some(command) = value.strip_prefix("color ") {
        let mut fields = command.split_ascii_whitespace();
        match fields.next() {
            Some("push") => {
                color_stack.push(*fill);
                *fill = parse_color(fields)?;
                return Ok(());
            }
            Some("pop") if fields.next().is_none() => {
                *fill = color_stack
                    .pop()
                    .ok_or(LatexDviError::InvalidDvi("color stack underflow"))?;
                return Ok(());
            }
            Some("rgb") | Some("gray") => {
                *fill = parse_color_command(command)?;
                return Ok(());
            }
            _ => return Err(LatexDviError::UnknownSemanticSpecial(Arc::from(value))),
        }
    }
    // These are the only backend metadata specials accepted without a retained
    // counterpart. LaTeX3 emits this exact dvips prologue declaration and
    // dvisvgm emits HiResBoundingBox metadata; both are harmless metadata.
    // Other headers and dvisvgm specials may paint or transform and must be
    // rejected rather than silently dropped.
    if value == "header=l3backend-dvips.pro" {
        return Ok(());
    }
    if let Some(bounds) = value.strip_prefix("ps::%%HiResBoundingBox: ") {
        if bounds.split_ascii_whitespace().count() == 4
            && bounds.split_ascii_whitespace().all(|field| {
                field
                    .strip_suffix("pt")
                    .unwrap_or(field)
                    .parse::<f32>()
                    .is_ok_and(|value| value.is_finite())
            })
        {
            return Ok(());
        }
        return Err(LatexDviError::UnknownSemanticSpecial(Arc::from(value)));
    }
    if value.starts_with("papersize=") {
        return Ok(());
    }
    if !value.starts_with("noon:") {
        return Err(LatexDviError::UnknownSemanticSpecial(Arc::from(value)));
    }
    let mut fields = value.split(':');
    let _ = fields.next();
    let kind = fields.next().unwrap_or_default();
    let start = fields.next().and_then(|n| n.parse::<u32>().ok());
    let end = fields.next().and_then(|n| n.parse::<u32>().ok());
    let semantic_key = fields
        .next()
        .filter(|v| !v.is_empty())
        .map(Arc::<str>::from);
    let valid = |s: u32, e: u32| s <= e && e <= source_len;
    match (kind, start, end) {
        ("source", Some(s), Some(e)) if valid(s, e) => {
            *span = TextSourceSpan::new(s, e);
            *key = semantic_key;
            Ok(())
        }
        ("part", Some(s), Some(e)) if valid(s, e) => {
            if let Some(last) = parts.last_mut() {
                last.cluster_count = clusters
                    .checked_sub(last.first_cluster)
                    .ok_or(LatexDviError::InvalidDvi("part cluster range"))?;
                last.vector_count = vectors
                    .checked_sub(last.first_vector)
                    .ok_or(LatexDviError::InvalidDvi("part vector range"))?;
            }
            parts.push(TextPart {
                source_span: TextSourceSpan::new(s, e),
                first_cluster: clusters,
                cluster_count: 0,
                first_vector: vectors,
                vector_count: 0,
                semantic_key,
            });
            Ok(())
        }
        _ => Err(LatexDviError::UnknownSemanticSpecial(Arc::from(value))),
    }
}
fn parse_color_command(command: &str) -> Result<Option<Color>, LatexDviError> {
    parse_color(command.split_ascii_whitespace())
}
fn parse_color<'a>(
    mut fields: impl Iterator<Item = &'a str>,
) -> Result<Option<Color>, LatexDviError> {
    let kind = fields
        .next()
        .ok_or(LatexDviError::InvalidDvi("missing color"))?;
    let number = |value: Option<&str>| {
        value
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or(LatexDviError::InvalidDvi("invalid color"))
    };
    let color = match kind {
        "rgb" => Color::rgba(
            number(fields.next())?,
            number(fields.next())?,
            number(fields.next())?,
            1.0,
        ),
        "gray" => {
            let c = number(fields.next())?;
            Color::rgba(c, c, c, 1.0)
        }
        _ => return Err(LatexDviError::InvalidDvi("unsupported color model")),
    };
    if fields.next().is_some() {
        return Err(LatexDviError::InvalidDvi("invalid color"));
    }
    Ok(Some(color))
}

struct Tfm {
    bc: u16,
    ec: u16,
    checksum: u32,
    info: Vec<u8>,
    widths: Vec<i32>,
}
impl Tfm {
    fn parse(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() < 24 {
            return Err(());
        }
        let word = |i| u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        let lf = word(0);
        let lh = word(2);
        let bc = word(4);
        let ec = word(6);
        let nw = word(8);
        if bc > ec || lf * 4 > bytes.len() || nw == 0 {
            return Err(());
        }
        let info_start = (6 + lh) * 4;
        let count = ec - bc + 1;
        let width_start = info_start + count * 4;
        if width_start + nw * 4 > bytes.len() {
            return Err(());
        }
        let checksum = if lh >= 1 {
            u32::from_be_bytes(bytes[24..28].try_into().map_err(|_| ())?)
        } else {
            0
        };
        let mut widths = Vec::with_capacity(nw);
        for i in 0..nw {
            widths.push(i32::from_be_bytes(
                bytes[width_start + i * 4..width_start + i * 4 + 4]
                    .try_into()
                    .map_err(|_| ())?,
            ));
        }
        Ok(Self {
            bc: bc as u16,
            ec: ec as u16,
            checksum,
            info: bytes[info_start..width_start].to_vec(),
            widths,
        })
    }
    fn width(&self, code: u32) -> Option<i32> {
        let code = u16::try_from(code).ok()?;
        if code < self.bc || code > self.ec {
            return None;
        }
        let index = (code - self.bc) as usize;
        self.widths.get(self.info[index * 4] as usize).copied()
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn is_end(&self) -> bool {
        self.offset == self.bytes.len()
    }
    fn u8(&mut self) -> Result<u8, LatexDviError> {
        let b = *self
            .bytes
            .get(self.offset)
            .ok_or(LatexDviError::Truncated)?;
        self.offset += 1;
        Ok(b)
    }
    fn u16(&mut self) -> Result<u16, LatexDviError> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, LatexDviError> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], LatexDviError> {
        let end = self.offset.checked_add(n).ok_or(LatexDviError::Truncated)?;
        let out = self
            .bytes
            .get(self.offset..end)
            .ok_or(LatexDviError::Truncated)?;
        self.offset = end;
        Ok(out)
    }
    fn skip(&mut self, n: usize) -> Result<(), LatexDviError> {
        self.bytes(n).map(|_| ())
    }
    fn unsigned(&mut self, n: usize) -> Result<u32, LatexDviError> {
        if !(1..=4).contains(&n) {
            return Err(LatexDviError::InvalidDvi("invalid operand width"));
        }
        let mut v = 0u32;
        for b in self.bytes(n)? {
            v = (v << 8) | *b as u32;
        }
        Ok(v)
    }
    fn signed(&mut self, n: usize) -> Result<i64, LatexDviError> {
        let raw = self.unsigned(n)? as i64;
        let bits = (n * 8) as u32;
        Ok(if raw & (1_i64 << (bits - 1)) != 0 {
            raw - (1_i64 << bits)
        } else {
            raw
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{TextLayoutBackend, TextLayoutBackendKind};

    #[test]
    fn tfm_reads_exact_fixed_widths() {
        // Six header words, no header, one character (65), two width entries.
        let mut bytes = vec![0_u8; 9 * 4];
        for (offset, value) in [(0, 9_u16), (2, 0), (4, 65), (6, 65), (8, 2)] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes[24] = 1;
        bytes[32..36].copy_from_slice(&(1_i32 << 19).to_be_bytes());
        let tfm = Tfm::parse(&bytes).unwrap();
        assert_eq!(tfm.width(65), Some(1_i32 << 19));
        assert_eq!(fixed_mul(tfm.width(65).unwrap(), 2 << 20).unwrap(), 1 << 20);
    }

    #[test]
    fn specials_keep_markers_and_reject_unknown_semantics() {
        let mut span = TextSourceSpan::new(0, 3);
        let mut key = None;
        let mut parts = Vec::new();
        let mut fill = None;
        let mut colors = Vec::new();
        parse_special(
            b"noon:source:1:3:x",
            3,
            &mut span,
            &mut key,
            &mut parts,
            0,
            0,
            &mut fill,
            &mut colors,
        )
        .unwrap();
        assert_eq!(span, TextSourceSpan::new(1, 3));
        assert_eq!(key.as_deref(), Some("x"));
        parse_special(
            b"color push rgb 1 0 0",
            3,
            &mut span,
            &mut key,
            &mut parts,
            0,
            0,
            &mut fill,
            &mut colors,
        )
        .unwrap();
        assert_eq!(fill.unwrap().red, 1.0);
        assert!(matches!(
            parse_special(
                b"ps: raw",
                3,
                &mut span,
                &mut key,
                &mut parts,
                0,
                0,
                &mut fill,
                &mut colors
            ),
            Err(LatexDviError::UnknownSemanticSpecial(_))
        ));
    }

    #[test]
    fn malformed_dvi_is_rejected_before_font_resolution() {
        let artifact = TextLayoutArtifact {
            backend: TextLayoutBackend {
                kind: TextLayoutBackendKind::Latex,
                version: Arc::from(LATEX_DVI_BACKEND_VERSION),
            },
            template_fingerprint: Arc::from("test"),
            artifact_fingerprint: Arc::from("test"),
            backend_payload_key: None,
        };
        assert!(matches!(
            normalize_dvi(
                "x",
                TextSourceKind::MathTex,
                artifact,
                &[247, 3],
                &[],
                DviLimits::default()
            ),
            Err(LatexDviError::UnsupportedVersion(3))
        ));
    }
}
