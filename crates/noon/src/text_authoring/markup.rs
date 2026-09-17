//! Public markup authoring resolves styles once, before retained resource import.
use super::*;
use noon_text::{markup::normalize_markup_text, shaping::NativeTextSpan};
use swash::{FontRef, Stretch, Style as FontStyle, Weight};

/// Native retained text with a bounded Pango-style markup vocabulary.
///
/// Supports `b`, `i`, `tt`, and `span` with `foreground`/`fgcolor` or
/// `font_family`, plus XML entities and newlines. Other tags/attributes fail
/// explicitly. Source-part ranges refer to decoded UTF-8 text, without tags.
/// Convert into [`Text`] or pass directly to [`crate::Scene::text`].
#[derive(Clone, Debug, PartialEq)]
pub struct MarkupText(Text);

impl MarkupText {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        let mut text = Text::new(source);
        text.markup = true;
        Self(text)
    }

    /// Original markup source; retained source-part ranges use decoded text.
    pub fn source(&self) -> &str {
        self.0.source()
    }
    pub fn font_family(&self) -> &str {
        self.0.font_family()
    }
    pub fn font_size(&self) -> f32 {
        self.0.font_size()
    }
    pub fn line_spacing(&self) -> f32 {
        self.0.line_spacing()
    }
    pub fn with_font(self, family: impl Into<Arc<str>>) -> Self {
        Self(self.0.with_font(family))
    }
    pub fn with_font_face(self, face: NativeFontFace) -> Self {
        Self(self.0.with_font_face(face))
    }
    pub fn with_font_size(self, size: f32) -> Self {
        Self(self.0.with_font_size(size))
    }
    pub fn with_line_spacing(self, spacing: f32) -> Self {
        Self(self.0.with_line_spacing(spacing))
    }
    pub fn color(self, color: Color) -> Self {
        Self(self.0.color(color))
    }
    pub fn set_opacity(self, opacity: f32) -> Self {
        Self(self.0.set_opacity(opacity))
    }
    pub fn shift(self, offset: Vec2) -> Self {
        Self(self.0.shift(offset))
    }
    pub fn move_to(self, point: Vec2) -> Self {
        Self(self.0.move_to(point))
    }
    pub fn scale(self, factor: f32) -> Self {
        Self(self.0.scale(factor))
    }
    pub fn scale_xy(self, factor: Vec2) -> Self {
        Self(self.0.scale_xy(factor))
    }
    pub fn rotate(self, angle: f32) -> Self {
        Self(self.0.rotate(angle))
    }
}

impl From<MarkupText> for Text {
    fn from(value: MarkupText) -> Self {
        value.0
    }
}

pub(super) fn compile(
    text: &Text,
    base: &NativeFontFace,
    options: &NativeTextOptions,
    compiler: &mut NativeTextCompiler,
) -> Result<NativeTextResourceArtifact, TextAuthoringError> {
    let normalized = normalize_markup_text(&text.source).map_err(TextAuthoringError::Markup)?;
    let mut spans = Vec::with_capacity(normalized.spans.len());
    // Author-time lookup is cached per distinct style, never repeated per glyph.
    let mut faces: Vec<(Arc<str>, bool, bool, NativeFontFace)> = Vec::new();
    for span in normalized.spans.iter() {
        let style = &span.style;
        let family = style.font_family.as_deref().unwrap_or(if style.monospace {
            DEFAULT_NATIVE_TEXT_FONT_FAMILY
        } else {
            &text.font_family
        });
        let inherited = style.font_family.is_none() && !style.monospace;
        let font = if inherited && !style.bold && !style.italic {
            base.clone()
        } else if inherited && text.font_face.is_some() {
            // Explicit faces never resolve through the bundled-family cache.
            if !matches_style(base, style.bold, style.italic) {
                return Err(unavailable(family, style.bold, style.italic));
            }
            base.clone()
        } else if let Some((_, _, _, face)) = faces.iter().find(|(name, bold, italic, _)| {
            name.as_ref() == family && *bold == style.bold && *italic == style.italic
        }) {
            face.clone()
        } else {
            let face = styled_font(family, style.bold, style.italic)?;
            faces.push((Arc::from(family), style.bold, style.italic, face.clone()));
            face
        };
        let fill = style.foreground.as_deref().map(parse_color).transpose()?;
        spans.push(NativeTextSpan {
            source_span: span.source_span,
            font,
            fill,
        });
    }
    Ok(compiler.compile_styled(&normalized.source, base, options, &spans)?)
}

fn unavailable(family: &str, bold: bool, italic: bool) -> TextAuthoringError {
    TextAuthoringError::FontStyleUnavailable {
        family: Arc::from(family),
        bold,
        italic,
    }
}

fn matches_style(face: &NativeFontFace, bold: bool, italic: bool) -> bool {
    FontRef::from_index(&face.data, face.face_index as usize)
        .is_some_and(|font| attributes_match(font, bold, italic))
}

