use std::ops::Range;

use bytemuck::Zeroable;

use crate::{FramePreparer, MegaPathBatch, OrderedRenderBatch, PathInstance, RenderPrimitive};

impl FramePreparer {
    /// Builds the static packed index/attribute stream used by unique vector paths.
    ///
    /// This is a compaction operation and runs only on a full frame rebuild. Live
    /// geometry edits detach the affected path from this stream instead of rewriting
    /// every packed index or per-vertex attribute that follows it.
    pub(crate) fn rebuild_mega_path_draws(&mut self) {
        self.mega_path_indices.clear();
        self.mega_path_vertex_instances.clear();
        self.mega_path_batches.clear();
        self.mega_path_segments.clear();
        self.mega_path_offsets.clear();
        self.mega_path_detached.clear();
        self.mega_path_instance_dirty_ranges.clear();
        self.mega_path_index_dirty_ranges.clear();
        self.mega_path_index_dirty = false;
        if self.individual_path_draws && !self.mixed_mega_path_draws {
            return;
        }

        let eligible = self
            .path_batches
            .iter()
            .enumerate()
            .map(|(index, batch)| {
                self.path_mesh_cache[self.path_batch_cache_indices[index]]
                    .sampled
                    .is_none()
                    && crate::single_filled_convex_polygon(
                        &self.path_mesh_cache[self.path_batch_cache_indices[index]].mesh,
                    )
                    .is_none()
                    && batch.instance_range.end == batch.instance_range.start + 1
                    && !batch.index_range.is_empty()
                    && !self
                        .individual_path_objects
                        .contains(&self.path_ids[batch.instance_range.start as usize])
            })
            .collect::<Vec<_>>();
        self.mega_path_segments
            .resize(self.path_batches.len(), None);
        self.mega_path_detached
            .resize(self.path_batches.len(), false);
        if !eligible.iter().any(|&eligible| eligible) {
            return;
        }

        self.mega_path_vertex_instances
            .resize(self.path_vertices.len(), PathInstance::zeroed());
        for (batch_index, &eligible) in eligible.iter().enumerate() {
            if !eligible {
                continue;
            }
            let batch = &self.path_batches[batch_index];
            let instance = self.paths[batch.instance_range.start as usize];
            let vertex_range = range_usize(&self.path_batch_vertex_ranges[batch_index]);
            self.mega_path_vertex_instances[vertex_range].fill(instance);
        }

        // Pack unique-path indices in semantic painter order. This gives each
        // unique path a stable slice that later render-batch rebuilding can reuse
        // without touching the packed GPU buffer.
        for ordered_batch in &self.render_batches {
            let RenderPrimitive::Path {
                batch: path_batch_index,
            } = ordered_batch.primitive
            else {
                continue;
            };
            if !eligible[path_batch_index] || self.mega_path_segments[path_batch_index].is_some() {
                continue;
            }

            let path_batch = &self.path_batches[path_batch_index];
            let index_range = range_usize(&path_batch.index_range);
            let packed_start = u32::try_from(self.mega_path_indices.len())
                .expect("mega path index count exceeds renderer limits");
            self.mega_path_indices
                .extend_from_slice(&self.path_indices[index_range]);
            let packed_end = u32::try_from(self.mega_path_indices.len())
                .expect("mega path index count exceeds renderer limits");
            let segment = packed_start..packed_end;
            self.mega_path_segments[path_batch_index] = Some(segment.clone());
            self.mega_path_offsets.push(packed_start);
            // The painter traversal borrows render_batches; update disjoint cache fields.
            self.zero_contribution[4].set(
                segment,
                !self.paths[path_batch.instance_range.start as usize]
                    .style
                    .may_contribute_color(),
            );
        }

        self.rebuild_mega_render_batches();
        if !self.mega_path_indices.is_empty() {
            self.mega_path_index_dirty = true;
            self.mega_path_index_dirty_ranges
                .push(0..self.mega_path_indices.len());
        }
        if !self.mega_path_vertex_instances.is_empty() {
            self.mega_path_instance_dirty_ranges
                .push(0..self.mega_path_vertex_instances.len());
        }
    }

