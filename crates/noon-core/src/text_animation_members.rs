use crate::{TextRenderItem, TextResource, TextSourceKind, TextSourceSpan};

/// Stable reference to one shaped glyph inside an immutable [`TextResource`].
///
/// This is internal retained-content identity, not a semantic scene object. A public
/// `Text` remains one object while animation/render code can address the rendered
/// glyph members that Manim exposes through its SVG submobject family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextAnimationGlyphRef {
    pub run_index: u32,
    pub glyph_index: u32,
}

/// One rendered plain-text animation member in shaped painter order.
///
/// ManimCE v0.21 Cairo animates `Text` through `family_members_with_points()`. Default
/// `Text` builds that family from rendered SVG glyph submobjects; whitespace/newlines
/// are stripped, ligatures naturally collapse to one rendered glyph, and a shaped
/// source cluster that emits multiple visible glyphs remains multiple family members.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextAnimationMember {
    pub source_span: TextSourceSpan,
    pub glyph: TextAnimationGlyphRef,
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

/// Derive the rendered-glyph family that ManimCE v0.21 animates for default `Text`.
///
/// Members follow first painter-stream appearance. Each non-whitespace shaped glyph is
/// one member, including multiple glyphs that share one source cluster span. A ligature
/// produced as one shaped glyph therefore remains one member. Whitespace-only source
/// spans are excluded explicitly because shaping backends may retain advance glyphs for
/// them even though Manim strips whitespace from the SVG submobject family.
///
/// This intentionally accepts only `Plain` resources; Typst/Tex family semantics must
/// be defined by their own source model. The result is derived data and must not create
/// externally visible semantic object identities or authoring-wire glyph IDs.
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
            members.push(TextAnimationMember {
                source_span: span,
                glyph: TextAnimationGlyphRef {
                    run_index,
                    glyph_index,
                },
            });
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
        assert_eq!(members[0].glyph.glyph_index, 0);
        assert_eq!(members[1].source_span, TextSourceSpan::new(2, 3));
        assert_eq!(members[1].glyph.glyph_index, 2);
    }

    #[test]
    fn multiple_glyphs_in_one_source_cluster_remain_distinct_animation_members() {
        let span = TextSourceSpan::new(0, 2);
        let text = resource(
            "fi",
            vec![run(vec![glyph(span, 0, 0.0), glyph(span, 0, 0.5)])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].source_span, span);
        assert_eq!(members[1].source_span, span);
        assert_eq!(members[0].glyph.glyph_index, 0);
        assert_eq!(members[1].glyph.glyph_index, 1);
    }

    #[test]
    fn one_ligature_glyph_spanning_multiple_source_characters_is_one_member() {
        let span = TextSourceSpan::new(0, 2);
        let text = resource(
            "fi",
            vec![run(vec![glyph(span, 0, 0.0)])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].source_span, span);
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
    fn repeated_source_span_across_runs_remains_distinct_rendered_members() {
        let span = TextSourceSpan::new(0, 2);
        let text = resource(
            "fi",
            vec![
                run(vec![glyph(span, 0, 0.0)]),
                run(vec![glyph(span, 0, 1.0)]),
            ],
            vec![TextRenderItem::GlyphRun(0), TextRenderItem::GlyphRun(1)],
        );
        let members = plain_text_animation_members(&text).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].glyph.run_index, 0);
        assert_eq!(members[1].glyph.run_index, 1);
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
