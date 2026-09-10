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

        Ok(project_source_parts(self, &[source_span])?.remove(0))
    }

    /// Return every non-overlapping occurrence of `needle` in authored UTF-8 source order.
    ///
    /// Empty needles intentionally select nothing. This mirrors text-part selection rather
    /// than Rust's zero-width string matching and avoids manufacturing identity-only parts at
    /// every source boundary. One batched projection traverses the retained
    /// glyph/vector records once, routing each record only to intersecting matches.
    pub fn source_parts_for(&self, needle: &str) -> Result<Vec<TextPart>, TextPartQueryError> {
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        let spans = self
            .source
            .match_indices(needle)
            .map(|(start, matched)| {
                let end = start
                    .checked_add(matched.len())
                    .ok_or(TextPartQueryError::InvalidSourceSpan)?;
                let start =
                    u32::try_from(start).map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
                let end = u32::try_from(end).map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
                Ok(TextSourceSpan::new(start, end))
            })
            .collect::<Result<Vec<_>, _>>()?;
        project_source_parts(self, &spans)
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

#[derive(Default)]
struct SourceSelection {
    clusters: Vec<u32>,
    first_vector: Option<u32>,
    last_vector: u32,
    vector_count: u32,
    // Prefix events: this record can establish the insertion position for
    // this and every later query. One prefix pass resolves all empty ranges.
    cluster_insertion: Option<u32>,
    vector_insertion: u32,
}

/// Project sorted, non-overlapping source ranges in one resource traversal.
/// Each glyph/vector visits only intersecting queries, found by binary search.
/// Repeated substring matching does not rescan the resource for every match.
fn project_source_parts(
    resource: &TextResource,
    queries: &[TextSourceSpan],
) -> Result<Vec<TextPart>, TextPartQueryError> {
    if queries.is_empty() {
        return Ok(Vec::new());
    }
    let mut selected: Vec<SourceSelection> =
        queries.iter().map(|_| SourceSelection::default()).collect();
    for glyph in resource.runs.iter().flat_map(|run| run.glyphs.iter()) {
        let span = glyph.cluster.source_span;
        let ordinal = glyph.cluster.cluster_ordinal;
        let first = queries.partition_point(|query| query.end <= span.start);
        for (query, selection) in queries[first..].iter().zip(&mut selected[first..]) {
            if query.start >= span.end {
                break;
            }
            if spans_overlap(span, *query) {
                selection.clusters.push(ordinal);
            }
        }
        let insertion = queries.partition_point(|query| query.start < span.end);
        if let Some(selection) = selected.get_mut(insertion) {
            selection.cluster_insertion = selection.cluster_insertion.max(Some(ordinal));
        }
    }
    for (index, item) in resource.vector_items.iter().enumerate() {
        let Some(span) = item.source_span else {
            continue;
        };
        let ordinal = u32::try_from(index).map_err(|_| TextPartQueryError::NonContiguousVectors)?;
        let first = queries.partition_point(|query| query.end <= span.start);
        for (query, selection) in queries[first..].iter().zip(&mut selected[first..]) {
            if query.start >= span.end {
                break;
            }
            if spans_overlap(span, *query) {
                selection.first_vector.get_or_insert(ordinal);
                selection.last_vector = ordinal;
                selection.vector_count = selection
                    .vector_count
                    .checked_add(1)
                    .ok_or(TextPartQueryError::NonContiguousVectors)?;
            }
        }
        let insertion = queries.partition_point(|query| query.start < span.end);
        if let Some(selection) = selected.get_mut(insertion) {
            selection.vector_insertion = selection.vector_insertion.max(
                ordinal
                    .checked_add(1)
                    .ok_or(TextPartQueryError::NonContiguousVectors)?,
            );
        }
    }
    let mut cluster_insertion = None;
    let mut vector_insertion = 0;
    queries
        .iter()
        .zip(selected)
        .map(|(&source_span, mut selection)| {
            cluster_insertion = cluster_insertion.max(selection.cluster_insertion);
            vector_insertion = vector_insertion.max(selection.vector_insertion);
            selection.clusters.sort_unstable();
            selection.clusters.dedup();
            let (first_cluster, cluster_count) = if let (Some(&first), Some(&last)) =
                (selection.clusters.first(), selection.clusters.last())
            {
                let count = last
                    .checked_sub(first)
                    .and_then(|delta| delta.checked_add(1))
                    .ok_or(TextPartQueryError::NonContiguousClusters)?;
                if usize::try_from(count).ok() != Some(selection.clusters.len()) {
                    return Err(TextPartQueryError::NonContiguousClusters);
                }
                (first, count)
            } else {
                (
                    cluster_insertion
                        .and_then(|ordinal: u32| ordinal.checked_add(1))
                        .unwrap_or(0),
                    0,
                )
            };
            let first_vector = match selection.first_vector {
                Some(first) => {
                    if selection
                        .last_vector
                        .checked_sub(first)
                        .and_then(|delta| delta.checked_add(1))
                        != Some(selection.vector_count)
                    {
                        return Err(TextPartQueryError::NonContiguousVectors);
                    }
                    first
                }
                None => vector_insertion,
            };
            Ok(TextPart {
                source_span,
                first_cluster,
                cluster_count,
                first_vector,
                vector_count: selection.vector_count,
                semantic_key: Some(source_part_key(source_span)),
            })
        })
        .collect()
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
    #[test]
    fn repeated_source_matches_project_all_ranges_in_one_batch() {
        let source = "é\n".repeat(10_000);
        let resource = sample_text(&source);
        let matches = resource.source_parts_for("é").unwrap();
        let breaks = resource.source_parts_for("\n").unwrap();
        assert_eq!(matches.len(), 10_000);
        assert_eq!(breaks.len(), matches.len());
        for (index, (part, newline)) in matches.iter().zip(breaks).enumerate() {
            assert_eq!(
                part.source_span,
                TextSourceSpan::new(index as u32 * 3, index as u32 * 3 + 2)
            );
            assert_eq!((part.first_cluster, part.cluster_count), (index as u32, 1));
            assert_eq!(
                (newline.first_cluster, newline.cluster_count),
                (index as u32 + 1, 0)
            );
        }
    }

    #[test]
    fn out_of_source_order_clusters_and_duplicate_glyphs_keep_contiguous_selection() {
        let mut resource = sample_text("ab ab");
        let mut run = resource.runs[0].clone();
        let mut glyphs = run.glyphs.to_vec();
        glyphs.reverse();
        glyphs.push(glyphs[0].clone());
        run.glyphs = glyphs.into();
        resource.runs = Arc::from([run]);
        let parts = resource.source_parts_for("ab").unwrap();
        assert_eq!((parts[0].first_cluster, parts[0].cluster_count), (0, 2));
        assert_eq!((parts[1].first_cluster, parts[1].cluster_count), (3, 2));
        for part in parts {
            assert_eq!(part, resource.source_part(part.source_span).unwrap());
        }
    }
    #[test]
    fn vector_ranges_preserve_physical_order_and_reject_disconnected_coverage() {
        let mut resource = sample_text("ab ab");
        let mut arena = crate::GeometryResourceArena::new();
        let geometry = arena.insert_path(crate::VectorPath::new());
        let item = |span| crate::TextVectorItem {
            geometry,
            transform: TextAffineTransform::IDENTITY,
            style: crate::TextVectorStyle::default(),
            source_span: span,
            semantic_key: None,
        };
        resource.vector_items = Arc::from([
            item(Some(TextSourceSpan::new(0, 2))),
            item(Some(TextSourceSpan::new(3, 5))),
        ]);
        let parts = resource.source_parts_for("ab").unwrap();
        assert_eq!((parts[0].first_vector, parts[0].vector_count), (0, 1));
        assert_eq!((parts[1].first_vector, parts[1].vector_count), (1, 1));
        let space = resource.source_part(TextSourceSpan::new(2, 3)).unwrap();
        assert_eq!((space.first_vector, space.vector_count), (1, 0));
        resource.vector_items = Arc::from([
            item(Some(TextSourceSpan::new(0, 2))),
            item(None),
            item(Some(TextSourceSpan::new(0, 2))),
        ]);
        assert_eq!(
            resource.source_parts_for("ab"),
            Err(TextPartQueryError::NonContiguousVectors)
        );
    }
}