    /// Appends one unique path to the packed painter-order stream without
    /// rewriting any existing mega indices or vertex-instance attributes.
    pub(crate) fn append_mega_path_draw(
        &mut self,
        path_batch_index: usize,
        packed: PathInstance,
    ) -> bool {
        if self.individual_path_draws && !self.mixed_mega_path_draws {
            return false;
        }
        let Some(path_batch) = self.path_batches.get(path_batch_index).cloned() else {
            return false;
        };
        if self.path_mesh_cache[self.path_batch_cache_indices[path_batch_index]]
            .sampled
            .is_some()
            || crate::single_filled_convex_polygon(
                &self.path_mesh_cache[self.path_batch_cache_indices[path_batch_index]].mesh,
            )
            .is_some()
        {
            return false;
        }
        if path_batch.instance_range.end != path_batch.instance_range.start + 1
            || path_batch.index_range.is_empty()
            || self
                .individual_path_objects
                .contains(&self.path_ids[path_batch.instance_range.start as usize])
        {
            return false;
        }
        let Some(vertex_range) = self.path_batch_vertex_ranges.get(path_batch_index).cloned()
        else {
            return false;
        };

        if self.mega_path_segments.len() <= path_batch_index {
            self.mega_path_segments.resize(path_batch_index + 1, None);
        }
        if self.mega_path_detached.len() <= path_batch_index {
            self.mega_path_detached.resize(path_batch_index + 1, false);
        }

        let source = range_usize(&path_batch.index_range);
        let segment_start = u32::try_from(self.mega_path_indices.len())
            .expect("mega path index count exceeds renderer limits");
        self.mega_path_indices
            .extend_from_slice(&self.path_indices[source]);
        let segment_end = u32::try_from(self.mega_path_indices.len())
            .expect("mega path index count exceeds renderer limits");
        let segment = segment_start..segment_end;
        self.mega_path_segments[path_batch_index] = Some(segment.clone());
        self.mega_path_offsets.push(segment_start);
        self.set_mega_zero_contribution(segment.clone(), packed.style);
        self.mega_path_detached[path_batch_index] = false;
        push_dirty_range(
            &mut self.mega_path_index_dirty_ranges,
            range_usize(&segment),
        );
        self.mega_path_index_dirty = true;

        if self.mega_path_vertex_instances.len() < self.path_vertices.len() {
            self.mega_path_vertex_instances
                .resize(self.path_vertices.len(), PathInstance::zeroed());
        }
        let vertex_range = range_usize(&vertex_range);
        self.mega_path_vertex_instances[vertex_range.clone()].fill(packed);
        push_dirty_range(&mut self.mega_path_instance_dirty_ranges, vertex_range);

        if let Some(ordered) = self.render_batches.last_mut() {
            if let RenderPrimitive::MegaPath { batch } = ordered.primitive {
                let mega = &mut self.mega_path_batches[batch];
                if mega.index_range.end == segment.start && mega.coverage == path_batch.coverage {
                    mega.index_range.end = segment.end;
                    mega.path_count += 1;
                    ordered.instance_range.end += 1;
                    return true;
                }
            }
        }

        let batch = self.mega_path_batches.len();
        self.mega_path_batches.push(MegaPathBatch {
            index_range: segment,
            path_count: 1,
            coverage: path_batch.coverage,
        });
        self.render_batches.push(OrderedRenderBatch {
            primitive: RenderPrimitive::MegaPath { batch },
            instance_range: 0..1,
        });
        true
    }

    /// Rebuilds only CPU-side ordered draw descriptors around the immutable packed
    /// mega stream. Detached paths remain regular `Path` draws at their exact
    /// painter position until the next full rebuild compacts the mega stream.
    pub(crate) fn rebuild_mega_render_batches(&mut self) {
        self.mega_path_batches.clear();
        if self.individual_path_draws && !self.mixed_mega_path_draws {
            return;
        }
        let ordered = std::mem::take(&mut self.render_batches);
        let mut active_mega = None::<usize>;

        for ordered_batch in ordered {
            let RenderPrimitive::Path {
                batch: path_batch_index,
            } = ordered_batch.primitive
            else {
                active_mega = None;
                self.render_batches.push(ordered_batch);
                continue;
            };

            let segment = self
                .mega_path_segments
                .get(path_batch_index)
                .and_then(|segment| segment.as_ref())
                .filter(|_| {
                    !self
                        .mega_path_detached
                        .get(path_batch_index)
                        .copied()
                        .unwrap_or(true)
                })
                .cloned();
            let Some(segment) = segment else {
                active_mega = None;
                self.render_batches.push(ordered_batch);
                continue;
            };

            if let Some(mega_index) = active_mega {
                let mega = &mut self.mega_path_batches[mega_index];
                if mega.index_range.end == segment.start
                    && mega.coverage == self.path_batches[path_batch_index].coverage
                {
                    mega.index_range.end = segment.end;
                    mega.path_count += 1;
                    let ordered = self
                        .render_batches
                        .last_mut()
                        .expect("active mega batch must have an ordered batch");
                    ordered.instance_range.end += 1;
                    continue;
                }
            }

            let mega_index = self.mega_path_batches.len();
            self.mega_path_batches.push(MegaPathBatch {
                index_range: segment,
                path_count: 1,
                coverage: self.path_batches[path_batch_index].coverage,
            });
            self.render_batches.push(OrderedRenderBatch {
                primitive: RenderPrimitive::MegaPath { batch: mega_index },
                instance_range: 0..1,
            });
            active_mega = Some(mega_index);
        }
    }

