//! Native OpenType text shaping for Noon.
//!
//! This module is a deterministic backend. Public markup parsing, font resolution,
//! fallback, bidi, and semantic scene ownership remain outside this crate.

use std::{fmt, sync::Arc};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, FontResourceError, FontVariationSetting, GlyphRun,
    PositionedGlyph, Rect, TextAffineTransform, TextClusterIdentity, TextDirection,
    TextLayoutArtifact, TextLayoutBackend, TextLayoutBackendKind, TextPart, TextRenderItem,
    TextResource, TextResourceValidationError, TextSourceKind, TextSourceSpan, Vec2,
};
use swash::{shape::ShapeContext, text::Script, FontRef};

pub const NATIVE_TEXT_BACKEND_VERSION: &str = "swash-0.2.10";
const NATIVE_TEXT_TEMPLATE_VERSION: &str = "noon-native-styled-multiline-v2";
const MANIM_DEFAULT_LINE_SPACING: f32 = 0.3;

/// Exact immutable OpenType face input. `face_key` derives from the bytes and
/// collection index, so a glyph identity never resolves to a different file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeFontFace {
    pub family: Arc<str>,
    pub face_key: Arc<str>,
    pub face_index: u32,
    pub data: Arc<[u8]>,
}

impl NativeFontFace {
    pub fn new(
        family: impl Into<Arc<str>>,
        data: impl Into<Arc<[u8]>>,
        face_index: u32,
    ) -> Result<Self, NativeTextError> {
        let family = family.into();
        let data = data.into();
        FontRef::from_index(data.as_ref(), face_index as usize)
            .ok_or(NativeTextError::InvalidFontFace { face_index })?;
        let face_key = Arc::<str>::from(format!(
            "native-{:016x}-{}",
            fingerprint_u64(data.as_ref()),
            face_index
        ));
        Ok(Self {
            family,
            face_key,
            face_index,
            data,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeTextOptions {
    pub font_size: f32,
    /// `-1` selects Manim's 30% extra spacing; any other value produces
    /// `font_size * (1 + line_spacing)`.
    pub line_spacing: f32,
    pub fill: Option<Color>,
    pub variations: Arc<[FontVariationSetting]>,
}

impl NativeTextOptions {
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size,
            line_spacing: -1.0,
            fill: None,
            variations: Arc::from([]),
        }
    }
}

/// One exact face and optional intrinsic fill for a contiguous decoded UTF-8 range.
///
/// `None` fill inherits `NativeTextOptions::fill`. Styled compilation requires
/// nonempty spans to be ordered and gap-free over all nonempty source bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeTextSpan {
    pub source_span: TextSourceSpan,
    pub font: NativeFontFace,
    pub fill: Option<Color>,
}

#[derive(Clone, Debug)]
pub struct NativeTextResourceArtifact {
    pub resource: TextResource,
    pub fonts: FontResourceArena,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeTextError {
    InvalidFontFace {
        face_index: u32,
    },
    InvalidFontSize,
    InvalidLineSpacing,
    SourceTooLarge,
    InvalidStyleSpan {
        source_span: TextSourceSpan,
    },
    StyleSpanCoverage {
        expected_start: u32,
        found_start: u32,
    },
    UnshapableStyleBoundary {
        offset: u32,
    },
    InvalidResource(TextResourceValidationError),
    FontResource(FontResourceError),
}

impl fmt::Display for NativeTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFontFace { face_index } => write!(
                formatter,
                "invalid OpenType font face at index {face_index}"
            ),
            Self::InvalidFontSize => write!(
                formatter,
                "native text font size must be finite and positive"
            ),
            Self::InvalidLineSpacing => write!(
                formatter,
                "native text line spacing must produce a finite positive line advance"
            ),
            Self::SourceTooLarge => write!(
                formatter,
                "native text source exceeds Noon's text span space"
            ),
            Self::InvalidStyleSpan { source_span } => write!(
                formatter,
                "invalid styled text span {}..{}",
                source_span.start, source_span.end
            ),
            Self::StyleSpanCoverage {
                expected_start,
                found_start,
            } => write!(
                formatter,
                "styled text spans are not gap-free at byte {expected_start} (found {found_start})"
            ),
            Self::UnshapableStyleBoundary { offset } => write!(
                formatter,
                "styled paint boundary at byte {offset} cuts a shaped cluster"
            ),
            Self::InvalidResource(error) => {
                write!(formatter, "invalid normalized text resource: {error}")
            }
            Self::FontResource(error) => write!(formatter, "invalid native font resource: {error}"),
        }
    }
}

