#![forbid(unsafe_code)]

//! Native OpenType text shaping for Noon.
//!
//! This crate is a backend, not a renderer. It accepts exact immutable font bytes,
//! shapes source text with Swash, and normalizes the result directly into Noon's
//! backend-neutral `TextResource` + `FontResourceArena` contract. Public Manim-style
//! font discovery, markup spans, fallback, and richer script itemization are layered
//! above this low-level deterministic boundary.

use std::{fmt, sync::Arc};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, FontResourceError, FontVariationSetting, GlyphRun,
    PositionedGlyph, Rect, TextAffineTransform, TextClusterIdentity, TextDirection,
    TextLayoutArtifact, TextLayoutBackend, TextLayoutBackendKind, TextPart, TextRenderItem,
    TextResource, TextResourceValidationError, TextSourceKind, TextSourceSpan, Vec2,
};
use swash::{shape::ShapeContext, text::Script, FontRef};

pub const NATIVE_TEXT_BACKEND_VERSION: &str = "swash-0.2.10";
const NATIVE_TEXT_TEMPLATE_VERSION: &str = "noon-native-multiline-v1";
const MANIM_DEFAULT_LINE_SPACING: f32 = 0.3;

/// Exact immutable OpenType face input. `face_key` is derived from the bytes and
/// collection index so a shaped glyph identity can never silently resolve to a
/// different font file.
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
    /// Manim-compatible line-spacing input. `-1` selects the default 30% extra
    /// spacing; any other value produces a line advance of
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

#[derive(Clone, Debug)]
pub struct NativeTextResourceArtifact {
    pub resource: TextResource,
    pub fonts: FontResourceArena,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeTextError {
    InvalidFontFace { face_index: u32 },
    InvalidFontSize,
    InvalidLineSpacing,
    SourceTooLarge,
    InvalidResource(TextResourceValidationError),
    FontResource(FontResourceError),
}

impl fmt::Display for NativeTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFontFace { face_index } => {
                write!(
                    formatter,
                    "invalid OpenType font face at index {face_index}"
                )
            }
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

/// Persistent native shaper. Keeping the Swash context alive amortizes its internal
/// shaping caches across repeated compilation without retaining per-object glyph
/// state in the frontend.
pub struct NativeTextCompiler {
    shape_context: ShapeContext,
}

impl NativeTextCompiler {
    pub fn new() -> Self {
        Self {
            shape_context: ShapeContext::new(),
        }
    }

