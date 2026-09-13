//! Manim-compatible inherited `SVGMobject(svg_default=...)` fallback styling.
//!
//! Manim v0.21 rewrites the parsed SVG tree before shape conversion: fallback
//! paint lives on an outer group, while presentation attributes from the original
//! root live on an inner group and therefore win by normal SVG inheritance. This
//! module reproduces that author-time contract and then delegates to the ordinary
//! retained SVG importer. No SVG-specific state survives semantic publication.

use crate::{MobjectFamily, Scene, StyleUpdate, SvgAuthoringError, SvgImportOptions};
use noon_core::{Color, SemanticStore, SemanticStyle};
use std::{cell::RefCell, rc::Rc};

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const SVG_INITIAL_STROKE_WIDTH: f64 = 1.0;
const MANIM_ZERO_STROKE_SENTINEL: f64 = 0.000_001;
const ROOT_STYLE_KEYS: [&str; 6] = [
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "style",
];

/// Fallback paint inherited by SVG leaves that do not specify those properties.
///
/// This is distinct from [`StyleUpdate`]: SVG defaults participate in parser-time
/// inheritance and lose to explicit root/leaf SVG presentation, while a
/// `StyleUpdate` is an explicit post-parse override and therefore wins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgDefaultStyle {
    pub color: Option<Color>,
    pub opacity: Option<f64>,
    pub fill_color: Option<Color>,
    pub fill_opacity: Option<f64>,
    pub stroke_width: Option<f64>,
    pub stroke_color: Option<Color>,
    pub stroke_opacity: Option<f64>,
}

impl Default for SvgDefaultStyle {
    fn default() -> Self {
        Self {
            color: None,
            opacity: None,
            fill_color: None,
            fill_opacity: None,
            stroke_width: Some(0.0),
            stroke_color: None,
            stroke_opacity: None,
        }
    }
}

impl SvgDefaultStyle {
    fn effective_style(self) -> StyleUpdate {
        StyleUpdate {
            fill_color: self.fill_color.or(self.color),
            fill_opacity: self.fill_opacity.or(self.opacity),
            stroke_color: self.stroke_color.or(self.color),
            stroke_width: self.stroke_width,
            stroke_opacity: self.stroke_opacity.or(self.opacity),
        }
    }

    fn validate(self) -> Result<(), SvgAuthoringError> {
        let mut style = SemanticStyle::default();
        self.effective_style().apply(&mut style)?;
        Ok(())
    }
}

impl MobjectFamily {
    /// Parse static SVG with Manim-style inherited parser fallback paint.
    pub fn from_svg_str_with_svg_default(
        store: Rc<RefCell<SemanticStore>>,
        source: &str,
        svg_default: SvgDefaultStyle,
    ) -> Result<Self, SvgAuthoringError> {
        Self::from_svg_str_with_options_and_svg_default(
            store,
            source,
            SvgImportOptions::default(),
            svg_default,
        )
    }

    /// Parse static SVG with explicit placement and inherited parser fallback paint.
    pub fn from_svg_str_with_options_and_svg_default(
        store: Rc<RefCell<SemanticStore>>,
        source: &str,
        options: SvgImportOptions,
        svg_default: SvgDefaultStyle,
    ) -> Result<Self, SvgAuthoringError> {
        let wrapped = source_with_svg_default(source, svg_default)?;
        Self::from_svg_str_with_options(store, &wrapped, options)
    }
}

impl Scene {
    /// Parse static SVG with Manim-style inherited parser fallback paint.
    pub fn svg_from_str_with_svg_default(
        &self,
        source: &str,
        svg_default: SvgDefaultStyle,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str_with_svg_default(
            Rc::clone(self.integration_store()),
            source,
            svg_default,
        )
    }

    /// Parse static SVG with explicit placement and inherited parser fallback paint.
    pub fn svg_from_str_with_options_and_svg_default(
        &self,
        source: &str,
        options: SvgImportOptions,
        svg_default: SvgDefaultStyle,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str_with_options_and_svg_default(
            Rc::clone(self.integration_store()),
            source,
            options,
            svg_default,
        )
    }
}