impl std::error::Error for NativeTextError {}

#[derive(Clone, Copy)]
struct SourceLine<'a> {
    start: u32,
    text: &'a str,
}

/// Persistent native shaper. Keeping Swash's context alive amortizes internal
/// caches without retaining frontend-specific text state.
pub struct NativeTextCompiler {
    shape_context: ShapeContext,
}

impl NativeTextCompiler {
    pub fn new() -> Self {
        Self {
            shape_context: ShapeContext::new(),
        }
    }

    /// Shape unstyled source through the same core pipeline as markup text.
    pub fn compile_plain(
        &mut self,
        source: &str,
        font: &NativeFontFace,
        options: &NativeTextOptions,
    ) -> Result<NativeTextResourceArtifact, NativeTextError> {
        let source_len =
            u32::try_from(source.len()).map_err(|_| NativeTextError::SourceTooLarge)?;
        let spans = if source.is_empty() {
            Vec::new()
        } else {
            vec![NativeTextSpan {
                source_span: TextSourceSpan::new(0, source_len),
                font: font.clone(),
                fill: None,
            }]
        };
        self.compile(source, font, options, &spans, TextSourceKind::Plain)
    }

    /// Shape decoded markup text using exact faces resolved by the public authoring
    /// layer. Same-font paint changes remain one Swash shaping operation; the
    /// resulting clusters are projected into fill runs only at safe boundaries.
    pub fn compile_styled(
        &mut self,
        source: &str,
        base_font: &NativeFontFace,
        options: &NativeTextOptions,
        spans: &[NativeTextSpan],
    ) -> Result<NativeTextResourceArtifact, NativeTextError> {
        self.compile(source, base_font, options, spans, TextSourceKind::Markup)
    }

