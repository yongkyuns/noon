//! Stable source-part queries over normalized retained text resources.
//!
//! Public text APIs select authored substrings; renderers consume shaped runs and vectors.
//! This module bridges those views without introducing frontend-owned glyph geometry or a
//! second text document. Source spans are the stable identity. Cluster/vector ranges are
//! derived observations of the currently compiled resource and may change when shaping or
//! compilation changes while the authored source span remains the same.

use std::{fmt, sync::Arc};

use super::{TextPart, TextResource, TextSourceSpan};

const SOURCE_PART_KEY_PREFIX: &str = "noon:text:source";

/// Failure to project an authored source range onto one normalized text resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPartQueryError {
    /// The byte range is reversed, outside the source, or splits a UTF-8 code point.
    InvalidSourceSpan,
    /// The normalized cluster identities selected by the source span are not contiguous.
    NonContiguousClusters,
    /// The normalized vector items selected by the source span are not contiguous.
    NonContiguousVectors,
}

impl fmt::Display for TextPartQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceSpan => write!(formatter, "invalid text source span"),
            Self::NonContiguousClusters => {
                write!(
                    formatter,
                    "text source span maps to non-contiguous clusters"
                )
            }
            Self::NonContiguousVectors => {
                write!(
                    formatter,
                    "text source span maps to non-contiguous vector items"
                )
            }
        }
    }
}

impl std::error::Error for TextPartQueryError {}

impl TextResource {
    /// Project one UTF-8 source span onto normalized cluster/vector ranges.
    ///
    /// `source_span` itself is the stable semantic identity. The synthesized semantic key is
    /// therefore deterministic across resource recompilation, paint changes, and ordinary
    /// object transforms. Cluster/vector ranges are deliberately recomputed from the current
    /// normalized resource rather than cached in a frontend sidecar.
    pub fn source_part(&self, source_span: TextSourceSpan) -> Result<TextPart, TextPartQueryError> {
        let start = usize::try_from(source_span.start)
            .map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
        let end =
            usize::try_from(source_span.end).map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
        if start > end
            || end > self.source.len()
            || !self.source.is_char_boundary(start)
            || !self.source.is_char_boundary(end)
        {
            return Err(TextPartQueryError::InvalidSourceSpan);
        }

        let (first_cluster, cluster_count) = contiguous_cluster_range(self, source_span)?;
        let (first_vector, vector_count) = contiguous_vector_range(self, source_span)?;

        Ok(TextPart {
            source_span,
            first_cluster,
            cluster_count,
            first_vector,
            vector_count,
            semantic_key: Some(source_part_key(source_span)),
        })
    }

    /// Return every non-overlapping occurrence of `needle` in authored UTF-8 source order.
    ///
    /// Empty needles intentionally select nothing. This mirrors text-part selection rather
    /// than Rust's zero-width string matching and avoids manufacturing identity-only parts at
    /// every source boundary.
    pub fn source_parts_for(&self, needle: &str) -> Result<Vec<TextPart>, TextPartQueryError> {
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        self.source
            .match_indices(needle)
            .map(|(start, matched)| {
                let end = start
                    .checked_add(matched.len())
                    .ok_or(TextPartQueryError::InvalidSourceSpan)?;
                let start =
                    u32::try_from(start).map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
                let end = u32::try_from(end).map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
                self.source_part(TextSourceSpan::new(start, end))
            })
            .collect()
    }
}

fn source_part_key(span: TextSourceSpan) -> Arc<str> {
    Arc::from(format!(
        "{SOURCE_PART_KEY_PREFIX}:{}:{}",
        span.start, span.end
    ))
}

fn spans_overlap(candidate: TextSourceSpan, query: TextSourceSpan) -> bool {
    candidate.start < query.end && query.start < candidate.end
}

fn contiguous_cluster_range(
    resource: &TextResource,
    query: TextSourceSpan,
) -> Result<(u32, u32), TextPartQueryError> {
    let mut selected = resource
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .filter(|glyph| spans_overlap(glyph.cluster.source_span, query))
        .map(|glyph| glyph.cluster.cluster_ordinal)
        .collect::<Vec<_>>();
    selected.sort_unstable();
    selected.dedup();

    if let (Some(first), Some(last)) = (selected.first().copied(), selected.last().copied()) {
        let count = last
            .checked_sub(first)
            .and_then(|delta| delta.checked_add(1))
            .ok_or(TextPartQueryError::NonContiguousClusters)?;
        if usize::try_from(count).ok() != Some(selected.len()) {
            return Err(TextPartQueryError::NonContiguousClusters);
        }
        return Ok((first, count));
    }

    // Source-only spans such as a newline still have deterministic insertion identity.
    // Place their empty cluster range immediately after clusters ending before the span.
    let insertion = resource
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .filter(|glyph| glyph.cluster.source_span.end <= query.start)
        .map(|glyph| glyph.cluster.cluster_ordinal)
        .max()
        .and_then(|ordinal| ordinal.checked_add(1))
        .unwrap_or(0);
    Ok((insertion, 0))
}

