//! Source-addressed styling for retained semantic text resources.
//!
//! Source selection remains owned by `text_part_authoring`; this module consumes
//! those landed queries and owns only immutable content-resource replacement.

use noon_core::{
    Color, FontResourceArena, FontResourceError, GeometryResourceArena, SemanticNodeId,
    SemanticTextImportError, TextSourceFill, TextSourceKind, TextSourceSpan, TextSourceStyleError,
};

use crate::{AuthoringError, Mobject, TextPartAuthoringError};

/// Failure to apply source-addressed styling to one semantic text object.
#[derive(Clone, Debug, PartialEq)]
pub enum TextStyleAuthoringError {
    /// Existing Rust-owned source-part selection failed.
    Selection(TextPartAuthoringError),
    /// Source styling is currently owned only by native plain `Text`.
    UnsupportedSourceKind {
        node: SemanticNodeId,
        kind: TextSourceKind,
    },
    /// A Manim-style `[start:end]` selector was recognized but invalid.
    InvalidSelector(String),
    /// The requested source style cannot be represented without changing shaping.
    Style(TextSourceStyleError),
    /// Re-registering the immutable styled resource failed before publication.
    Import(SemanticTextImportError),
    /// Rebuilding the exact font dependency set for the styled resource failed.
    Font(FontResourceError),
}

impl std::fmt::Display for TextStyleAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(error) => error.fmt(formatter),
            Self::UnsupportedSourceKind { node, kind } => write!(
                formatter,
                "semantic object {node:?} has unsupported source kind {kind:?} for native range styling"
            ),
            Self::InvalidSelector(selector) => {
                write!(formatter, "invalid Manim text source selector {selector:?}")
            }
            Self::Style(error) => error.fmt(formatter),
            Self::Import(error) => error.fmt(formatter),
            Self::Font(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextStyleAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Selection(error) => Some(error),
            Self::Style(error) => Some(error),
            Self::Import(error) => Some(error),
            Self::Font(error) => Some(error),
            Self::UnsupportedSourceKind { .. } | Self::InvalidSelector(_) => None,
        }
    }
}

impl From<AuthoringError> for TextStyleAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Selection(TextPartAuthoringError::Authoring(error))
    }
}

impl From<TextPartAuthoringError> for TextStyleAuthoringError {
    fn from(error: TextPartAuthoringError) -> Self {
        Self::Selection(error)
    }
}

impl From<TextSourceStyleError> for TextStyleAuthoringError {
    fn from(error: TextSourceStyleError) -> Self {
        Self::Style(error)
    }
}

impl From<SemanticTextImportError> for TextStyleAuthoringError {
    fn from(error: SemanticTextImportError) -> Self {
        Self::Import(error)
    }
}

impl From<FontResourceError> for TextStyleAuthoringError {
    fn from(error: FontResourceError) -> Self {
        Self::Font(error)
    }
}

impl Mobject {
    /// Resolve one Manim `t2c` selector against the original authored string.
    ///
    /// Substring selectors delegate to the landed Rust source matcher. Slice
    /// selectors use Unicode character indexes into the original source, then
    /// project those indexes to the retained UTF-8 byte identity.
    fn text_source_parts_for_style_selector(
        &self,
        selector: &str,
    ) -> Result<Vec<noon_core::TextPart>, TextStyleAuthoringError> {
        let source = {
            let state = self.state()?;
            let handle = state.content.text().ok_or_else(|| {
                TextStyleAuthoringError::Selection(TextPartAuthoringError::NotText(self.node_id()))
            })?;
            let store = self.integration_store().borrow();
            store
                .text_resources()
                .get(handle)
                .ok_or(TextStyleAuthoringError::Selection(
                    TextPartAuthoringError::Authoring(AuthoringError::MissingTextResource(handle)),
                ))?
                .source
                .clone()
        };

        if let Some(span) = manim_slice_source_span(source.as_ref(), selector)? {
            Ok(vec![self.text_source_part(span)?])
        } else {
            Ok(self.text_source_parts_for(selector)?)
        }
    }

    /// Apply one intrinsic color override to an authored UTF-8 source span.
    pub fn set_text_source_fill(
        &mut self,
        source_span: TextSourceSpan,
        color: Color,
    ) -> Result<(), TextStyleAuthoringError> {
        self.set_text_source_fills(&[TextSourceFill::new(source_span, color)])
    }

    /// Color every non-overlapping occurrence of `needle` using the landed matcher.
    pub fn set_text_fill_for(
        &mut self,
        needle: &str,
        color: Color,
    ) -> Result<(), TextStyleAuthoringError> {
        let fills = self
            .text_source_parts_for(needle)?
            .into_iter()
            .map(|part| TextSourceFill::new(part.source_span, color))
            .collect::<Vec<_>>();
        self.set_text_source_fills(&fills)
    }

