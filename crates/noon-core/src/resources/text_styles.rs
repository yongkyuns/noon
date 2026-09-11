//! Source-addressed styling over immutable retained text resources.
//!
//! Frontends express authored source ranges; this module projects them onto the
//! existing normalized glyph runs without creating a second text document or
//! reshaping in a frontend. A styled result is a new immutable `TextResource`.
//! Glyph, cluster, vector, layout and source identities are preserved.

use std::sync::Arc;

use super::{
    GlyphRun, PositionedGlyph, TextPartQueryError, TextRenderItem, TextResource,
    TextResourceValidationError, TextSourceSpan,
};
use crate::Color;

/// One authored intrinsic fill override for a UTF-8 source range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSourceFill {
    pub source_span: TextSourceSpan,
    pub color: Color,
}

impl TextSourceFill {
    pub const fn new(source_span: TextSourceSpan, color: Color) -> Self {
        Self { source_span, color }
    }
}

/// Failure to project source-addressed style onto an already-shaped resource.
#[derive(Clone, Debug, PartialEq)]
pub enum TextSourceStyleError {
    Query(TextPartQueryError),
    /// Manim Text treats two overlapping non-default settings for the same style
    /// property as ambiguous. Preserve that rule instead of relying on map order.
    AmbiguousFillOverlap {
        left: TextSourceSpan,
        right: TextSourceSpan,
    },
    /// The requested source range intersects, but does not contain, a shaped cluster.
    /// Supporting this case requires a shaping-boundary decision by the text backend.
    SplitsCluster {
        source_span: TextSourceSpan,
        cluster_span: TextSourceSpan,
    },
    InvalidResource(TextResourceValidationError),
}

impl std::fmt::Display for TextSourceStyleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query(error) => error.fmt(formatter),
            Self::AmbiguousFillOverlap { left, right } => write!(
                formatter,
                "ambiguous text fill overlap {}..{} with {}..{}",
                left.start, left.end, right.start, right.end
            ),
            Self::SplitsCluster {
                source_span,
                cluster_span,
            } => write!(
                formatter,
                "text style span {}..{} splits shaped cluster {}..{}",
                source_span.start, source_span.end, cluster_span.start, cluster_span.end
            ),
            Self::InvalidResource(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextSourceStyleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Query(error) => Some(error),
            Self::InvalidResource(error) => Some(error),
            Self::AmbiguousFillOverlap { .. } | Self::SplitsCluster { .. } => None,
        }
    }
}

impl From<TextPartQueryError> for TextSourceStyleError {
    fn from(error: TextPartQueryError) -> Self {
        Self::Query(error)
    }
}

impl TextResource {
    /// Return a new immutable resource with intrinsic fills applied to source ranges.
    ///
    /// The operation never reshapes. It only splits existing glyph runs when their
    /// effective intrinsic fill changes, preserving every glyph's source span,
    /// cluster ordinal, position, advance, bounds, font and affine transform.
    /// Unstyled segments retain the original run fill and therefore continue to
    /// inherit the owning mobject color when that fill is `None`.
    pub fn with_source_fills(
        &self,
        fills: &[TextSourceFill],
    ) -> Result<Self, TextSourceStyleError> {
        if fills.is_empty() {
            return Ok(self.clone());
        }

        let styled_parts = fills
            .iter()
            .map(|fill| self.source_part(fill.source_span))
            .collect::<Result<Vec<_>, _>>()?;

        // Manim's Text setting merge rejects any two explicit values for the
        // same style property when their authored ranges overlap, even when the
        // values happen to compare equal. Do this before inspecting shaped
        // clusters so whitespace-only overlaps behave consistently too.
        for (index, left) in fills.iter().enumerate() {
            for right in &fills[index + 1..] {
                if spans_overlap(left.source_span, right.source_span) {
                    return Err(TextSourceStyleError::AmbiguousFillOverlap {
                        left: left.source_span,
                        right: right.source_span,
                    });
                }
            }
        }

        // Validate every cluster boundary before allocating the replacement runs.
        for run in self.runs.iter() {
            for glyph in run.glyphs.iter() {
                let cluster_span = glyph.cluster.source_span;
                for fill in fills {
                    if spans_overlap(fill.source_span, cluster_span)
                        && !span_contains(fill.source_span, cluster_span)
                    {
                        return Err(TextSourceStyleError::SplitsCluster {
                            source_span: fill.source_span,
                            cluster_span,
                        });
                    }
                }
            }
        }

        let mut runs = Vec::new();
        let mut render_items = Vec::with_capacity(self.render_items.len());
        for item in self.render_items.iter().copied() {
            match item {
                TextRenderItem::GlyphRun(index) => {
                    let run = &self.runs[index as usize];
                    for styled in split_run_by_fill(run, fills) {
                        let next_index = u32::try_from(runs.len())
                            .expect("styled text run count exceeds u32 resource limits");
                        runs.push(styled);
                        render_items.push(TextRenderItem::GlyphRun(next_index));
                    }
                }
                TextRenderItem::Vector(index) => render_items.push(TextRenderItem::Vector(index)),
            }
        }

        let mut parts = self.parts.to_vec();
        for part in styled_parts {
            if !parts
                .iter()
                .any(|existing| existing.source_span == part.source_span)
            {
                parts.push(part);
            }
        }

        let mut styled = self.clone();
        styled.runs = runs.into();
        styled.render_items = render_items.into();
        styled.parts = parts.into();
        styled
            .validate()
            .map_err(TextSourceStyleError::InvalidResource)?;
        Ok(styled)
    }
}