fn source_with_svg_default(
    source: &str,
    svg_default: SvgDefaultStyle,
) -> Result<String, SvgAuthoringError> {
    svg_default.validate()?;
    let document = parse_xml(source)?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Ok(source.to_owned());
    }

    let root_range = root.range();
    let Some(open_end) = opening_tag_end(source, root_range.start, root_range.end) else {
        return Ok(source.to_owned());
    };
    let Some(close_start) = source[root_range.start..root_range.end].rfind("</").map(|offset| {
        root_range.start + offset
    }) else {
        // A self-closing root contains no drawable descendants. Validation above
        // is still observable, but no fallback style needs to be materialized.
        return Ok(source.to_owned());
    };

    let mut wrapped = String::with_capacity(source.len() + 256);
    wrapped.push_str("<svg xmlns=\"");
    wrapped.push_str(SVG_NAMESPACE);
    wrapped.push('"');
    for namespace in root.namespaces() {
        let Some(prefix) = namespace.name() else {
            continue;
        };
        if prefix == "xml" {
            continue;
        }
        push_attribute(
            &mut wrapped,
            &format!("xmlns:{prefix}"),
            namespace.uri(),
        );
    }
    wrapped.push_str(" stroke-width=\"");
    wrapped.push_str(&SVG_INITIAL_STROKE_WIDTH.to_string());
    wrapped.push_str("\"><g");
    push_config_attributes(&mut wrapped, svg_default);
    wrapped.push_str("><g");
    for key in ROOT_STYLE_KEYS {
        if let Some(value) = root.attribute(key) {
            push_attribute(&mut wrapped, key, value);
        }
    }
    wrapped.push('>');
    wrapped.push_str(&source[open_end..close_start]);
    wrapped.push_str("</g></g></svg>");
    Ok(wrapped)
}

fn parse_xml(source: &str) -> Result<usvg::roxmltree::Document<'_>, SvgAuthoringError> {
    usvg::roxmltree::Document::parse_with_options(
        source,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(SvgAuthoringError::Xml)
}

fn opening_tag_end(source: &str, start: usize, end: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in source[start..end].char_indices() {
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '>' => return Some(start + offset + character.len_utf8()),
            _ => {}
        }
    }
    None
}

fn push_config_attributes(output: &mut String, svg_default: SvgDefaultStyle) {
    let effective = svg_default.effective_style();
    if let Some(color) = effective.fill_color {
        push_attribute(output, "fill", &color_hex(color));
    }
    if let Some(opacity) = effective.fill_opacity {
        push_attribute(output, "fill-opacity", &opacity.to_string());
    }
    if let Some(color) = effective.stroke_color {
        push_attribute(output, "stroke", &color_hex(color));
    }
    if let Some(opacity) = effective.stroke_opacity {
        push_attribute(output, "stroke-opacity", &opacity.to_string());
    }
    if let Some(width) = effective.stroke_width {
        let value = if width == 0.0 {
            MANIM_ZERO_STROKE_SENTINEL
        } else {
            width
        };
        push_attribute(output, "stroke-width", &value.to_string());
    }
}

fn color_hex(color: Color) -> String {
    fn channel(value: f32) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(color.red),
        channel(color.green),
        channel(color.blue)
    )
}