    /// Apply one complete Manim `t2c` selector batch as a single validated edit.
    pub fn set_text_fills_for_selectors(
        &mut self,
        selectors: &[(String, Color)],
    ) -> Result<(), TextStyleAuthoringError> {
        let mut fills = Vec::new();
        for (selector, color) in selectors {
            fills.extend(
                self.text_source_parts_for_style_selector(selector)?
                    .into_iter()
                    .map(|part| TextSourceFill::new(part.source_span, *color)),
            );
        }
        self.set_text_source_fills(&fills)
    }

    /// Apply source-addressed fill overrides by replacing only immutable text content.
    ///
    /// All selectors, overlap rules and cluster boundaries are validated before the
    /// semantic object state is changed. The styled resource reuses shaped glyphs,
    /// advances, line metrics, transforms and bounds; only run partitioning/paint changes.
    pub fn set_text_source_fills(
        &mut self,
        fills: &[TextSourceFill],
    ) -> Result<(), TextStyleAuthoringError> {
        let state = self.state()?;
        let handle = state.content.text().ok_or_else(|| {
            TextStyleAuthoringError::Selection(TextPartAuthoringError::NotText(self.node_id()))
        })?;
        if fills.is_empty() {
            return Ok(());
        }

        let (styled, fonts) =
            {
                let store = self.integration_store().borrow();
                let resource = store.text_resources().get(handle).ok_or(
                    TextStyleAuthoringError::Selection(TextPartAuthoringError::Authoring(
                        AuthoringError::MissingTextResource(handle),
                    )),
                )?;
                if resource.kind != TextSourceKind::Plain {
                    return Err(TextStyleAuthoringError::UnsupportedSourceKind {
                        node: self.node_id(),
                        kind: resource.kind,
                    });
                }
                debug_assert!(resource.vector_items.is_empty());

                // `with_source_fills` validates every source and shaped-cluster boundary
                // before either the scratch dependency arena or semantic store is touched.
                let styled = resource.with_source_fills(fills)?;
                let mut fonts = FontResourceArena::new();
                for run in styled.runs.iter() {
                    let font = store
                        .font_resources()
                        .get_for_face(&run.font)
                        .expect("validated semantic text retains its immutable font dependency");
                    fonts.intern_face(&run.font, font.data.clone())?;
                }
                (styled, fonts)
            };

        let geometry = GeometryResourceArena::new();
        let replacement = self
            .integration_store()
            .borrow_mut()
            .import_text_resource(styled, &fonts, &geometry)?;
        let mut next = state;
        next.content = replacement.into();
        self.commit_state(next)?;
        Ok(())
    }
}

/// Parse ManimCE v0.21.0's `[start:end]` selector subset.
fn manim_slice_source_span(
    source: &str,
    selector: &str,
) -> Result<Option<TextSourceSpan>, TextStyleAuthoringError> {
    let Some(rest) = selector.strip_prefix('[') else {
        return Ok(None);
    };
    let Some(close) = rest.find(']') else {
        return Ok(None);
    };
    let body = &rest[..close];
    let Some((start_text, end_text)) = body.split_once(':') else {
        return Ok(None);
    };
    if end_text.contains(':')
        || !start_text
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-')
        || !end_text
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-')
    {
        return Ok(None);
    }

    let char_len = isize::try_from(source.chars().count())
        .map_err(|_| TextStyleAuthoringError::InvalidSelector(selector.to_owned()))?;
    let parse_endpoint = |value: &str, default: isize| -> Result<isize, TextStyleAuthoringError> {
        if value.is_empty() {
            return Ok(default);
        }
        let parsed = value
            .parse::<isize>()
            .map_err(|_| TextStyleAuthoringError::InvalidSelector(selector.to_owned()))?;
        Ok(if parsed < 0 {
            char_len + parsed
        } else {
            parsed
        })
    };
    let start = parse_endpoint(start_text, 0)?;
    let end = parse_endpoint(end_text, char_len)?;
    if start < 0 || end < 0 || start > end || start > char_len || end > char_len {
        return Err(TextStyleAuthoringError::InvalidSelector(
            selector.to_owned(),
        ));
    }

    let byte_index = |character_index: isize| -> Result<u32, TextStyleAuthoringError> {
        let character_index = usize::try_from(character_index)
            .map_err(|_| TextStyleAuthoringError::InvalidSelector(selector.to_owned()))?;
        let byte = if character_index == source.chars().count() {
            source.len()
        } else {
            source
                .char_indices()
                .nth(character_index)
                .map(|(byte, _)| byte)
                .ok_or_else(|| TextStyleAuthoringError::InvalidSelector(selector.to_owned()))?
        };
        u32::try_from(byte)
            .map_err(|_| TextStyleAuthoringError::InvalidSelector(selector.to_owned()))
    };

    Ok(Some(TextSourceSpan::new(
        byte_index(start)?,
        byte_index(end)?,
    )))
}

