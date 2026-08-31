use std::collections::BTreeMap;

use crate::{TextRenderItem, TextResource, TextSourceKind, TextSourceSpan};

/// Stable reference to one shaped glyph inside an immutable [`TextResource`].
///
/// This is internal retained-content identity, not a semantic scene object. A public
/// `Text` remains one object while animation/render code can address the glyphs that
/// form one rendered source cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextAnimationGlyphRef {
    pub run_index: u32,
    pub glyph_index: u32,
}

/// One rendered plain-text animation member in shaped painter order.
///
/// Multiple glyphs may belong to the same source cluster. Whitespace-only source
/// clusters are excluded even when the shaper emits an advance glyph for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextAnimationMember {
    pub source_span: TextSourceSpan,
    pub glyphs: Vec<TextAnimationGlyphRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnimationMemberError {
    UnsupportedSourceKind(TextSourceKind),
    VectorContent,
    MissingRun(u32),
    InvalidSourceSpan(TextSourceSpan),
    TooManyGlyphs,
}

impl std::fmt::Display for TextAnimationMemberError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSourceKind(kind) => {
                write!(
                    formatter,
                    "text animation members require plain Text, got {kind:?}"
                )
            }
            Self::VectorContent => formatter
                .write_str("plain Text animation members cannot contain backend vector items"),
            Self::MissingRun(index) => {
                write!(
                    formatter,
                    "text render stream references missing glyph run {index}"
                )
            }
            Self::InvalidSourceSpan(span) => write!(
                formatter,
                "text animation glyph source span {}..{} is outside the UTF-8 source",
                span.start, span.end
            ),
            Self::TooManyGlyphs => formatter.write_str(
                "text animation member glyph index exceeds the retained u32 identity range",
            ),
        }
    }
}

impl std::error::Error for TextAnimationMemberError {}

/// Derive Manim-like rendered-character family identity from normalized native text.
///
/// Members are ordered by first appearance in the resource painter stream. Glyphs
/// sharing the same UTF-8 source cluster span stay together even when the shaper emits
/// multiple glyphs for that cluster. Whitespace-only source clusters are excluded
/// explicitly because shaping backends may retain an advance glyph for them. This
/// intentionally accepts only `Plain` resources; Typst/Tex family semantics must be
/// defined by their own source model rather than inheriting plain-text assumptions.
///
/// The result is derived data. It must not be serialized into scene or authoring wire
/// formats and must not create externally visible semantic object identities.
pub fn plain_text_animation_members(
    resource: &TextResource,
) -> Result<Vec<TextAnimationMember>, TextAnimationMemberError> {
    if resource.kind != TextSourceKind::Plain {
        return Err(TextAnimationMemberError::UnsupportedSourceKind(
            resource.kind,
        ));
    }
    if !resource.vector_items.is_empty() {
        return Err(TextAnimationMemberError::VectorContent);
    }

    let mut members = Vec::<TextAnimationMember>::new();
    let mut by_span = BTreeMap::<TextSourceSpan, usize>::new();

    for item in resource.render_items.iter() {
        let TextRenderItem::GlyphRun(run_index) = *item else {
            return Err(TextAnimationMemberError::VectorContent);
        };
        let run = resource
            .runs
            .get(run_index as usize)
            .ok_or(TextAnimationMemberError::MissingRun(run_index))?;
        for (glyph_index, glyph) in run.glyphs.iter().enumerate() {
            let span = glyph.cluster.source_span;
            if source_span_is_whitespace(resource, span)? {
                continue;
            }
            let glyph_index =
                u32::try_from(glyph_index).map_err(|_| TextAnimationMemberError::TooManyGlyphs)?;
            let glyph_ref = TextAnimationGlyphRef {
                run_index,
                glyph_index,
            };
            if let Some(&member_index) = by_span.get(&span) {
                members[member_index].glyphs.push(glyph_ref);
            } else {
                let member_index = members.len();
                by_span.insert(span, member_index);
                members.push(TextAnimationMember {
                    source_span: span,
                    glyphs: vec![glyph_ref],
                });
            }
        }
    }

    Ok(members)
}