fn push_attribute(output: &mut String, name: &str, value: &str) {
    output.push(' ');
    output.push_str(name);
    output.push_str("=\"");
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{SemanticPaint, SemanticStyle};

    fn raw_options() -> SvgImportOptions {
        SvgImportOptions {
            should_center: false,
            height: None,
            width: None,
        }
    }

    fn member_styles(scene: &Scene, family: &MobjectFamily) -> Vec<SemanticStyle> {
        let store = scene.integration_store().borrow();
        store
            .semantic_family_members_checked(family.node_id())
            .unwrap()
            .iter()
            .map(|member| store.semantic_object_state_checked(*member).unwrap().style)
            .collect()
    }

    #[test]
    fn svg_default_supplies_missing_leaf_paint() {
        let scene = Scene::new();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M0 0 L10 0 L10 10 Z"/>
        </svg>"##;
        let family = scene
            .svg_from_str_with_options_and_svg_default(
                source,
                raw_options(),
                SvgDefaultStyle {
                    fill_color: Some(Color::from_hex(0x123456)),
                    fill_opacity: Some(0.25),
                    stroke_color: Some(Color::from_hex(0xABCDEF)),
                    stroke_opacity: Some(0.5),
                    stroke_width: Some(3.0),
                    ..SvgDefaultStyle::default()
                },
            )
            .unwrap();
        let styles = member_styles(&scene, &family);
        assert_eq!(styles.len(), 1);
        assert_eq!(
            styles[0].fill,
            Some(SemanticPaint::Solid(Color::from_hex(0x123456)))
        );
        assert_eq!(
            styles[0].stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0xABCDEF)))
        );
        assert!((styles[0].fill_opacity - 0.25).abs() < 1e-6);
        assert!((styles[0].stroke_opacity - 0.5).abs() < 1e-6);
        assert!((styles[0].stroke_width - 0.03).abs() < 1e-6);
    }

    #[test]
    fn root_and_leaf_presentation_override_svg_default() {
        let scene = Scene::new();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" fill="#112233" stroke="#445566" stroke-width="4">
            <rect x="0" y="0" width="5" height="5"/>
            <rect x="5" y="0" width="5" height="5" fill="#778899" stroke="#AABBCC" stroke-width="6"/>
        </svg>"##;
        let family = scene
            .svg_from_str_with_options_and_svg_default(
                source,
                raw_options(),
                SvgDefaultStyle {
                    color: Some(Color::from_hex(0xFF0000)),
                    stroke_width: Some(2.0),
                    ..SvgDefaultStyle::default()
                },
            )
            .unwrap();
        let styles = member_styles(&scene, &family);
        assert_eq!(styles.len(), 2);
        assert_eq!(
            styles[0].fill,
            Some(SemanticPaint::Solid(Color::from_hex(0x112233)))
        );
        assert_eq!(
            styles[0].stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0x445566)))
        );
        assert!((styles[0].stroke_width - 0.04).abs() < 1e-6);
        assert_eq!(
            styles[1].fill,
            Some(SemanticPaint::Solid(Color::from_hex(0x778899)))
        );
        assert_eq!(
            styles[1].stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0xAABBCC)))
        );
        assert!((styles[1].stroke_width - 0.06).abs() < 1e-6);
    }

    #[test]
    fn specific_svg_defaults_override_generic_color_and_opacity() {
        let scene = Scene::new();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M0 0 L10 0 L10 10 Z"/>
        </svg>"##;
        let family = scene
            .svg_from_str_with_options_and_svg_default(
                source,
                raw_options(),
                SvgDefaultStyle {
                    color: Some(Color::from_hex(0x0000FF)),
                    opacity: Some(0.2),
                    fill_color: Some(Color::from_hex(0xFF0000)),
                    fill_opacity: Some(0.3),
                    stroke_color: Some(Color::from_hex(0x00FF00)),
                    stroke_opacity: Some(0.4),
                    stroke_width: Some(2.0),
                },
            )
            .unwrap();
        let style = member_styles(&scene, &family)[0].clone();
        assert_eq!(
            style.fill,
            Some(SemanticPaint::Solid(Color::from_hex(0xFF0000)))
        );
        assert_eq!(
            style.stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0x00FF00)))
        );
        assert!((style.fill_opacity - 0.3).abs() < 1e-6);
        assert!((style.stroke_opacity - 0.4).abs() < 1e-6);
    }

    #[test]
    fn zero_svg_default_stroke_width_retains_stroke_only_path() {
        let scene = Scene::new();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M0 0 L10 0" fill="none"/>
        </svg>"##;
        let family = scene
            .svg_from_str_with_options_and_svg_default(
                source,
                raw_options(),
                SvgDefaultStyle {
                    stroke_color: Some(Color::from_hex(0xFFFFFF)),
                    stroke_width: Some(0.0),
                    ..SvgDefaultStyle::default()
                },
            )
            .unwrap();
        let styles = member_styles(&scene, &family);
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].stroke_width, 0.0);
    }

    #[test]
    fn absent_svg_default_stroke_width_uses_svg_initial_width() {
        let scene = Scene::new();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M0 0 L10 0" fill="none" stroke="#FFFFFF"/>
        </svg>"##;
        let family = scene
            .svg_from_str_with_options_and_svg_default(
                source,
                raw_options(),
                SvgDefaultStyle {
                    stroke_width: None,
                    ..SvgDefaultStyle::default()
                },
            )
            .unwrap();
        let style = member_styles(&scene, &family)[0].clone();
        assert!((style.stroke_width - 0.01).abs() < 1e-6);
    }

    #[test]
    fn svg_default_preserves_prefixed_namespace_declarations() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:meta="urn:noon:test">
            <path meta:label="shape" d="M0 0 L10 0 L10 10 Z"/>
        </svg>"##;
        let wrapped = source_with_svg_default(source, SvgDefaultStyle::default()).unwrap();
        let document = parse_xml(&wrapped).unwrap();
        assert_eq!(
            document.root_element().lookup_namespace_uri(Some("meta")),
            Some("urn:noon:test")
        );
    }

    #[test]
    fn invalid_svg_default_fails_before_publication() {
        let scene = Scene::new();
        let before = scene.revision();
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 0"/></svg>"##;
        assert!(matches!(
            scene.svg_from_str_with_svg_default(
                source,
                SvgDefaultStyle {
                    opacity: Some(1.5),
                    ..SvgDefaultStyle::default()
                }
            ),
            Err(SvgAuthoringError::Authoring(_))
        ));
        assert_eq!(scene.revision(), before);
    }
}