    fn compile(
        &mut self,
        source: &str,
        base_font: &NativeFontFace,
        options: &NativeTextOptions,
        spans: &[NativeTextSpan],
        kind: TextSourceKind,
    ) -> Result<NativeTextResourceArtifact, NativeTextError> {
        if !options.font_size.is_finite() || options.font_size <= 0.0 {
            return Err(NativeTextError::InvalidFontSize);
        }
        validate_line_spacing(options)?;
        let source_len =
            u32::try_from(source.len()).map_err(|_| NativeTextError::SourceTooLarge)?;
        validate_style_spans(source, spans, source_len)?;
        let lines = split_source_lines(source)?;
        let variations = options
            .variations
            .iter()
            .map(|setting| (setting.tag, setting.value))
            .collect::<Vec<_>>();
        let line_advance = match kind {
            TextSourceKind::Markup => {
                // ManimCE 0.21 passes this value to ManimPango's legacy ignored
                // positional parameter. Its MarkupText SVG therefore uses Pango's
                // natural baseline advance, including for an explicit option.
                let metrics =
                    font_metrics(&mut self.shape_context, base_font, options, &variations)?;
                metrics.ascent + metrics.descent + metrics.leading
            }
            TextSourceKind::Plain => configured_line_advance(options),
            _ => unreachable!("native compiler only accepts plain or markup source"),
        };

        let mut runs = Vec::new();
        let mut render_items = Vec::new();
        let mut layout_bounds: Option<Rect> = None;
        let mut cluster_ordinal = 0_u32;
        let mut used_fonts = Vec::<(FontFaceIdentity, Arc<[u8]>)>::new();

        for (line_index, line) in lines.iter().copied().enumerate() {
            let baseline_y = -(line_index as f32) * line_advance;
            if line.text.is_empty() {
                let identity = font_identity(base_font, options);
                let metrics =
                    font_metrics(&mut self.shape_context, base_font, options, &variations)?;
                push_run(
                    &mut runs,
                    &mut render_items,
                    identity.clone(),
                    options,
                    options.fill,
                    baseline_y,
                    Vec::new(),
                );
                remember_font(&mut used_fonts, identity, base_font.data.clone());
                let bounds = if source.is_empty() {
                    Rect::new(Vec2::ZERO, Vec2::ZERO)
                } else {
                    Rect::new(
                        Vec2::new(0.0, -metrics.descent + baseline_y),
                        Vec2::new(0.0, metrics.ascent + baseline_y),
                    )
                };
                layout_bounds =
                    Some(layout_bounds.map_or(bounds, |existing| existing.union(bounds)));
                continue;
            }

            let line_end = line.start
                + u32::try_from(line.text.len())
                    .expect("line byte length fits after source validation");
            let line_span_range = spans_for_line(spans, line.start, line_end);
            debug_assert!(!line_span_range.is_empty());
            let mut group_start = line_span_range.start;
            let mut line_cursor_x = 0.0_f32;
            while group_start < line_span_range.end {
                let font = &spans[group_start].font;
                let mut group_end = group_start + 1;
                while group_end < line_span_range.end && &spans[group_end].font == font {
                    group_end += 1;
                }
                let group = &spans[group_start..group_end];
                let group_span = TextSourceSpan::new(
                    group[0].source_span.start.max(line.start),
                    group[group.len() - 1].source_span.end.min(line_end),
                );
                let local_start =
                    usize::try_from(group_span.start - line.start).expect("line offset fits usize");
                let local_end =
                    usize::try_from(group_span.end - line.start).expect("line offset fits usize");
                let identity = font_identity(font, options);
                let shaped = self.shape_group(
                    &line.text[local_start..local_end],
                    group_span.start,
                    group,
                    font,
                    line_cursor_x,
                    options,
                    &variations,
                    &mut cluster_ordinal,
                )?;
                let ShapedGroup {
                    end_x,
                    runs: shaped_runs,
                } = shaped;
                for ShapedRun {
                    fill,
                    glyphs,
                    start_x,
                    end_x,
                    metrics,
                } in shaped_runs
                {
                    let bounds = Rect::new(
                        Vec2::new(start_x.min(end_x), -metrics.descent + baseline_y),
                        Vec2::new(start_x.max(end_x), metrics.ascent + baseline_y),
                    );
                    layout_bounds =
                        Some(layout_bounds.map_or(bounds, |existing| existing.union(bounds)));
                    push_run(
                        &mut runs,
                        &mut render_items,
                        identity.clone(),
                        options,
                        fill,
                        baseline_y,
                        glyphs,
                    );
                }
                remember_font(&mut used_fonts, identity, font.data.clone());
                line_cursor_x = end_x;
                group_start = group_end;
            }
        }

        let layout_bounds = layout_bounds.unwrap_or_else(|| Rect::new(Vec2::ZERO, Vec2::ZERO));
        let center = layout_bounds.center();
        let recenter = TextAffineTransform::translation(-center.x, -center.y);
        for run in &mut runs {
            run.transform = run.transform.then(recenter);
        }
        let resource = TextResource {
            source: Arc::from(source),
            kind,
            runs: runs.into(),
            vector_items: Arc::from([]),
            render_items: render_items.into(),
            parts: Arc::from([TextPart {
                source_span: TextSourceSpan::new(0, source_len),
                first_cluster: 0,
                cluster_count: cluster_ordinal,
                first_vector: 0,
                vector_count: 0,
                semantic_key: None,
            }]),
            bounds: Rect::new(layout_bounds.min - center, layout_bounds.max - center),
            baseline: -center.y,
            layout_artifact: Some(layout_artifact(source, base_font, options, spans, kind)),
        };
        resource
            .validate()
            .map_err(NativeTextError::InvalidResource)?;

        let mut fonts = FontResourceArena::new();
        for (identity, data) in used_fonts {
            fonts
                .intern_face(&identity, data)
                .map_err(NativeTextError::FontResource)?;
        }
        Ok(NativeTextResourceArtifact { resource, fonts })
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_group(
        &mut self,
        text: &str,
        source_start: u32,
        spans: &[NativeTextSpan],
        font: &NativeFontFace,
        initial_x: f32,
        options: &NativeTextOptions,
        variations: &[([u8; 4], f32)],
        cluster_ordinal: &mut u32,
    ) -> Result<ShapedGroup, NativeTextError> {
        let font_ref = FontRef::from_index(font.data.as_ref(), font.face_index as usize).ok_or(
            NativeTextError::InvalidFontFace {
                face_index: font.face_index,
            },
        )?;
        let mut shaper = self
            .shape_context
            .builder_with_id(
                font_ref,
                [
                    fingerprint_u64(font.face_key.as_bytes()),
                    u64::from(font.face_index),
                ],
            )
            .script(Script::Latin)
            .size(options.font_size)
            .variations(variations)
            .build();
        let metrics = shaper.metrics();
        shaper.add_str(text);
        let mut cursor_x = initial_x;
        let mut projected = Vec::<ProjectedRun>::new();
        let mut paint_span = 0_usize;
        let mut boundary_error = None;
        shaper.shape_with(|cluster| {
            let source_span = TextSourceSpan::new(
                source_start.saturating_add(cluster.source.start),
                source_start.saturating_add(cluster.source.end),
            );
            let fill = match cluster_fill(source_span, spans, &mut paint_span, options.fill) {
                Ok(fill) => fill,
                Err(error) => {
                    boundary_error = Some(error);
                    return;
                }
            };
            let ordinal = *cluster_ordinal;
            let run = if projected.last().map(|run| run.fill) == Some(fill) {
                projected.last_mut().expect("last run exists")
            } else {
                projected.push(ProjectedRun {
                    fill,
                    glyphs: Vec::new(),
                    start_x: cursor_x,
                    end_x: cursor_x,
                });
                projected.last_mut().expect("new run exists")
            };
            for glyph in cluster.glyphs {
                let origin = Vec2::new(cursor_x + glyph.x, glyph.y);
                let advance = Vec2::new(glyph.advance, 0.0);
                let right = origin.x + glyph.advance.max(0.0);
                run.glyphs.push(PositionedGlyph {
                    glyph_id: u32::from(glyph.id),
                    cluster: TextClusterIdentity {
                        source_span,
                        cluster_ordinal: ordinal,
                        semantic_key: None,
                    },
                    origin,
                    advance,
                    bounds: Rect::new(
                        Vec2::new(origin.x.min(right), -metrics.descent),
                        Vec2::new(origin.x.max(right), metrics.ascent),
                    ),
                });
            }
            *cluster_ordinal = cluster_ordinal.saturating_add(1);
            cursor_x += cluster.advance();
            run.end_x = cursor_x;
        });
        if let Some(error) = boundary_error {
            return Err(error);
        }
        let runs = projected
            .into_iter()
            .map(|run| ShapedRun {
                fill: run.fill,
                glyphs: run.glyphs,
                start_x: run.start_x,
                end_x: run.end_x,
                metrics,
            })
            .collect();
        Ok(ShapedGroup {
            end_x: cursor_x,
            runs,
        })
    }
}

impl Default for NativeTextCompiler {
    fn default() -> Self {
        Self::new()
    }
}

struct ProjectedRun {
    fill: Option<Color>,
    glyphs: Vec<PositionedGlyph>,
    start_x: f32,
    end_x: f32,
}
struct ShapedGroup {
    end_x: f32,
    runs: Vec<ShapedRun>,
}

struct ShapedRun {
    fill: Option<Color>,
    glyphs: Vec<PositionedGlyph>,
    start_x: f32,
    end_x: f32,
    metrics: swash::Metrics,
}

fn push_run(
    runs: &mut Vec<GlyphRun>,
    render_items: &mut Vec<TextRenderItem>,
    font: FontFaceIdentity,
    options: &NativeTextOptions,
    fill: Option<Color>,
    baseline_y: f32,
    glyphs: Vec<PositionedGlyph>,
) {
    let index = u32::try_from(runs.len()).expect("native text run count exceeds retained limits");
    runs.push(GlyphRun {
        font,
        variations: options.variations.clone(),
        font_size: options.font_size,
        direction: TextDirection::LeftToRight,
        fill,
        stroke: None,
        transform: TextAffineTransform::translation(0.0, baseline_y),
        glyphs: glyphs.into(),
    });
    render_items.push(TextRenderItem::GlyphRun(index));
}

fn font_identity(font: &NativeFontFace, options: &NativeTextOptions) -> FontFaceIdentity {
    FontFaceIdentity {
        family: font.family.clone(),
        face_key: font.face_key.clone(),
        face_index: font.face_index,
        variation_key: Arc::from(variation_identity(options.variations.as_ref())),
    }
}

fn remember_font(
    fonts: &mut Vec<(FontFaceIdentity, Arc<[u8]>)>,
    identity: FontFaceIdentity,
    data: Arc<[u8]>,
) {
    if !fonts.iter().any(|(existing, _)| existing == &identity) {
        fonts.push((identity, data));
    }
}

fn font_metrics(
    context: &mut ShapeContext,
    font: &NativeFontFace,
    options: &NativeTextOptions,
    variations: &[([u8; 4], f32)],
) -> Result<swash::Metrics, NativeTextError> {
    let font_ref = FontRef::from_index(font.data.as_ref(), font.face_index as usize).ok_or(
        NativeTextError::InvalidFontFace {
            face_index: font.face_index,
        },
    )?;
    Ok(context
        .builder_with_id(
            font_ref,
            [
                fingerprint_u64(font.face_key.as_bytes()),
                u64::from(font.face_index),
            ],
        )
        .script(Script::Latin)
        .size(options.font_size)
        .variations(variations)
        .build()
        .metrics())
}

fn validate_style_spans(
    source: &str,
    spans: &[NativeTextSpan],
    source_len: u32,
) -> Result<(), NativeTextError> {
    if source.is_empty() {
        if let Some(span) = spans.first() {
            return Err(NativeTextError::InvalidStyleSpan {
                source_span: span.source_span,
            });
        }
        return Ok(());
    }
    let mut expected = 0_u32;
    for span in spans {
        let range = span.source_span;
        let valid = range.start < range.end
            && range.end <= source_len
            && source.is_char_boundary(range.start as usize)
            && source.is_char_boundary(range.end as usize);
        if !valid {
            return Err(NativeTextError::InvalidStyleSpan { source_span: range });
        }
        if range.start != expected {
            return Err(NativeTextError::StyleSpanCoverage {
                expected_start: expected,
                found_start: range.start,
            });
        }
        expected = range.end;
    }
    if expected != source_len {
        return Err(NativeTextError::StyleSpanCoverage {
            expected_start: expected,
            found_start: source_len,
        });
    }
    Ok(())
}

fn spans_for_line(
    spans: &[NativeTextSpan],
    line_start: u32,
    line_end: u32,
) -> std::ops::Range<usize> {
    let start = spans.partition_point(|span| span.source_span.end <= line_start);
    let end = spans.partition_point(|span| span.source_span.start < line_end);
    start..end
}

/// Returns one effective paint for a complete shaped cluster while advancing over
/// ordered spans only once. A same-face cluster may cross author paint spans when
/// their resolved fills agree; only a paint change inside that cluster is lossy.
fn cluster_fill(
    source_span: TextSourceSpan,
    spans: &[NativeTextSpan],
    span_cursor: &mut usize,
    inherited: Option<Color>,
) -> Result<Option<Color>, NativeTextError> {
    while spans
        .get(*span_cursor)
        .is_some_and(|span| span.source_span.end <= source_span.start)
    {
        *span_cursor += 1;
    }
    let Some(first) = spans.get(*span_cursor) else {
        return Err(NativeTextError::UnshapableStyleBoundary {
            offset: source_span.start,
        });
    };
    if source_span.start < first.source_span.start {
        return Err(NativeTextError::UnshapableStyleBoundary {
            offset: source_span.start,
        });
    }

    let fill = first.fill.or(inherited);
    let mut index = *span_cursor;
    while source_span.end > spans[index].source_span.end {
        let boundary = spans[index].source_span.end;
        index += 1;
        let Some(next) = spans.get(index) else {
            return Err(NativeTextError::UnshapableStyleBoundary { offset: boundary });
        };
        if next.fill.or(inherited) != fill {
            return Err(NativeTextError::UnshapableStyleBoundary { offset: boundary });
        }
    }
    *span_cursor = index;
    Ok(fill)
}

fn validate_line_spacing(options: &NativeTextOptions) -> Result<(), NativeTextError> {
    if !options.line_spacing.is_finite() {
        return Err(NativeTextError::InvalidLineSpacing);
    }
    let advance = configured_line_advance(options);
    if !advance.is_finite() || advance <= 0.0 {
        return Err(NativeTextError::InvalidLineSpacing);
    }
    Ok(())
}

fn configured_line_advance(options: &NativeTextOptions) -> f32 {
    let extra = if options.line_spacing == -1.0 {
        MANIM_DEFAULT_LINE_SPACING
    } else {
        options.line_spacing
    };
    options.font_size * (1.0 + extra)
}

fn split_source_lines(source: &str) -> Result<Vec<SourceLine<'_>>, NativeTextError> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0_usize;
    let mut index = 0_usize;
    while index < bytes.len() {
        let separator_len = match bytes[index] {
            b'\n' => 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(SourceLine {
            start: u32::try_from(start).map_err(|_| NativeTextError::SourceTooLarge)?,
            text: &source[start..index],
        });
        index += separator_len;
        start = index;
    }
    lines.push(SourceLine {
        start: u32::try_from(start).map_err(|_| NativeTextError::SourceTooLarge)?,
        text: &source[start..],
    });
    Ok(lines)
}

fn layout_artifact(
    source: &str,
    base_font: &NativeFontFace,
    options: &NativeTextOptions,
    spans: &[NativeTextSpan],
    kind: TextSourceKind,
) -> TextLayoutArtifact {
    let mut identity = format!("{NATIVE_TEXT_BACKEND_VERSION}\0{NATIVE_TEXT_TEMPLATE_VERSION}\0{:?}\0{}\0{}\0{:08x}\0{:08x}\0", kind, base_font.face_key, base_font.face_index, options.font_size.to_bits(), options.line_spacing.to_bits());
    append_color_identity(&mut identity, options.fill);
    for setting in options.variations.iter() {
        identity.push('\0');
        for byte in setting.tag {
            identity.push(char::from(byte));
        }
        identity.push('=');
        identity.push_str(&format!("{:08x}", setting.value.to_bits()));
    }
    for span in spans {
        identity.push_str(&format!(
            "\0{}:{}:{}:{}",
            span.source_span.start, span.source_span.end, span.font.face_key, span.font.face_index
        ));
        append_color_identity(&mut identity, span.fill);
    }
    identity.push('\0');
    identity.push_str(source);
    TextLayoutArtifact {
        backend: TextLayoutBackend {
            kind: TextLayoutBackendKind::NativeText,
            version: Arc::from(NATIVE_TEXT_BACKEND_VERSION),
        },
        template_fingerprint: Arc::from(format!(
            "{:016x}",
            fingerprint_u64(NATIVE_TEXT_TEMPLATE_VERSION.as_bytes())
        )),
        artifact_fingerprint: Arc::from(format!("{:016x}", fingerprint_u64(identity.as_bytes()))),
        backend_payload_key: None,
    }
}

fn append_color_identity(identity: &mut String, color: Option<Color>) {
    match color {
        Some(color) => identity.push_str(&format!(
            ":{:08x}:{:08x}:{:08x}:{:08x}",
            color.red.to_bits(),
            color.green.to_bits(),
            color.blue.to_bits(),
            color.alpha.to_bits()
        )),
        None => identity.push_str(":inherit"),
    }
}

fn variation_identity(settings: &[FontVariationSetting]) -> String {
    let mut identity = String::new();
    for setting in settings {
        if !identity.is_empty() {
            identity.push(';');
        }
        for byte in setting.tag {
            identity.push(char::from(byte));
        }
        identity.push('=');
        identity.push_str(&format!("{:08x}", setting.value.to_bits()));
    }
    identity
}

fn fingerprint_u64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn bundled_font() -> NativeFontFace {
        let bytes = typst_assets::fonts()
            .next()
            .expect("Typst test assets include at least one font");
        NativeFontFace::new("Bundled Test Font", Arc::<[u8]>::from(bytes), 0).unwrap()
    }
    fn alternate_font() -> NativeFontFace {
        let bytes = typst_assets::fonts()
            .nth(1)
            .expect("Typst test assets include an alternate font");
        NativeFontFace::new("Bundled Alternate Font", Arc::<[u8]>::from(bytes), 0).unwrap()
    }
    fn libertinus_serif_regular_font() -> NativeFontFace {
        let bytes = typst_assets::fonts()
            .next()
            .expect("Typst test assets include Libertinus Serif Regular first");
        NativeFontFace::new("Libertinus Serif Regular", Arc::<[u8]>::from(bytes), 0).unwrap()
    }
    fn spans(
        _source: &str,
        font: &NativeFontFace,
        cuts: &[(usize, Option<Color>)],
    ) -> Vec<NativeTextSpan> {
        let mut start = 0;
        cuts.iter()
            .map(|(end, fill)| {
                let span = NativeTextSpan {
                    source_span: TextSourceSpan::new(start as u32, *end as u32),
                    font: font.clone(),
                    fill: *fill,
                };
                start = *end;
                span
            })
            .collect()
    }

