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
        self.mega_path_detached.clear();
        self.mega_path_instance_dirty_ranges.clear();
        self.mega_path_index_dirty = false;

        let eligible = self
            .path_batches
            .iter()
            .map(|batch| {
                batch.instance_range.end == batch.instance_range.start + 1
                    && !batch.index_range.is_empty()
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
            self.mega_path_segments[path_batch_index] = Some(packed_start..packed_end);
        }

        self.rebuild_mega_render_batches();
        if !self.mega_path_indices.is_empty() {
            self.mega_path_index_dirty = true;
        }
        if !self.mega_path_vertex_instances.is_empty() {
            self.mega_path_instance_dirty_ranges
                .push(0..self.mega_path_vertex_instances.len());
        }
    }

    /// Rebuilds only CPU-side ordered draw descriptors around the immutable packed
    /// mega stream. Detached paths remain regular `Path` draws at their exact
    /// painter position until the next full rebuild compacts the mega stream.
    pub(crate) fn rebuild_mega_render_batches(&mut self) {
        self.mega_path_batches.clear();
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
                if mega.index_range.end == segment.start {
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
            });
            self.render_batches.push(OrderedRenderBatch {
                primitive: RenderPrimitive::MegaPath { batch: mega_index },
                instance_range: 0..1,
            });
            active_mega = Some(mega_index);
        }
    }

    pub(crate) fn detach_mega_path(&mut self, path_batch_index: usize) {
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
        if self.mega_path_indices.is_empty()
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
        if last.end >= range.start {
            last.end = last.end.max(range.end);
            return;
        }
    }
    ranges.push(range);
}