    pub(crate) fn detach_mega_path(&mut self, path_batch_index: usize) {
        if self.individual_path_draws && !self.mixed_mega_path_draws {
            return;
        }
        if self
            .mega_path_segments
            .get(path_batch_index)
            .is_some_and(Option::is_some)
        {
            self.mega_path_detached[path_batch_index] = true;
        }
    }

    pub(crate) fn update_mega_path_instance(
        &mut self,
        path_batch_index: usize,
        packed: PathInstance,
    ) {
        // A non-detached path need not own a mega segment: sampled fills
        // live only in the mutable path arena, whose vertex ranges can grow.
        if self.individual_path_draws && !self.mixed_mega_path_draws
            || !self
                .mega_path_segments
                .get(path_batch_index)
                .is_some_and(Option::is_some)
            || self.mega_path_indices.is_empty()
            || self
                .mega_path_detached
                .get(path_batch_index)
                .copied()
                .unwrap_or(true)
            || self
                .path_batches
                .get(path_batch_index)
                .is_none_or(|batch| batch.instance_range.end != batch.instance_range.start + 1)
        {
            return;
        }
        let segment = self.mega_path_segments[path_batch_index]
            .clone()
            .expect("checked mega segment");
        self.set_mega_zero_contribution(segment, packed.style);
        let Some(vertex_range) = self.path_batch_vertex_ranges.get(path_batch_index) else {
            return;
        };
        let range = range_usize(vertex_range);
        if self.mega_path_vertex_instances[range.clone()]
            .iter()
            .all(|instance| *instance == packed)
        {
            return;
        }
        self.mega_path_vertex_instances[range.clone()].fill(packed);
        push_dirty_range(&mut self.mega_path_instance_dirty_ranges, range);
    }
}