fn source_span_is_whitespace(
    resource: &TextResource,
    span: TextSourceSpan,
) -> Result<bool, TextAnimationMemberError> {
    let start = span.start as usize;
    let end = span.end as usize;
    let source = resource
        .source
        .get(start..end)
        .ok_or(TextAnimationMemberError::InvalidSourceSpan(span))?;
    Ok(!source.is_empty() && source.chars().all(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        FontFaceIdentity, GlyphRun, PositionedGlyph, Rect, TextAffineTransform,
        TextClusterIdentity, TextDirection, TextLayoutArtifact, TextLayoutBackend,
        TextLayoutBackendKind, Vec2,
    };

    fn glyph(span: TextSourceSpan, ordinal: u32, x: f32) -> PositionedGlyph {
        PositionedGlyph {
            glyph_id: ordinal + 1,
            cluster: TextClusterIdentity {
                source_span: span,
                cluster_ordinal: ordinal,
                semantic_key: None,
            },
            origin: Vec2::new(x, 0.0),
            advance: Vec2::new(1.0, 0.0),
            bounds: Rect::new(Vec2::new(x, 0.0), Vec2::new(x + 1.0, 1.0)),
        }
    }

    fn run(glyphs: Vec<PositionedGlyph>) -> GlyphRun {
        GlyphRun {
            font: FontFaceIdentity {
                family: Arc::from("Test"),
                face_key: Arc::from("test-face"),
                face_index: 0,
                variation_key: Arc::from(""),
            },
            variations: Arc::from([]),
            font_size: 24.0,
            direction: TextDirection::LeftToRight,
            fill: None,
            stroke: None,
            transform: TextAffineTransform::IDENTITY,
            glyphs: glyphs.into(),
        }
    }

    fn resource(
        source: &str,
        runs: Vec<GlyphRun>,
        render_items: Vec<TextRenderItem>,
    ) -> TextResource {
        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: runs.into(),
            vector_items: Arc::from([]),
            render_items: render_items.into(),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
            baseline: 0.0,
            layout_artifact: Some(TextLayoutArtifact {
                backend: TextLayoutBackend {
                    kind: TextLayoutBackendKind::NativeText,
                    version: Arc::from("test"),
                },
                template_fingerprint: Arc::from("template"),
                artifact_fingerprint: Arc::from("artifact"),
                backend_payload_key: None,
            }),
        }
    }

    #[test]
    fn whitespace_advance_glyphs_do_not_create_fake_animation_members() {
        let text = resource(
            "A B",
            vec![run(vec![
                glyph(TextSourceSpan::new(0, 1), 0, 0.0),
                glyph(TextSourceSpan::new(1, 2), 1, 1.0),
                glyph(TextSourceSpan::new(2, 3), 2, 2.0),
            ])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].source_span, TextSourceSpan::new(0, 1));
        assert_eq!(members[1].source_span, TextSourceSpan::new(2, 3));
    }

    #[test]
    fn multiple_glyphs_in_one_source_cluster_remain_one_animation_member() {
        let text = resource(
            "fi",
            vec![run(vec![
                glyph(TextSourceSpan::new(0, 2), 0, 0.0),
                glyph(TextSourceSpan::new(0, 2), 1, 0.5),
            ])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].source_span, TextSourceSpan::new(0, 2));
        assert_eq!(
            members[0].glyphs,
            vec![
                TextAnimationGlyphRef {
                    run_index: 0,
                    glyph_index: 0,
                },
                TextAnimationGlyphRef {
                    run_index: 0,
                    glyph_index: 1,
                },
            ]
        );
    }

    #[test]
    fn first_painter_appearance_defines_member_order_across_runs() {
        let text = resource(
            "AB",
            vec![
                run(vec![glyph(TextSourceSpan::new(0, 1), 0, 0.0)]),
                run(vec![glyph(TextSourceSpan::new(1, 2), 1, 1.0)]),
            ],
            vec![TextRenderItem::GlyphRun(1), TextRenderItem::GlyphRun(0)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(
            members
                .iter()
                .map(|member| member.source_span)
                .collect::<Vec<_>>(),
            vec![TextSourceSpan::new(1, 2), TextSourceSpan::new(0, 1)]
        );
    }

    #[test]
    fn repeated_source_span_is_aggregated_without_duplicate_family_identity() {
        let span = TextSourceSpan::new(0, 2);
        let text = resource(
            "fi",
            vec![
                run(vec![glyph(span, 0, 0.0)]),
                run(vec![glyph(span, 1, 1.0)]),
            ],
            vec![TextRenderItem::GlyphRun(0), TextRenderItem::GlyphRun(1)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].glyphs.len(), 2);
    }

    #[test]
    fn non_plain_vector_and_malformed_source_content_fail_closed() {
        let mut text = resource(
            "A",
            vec![run(vec![glyph(TextSourceSpan::new(0, 1), 0, 0.0)])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        text.kind = TextSourceKind::Typst;
        assert_eq!(
            plain_text_animation_members(&text),
            Err(TextAnimationMemberError::UnsupportedSourceKind(
                TextSourceKind::Typst
            ))
        );

        text.kind = TextSourceKind::Plain;
        text.render_items = Arc::from([TextRenderItem::Vector(0)]);
        assert_eq!(
            plain_text_animation_members(&text),
            Err(TextAnimationMemberError::VectorContent)
        );

        text.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);
        text.runs = Arc::from([run(vec![glyph(TextSourceSpan::new(0, 2), 0, 0.0)])]);
        assert_eq!(
            plain_text_animation_members(&text),
            Err(TextAnimationMemberError::InvalidSourceSpan(
                TextSourceSpan::new(0, 2)
            ))
        );
    }

    #[test]
    fn malformed_render_run_reference_is_rejected() {
        let text = resource("A", vec![], vec![TextRenderItem::GlyphRun(7)]);
        assert_eq!(
            plain_text_animation_members(&text),
            Err(TextAnimationMemberError::MissingRun(7))
        );
    }
}