#[cfg(all(test, feature = "native-text", feature = "bundled-fonts"))]
mod tests {
    use super::*;
    use crate::{Scene, Text, BLUE, RED};

    #[test]
    fn range_fill_changes_paint_only_and_preserves_semantic_geometry() {
        let scene = Scene::new();
        let mut label = scene.text(Text::new("Noon blue Noon")).unwrap();
        let before_state = label.state().unwrap();
        let before_handle = before_state.content.text().unwrap();
        let before_layout_bounds = label.layout_bounds().unwrap();
        let before_width = label.width().unwrap();
        let before_height = label.height().unwrap();
        let before_center = label.center().unwrap();
        let before_resource = {
            let store = label.integration_store().borrow();
            store.text_resources().get(before_handle).unwrap().clone()
        };
        let before_geometry = before_resource
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs.iter().map(|glyph| {
                    (
                        run.font.clone(),
                        run.variations.clone(),
                        run.font_size,
                        run.direction,
                        run.stroke.clone(),
                        run.transform,
                        glyph.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();

        label
            .set_text_fills_for_selectors(&[("Noon".into(), RED), ("[5:9]".into(), BLUE)])
            .unwrap();

        let after_state = label.state().unwrap();
        let after_handle = after_state.content.text().unwrap();
        assert_ne!(after_handle, before_handle);
        assert_eq!(after_state.transform, before_state.transform);
        assert_eq!(after_state.style, before_state.style);
        assert_eq!(label.layout_bounds().unwrap(), before_layout_bounds);
        assert_eq!(label.width().unwrap(), before_width);
        assert_eq!(label.height().unwrap(), before_height);
        assert_eq!(label.center().unwrap(), before_center);

        let store = label.integration_store().borrow();
        let resource = store.text_resources().get(after_handle).unwrap();
        assert_eq!(resource.source, before_resource.source);
        assert_eq!(resource.kind, before_resource.kind);
        assert_eq!(resource.bounds, before_resource.bounds);
        assert_eq!(resource.baseline, before_resource.baseline);
        assert_eq!(resource.layout_artifact, before_resource.layout_artifact);
        assert_eq!(resource.vector_items, before_resource.vector_items);
        let after_geometry = resource
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs.iter().map(|glyph| {
                    (
                        run.font.clone(),
                        run.variations.clone(),
                        run.font_size,
                        run.direction,
                        run.stroke.clone(),
                        run.transform,
                        glyph.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(after_geometry, before_geometry);
        assert!(resource.runs.iter().any(|run| run.fill == Some(RED)));
        assert!(resource.runs.iter().any(|run| run.fill == Some(BLUE)));
    }

    #[test]
    fn selector_resolution_reuses_landed_source_queries_and_utf8_identity() {
        let scene = Scene::new();
        let mut label = scene.text(Text::new("Hé llo World")).unwrap();
        label
            .set_text_fills_for_selectors(&[("World".into(), RED), ("[2:7]".into(), BLUE)])
            .unwrap();

        assert_eq!(
            label.text_source_parts_for("World").unwrap()[0].source_span,
            TextSourceSpan::new(8, 13)
        );
        assert_eq!(
            label
                .text_source_part(TextSourceSpan::new(3, 8))
                .unwrap()
                .source_span,
            TextSourceSpan::new(3, 8)
        );
    }

    #[test]
    fn conflicting_selector_fills_reject_without_object_state_changes() {
        let scene = Scene::new();
        let mut label = scene.text(Text::new("abcdef")).unwrap();
        let before = label.state().unwrap();

        assert!(matches!(
            label.set_text_fills_for_selectors(&[("[1:5]".into(), RED), ("[3:]".into(), BLUE)]),
            Err(TextStyleAuthoringError::Style(
                TextSourceStyleError::AmbiguousFillOverlap { .. }
            ))
        ));
        assert_eq!(label.state().unwrap(), before);
    }

    #[test]
    fn cluster_split_rejects_without_object_state_changes() {
        let scene = Scene::new();
        let mut label = scene.text(Text::new("é")).unwrap();
        let before = label.state().unwrap();

        assert!(matches!(
            label.set_text_source_fill(TextSourceSpan::new(1, 2), RED),
            Err(TextStyleAuthoringError::Style(TextSourceStyleError::Query(
                noon_core::TextPartQueryError::InvalidSourceSpan
            )))
        ));
        assert_eq!(label.state().unwrap(), before);
    }
}