    /// Shape deterministic LTR text directly into backend-neutral retained runs.
    ///
    /// Newlines are layout boundaries rather than glyphs. Each visual line becomes
    /// one `GlyphRun`, while source-cluster spans continue to address the original
    /// UTF-8 input including bytes before/after CR/LF separators. Script itemization,
    /// bidi reordering, fallback and styled run splitting remain explicit follow-up
    /// work instead of being approximated in frontend wrappers.
    pub fn compile_plain(
        &mut self,
        source: &str,
        font: &NativeFontFace,
        options: &NativeTextOptions,
    ) -> Result<NativeTextResourceArtifact, NativeTextError> {
        if !options.font_size.is_finite() || options.font_size <= 0.0 {
            return Err(NativeTextError::InvalidFontSize);
        }
        let line_advance = line_advance(options)?;
        let source_len =
            u32::try_from(source.len()).map_err(|_| NativeTextError::SourceTooLarge)?;
        let lines = split_source_lines(source)?;
        let variations = options
            .variations
            .iter()
            .map(|setting| (setting.tag, setting.value))
            .collect::<Vec<_>>();
        let font_identity = FontFaceIdentity {
            family: font.family.clone(),
            face_key: font.face_key.clone(),
            face_index: font.face_index,
            variation_key: Arc::from(variation_identity(options.variations.as_ref())),
        };

        let mut runs = Vec::with_capacity(lines.len());
        let mut render_items = Vec::with_capacity(lines.len());
        let mut cluster_ordinal = 0_u32;
        let mut layout_bounds: Option<Rect> = None;

        for (line_index, line) in lines.iter().copied().enumerate() {
            let font_ref = FontRef::from_index(font.data.as_ref(), font.face_index as usize)
                .ok_or(NativeTextError::InvalidFontFace {
                    face_index: font.face_index,
                })?;
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
                .variations(&variations)
                .build();
            let metrics = shaper.metrics();
            shaper.add_str(line.text);

            let mut glyphs = Vec::new();
            let mut cursor_x = 0.0_f32;
            shaper.shape_with(|cluster| {
                let source_span = TextSourceSpan::new(
                    line.start.saturating_add(cluster.source.start),
                    line.start.saturating_add(cluster.source.end),
                );
                let shaped_cluster_ordinal = cluster_ordinal;
                for glyph in cluster.glyphs {
                    let origin = Vec2::new(cursor_x + glyph.x, glyph.y);
                    let advance = Vec2::new(glyph.advance, 0.0);
                    let right = origin.x + glyph.advance.max(0.0);
                    glyphs.push(PositionedGlyph {
                        glyph_id: u32::from(glyph.id),
                        cluster: TextClusterIdentity {
                            source_span,
                            cluster_ordinal: shaped_cluster_ordinal,
                            semantic_key: None,
                        },
                        origin,
                        advance,
                        // Swash reports descent as a positive distance below the baseline;
                        // retained Y coordinates use negative values below the baseline.
                        bounds: Rect::new(
                            Vec2::new(origin.x.min(right), -metrics.descent),
                            Vec2::new(origin.x.max(right), metrics.ascent),
                        ),
                    });
                }
                cluster_ordinal = cluster_ordinal.saturating_add(1);
                cursor_x += cluster.advance();
            });

            let baseline_y = -(line_index as f32) * line_advance;
            let line_bounds = if source.is_empty() {
                Rect::new(Vec2::ZERO, Vec2::ZERO)
            } else {
                Rect::new(
                    Vec2::new(0.0_f32.min(cursor_x), -metrics.descent + baseline_y),
                    Vec2::new(0.0_f32.max(cursor_x), metrics.ascent + baseline_y),
                )
            };
            layout_bounds = Some(match layout_bounds {
                Some(bounds) => bounds.union(line_bounds),
                None => line_bounds,
            });

            let run_index = u32::try_from(runs.len())
                .expect("native text line count exceeds u32 retained run limits");
            runs.push(GlyphRun {
                font: font_identity.clone(),
                variations: options.variations.clone(),
                font_size: options.font_size,
                direction: TextDirection::LeftToRight,
                fill: options.fill,
                stroke: None,
                transform: TextAffineTransform::translation(0.0, baseline_y),
                glyphs: glyphs.into(),
            });
            render_items.push(TextRenderItem::GlyphRun(run_index));
        }

        let layout_bounds = layout_bounds.unwrap_or_else(|| Rect::new(Vec2::ZERO, Vec2::ZERO));
        let center = layout_bounds.center();
        let recenter = TextAffineTransform::translation(-center.x, -center.y);
        for run in &mut runs {
            run.transform = run.transform.then(recenter);
        }
        let centered_bounds = Rect::new(layout_bounds.min - center, layout_bounds.max - center);
        let full_span = TextSourceSpan::new(0, source_len);
        let resource = TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: runs.into(),
            vector_items: Arc::from([]),
            render_items: render_items.into(),
            parts: Arc::from([TextPart {
                source_span: full_span,
                first_cluster: 0,
                cluster_count: cluster_ordinal,
                first_vector: 0,
                vector_count: 0,
                semantic_key: None,
            }]),
            bounds: centered_bounds,
            // The first visual line owns the resource baseline before recentering.
            baseline: -center.y,
            layout_artifact: Some(layout_artifact(source, font, options)),
        };
        resource
            .validate()
            .map_err(NativeTextError::InvalidResource)?;

        let mut fonts = FontResourceArena::new();
        fonts
            .intern_face(&font_identity, font.data.clone())
            .map_err(NativeTextError::FontResource)?;
        Ok(NativeTextResourceArtifact { resource, fonts })
    }
}

impl Default for NativeTextCompiler {
    fn default() -> Self {
        Self::new()
    }
}