fn range_usize(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

fn push_dirty_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = ranges.last_mut() {
        // Source-slot order need not match resident vertex order after preload
        // or arena reuse. Preserve earlier ranges for the existing normalization
        // pass instead of silently dropping their lower start (or filling gaps).
        if last.start <= range.start && last.end >= range.start {
            last.end = last.end.max(range.end);
            return;
        }
    }
    ranges.push(range);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversed_mega_dirty_ranges_preserve_clean_gaps() {
        let mut ranges = Vec::new();
        push_dirty_range(&mut ranges, 16..24);
        push_dirty_range(&mut ranges, 0..8);
        crate::normalize_dirty_ranges(&mut ranges);
        assert_eq!(ranges, vec![0..8, 16..24]);
    }

    #[test]
    fn mega_dirty_ranges_preserve_every_small_three_write_union() {
        let candidates = (0..=6)
            .flat_map(|start| (start..=6).map(move |end| start..end))
            .collect::<Vec<_>>();
        let coverage = |ranges: &[Range<usize>]| {
            ranges
                .iter()
                .flat_map(|range| range.clone())
                .fold(0_u64, |mask, index| mask | (1_u64 << index))
        };
        for first in &candidates {
            for second in &candidates {
                for third in &candidates {
                    let inputs = [first.clone(), second.clone(), third.clone()];
                    let mut actual = Vec::new();
                    for range in &inputs {
                        push_dirty_range(&mut actual, range.clone());
                    }
                    crate::normalize_dirty_ranges(&mut actual);
                    assert_eq!(
                        coverage(&actual),
                        coverage(&inputs),
                        "dirty upload coverage changed for {inputs:?}: {actual:?}"
                    );
                    assert!(actual.windows(2).all(|pair| pair[0].end < pair[1].start));
                }
            }
        }
    }

    #[test]
    fn mixed_preloaded_paths_publish_reverse_order_instance_updates() {
        use crate::PathMeshPreload;
        use noon_core::{
            Color, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2, VectorPath,
        };
        use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

        let geometries = (0..3)
            .map(|index| {
                GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-1.0, index as f32 * 0.2))
                        .line_to(Vec2::new(1.0 + index as f32 * 0.1, -0.5)),
                )
            })
            .collect::<Vec<_>>();
        let style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.08,
            ..Style::default()
        };
        let mut frame = FrameState {
            time: 0.0,
            objects: geometries
                .iter()
                .enumerate()
                .map(|(index, geometry)| FrameObjectState {
                    glow: None,
                    spatial: None,
                    id: ObjectId::new(index as u64 + 1),
                    z_index: 0.0,
                    content: ObjectContentRef::Geometry(geometry.clone()),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style,
                    appearance: 1.0,
                })
                .collect(),
            presences: vec![true; 3],
            reveals: vec![1.0; 3],
            morphs: vec![0.0; 3],
            render_geometries: vec![None; 3],
            render_transforms: vec![None; 3],
            family_animations: vec![],
            family_animation_plan_indices: vec![],
        };
        let requests = geometries
            .iter()
            .rev()
            .map(|geometry| PathMeshPreload {
                geometry,
                style,
                transform: Transform2D::IDENTITY,
            })
            .collect::<Vec<_>>();
        let mut preparer = FramePreparer::for_mixed_retained_path_draws();
        preparer.preload_paths(&requests).unwrap();
        let (mut uploaded, batches) = {
            let initial = preparer.prepare(&frame);
            assert_eq!(initial.stats.mega_path_count, 3);
            let batches = (0..3)
                .map(
                    |index| match initial.observe_object(index).unwrap().primitive {
                        RenderPrimitive::Path { batch } => batch,
                        primitive => panic!("expected a retained source path, got {primitive:?}"),
                    },
                )
                .collect::<Vec<_>>();
            (initial.mega_path_vertex_instances.to_vec(), batches)
        };
        let source_ranges = batches
            .iter()
            .map(|&batch| range_usize(&preparer.path_batch_vertex_ranges[batch]))
            .collect::<Vec<_>>();
        assert!(source_ranges[2].end <= source_ranges[1].start);
        assert!(source_ranges[1].end <= source_ranges[0].start);
        assert!(source_ranges.iter().all(|range| !range.is_empty()));
        let expected_dirty_vertices = source_ranges[0].len() + source_ranges[2].len();

        // Sorted source slots deliberately visit descending resident addresses.
        // Copy only advertised dirty slices into a retained upload mirror: a CPU
        // frame can look correct while a missed range leaves GPU data stale.
        for (first_opacity, last_opacity) in [(0.25, 0.5), (1.0, 1.0), (0.6, 0.3)] {
            frame.objects[0].style.opacity = first_opacity;
            frame.objects[2].style.opacity = last_opacity;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0, 2]));
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            assert_eq!(prepared.stats.path_vertices_repacked, 0);
            assert_eq!(prepared.stats.path_indices_repacked, 0);
            assert!(!prepared.mega_path_index_dirty);
            assert_eq!(
                prepared.stats.mega_path_instance_vertices_repacked,
                expected_dirty_vertices
            );
            assert!(prepared
                .mega_path_instance_dirty_ranges
                .iter()
                .all(|range| {
                    range.end <= source_ranges[1].start || range.start >= source_ranges[1].end
                }));
            for range in prepared.mega_path_instance_dirty_ranges {
                uploaded[range.clone()]
                    .copy_from_slice(&prepared.mega_path_vertex_instances[range.clone()]);
            }
            assert_eq!(uploaded.as_slice(), prepared.mega_path_vertex_instances);
        }

        let clean = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![]));
        assert!(clean.mega_path_instance_dirty_ranges.is_empty());
        assert!(clean.mega_path_index_dirty_ranges.is_empty());
        assert_eq!(clean.stats.full_rebuilds, 0);
        let rebuilt = preparer.prepare(&frame);
        assert_eq!(uploaded.as_slice(), rebuilt.mega_path_vertex_instances);
    }
}