fn split_run_by_fill(run: &GlyphRun, fills: &[TextSourceFill]) -> Vec<GlyphRun> {
    if run.glyphs.is_empty() {
        return vec![run.clone()];
    }

    let mut result = Vec::new();
    let mut segment_start = 0_usize;
    let mut current_fill = effective_fill(run.fill, run.glyphs[0].cluster.source_span, fills);

    for index in 1..run.glyphs.len() {
        let fill = effective_fill(run.fill, run.glyphs[index].cluster.source_span, fills);
        if fill != current_fill {
            result.push(run_segment(run, segment_start, index, current_fill));
            segment_start = index;
            current_fill = fill;
        }
    }
    result.push(run_segment(
        run,
        segment_start,
        run.glyphs.len(),
        current_fill,
    ));
    result
}

fn effective_fill(
    base: Option<Color>,
    cluster_span: TextSourceSpan,
    fills: &[TextSourceFill],
) -> Option<Color> {
    fills
        .iter()
        .find(|fill| span_contains(fill.source_span, cluster_span))
        .map_or(base, |fill| Some(fill.color))
}

fn run_segment(run: &GlyphRun, start: usize, end: usize, fill: Option<Color>) -> GlyphRun {
    let mut segment = run.clone();
    segment.fill = fill;
    segment.glyphs = Arc::<[PositionedGlyph]>::from(run.glyphs[start..end].to_vec());
    segment
}

fn spans_overlap(left: TextSourceSpan, right: TextSourceSpan) -> bool {
    left.start < right.end && right.start < left.end
}