    #[test]
    fn plain_text_shapes_directly_into_backend_neutral_resource() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("Hello", &font, &NativeTextOptions::new(32.0))
            .unwrap();
        assert_eq!(artifact.resource.kind, TextSourceKind::Plain);
        assert_eq!(artifact.resource.runs.len(), 1);
        assert!(artifact.resource.glyph_count() >= 5);
        assert_eq!(artifact.resource.vector_count(), 0);
        assert_eq!(
            artifact.resource.render_items.as_ref(),
            &[TextRenderItem::GlyphRun(0)]
        );
        assert_eq!(artifact.fonts.len(), 1);
        assert!(artifact
            .fonts
            .get_for_face(&artifact.resource.runs[0].font)
            .is_some());
        assert_eq!(
            artifact
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .backend
                .kind,
            TextLayoutBackendKind::NativeText
        );
    }
    #[test]
    fn source_spans_are_utf8_byte_ranges() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("café", &font, &NativeTextOptions::new(24.0))
            .unwrap();
        for glyph in artifact.resource.runs[0].glyphs.iter() {
            assert!(glyph.cluster.source_span.start <= glyph.cluster.source_span.end);
            assert!(glyph.cluster.source_span.end <= "café".len() as u32);
            assert!("café".is_char_boundary(glyph.cluster.source_span.start as usize));
            assert!("café".is_char_boundary(glyph.cluster.source_span.end as usize));
        }
    }
    #[test]
    fn glyphs_from_one_shaped_cluster_share_one_cluster_ordinal() {
        let source = "q\u{0301}b";
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain(source, &font, &NativeTextOptions::new(24.0))
            .unwrap();
        let mut by_span = BTreeMap::<TextSourceSpan, Vec<u32>>::new();
        for glyph in artifact.resource.runs[0].glyphs.iter() {
            by_span
                .entry(glyph.cluster.source_span)
                .or_default()
                .push(glyph.cluster.cluster_ordinal);
        }
        let multi = by_span
            .values()
            .find(|ordinals| ordinals.len() > 1)
            .expect("combining mark fixture shapes a multi-glyph cluster");
        assert!(multi.iter().all(|ordinal| *ordinal == multi[0]));
    }
    #[test]
    fn multiline_layout_preserves_global_source_offsets_and_line_runs() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("A\r\ncafé", &font, &NativeTextOptions::new(24.0))
            .unwrap();
        assert_eq!(artifact.resource.runs.len(), 2);
        assert!(artifact.resource.runs[1].glyphs.iter().all(|glyph| glyph
            .cluster
            .source_span
            .start
            >= 3));
        assert!(artifact.resource.bounds.height() > 24.0);
    }
    #[test]
    fn blank_lines_are_retained_as_layout_spacing_without_fake_glyphs() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("A\n\nB", &font, &NativeTextOptions::new(20.0))
            .unwrap();
        assert_eq!(artifact.resource.runs.len(), 3);
        assert!(artifact.resource.runs[1].glyphs.is_empty());
        assert_eq!(artifact.resource.glyph_count(), 2);
    }
    #[test]
    fn styled_faces_and_fills_create_valid_markup_resource_and_intern_all_faces() {
        let first = bundled_font();
        let second = alternate_font();
        let source = "Aβ";
        let style_spans = vec![
            NativeTextSpan {
                source_span: TextSourceSpan::new(0, 1),
                font: first.clone(),
                fill: Some(Color::RED),
            },
            NativeTextSpan {
                source_span: TextSourceSpan::new(1, source.len() as u32),
                font: second.clone(),
                fill: Some(Color::BLUE),
            },
        ];
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_styled(source, &first, &NativeTextOptions::new(24.0), &style_spans)
            .unwrap();
        assert_eq!(artifact.resource.kind, TextSourceKind::Markup);
        assert_eq!(artifact.resource.runs.len(), 2);
        assert_eq!(artifact.resource.runs[0].fill, Some(Color::RED));
        assert_eq!(artifact.resource.runs[1].fill, Some(Color::BLUE));
        assert_eq!(artifact.fonts.len(), 2);
        assert!(
            artifact.resource.runs[1].glyphs[0].origin.x
                > artifact.resource.runs[0].glyphs[0].origin.x
        );
        assert!(artifact
            .resource
            .runs
            .iter()
            .all(|run| artifact.fonts.get_for_face(&run.font).is_some()));
        assert!(artifact
            .resource
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .all(|glyph| glyph.cluster.source_span.end <= source.len() as u32));
        artifact.resource.validate().unwrap();
    }
    #[test]
    fn fill_only_style_boundaries_keep_shaping_together_then_split_runs() {
        let font = bundled_font();
        let source = "AB";
        let style_spans = spans(
            source,
            &font,
            &[(1, Some(Color::RED)), (2, Some(Color::BLUE))],
        );
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_styled(source, &font, &NativeTextOptions::new(24.0), &style_spans)
            .unwrap();
        assert_eq!(artifact.resource.runs.len(), 2);
        assert_eq!(artifact.resource.runs[0].fill, Some(Color::RED));
        assert_eq!(artifact.resource.runs[1].fill, Some(Color::BLUE));
    }
    #[test]
    fn ligature_paint_boundaries_allow_equal_fill_and_reject_different_fill() {
        // Typst's first bundled face is Libertinus Serif Regular, which has an
        // `ffi` ligature. Assert that Swash reports the complete source cluster
        // before checking the no-silent-repartitioning contract.
        let font = libertinus_serif_regular_font();
        let source = "ffi";
        let mut options = NativeTextOptions::new(24.0);
        options.fill = Some(Color::RED);
        let mut compiler = NativeTextCompiler::new();
        let plain = compiler.compile_plain(source, &font, &options).unwrap();
        assert!(plain.resource.runs[0].glyphs.iter().any(|glyph| {
            glyph.cluster.source_span == TextSourceSpan::new(0, source.len() as u32)
        }));

        let equal_fill = spans(
            source,
            &font,
            &[(1, None), (source.len(), Some(Color::RED))],
        );
        let styled = compiler
            .compile_styled(source, &font, &options, &equal_fill)
            .unwrap();
        assert_eq!(styled.resource.runs, plain.resource.runs);
        assert_eq!(styled.resource.bounds, plain.resource.bounds);
        assert_eq!(styled.resource.baseline, plain.resource.baseline);
        assert_eq!(styled.resource.render_items, plain.resource.render_items);

        let different_fill = spans(
            source,
            &font,
            &[(1, None), (source.len(), Some(Color::BLUE))],
        );
        assert!(matches!(
            compiler.compile_styled(source, &font, &options, &different_fill),
            Err(NativeTextError::UnshapableStyleBoundary { offset: 1 })
        ));
    }

    #[test]
    fn styled_multiline_clusters_keep_global_decoded_byte_offsets() {
        let font = bundled_font();
        let source = "A\nβ";
        let style_spans = spans(
            source,
            &font,
            &[(1, Some(Color::RED)), (source.len(), Some(Color::BLUE))],
        );
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_styled(source, &font, &NativeTextOptions::new(24.0), &style_spans)
            .unwrap();
        assert!(artifact
            .resource
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .any(|glyph| glyph.cluster.source_span.start >= 2));
        artifact.resource.validate().unwrap();
    }
    #[test]
    fn markup_uses_pango_natural_leading_and_ignores_legacy_spacing_argument() {
        let font = bundled_font();
        let source = "A\nB";
        let style_spans = spans(source, &font, &[(source.len(), None)]);
        let mut compiler = NativeTextCompiler::new();
        let default = compiler
            .compile_styled(source, &font, &NativeTextOptions::new(42.0), &style_spans)
            .unwrap();
        let mut explicit_options = NativeTextOptions::new(42.0);
        explicit_options.line_spacing = 0.8;
        let explicit = compiler
            .compile_styled(source, &font, &explicit_options, &style_spans)
            .unwrap();

        let default_delta =
            default.resource.runs[0].transform.ty - default.resource.runs[1].transform.ty;
        let explicit_delta =
            explicit.resource.runs[0].transform.ty - explicit.resource.runs[1].transform.ty;
        let metrics =
            font_metrics(&mut compiler.shape_context, &font, &explicit_options, &[]).unwrap();
        let natural_advance = metrics.ascent + metrics.descent + metrics.leading;

        assert!((default_delta - natural_advance).abs() < 1e-4);
        assert!((explicit_delta - natural_advance).abs() < 1e-4);
        assert_eq!(default.resource.bounds, explicit.resource.bounds);
    }
    #[test]
    fn invalid_style_ranges_and_gaps_are_rejected() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let source = "café";
        let invalid = vec![NativeTextSpan {
            source_span: TextSourceSpan::new(3, 4),
            font: font.clone(),
            fill: None,
        }];
        assert!(matches!(
            compiler.compile_styled(source, &font, &NativeTextOptions::new(20.0), &invalid),
            Err(NativeTextError::InvalidStyleSpan { .. })
        ));
        let gapped = vec![
            NativeTextSpan {
                source_span: TextSourceSpan::new(0, 1),
                font: font.clone(),
                fill: None,
            },
            NativeTextSpan {
                source_span: TextSourceSpan::new(2, source.len() as u32),
                font,
                fill: None,
            },
        ];
        assert!(matches!(
            compiler.compile_styled(
                source,
                &bundled_font(),
                &NativeTextOptions::new(20.0),
                &gapped
            ),
            Err(NativeTextError::StyleSpanCoverage { .. })
        ));
    }
    #[test]
    fn alternate_face_changes_layout_artifact_identity() {
        let font = bundled_font();
        let alternate = alternate_font();
        let source = "AB";
        let first = spans(source, &font, &[(2, None)]);
        let second = vec![NativeTextSpan {
            source_span: TextSourceSpan::new(0, 2),
            font: alternate,
            fill: None,
        }];
        let mut compiler = NativeTextCompiler::new();
        let one = compiler
            .compile_styled(source, &font, &NativeTextOptions::new(20.0), &first)
            .unwrap();
        let two = compiler
            .compile_styled(source, &font, &NativeTextOptions::new(20.0), &second)
            .unwrap();
        assert_ne!(
            one.resource.layout_artifact.unwrap().artifact_fingerprint,
            two.resource.layout_artifact.unwrap().artifact_fingerprint
        );
    }
}