fn contiguous_vector_range(
    resource: &TextResource,
    query: TextSourceSpan,
) -> Result<(u32, u32), TextPartQueryError> {
    let selected = resource
        .vector_items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.source_span
                .is_some_and(|span| spans_overlap(span, query))
        })
        .map(|(index, _)| {
            u32::try_from(index).map_err(|_| TextPartQueryError::NonContiguousVectors)
        })
        .collect::<Result<Vec<_>, _>>()?;

    if let (Some(first), Some(last)) = (selected.first().copied(), selected.last().copied()) {
        let count = last
            .checked_sub(first)
            .and_then(|delta| delta.checked_add(1))
            .ok_or(TextPartQueryError::NonContiguousVectors)?;
        if usize::try_from(count).ok() != Some(selected.len()) {
            return Err(TextPartQueryError::NonContiguousVectors);
        }
        return Ok((first, count));
    }

    let insertion = resource
        .vector_items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.source_span.is_some_and(|span| span.end <= query.start))
        .map(|(index, _)| index + 1)
        .max()
        .unwrap_or(0);
    Ok((
        u32::try_from(insertion).map_err(|_| TextPartQueryError::NonContiguousVectors)?,
        0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FontFaceIdentity, GlyphRun, PositionedGlyph, Rect, TextAffineTransform,
        TextClusterIdentity, TextDirection, TextRenderItem, TextSourceKind, Vec2,
    };

    fn sample_text(source: &str) -> TextResource {
        let glyphs: Arc<[PositionedGlyph]> = source
            .char_indices()
            .filter(|(_, character)| *character != '\n')
            .enumerate()
            .map(|(ordinal, (byte, character))| {
                let end = byte + character.len_utf8();
                PositionedGlyph {
                    glyph_id: character as u32,
                    cluster: TextClusterIdentity {
                        source_span: TextSourceSpan::new(byte as u32, end as u32),
                        cluster_ordinal: ordinal as u32,
                        semantic_key: None,
                    },
                    origin: Vec2::new(ordinal as f32, 0.0),
                    advance: Vec2::new(1.0, 0.0),
                    bounds: Rect::new(
                        Vec2::new(ordinal as f32, -0.2),
                        Vec2::new(ordinal as f32 + 0.8, 0.8),
                    ),
                }
            })
            .collect::<Vec<_>>()
            .into();

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
                glyphs,
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
    fn substring_parts_keep_utf8_source_identity_and_cluster_coverage() {
        let resource = sample_text("abé abé");
        let parts = resource.source_parts_for("abé").unwrap();

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].source_span, TextSourceSpan::new(0, 4));
        assert_eq!((parts[0].first_cluster, parts[0].cluster_count), (0, 3));
        assert_eq!(
            parts[0].semantic_key.as_deref(),
            Some("noon:text:source:0:4")
        );
        assert_eq!(parts[1].source_span, TextSourceSpan::new(5, 9));
        assert_eq!((parts[1].first_cluster, parts[1].cluster_count), (4, 3));
        assert_eq!(
            parts[1].semantic_key.as_deref(),
            Some("noon:text:source:5:9")
        );
    }

    #[test]
    fn source_part_identity_is_stable_across_recompiled_resources() {
        let first = sample_text("hello hello");
        let second = sample_text("hello hello");

        let first_parts = first.source_parts_for("hello").unwrap();
        let second_parts = second.source_parts_for("hello").unwrap();
        assert_eq!(first_parts, second_parts);
    }

    #[test]
    fn source_only_newline_part_has_stable_empty_cluster_range() {
        let resource = sample_text("a\nb");
        let part = resource.source_part(TextSourceSpan::new(1, 2)).unwrap();

        assert_eq!(part.source_span, TextSourceSpan::new(1, 2));
        assert_eq!((part.first_cluster, part.cluster_count), (1, 0));
        assert_eq!(part.semantic_key.as_deref(), Some("noon:text:source:1:2"));
    }

    #[test]
    fn invalid_utf8_source_boundaries_are_rejected() {
        let resource = sample_text("é");
        assert_eq!(
            resource.source_part(TextSourceSpan::new(1, 2)),
            Err(TextPartQueryError::InvalidSourceSpan)
        );
    }

    #[test]
    fn empty_substring_does_not_create_zero_width_parts() {
        let resource = sample_text("abc");
        assert!(resource.source_parts_for("").unwrap().is_empty());
    }
}