fn span_contains(outer: TextSourceSpan, inner: TextSourceSpan) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FontFaceIdentity, Rect, TextAffineTransform, TextClusterIdentity, TextDirection,
        TextSourceKind, Vec2, BLUE, RED,
    };

    fn glyph(character: char, start: u32, end: u32, ordinal: u32) -> PositionedGlyph {
        PositionedGlyph {
            glyph_id: character as u32,
            cluster: TextClusterIdentity {
                source_span: TextSourceSpan::new(start, end),
                cluster_ordinal: ordinal,
                semantic_key: None,
            },
            origin: Vec2::new(ordinal as f32, 0.0),
            advance: Vec2::new(1.0, 0.0),
            bounds: Rect::new(
                Vec2::new(ordinal as f32, -0.2),
                Vec2::new(ordinal as f32 + 0.8, 0.8),
            ),
        }
    }

    fn resource(source: &str, glyphs: Vec<PositionedGlyph>) -> TextResource {
        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: Arc::from([GlyphRun {
                font: FontFaceIdentity {
                    family: Arc::from("Test Sans"),
                    face_key: Arc::from("test-sans-v1"),
                    face_index: 0,
                    variation_key: Arc::from(""),
                },
                variations: Arc::from([]),
                font_size: 48.0,
                direction: TextDirection::LeftToRight,
                fill: None,
                stroke: None,
                transform: TextAffineTransform::IDENTITY,
                glyphs: glyphs.into(),
            }]),
            vector_items: Arc::from([]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::new(0.0, -0.2), Vec2::new(source.len() as f32, 0.8)),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    #[test]
    fn source_fill_splits_runs_without_relayout_or_cluster_identity_changes() {
        let original = resource(
            "abcd",
            vec![
                glyph('a', 0, 1, 0),
                glyph('b', 1, 2, 1),
                glyph('c', 2, 3, 2),
                glyph('d', 3, 4, 3),
            ],
        );
        let styled = original
            .with_source_fills(&[TextSourceFill::new(TextSourceSpan::new(1, 3), RED)])
            .unwrap();

        assert_eq!(styled.runs.len(), 3);
        assert_eq!(styled.runs[0].fill, None);
        assert_eq!(styled.runs[1].fill, Some(RED));
        assert_eq!(styled.runs[2].fill, None);
        let before = original
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .cloned()
            .collect::<Vec<_>>();
        let after = styled
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        assert_eq!(styled.bounds, original.bounds);
        assert_eq!(styled.baseline, original.baseline);
        assert_eq!(styled.parts.len(), 1);
        assert_eq!(styled.parts[0].source_span, TextSourceSpan::new(1, 3));
        assert_eq!(
            styled.parts[0].semantic_key.as_deref(),
            Some("noon:text:source:1:3")
        );
    }

    #[test]
    fn conflicting_overlapping_fills_are_ambiguous_before_run_changes() {
        let original = resource(
            "abc",
            vec![
                glyph('a', 0, 1, 0),
                glyph('b', 1, 2, 1),
                glyph('c', 2, 3, 2),
            ],
        );
        assert_eq!(
            original.with_source_fills(&[
                TextSourceFill::new(TextSourceSpan::new(0, 3), RED),
                TextSourceFill::new(TextSourceSpan::new(1, 2), BLUE),
            ]),
            Err(TextSourceStyleError::AmbiguousFillOverlap {
                left: TextSourceSpan::new(0, 3),
                right: TextSourceSpan::new(1, 2),
            })
        );
    }

    #[test]
    fn equal_overlapping_explicit_fills_are_also_ambiguous() {
        let original = resource(
            "abc",
            vec![
                glyph('a', 0, 1, 0),
                glyph('b', 1, 2, 1),
                glyph('c', 2, 3, 2),
            ],
        );
        assert_eq!(
            original.with_source_fills(&[
                TextSourceFill::new(TextSourceSpan::new(0, 3), RED),
                TextSourceFill::new(TextSourceSpan::new(1, 2), RED),
            ]),
            Err(TextSourceStyleError::AmbiguousFillOverlap {
                left: TextSourceSpan::new(0, 3),
                right: TextSourceSpan::new(1, 2),
            })
        );
    }

    #[test]
    fn source_range_that_splits_a_shaped_cluster_is_explicitly_rejected() {
        let original = resource("fi", vec![glyph('f', 0, 2, 0)]);
        assert_eq!(
            original.with_source_fills(&[TextSourceFill::new(TextSourceSpan::new(1, 2), RED,)]),
            Err(TextSourceStyleError::SplitsCluster {
                source_span: TextSourceSpan::new(1, 2),
                cluster_span: TextSourceSpan::new(0, 2),
            })
        );
    }

    #[test]
    fn source_only_span_adds_identity_without_changing_runs() {
        let original = resource("a\nb", vec![glyph('a', 0, 1, 0), glyph('b', 2, 3, 1)]);
        let styled = original
            .with_source_fills(&[TextSourceFill::new(TextSourceSpan::new(1, 2), RED)])
            .unwrap();
        assert_eq!(styled.runs, original.runs);
        assert_eq!(styled.parts[0].source_span, TextSourceSpan::new(1, 2));
        assert_eq!(styled.parts[0].cluster_count, 0);
    }
}
