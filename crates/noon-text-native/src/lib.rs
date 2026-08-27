#![forbid(unsafe_code)]

//! Native OpenType text shaping for Noon.
//!
//! This crate is a backend, not a renderer. It accepts exact immutable font bytes,
//! shapes source text with Swash, and normalizes the result directly into Noon's
//! backend-neutral `TextResource` + `FontResourceArena` contract. Public Manim-style
//! font discovery, multiline layout, markup spans, and fallback are layered above
//! this low-level deterministic boundary.

use std::{fmt, sync::Arc};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, FontResourceError, FontVariationSetting, GlyphRun,
    PositionedGlyph, Rect, TextAffineTransform, TextClusterIdentity, TextDirection,
    TextLayoutArtifact, TextLayoutBackend, TextLayoutBackendKind, TextPart, TextRenderItem,
    TextResource, TextResourceValidationError, TextSourceKind, TextSourceSpan, Vec2,
};
use swash::{shape::ShapeContext, text::Script, FontRef};

pub const NATIVE_TEXT_BACKEND_VERSION: &str = "swash-0.2.10";
const NATIVE_TEXT_TEMPLATE_VERSION: &str = "noon-native-single-run-v1";

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
    pub fill: Option<Color>,
    pub variations: Arc<[FontVariationSetting]>,
}

impl NativeTextOptions {
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size,
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
    SourceTooLarge,
    MultilineNotYetSupported,
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
            Self::SourceTooLarge => write!(
                formatter,
                "native text source exceeds Noon's text span space"
            ),
            Self::MultilineNotYetSupported => write!(
                formatter,
                "multiline native text layout is not yet implemented by this backend slice"
            ),
            Self::InvalidResource(error) => {
                write!(formatter, "invalid normalized text resource: {error}")
            }
            Self::FontResource(error) => write!(formatter, "invalid native font resource: {error}"),
        }
    }
}

impl std::error::Error for NativeTextError {}

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

    /// Shape one deterministic single-line LTR run directly into `TextResource`.
    ///
    /// This foundation intentionally keeps itemization narrow. Multiline layout,
    /// bidi reordering, script runs, fallback, and MarkupText span splitting remain
    /// explicit follow-up work rather than being approximated here.
    pub fn compile_plain(
        &mut self,
        source: &str,
        font: &NativeFontFace,
        options: &NativeTextOptions,
    ) -> Result<NativeTextResourceArtifact, NativeTextError> {
        if !options.font_size.is_finite() || options.font_size <= 0.0 {
            return Err(NativeTextError::InvalidFontSize);
        }
        if source.contains('\n') || source.contains('\r') {
            return Err(NativeTextError::MultilineNotYetSupported);
        }
        let source_len =
            u32::try_from(source.len()).map_err(|_| NativeTextError::SourceTooLarge)?;
        let font_ref = FontRef::from_index(font.data.as_ref(), font.face_index as usize).ok_or(
            NativeTextError::InvalidFontFace {
                face_index: font.face_index,
            },
        )?;
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
        shaper.add_str(source);

        let mut glyphs = Vec::new();
        let mut cursor_x = 0.0_f32;
        let mut cluster_ordinal = 0_u32;
        shaper.shape_with(|cluster| {
            let source_span = TextSourceSpan::new(cluster.source.start, cluster.source.end);
            for glyph in cluster.glyphs {
                let origin = Vec2::new(cursor_x + glyph.x, glyph.y);
                let advance = Vec2::new(glyph.advance, 0.0);
                let right = origin.x + glyph.advance.max(0.0);
                glyphs.push(PositionedGlyph {
                    glyph_id: u32::from(glyph.id),
                    cluster: TextClusterIdentity {
                        source_span,
                        cluster_ordinal,
                        semantic_key: None,
                    },
                    origin,
                    advance,
                    // Exact outlines remain lazy. The line-box bound is conservative
                    // and sufficient for semantic layout until exact ink metrics land.
                    bounds: Rect::new(
                        Vec2::new(origin.x.min(right), metrics.descent),
                        Vec2::new(origin.x.max(right), metrics.ascent),
                    ),
                });
                cluster_ordinal = cluster_ordinal.saturating_add(1);
            }
            cursor_x += cluster.advance();
        });

        let line_bounds = if glyphs.is_empty() {
            Rect::new(Vec2::ZERO, Vec2::ZERO)
        } else {
            Rect::new(
                Vec2::new(0.0_f32.min(cursor_x), metrics.descent),
                Vec2::new(0.0_f32.max(cursor_x), metrics.ascent),
            )
        };
        let center = line_bounds.center();
        let centered_bounds = Rect::new(line_bounds.min - center, line_bounds.max - center);
        let run = GlyphRun {
            font: font_identity.clone(),
            variations: options.variations.clone(),
            font_size: options.font_size,
            direction: TextDirection::LeftToRight,
            fill: options.fill,
            stroke: None,
            transform: TextAffineTransform::translation(-center.x, -center.y),
            glyphs: glyphs.into(),
        };
        let glyph_count = u32::try_from(run.glyphs.len()).unwrap_or(u32::MAX);
        let full_span = TextSourceSpan::new(0, source_len);
        let resource = TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: Arc::from([run]),
            vector_items: Arc::from([]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([TextPart {
                source_span: full_span,
                first_cluster: 0,
                cluster_count: glyph_count,
                first_vector: 0,
                vector_count: 0,
                semantic_key: None,
            }]),
            bounds: centered_bounds,
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

fn layout_artifact(
    source: &str,
    font: &NativeFontFace,
    options: &NativeTextOptions,
) -> TextLayoutArtifact {
    let mut identity = format!(
        "{NATIVE_TEXT_BACKEND_VERSION}\0{NATIVE_TEXT_TEMPLATE_VERSION}\0{}\0{}\0{:08x}\0{source}",
        font.face_key,
        font.face_index,
        options.font_size.to_bits()
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
    fn multiline_and_invalid_font_size_fail_explicitly() {
        let font = bundled_font();
        let mut compiler = NativeTextCompiler::new();
        assert_eq!(
            compiler
                .compile_plain("a\nb", &font, &NativeTextOptions::new(20.0))
                .unwrap_err(),
            NativeTextError::MultilineNotYetSupported
        );
        assert_eq!(
            compiler
                .compile_plain("a", &font, &NativeTextOptions::new(0.0))
                .unwrap_err(),
            NativeTextError::InvalidFontSize
        );
    }
}
