//! Stable source-part queries over normalized retained text resources.
//!
//! Public text APIs select authored substrings; renderers consume shaped runs and vectors.
//! This module bridges those views without introducing frontend-owned glyph geometry or a
//! second text document. Source spans are the stable identity. Cluster/vector ranges are
//! derived observations of the currently compiled resource and may change when shaping or
//! compilation changes while the authored source span remains the same.

use std::{fmt, sync::Arc};

use super::{GlyphRun, TextPart, TextRenderItem, TextResource, TextSourceSpan};
use crate::{Rect, Vec2};

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
    /// A selected vector refers to missing or retired geometry.
    MissingGeometry(crate::GeometryResourceHandle),
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
            Self::MissingGeometry(handle) => {
                write!(formatter, "text part geometry is unavailable: {handle:?}")
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
    /// Build one independently transformable retained leaf for an authored part.
    ///
    /// The leaf keeps the canonical source and shares font identities, variation
    /// arrays, layout identity, and vector geometry handles with the compiled
    /// resource. Only the small run/item index vectors and selected glyph records
    /// are projected once at authoring time. No outline extraction or geometry
    /// compilation is performed here or during ordinary frame evaluation.
    pub fn projected_part(
        &self,
        part: &TextPart,
        geometry: &impl crate::GeometryResourceLookup,
    ) -> Result<Self, TextPartQueryError> {
        let cluster_end = part
            .first_cluster
            .checked_add(part.cluster_count)
            .ok_or(TextPartQueryError::NonContiguousClusters)?;
        let vector_end = part
            .first_vector
            .checked_add(part.vector_count)
            .ok_or(TextPartQueryError::NonContiguousVectors)?;
        if cluster_end > u32::try_from(self.cluster_count()).unwrap_or(u32::MAX)
            || vector_end > u32::try_from(self.vector_count()).unwrap_or(u32::MAX)
        {
            return Err(TextPartQueryError::InvalidSourceSpan);
        }

        let mut projected_runs = Vec::new();
        let mut run_map = vec![None; self.runs.len()];
        let mut bounds = None;
        for (old_index, run) in self.runs.iter().enumerate() {
            let glyphs = run
                .glyphs
                .iter()
                .filter(|glyph| {
                    (part.first_cluster..cluster_end).contains(&glyph.cluster.cluster_ordinal)
                })
                .cloned()
                .map(|mut glyph| {
                    glyph.cluster.cluster_ordinal -= part.first_cluster;
                    glyph
                })
                .collect::<Vec<_>>();
            if glyphs.is_empty() {
                continue;
            }
            let mut projected: GlyphRun = run.clone();
            projected.glyphs = glyphs.into();
            for glyph in projected.glyphs.iter() {
                let glyph_bounds = transform_rect(glyph.bounds, run.transform);
                bounds =
                    Some(bounds.map_or(glyph_bounds, |current: Rect| current.union(glyph_bounds)));
            }
            let new_index = u32::try_from(projected_runs.len())
                .map_err(|_| TextPartQueryError::NonContiguousClusters)?;
            run_map[old_index] = Some(new_index);
            projected_runs.push(projected);
        }

        let vector_start = usize::try_from(part.first_vector)
            .map_err(|_| TextPartQueryError::NonContiguousVectors)?;
        let vector_end_usize =
            usize::try_from(vector_end).map_err(|_| TextPartQueryError::NonContiguousVectors)?;
        let projected_vectors = self.vector_items[vector_start..vector_end_usize].to_vec();
        // An indexed fraction/radical part owns only its selected rules. Using
        // the complete formula's bounds would corrupt part layout and matching.
        for vector in &projected_vectors {
            let crate::GeometryResource::VectorPath(path) = geometry
                .get(vector.geometry)
                .ok_or(TextPartQueryError::MissingGeometry(vector.geometry))?;
            if let Some(local) =
                crate::semantic_path_bounds(path, f64::from(vector.style.stroke_width)).layout
            {
                let local = Rect::new(
                    Vec2::new(local.min_x as f32, local.min_y as f32),
                    Vec2::new(local.max_x as f32, local.max_y as f32),
                );
                let vector_bounds = transform_rect(local, vector.transform);
                bounds = Some(
                    bounds.map_or(vector_bounds, |current: Rect| current.union(vector_bounds)),
                );
            }
        }

        let mut render_items = Vec::new();
        for item in self.render_items.iter().copied() {
            match item {
                TextRenderItem::GlyphRun(old) => {
                    if let Some(Some(new)) = run_map.get(old as usize) {
                        render_items.push(TextRenderItem::GlyphRun(*new));
                    }
                }
                TextRenderItem::Vector(old) if old >= part.first_vector && old < vector_end => {
                    render_items.push(TextRenderItem::Vector(old - part.first_vector));
                }
                TextRenderItem::Vector(_) => {}
            }
        }

        let projected_part = TextPart {
            source_span: part.source_span,
            first_cluster: 0,
            cluster_count: part.cluster_count,
            first_vector: 0,
            vector_count: part.vector_count,
            semantic_key: part.semantic_key.clone(),
        };
        let resource = Self {
            source: self.source.clone(),
            kind: self.kind,
            runs: projected_runs.into(),
            vector_items: projected_vectors.into(),
            render_items: render_items.into(),
            parts: std::sync::Arc::from([projected_part]),
            bounds: bounds.unwrap_or_else(|| Rect::new(Vec2::ZERO, Vec2::ZERO)),
            baseline: self.baseline,
            layout_artifact: self.layout_artifact.clone(),
        };
        resource
            .validate()
            .map_err(|_| TextPartQueryError::InvalidSourceSpan)?;
        Ok(resource)
    }

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

fn transform_rect(bounds: Rect, transform: super::TextAffineTransform) -> Rect {
    let corners = [
        bounds.min,
        Vec2::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Vec2::new(bounds.min.x, bounds.max.y),
    ]
    .map(|point| transform.transform_point(point));
    let mut min = corners[0];
    let mut max = corners[0];
    for point in corners.into_iter().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Rect::new(min, max)
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
    fn projected_part_is_a_valid_single_part_resource_with_local_indices() {
        let resource = sample_text("ab cd");
        let part = resource.source_parts_for("cd").unwrap().remove(0);

        let projected = resource
            .projected_part(&part, &crate::GeometryResourceArena::new())
            .unwrap();

        assert_eq!(projected.source.as_ref(), "ab cd");
        assert_eq!(projected.cluster_count(), 2);
        assert_eq!(projected.runs.len(), 1);
        assert_eq!(projected.runs[0].glyphs.len(), 2);
        assert_eq!(
            projected.render_items.as_ref(),
            [TextRenderItem::GlyphRun(0)]
        );
        assert_eq!(projected.parts.len(), 1);
        assert_eq!(projected.parts[0].source_span, TextSourceSpan::new(3, 5));
        assert_eq!(projected.parts[0].semantic_key, part.semantic_key);
        assert_eq!(projected.parts[0].first_cluster, 0);
        assert_eq!(projected.parts[0].cluster_count, 2);
        let queried = projected.source_parts_for("cd").unwrap();
        assert_eq!(queried[0].first_cluster, 0);
        assert_eq!(queried[0].cluster_count, 2);
        let selected_again = projected
            .projected_part(&queried[0], &crate::GeometryResourceArena::new())
            .unwrap();
        assert_eq!(selected_again.runs, projected.runs);
        projected.validate().unwrap();
    }

    #[test]
    fn projected_part_keeps_all_glyphs_of_one_cluster() {
        let mut resource = sample_text("ab");
        let runs = Arc::make_mut(&mut resource.runs);
        let mut glyphs = runs[0].glyphs.to_vec();
        glyphs.insert(1, glyphs[0].clone());
        runs[0].glyphs = glyphs.into();
        let geometry = crate::GeometryResourceArena::new();
        for (needle, count) in [("a", 2), ("b", 1)] {
            let part = resource.source_parts_for(needle).unwrap().remove(0);
            let projected = resource.projected_part(&part, &geometry).unwrap();
            assert_eq!(projected.glyph_count(), count);
            assert!(projected.runs[0]
                .glyphs
                .iter()
                .all(|glyph| glyph.cluster.cluster_ordinal == 0));
            assert_eq!(
                projected.source_parts_for(needle).unwrap()[0].first_cluster,
                0
            );
            assert_eq!(
                projected.runs[0].glyphs[0].glyph_id,
                needle.as_bytes()[0] as u32
            );
        }
    }

    #[test]
    fn projected_part_bounds_include_only_selected_vector_resources() {
        let mut geometry = crate::GeometryResourceArena::new();
        let rule = geometry.insert_path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0))
                .line_to(Vec2::new(1.0, 0.1))
                .line_to(Vec2::new(0.0, 0.1))
                .close(),
        );
        let mut resource = sample_text("ab");
        resource.vector_items = Arc::from(
            [4.0, 100.0]
                .into_iter()
                .enumerate()
                .map(|(i, x)| crate::TextVectorItem {
                    geometry: rule,
                    transform: TextAffineTransform::translation(x, 2.0),
                    style: crate::TextVectorStyle::default(),
                    source_span: Some(TextSourceSpan::new(i as u32, i as u32 + 1)),
                    semantic_key: None,
                })
                .collect::<Vec<_>>(),
        );
        resource.render_items = Arc::from([
            TextRenderItem::GlyphRun(0),
            TextRenderItem::Vector(0),
            TextRenderItem::Vector(1),
        ]);
        resource.bounds = Rect::new(Vec2::ZERO, Vec2::new(101.0, 2.1));
        let part = resource.source_parts_for("a").unwrap().remove(0);
        let projected = resource.projected_part(&part, &geometry).unwrap();
        assert_eq!(projected.bounds.max, Vec2::new(5.0, 2.1));
        assert_eq!(projected.vector_items.len(), 1);
        assert_eq!(projected.vector_items[0].geometry, rule);
        assert_eq!(
            resource.projected_part(&part, &crate::GeometryResourceArena::new()),
            Err(TextPartQueryError::MissingGeometry(rule))
        );
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