fn line_advance(options: &NativeTextOptions) -> Result<f32, NativeTextError> {
    if !options.line_spacing.is_finite() {
        return Err(NativeTextError::InvalidLineSpacing);
    }
    let extra = if options.line_spacing == -1.0 {
        MANIM_DEFAULT_LINE_SPACING
    } else {
        options.line_spacing
    };
    let advance = options.font_size * (1.0 + extra);
    if !advance.is_finite() || advance <= 0.0 {
        return Err(NativeTextError::InvalidLineSpacing);
    }
    Ok(advance)
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
    font: &NativeFontFace,
    options: &NativeTextOptions,
) -> TextLayoutArtifact {
    let mut identity = format!(
        "{NATIVE_TEXT_BACKEND_VERSION}\0{NATIVE_TEXT_TEMPLATE_VERSION}\0{}\0{}\0{:08x}\0{:08x}\0{source}",
        font.face_key,
        font.face_index,
        options.font_size.to_bits(),
        options.line_spacing.to_bits()
    );
    for setting in options.variations.iter() {
        identity.push('\0');
        for byte in setting.tag {
            identity.push(char::from(byte));
        }
        identity.push('=');
        identity.push_str(&format!("{:08x}", setting.value.to_bits()));
    }
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
    use std::collections::BTreeMap;

    use super::*;

    fn bundled_font() -> NativeFontFace {
        let bytes = typst_assets::fonts()
            .next()
            .expect("Typst test assets include at least one font");
        NativeFontFace::new("Bundled Test Font", Arc::<[u8]>::from(bytes), 0).unwrap()
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
        let source = "a\u{0301}b";
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
        let multi_glyph_cluster = by_span
            .values()
            .find(|ordinals| ordinals.len() > 1)
            .expect("combining-mark fixture must shape at least one multi-glyph cluster");
        assert!(multi_glyph_cluster
            .iter()
            .all(|ordinal| *ordinal == multi_glyph_cluster[0]));

        let mut first_ordinals = by_span
            .values()
            .map(|ordinals| ordinals[0])
            .collect::<Vec<_>>();
        first_ordinals.sort_unstable();
        first_ordinals.dedup();
        assert_eq!(first_ordinals, (0..first_ordinals.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn multiline_layout_preserves_global_source_offsets_and_line_runs() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("A\r\ncafé", &font, &NativeTextOptions::new(24.0))
            .unwrap();

        assert_eq!(artifact.resource.runs.len(), 2);
        assert_eq!(
            artifact.resource.render_items.as_ref(),
            &[TextRenderItem::GlyphRun(0), TextRenderItem::GlyphRun(1)]
        );
        assert!(!artifact.resource.runs[1].glyphs.is_empty());
        assert!(artifact.resource.runs[1].glyphs.iter().all(|glyph| glyph
            .cluster
            .source_span
            .start
            >= 3));
        assert!(artifact.resource.bounds.height() > 24.0);
    }

    #[test]
    fn manim_line_spacing_semantics_change_multiline_height() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let mut tight = NativeTextOptions::new(24.0);
        tight.line_spacing = 0.0;
        let mut wide = NativeTextOptions::new(24.0);
        wide.line_spacing = 4.0;

        let tight = compiler.compile_plain("A\nB", &font, &tight).unwrap();
        let wide = compiler.compile_plain("A\nB", &font, &wide).unwrap();
        assert!(wide.resource.bounds.height() > tight.resource.bounds.height());
        assert!((line_advance(&NativeTextOptions::new(20.0)).unwrap() - 26.0).abs() < 1e-5);
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
        assert!(artifact.resource.bounds.height() > 40.0);
    }

    #[test]
    fn identical_input_has_deterministic_font_and_layout_identity() {
        let first_font = bundled_font();
        let second_font = bundled_font();
        assert_eq!(first_font.face_key, second_font.face_key);

        let options = NativeTextOptions::new(28.0);
        let mut compiler = NativeTextCompiler::new();
        let first = compiler
            .compile_plain("Noon", &first_font, &options)
            .unwrap();
        let second = compiler
            .compile_plain("Noon", &second_font, &options)
            .unwrap();
        assert_eq!(first.resource.runs, second.resource.runs);
        assert_eq!(
            first
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint,
            second
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint
        );
    }

    #[test]
    fn line_spacing_participates_in_layout_identity() {
        let font = bundled_font();
        let mut first_options = NativeTextOptions::new(24.0);
        first_options.line_spacing = 0.0;
        let mut second_options = first_options.clone();
        second_options.line_spacing = 2.0;
        let mut compiler = NativeTextCompiler::new();
        let first = compiler
            .compile_plain("A\nB", &font, &first_options)
            .unwrap();
        let second = compiler
            .compile_plain("A\nB", &font, &second_options)
            .unwrap();
        assert_ne!(
            first
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint,
            second
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint
        );
    }

    #[test]
    fn layout_is_centered_without_outlining_glyphs() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler
            .compile_plain("Center", &font, &NativeTextOptions::new(20.0))
            .unwrap();
        let center = artifact.resource.bounds.center();
        assert!(center.x.abs() < 1e-5);
        assert!(center.y.abs() < 1e-5);
        assert!(artifact.resource.bounds.width() > 0.0);
        assert!(artifact.resource.bounds.height() > 0.0);
    }

    #[test]
    fn invalid_font_size_and_line_spacing_fail_explicitly() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        assert_eq!(
            compiler
                .compile_plain("a", &font, &NativeTextOptions::new(0.0))
                .unwrap_err(),
            NativeTextError::InvalidFontSize
        );
        let mut invalid_spacing = NativeTextOptions::new(20.0);
        invalid_spacing.line_spacing = -1.5;
        assert_eq!(
            compiler
                .compile_plain("a\nb", &font, &invalid_spacing)
                .unwrap_err(),
            NativeTextError::InvalidLineSpacing
        );
    }
}
