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

/// One rendered glyph animation member in shaped painter order.
///
/// ManimCE v0.21 Cairo animates `Text` through `family_members_with_points()`. Default
/// `Text` builds that family from rendered SVG glyph submobjects; whitespace/newlines
/// are stripped, ligatures naturally collapse to one rendered glyph, and a shaped
/// source cluster that emits multiple visible glyphs remains multiple family members.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextAnimationMember {
    pub source_span: TextSourceSpan,
    pub glyph: TextAnimationGlyphRef,
}

/// Identity for one non-glyph vector in a retained text resource's painter stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextAnimationVectorRef {
    pub vector_index: u32,
    pub source_span: Option<TextSourceSpan>,
}

/// One glyph or vector member in retained painter order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAnimationMemberKind {
    Glyph(TextAnimationMember),
    Vector(TextAnimationVectorRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnimationMemberError {
    VectorContent,
    MissingRun(u32),
    MissingVector(u32),
    InvalidSourceSpan(TextSourceSpan),
    TooManyGlyphs,
}

impl std::fmt::Display for TextAnimationMemberError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VectorContent => formatter
                .write_str("plain Text animation members cannot contain backend vector items"),
            Self::MissingRun(index) => {
                write!(
                    formatter,
                    "text render stream references missing glyph run {index}"
                )
            }
            Self::MissingVector(index) => write!(
                formatter,
                "text render stream references missing vector item {index}"
            ),
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

/// Derive every rendered glyph and vector member in the resource's painter order.
/// Non-plain sources may contain backend vectors; plain `Text` continues to reject
/// them because its established family contract consists of glyphs only.
pub fn text_animation_members(
    resource: &TextResource,
) -> Result<Vec<TextAnimationMemberKind>, TextAnimationMemberError> {
    let mut members = Vec::new();
    for item in resource.render_items.iter() {
        match *item {
            TextRenderItem::GlyphRun(run_index) => {
                let run = resource
                    .runs
                    .get(run_index as usize)
                    .ok_or(TextAnimationMemberError::MissingRun(run_index))?;
                for (glyph_index, glyph) in run.glyphs.iter().enumerate() {
                    let span = glyph.cluster.source_span;
                    if source_span_is_whitespace(resource, span)? {
                        continue;
                    }
                    let glyph_index = u32::try_from(glyph_index)
                        .map_err(|_| TextAnimationMemberError::TooManyGlyphs)?;
                    members.push(TextAnimationMemberKind::Glyph(TextAnimationMember {
                        source_span: span,
                        glyph: TextAnimationGlyphRef {
                            run_index,
                            glyph_index,
                        },
                    }));
                }
            }
            TextRenderItem::Vector(vector_index) => {
                if resource.kind == TextSourceKind::Plain {
                    return Err(TextAnimationMemberError::VectorContent);
                }
                let vector = resource
                    .vector_items
                    .get(vector_index as usize)
                    .ok_or(TextAnimationMemberError::MissingVector(vector_index))?;
                if let Some(span) = vector.source_span {
                    let start = span.start as usize;
                    let end = span.end as usize;
                    if resource.source.get(start..end).is_none() {
                        return Err(TextAnimationMemberError::InvalidSourceSpan(span));
                    }
                }
                members.push(TextAnimationMemberKind::Vector(TextAnimationVectorRef {
                    vector_index,
                    source_span: vector.source_span,
                }));
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
        FontFaceIdentity, GeometryId, GeometryResourceHandle, GlyphRun, PositionedGlyph, Rect,
        TextAffineTransform, TextClusterIdentity, TextDirection, TextLayoutArtifact,
        TextLayoutBackend, TextLayoutBackendKind, TextVectorItem, TextVectorStyle, Vec2,
    };

    fn glyph_members(resource: &TextResource) -> Vec<TextAnimationMember> {
        text_animation_members(resource)
            .unwrap()
            .into_iter()
            .map(|member| match member {
                TextAnimationMemberKind::Glyph(glyph) => glyph,
                TextAnimationMemberKind::Vector(_) => panic!("expected glyph-only test resource"),
            })
            .collect()
    }

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
        let members = glyph_members(&text);
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
        let members = glyph_members(&text);
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
        let members = glyph_members(&text);
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
        let members = glyph_members(&text);
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
        let members = glyph_members(&text);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].glyph.run_index, 0);
        assert_eq!(members[1].glyph.run_index, 1);
    }

    #[test]
    fn projected_mathtex_leaf_uses_the_retained_glyph_member_contract() {
        let mut value = resource(
            "x",
            vec![run(vec![glyph(TextSourceSpan::new(0, 1), 0, 0.0)])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        value.kind = TextSourceKind::MathTex;
        let members = glyph_members(&value);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].source_span, TextSourceSpan::new(0, 1));
    }

    #[test]
    fn plain_vector_and_malformed_source_content_fail_closed() {
        let mut text = resource(
            "A",
            vec![run(vec![glyph(TextSourceSpan::new(0, 1), 0, 0.0)])],
            vec![TextRenderItem::GlyphRun(0)],
        );
        text.kind = TextSourceKind::Plain;
        text.render_items = Arc::from([TextRenderItem::Vector(0)]);
        assert_eq!(
            text_animation_members(&text),
            Err(TextAnimationMemberError::VectorContent)
        );

        text.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);
        text.runs = Arc::from([run(vec![glyph(TextSourceSpan::new(0, 2), 0, 0.0)])]);
        assert_eq!(
            text_animation_members(&text),
            Err(TextAnimationMemberError::InvalidSourceSpan(
                TextSourceSpan::new(0, 2)
            ))
        );
    }

    #[test]
    fn malformed_render_run_reference_is_rejected() {
        let text = resource("A", vec![], vec![TextRenderItem::GlyphRun(7)]);
        assert_eq!(
            text_animation_members(&text),
            Err(TextAnimationMemberError::MissingRun(7))
        );
    }

    #[test]
    fn compiled_text_members_preserve_interleaved_glyph_and_vector_painter_order() {
        let mut text = resource(
            "x+1",
            vec![run(vec![glyph(TextSourceSpan::new(0, 1), 0, 0.0)])],
            vec![
                TextRenderItem::GlyphRun(0),
                TextRenderItem::Vector(0),
                TextRenderItem::GlyphRun(0),
            ],
        );
        text.kind = TextSourceKind::MathTex;
        text.vector_items = Arc::from([TextVectorItem {
            geometry: GeometryResourceHandle {
                arena: 0,
                id: GeometryId::new(1),
                version: 0,
            },
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: Some(TextSourceSpan::new(1, 2)),
            semantic_key: None,
        }]);

        for kind in [
            TextSourceKind::Tex,
            TextSourceKind::MathTex,
            TextSourceKind::Typst,
            TextSourceKind::MathTypst,
            TextSourceKind::Markup,
        ] {
            text.kind = kind;
            assert_eq!(
                text_animation_members(&text).unwrap(),
                vec![
                    TextAnimationMemberKind::Glyph(TextAnimationMember {
                        source_span: TextSourceSpan::new(0, 1),
                        glyph: TextAnimationGlyphRef {
                            run_index: 0,
                            glyph_index: 0,
                        },
                    }),
                    TextAnimationMemberKind::Vector(TextAnimationVectorRef {
                        vector_index: 0,
                        source_span: Some(TextSourceSpan::new(1, 2)),
                    }),
                    TextAnimationMemberKind::Glyph(TextAnimationMember {
                        source_span: TextSourceSpan::new(0, 1),
                        glyph: TextAnimationGlyphRef {
                            run_index: 0,
                            glyph_index: 0,
                        },
                    }),
                ]
            );
        }
    }

    #[test]
    fn tex_member_enumeration_rejects_missing_vector_and_malformed_vector_span() {
        let mut text = resource("x", vec![], vec![TextRenderItem::Vector(0)]);
        text.kind = TextSourceKind::Tex;
        assert_eq!(
            text_animation_members(&text),
            Err(TextAnimationMemberError::MissingVector(0))
        );

        text.vector_items = Arc::from([TextVectorItem {
            geometry: GeometryResourceHandle {
                arena: 0,
                id: GeometryId::new(1),
                version: 0,
            },
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: Some(TextSourceSpan::new(0, 2)),
            semantic_key: None,
        }]);
        assert_eq!(
            text_animation_members(&text),
            Err(TextAnimationMemberError::InvalidSourceSpan(
                TextSourceSpan::new(0, 2)
            ))
        );
    }
}