fn attributes_match(font: FontRef<'_>, bold: bool, italic: bool) -> bool {
    let attrs = font.attributes();
    attrs.stretch() == Stretch::NORMAL
        && attrs.weight() == if bold { Weight::BOLD } else { Weight::NORMAL }
        && if italic {
            attrs.style() != FontStyle::Normal
        } else {
            attrs.style() == FontStyle::Normal
        }
}

fn styled_font(
    family: &str,
    bold: bool,
    italic: bool,
) -> Result<NativeFontFace, TextAuthoringError> {
    #[cfg(feature = "bundled-fonts")]
    for data in typst_assets::fonts() {
        let Some(font) = FontRef::from_index(data, 0) else {
            continue;
        };
        if attributes_match(font, bold, italic)
            && font.localized_strings().any(|name| {
                matches!(
                    name.id(),
                    swash::StringId::Family
                        | swash::StringId::TypographicFamily
                        | swash::StringId::WwsFamily
                ) && name.to_string().eq_ignore_ascii_case(family)
            })
        {
            return Ok(NativeFontFace::new(
                Arc::<str>::from(family),
                Arc::<[u8]>::from(data),
                0,
            )?);
        }
    }
    Err(unavailable(family, bold, italic))
}

/// CSS/Pango basic colors, deliberately distinct from Manim's named palette.
fn parse_color(value: &str) -> Result<Color, TextAuthoringError> {
    let normalized = value.to_ascii_lowercase();
    let rgb = match normalized.as_str() {
        "black" => 0x000000,
        "silver" => 0xc0c0c0,
        "gray" | "grey" => 0x808080,
        "white" => 0xffffff,
        "maroon" => 0x800000,
        "red" => 0xff0000,
        "purple" => 0x800080,
        "fuchsia" | "magenta" => 0xff00ff,
        "green" => 0x008000,
        "lime" => 0x00ff00,
        "olive" => 0x808000,
        "yellow" => 0xffff00,
        "navy" => 0x000080,
        "blue" => 0x0000ff,
        "teal" => 0x008080,
        "aqua" | "cyan" => 0x00ffff,
        _ => {
            let hex = normalized
                .strip_prefix('#')
                .filter(|hex| {
                    matches!(hex.len(), 3 | 6) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or_else(|| TextAuthoringError::UnsupportedMarkupColor(Arc::from(value)))?;
            let rgb = u32::from_str_radix(hex, 16).expect("validated RGB digits");
            if hex.len() == 3 {
                ((rgb & 0xf00) << 8 | (rgb & 0xf0) << 4 | (rgb & 0xf)) * 17
            } else {
                rgb
            }
        }
    };
    Ok(Color::from_hex(rgb))
}

#[cfg(all(test, feature = "bundled-fonts"))]
mod tests {
    use super::*;

    #[test]
    fn markup_decodes_sources_and_retains_exact_fonts_and_intrinsic_colors() {
        let text: Text =
            MarkupText::new("A &amp; <b>B</b> <i>C</i>\n<span foreground='#58c4dd'>é</span>")
                .into();
        let artifact = text.compile_artifact_with_fill(None).unwrap();
        let resource = artifact.resource;
        assert_eq!(resource.kind, TextSourceKind::Markup);
        assert_eq!(resource.source.as_ref(), "A & B C\né");
        let faces = resource
            .runs
            .iter()
            .map(|run| run.font.face_key.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(faces.len(), 3);
        let colored = resource.runs.iter().find(|run| run.fill.is_some()).unwrap();
        assert_eq!(colored.fill, Some(Color::from_hex(0x58c4dd)));
        let cluster = colored.glyphs[0].cluster.source_span;
        assert_eq!(
            &resource.source[cluster.start as usize..cluster.end as usize],
            "é"
        );
        for run in resource.runs.iter() {
            assert!(artifact.fonts.get_for_face(&run.font).is_some());
        }
    }

    #[test]
    fn unstyled_markup_preserves_plain_glyph_identity() {
        let plain = Text::new("Noon\né & x")
            .compile_artifact_with_fill(None)
            .unwrap();
        let markup: Text = MarkupText::new("Noon\né &amp; x").into();
        let markup = markup.compile_artifact_with_fill(None).unwrap();
        let glyph_identity = |artifact: &NativeTextResourceArtifact| {
            artifact
                .resource
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter())
                .map(|glyph| (glyph.glyph_id, glyph.cluster.clone()))
                .collect::<Vec<_>>()
        };

        assert_eq!(plain.resource.source, markup.resource.source);
        assert_eq!(plain.resource.parts, markup.resource.parts);
        assert_eq!(glyph_identity(&plain), glyph_identity(&markup));
        assert_ne!(plain.resource.bounds, markup.resource.bounds);
    }

    #[test]
    fn core_fixture_matches_manim_pango_svg_metrics() {
        let text: Text = MarkupText::new(
            "<b>Noon</b> <i>markup</i> <tt>&lt;Rust&gt;</tt>\n<span foreground=\"#58c4dd\">bold</span> and <span fgcolor=\"#ff862f\">color</span>",
        )
        .with_font("DejaVu Sans Mono")
        .with_font_size(42.0)
        .into();
        let artifact = text.compile_artifact_with_fill(None).unwrap();

        // ManimPango's generated SVG places this fixture on integer Cairo
        // coordinates: every mono cell is 7 SVG px and the two baselines are
        // 13.581054 px apart. One SVG px is 3.6 native point-layout units.
        let mut baselines = artifact
            .resource
            .runs
            .iter()
            .map(|run| run.transform.ty)
            .collect::<Vec<_>>();
        baselines.sort_by(f32::total_cmp);
        baselines.dedup_by(|left, right| (*left - *right).abs() < 1e-4);
        assert_eq!(baselines.len(), 2);
        assert!((baselines[1] - baselines[0] - 13.581054 * 3.6).abs() < 0.01);

        for baseline in baselines {
            let mut glyph_metrics = artifact
                .resource
                .runs
                .iter()
                .filter(|run| (run.transform.ty - baseline).abs() < 1e-4)
                .flat_map(|run| {
                    run.glyphs
                        .iter()
                        .map(|glyph| (glyph.origin.x, glyph.advance.x))
                })
                .collect::<Vec<_>>();
            glyph_metrics.sort_by(|left, right| left.0.total_cmp(&right.0));
            for pair in glyph_metrics.windows(2) {
                assert!((pair[1].0 - pair[0].0 - 7.0 * 3.6).abs() < 1e-4);
            }
            assert!(glyph_metrics
                .iter()
                .all(|(_, advance)| (*advance - 7.0 * 3.6).abs() < 1e-4));
        }

        // Manim's semantic reference reports 6.24296875 x 1.13139645 scene
        // units. Its SVG-to-scene scale is 0.05; Noon's point scale is 1/72.
        assert!((f64::from(artifact.resource.bounds.width()) - 6.24296875 * 72.0).abs() < 0.05);
        assert!((f64::from(artifact.resource.bounds.height()) - 1.13139645 * 72.0).abs() < 0.1);
    }

    #[test]
    fn markup_source_parts_use_decoded_utf8_ranges() {
        let scene = crate::Scene::new();
        let text = scene
            .text(MarkupText::new("<b>é</b> &amp; <i>é</i>"))
            .unwrap();
        let parts = text.text_source_parts_for("é").unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].source_span, noon_core::TextSourceSpan::new(0, 2));
        assert_eq!(parts[1].source_span, noon_core::TextSourceSpan::new(5, 7));
        assert!(text.text_source_parts_for("<b>").unwrap().is_empty());
    }

    #[test]
    fn bundled_style_cache_cannot_override_an_explicit_font_face() {
        let base = bundled_native_font(DEFAULT_NATIVE_TEXT_FONT_FAMILY).unwrap();
        let text: Text =
            MarkupText::new("<span font_family='DejaVu Sans Mono'><b>A</b></span><b>B</b>")
                .with_font_face(base)
                .into();
        assert!(matches!(
            text.compile_artifact_with_fill(None),
            Err(TextAuthoringError::FontStyleUnavailable { bold: true, .. })
        ));
    }

    #[test]
    fn malformed_markup_does_not_import_resources_or_allocate_objects() {
        let scene = crate::Scene::new();
        let store = scene.integration_store();
        let before = store.borrow().text_resources().stats();
        for source in [
            "<b>unclosed",
            "<u>unsupported</u>",
            "<span foreground='invalid'>bad</span>",
            "<span font_family='missing face'>bad</span>",
        ] {
            assert!(scene.text(MarkupText::new(source)).is_err(), "{source}");
            assert_eq!(store.borrow().text_resources().stats(), before);
            assert_eq!(store.borrow().font_resources().stats().live_resources, 0);
        }
    }

    #[test]
    fn live_markup_presentation_reuses_retained_content() {
        let scene = crate::Scene::new();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let text = live
            .create_text(
                MarkupText::new("<b>Noon</b> <span foreground='green'>native</span>").into(),
            )
            .unwrap();
        let content = live.authored(&text).unwrap().content;
        live.add(&text).unwrap();
        let before = scene.integration_store().borrow().text_resources().stats();
        live.set_translation(&text, 1.0, -0.5).unwrap();
        assert_eq!(live.authored(&text).unwrap().content, content);
        assert_eq!(
            scene.integration_store().borrow().text_resources().stats(),
            before
        );
        assert_eq!(
            live.effective(&text).unwrap().transform.translation,
            Vec2::new(1.0, -0.5)
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .text_resources()
                .get(content.text().unwrap())
                .unwrap()
                .kind,
            TextSourceKind::Markup
        );
    }

    #[test]
    fn pango_colors_are_distinct_from_manim_palette_and_bad_values_fail() {
        assert_eq!(parse_color("green").unwrap(), Color::from_hex(0x008000));
        assert_eq!(parse_color("#a3F").unwrap(), Color::from_hex(0xaa33ff));
        for value in ["#-01", "#12345", "#１２３", "rgb(1,2,3)"] {
            assert!(parse_color(value).is_err());
        }
    }
}
