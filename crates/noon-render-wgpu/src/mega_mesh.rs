use std::ops::Range;

use bytemuck::Zeroable;

use crate::{FramePreparer, MegaPathBatch, OrderedRenderBatch, PathInstance, RenderPrimitive};

impl FramePreparer {
    /// Optional packing cannot consume a GPU buffer that exceeds the device
    /// limit. A rejected path still has its ordinary source draw and indices.
    fn packed_path_allocation_fits(&self, vertex_end: usize, index_count: usize) -> bool {
        let limit = self.packed_path_buffer_limit.unwrap_or(u64::MAX);
        rounded_capacity_fits(vertex_end, std::mem::size_of::<PathInstance>(), limit)
            && rounded_capacity_fits(index_count, std::mem::size_of::<u32>(), limit)
    }

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

        let mut eligible = self
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
        // This full-rebuild pass already visits the source batches. Check each
        // candidate before materializing the repeated vertex attributes or
        // painter-ordered packed indices. No additional frame traversal is added.
        let mut packed_index_count = 0usize;
        for (index, selected) in eligible.iter_mut().enumerate() {
            if !*selected {
                continue;
            }
            let source_indices = &self.path_batches[index].index_range;
            let index_count = (source_indices.end - source_indices.start) as usize;
            let next = packed_index_count.checked_add(index_count);
            if let Some(next) = next.filter(|&count| {
                self.packed_path_allocation_fits(
                    self.path_batch_vertex_ranges[index].end as usize,
                    count,
                )
            }) {
                packed_index_count = next;
            } else {
                *selected = false;
            }
        }
        self.mega_path_segments
            .resize(self.path_batches.len(), None);
        self.mega_path_detached
            .resize(self.path_batches.len(), false);
        // Packed indices retain the shared vertex addresses, but an unused
        // resident suffix needs no per-vertex instance attributes. Inspect only
        // the already selected batches during this full rebuild, not all meshes.
        let vertex_end = eligible
            .iter()
            .enumerate()
            .filter(|(_, eligible)| **eligible)
            .map(|(index, _)| self.path_batch_vertex_ranges[index].end as usize)
            .max();
        let Some(vertex_end) = vertex_end else {
            return;
        };

        self.mega_path_vertex_instances
            .resize(vertex_end, PathInstance::zeroed());
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
        let source_index_count =
            (path_batch.index_range.end - path_batch.index_range.start) as usize;
        let Some(next_index_count) = self.mega_path_indices.len().checked_add(source_index_count)
        else {
            return false;
        };
        if !self.packed_path_allocation_fits(vertex_range.end as usize, next_index_count) {
            return false;
        }

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

        let vertex_range = range_usize(&vertex_range);
        if self.mega_path_vertex_instances.len() < vertex_range.end {
            self.mega_path_vertex_instances
                .resize(vertex_range.end, PathInstance::zeroed());
        }
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

/// Match the allocation rounded by the GPU uploader, including overflow and
/// the 256-byte minimum. An empty optional stream needs no buffer allocation.
fn rounded_capacity_fits(elements: usize, stride: usize, limit: u64) -> bool {
    let Some(bytes) = elements.checked_mul(stride) else {
        return false;
    };
    bytes == 0
        || bytes
            .checked_next_power_of_two()
            .and_then(|_| u64::try_from(crate::buffer_growth_capacity(bytes)).ok())
            .is_some_and(|capacity| capacity <= limit)
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
    fn packed_admission_checks_rounding_index_limits_and_overflow() {
        let mut preparer = FramePreparer::for_mixed_retained_path_draws();
        preparer.set_packed_path_buffer_limit(65_536);
        assert!(preparer.packed_path_allocation_fits(819, 16_384));
        assert!(!preparer.packed_path_allocation_fits(820, 1));
        assert!(!preparer.packed_path_allocation_fits(1, 16_385));
        assert!(!preparer.packed_path_allocation_fits(usize::MAX, 1));
        assert!(!preparer.packed_path_allocation_fits(1, usize::MAX));
        assert!(preparer.packed_path_allocation_fits(0, 0));
        preparer.set_packed_path_buffer_limit(255);
        assert!(!preparer.packed_path_allocation_fits(1, 1));
    }

    #[test]
    fn high_address_preload_falls_back_on_real_limited_device() {
        const LIMIT: usize = 65_536;
        let (_, _, _, sample_ranges) = preloaded_prefix_fixture(1);
        let vertices_per_path = sample_ranges[0].end as usize;
        let count = LIMIT / (vertices_per_path * std::mem::size_of::<crate::PathVertex>());
        assert!(count > 1);
        let (mut preparer, mut frame, rows, ranges) = preloaded_prefix_fixture(count);
        let high_end = ranges[count - 1].end as usize;
        assert_eq!(preparer.path_vertices.len(), high_end);
        assert!(
            crate::buffer_growth_capacity(high_end * std::mem::size_of::<crate::PathVertex>())
                <= LIMIT
        );
        assert!(
            crate::buffer_growth_capacity(high_end * std::mem::size_of::<PathInstance>()) > LIMIT
        );
        assert!(
            crate::buffer_growth_capacity(preparer.path_indices.len() * std::mem::size_of::<u32>())
                <= LIMIT
        );

        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_buffer_size: LIMIT as u64,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(device.limits().max_buffer_size, LIMIT as u64);
        let mut renderer =
            crate::GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let preload = preparer.preloaded_frame();
        renderer
            .upload_preloaded_paths(&device, &queue, &preload)
            .expect("original geometry and index buffers fit the reduced device limit");

        append_fixture_row(&mut frame, rows[count - 1].clone());
        let unbounded = preparer.prepare(&frame);
        assert_eq!(unbounded.stats.mega_path_count, 1);
        assert!(
            crate::buffer_growth_capacity(std::mem::size_of_val(
                unbounded.mega_path_vertex_instances
            )) > LIMIT,
            "the real preparer produces a packed allocation exceeding the device"
        );

        // Never upload the unbounded witness: it is a deliberate negative.
        // Installing the exact device limit forces safe derivation and fallback.
        preparer.set_packed_path_buffer_limit(device.limits().max_buffer_size);
        let bounded = preparer.prepare_incremental(&frame, &Default::default());
        assert_eq!(bounded.stats.mega_path_count, 0);
        assert_eq!(bounded.stats.unsupported_count, 0);
        assert!(bounded.mega_path_indices.is_empty());
        assert!(bounded.mega_path_vertex_instances.is_empty());
        assert!(bounded
            .ordered_render_batches()
            .any(|batch| { matches!(batch.batch.primitive, RenderPrimitive::Path { .. }) }));
        renderer.upload(&device, &queue, &bounded);

        // The ordinary pipeline still encodes and submits on this exact device.
        renderer.set_viewport(&device, &queue, 32, 32);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon limited packed fallback target"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draws = renderer.encode(&mut encoder, &view, &bounded, wgpu::Color::BLACK);
        queue.submit(Some(encoder.finish()));
        assert_eq!(draws.draw_calls, 1);

        // A sufficient separate device can still select the packed path.
        preparer.set_packed_path_buffer_limit(wgpu::Limits::default().max_buffer_size);
        let recovered = preparer.prepare(&frame);
        assert_eq!(recovered.stats.mega_path_count, 1);
    }

    #[test]
    fn structural_append_uses_local_individual_fallback_at_packed_limit() {
        use noon_core::{GeometryRef, ObjectId, Vec2, VectorPath};
        use noon_runtime::FrameChanges;

        let (mut preparer, mut frame, rows, ranges) = preloaded_prefix_fixture(32);
        let first_end = ranges[0].end as usize;
        let limit = crate::buffer_growth_capacity(first_end * std::mem::size_of::<PathInstance>());
        preparer.set_packed_path_buffer_limit(limit as u64);
        append_fixture_row(&mut frame, rows[0].clone());
        let initial = preparer.prepare(&frame);
        assert_eq!(initial.stats.mega_path_count, 1);
        let packed_prefix = initial.mega_path_vertex_instances.to_vec();

        let mut added = rows[0].clone();
        added.id = ObjectId::new(1001);
        added.content = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-2.0, 0.125))
                .line_to(Vec2::new(2.0, 0.125)),
        )
        .into();
        append_fixture_row(&mut frame, added);
        let appended = preparer.prepare_incremental(
            &frame,
            &FrameChanges::with_structure(vec![], vec![1], vec![]),
        );
        assert_eq!(appended.stats.full_rebuilds, 0);
        assert_eq!(appended.stats.mega_path_count, 1);
        assert_eq!(appended.stats.unsupported_count, 0);
        assert_eq!(appended.mega_path_vertex_instances, packed_prefix.as_slice());
        assert_eq!(appended.mega_path_instance_dirty_ranges.len(), 0);
        assert!(appended
            .ordered_render_batches()
            .any(|batch| { matches!(batch.batch.primitive, RenderPrimitive::Path { .. }) }));

        let clean = preparer.prepare_incremental(&frame, &FrameChanges::default());
        assert_eq!(clean.stats.full_rebuilds, 0);
        assert!(clean.mega_path_instance_dirty_ranges.is_empty());

        preparer.set_packed_path_buffer_limit(u64::MAX);
        let recovered = preparer.prepare(&frame);
        assert_eq!(recovered.stats.mega_path_count, 2);
    }

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

    fn preloaded_prefix_fixture(
        count: usize,
    ) -> (
        FramePreparer,
        noon_runtime::FrameState,
        Vec<noon_runtime::FrameObjectState>,
        Vec<Range<usize>>,
    ) {
        use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D, Vec2, VectorPath};
        use noon_runtime::{FrameObjectState, FrameState};

        let style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.08,
            ..Style::default()
        };
        let geometries = (0..count)
            .map(|index| {
                let y = index as f32 * 0.25;
                GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-1.0, y))
                        .line_to(Vec2::new(1.0 + index as f32 * 0.01, y)),
                )
            })
            .collect::<Vec<_>>();
        let requests = geometries
            .iter()
            .map(|geometry| crate::PathMeshPreload {
                geometry,
                style,
                transform: Transform2D::IDENTITY,
            })
            .collect::<Vec<_>>();
        let mut preparer = FramePreparer::for_mixed_retained_path_draws();
        preparer.preload_paths(&requests).unwrap();
        let ranges = preparer
            .path_mesh_cache
            .iter()
            .map(|cached| range_usize(&cached.resident.as_ref().unwrap().vertices))
            .collect::<Vec<_>>();
        assert_eq!(ranges.len(), count);
        assert_eq!(ranges[0].start, 0);
        assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
        let rows = geometries
            .into_iter()
            .enumerate()
            .map(|(index, geometry)| FrameObjectState {
                glow: None,
                spatial: None,
                id: ObjectId::new(index as u64 + 1),
                z_index: 0.0,
                content: geometry.into(),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style,
                appearance: 1.0,
            })
            .collect();
        let frame = FrameState {
            time: 0.0,
            objects: vec![],
            presences: vec![],
            reveals: vec![],
            morphs: vec![],
            render_geometries: vec![],
            render_transforms: vec![],
            family_animations: vec![],
            family_animation_plan_indices: vec![],
        };
        (preparer, frame, rows, ranges)
    }

    fn append_fixture_row(
        frame: &mut noon_runtime::FrameState,
        row: noon_runtime::FrameObjectState,
    ) {
        frame.objects.push(row);
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);
        frame.family_animations.push(None);
        frame.family_animation_plan_indices.push(None);
    }

    #[test]
    fn mixed_preloaded_suffix_stays_out_of_packed_attribute_uploads() {
        for count in [8, 1024] {
            let (mut preparer, mut frame, rows, ranges) = preloaded_prefix_fixture(count);
            let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
            let mut renderer =
                crate::GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
            append_fixture_row(&mut frame, rows[0].clone());
            let prepared = preparer.prepare(&frame);
            assert_eq!(prepared.stats.mega_path_count, 1);
            assert_eq!(prepared.path_vertices.len(), ranges[count - 1].end);
            assert_eq!(prepared.mega_path_vertex_instances.len(), ranges[0].end);
            assert!(ranges.iter().all(|range| range.len() == ranges[0].len()));
            assert_eq!(prepared.path_vertices.len(), count * ranges[0].end);
            assert!(prepared
                .mega_path_indices
                .iter()
                .all(|&index| (index as usize) < ranges[0].end));

            // Exercise the real upload entrypoint on wgpu's validating noop
            // device. These are submitted byte counts, not physical VRAM/FPS.
            let mut writes = Vec::new();
            let upload = renderer.upload_with_trace(&device, &queue, &prepared, &mut writes);
            assert_eq!(
                upload.bytes_uploaded,
                writes.iter().map(|write| write.byte_length).sum::<usize>()
            );
            let packed_writes = writes
                .iter()
                .filter(|write| write.buffer == "mega_path_vertex_instance")
                .collect::<Vec<_>>();
            assert_eq!(packed_writes.len(), 1);
            assert_eq!(packed_writes[0].instance_range, 0..ranges[0].end);
            assert_eq!(
                packed_writes[0].byte_length,
                ranges[0].end * std::mem::size_of::<PathInstance>()
            );

            let clean = preparer.prepare_incremental(&frame, &Default::default());
            let upload = renderer.upload(&device, &queue, &clean);
            assert_eq!(upload.bytes_uploaded, 0);
            assert_eq!(upload.buffer_reallocations, 0);
        }
    }

    #[test]
    fn mixed_preloaded_append_bounds_attributes_and_preserves_high_address_reentry() {
        use noon_core::{GeometryRef, ObjectId, Vec2, VectorPath};
        use noon_runtime::FrameChanges;

        let (mut preparer, mut frame, rows, ranges) = preloaded_prefix_fixture(32);
        let empty = preparer.prepare(&frame);
        assert!(!empty.path_vertices.is_empty());
        assert!(empty.mega_path_vertex_instances.is_empty());
        assert!(empty.mega_path_instance_dirty_ranges.is_empty());

        append_fixture_row(&mut frame, rows[0].clone());
        let initial = preparer.prepare(&frame);
        assert_eq!(initial.mega_path_vertex_instances.len(), ranges[0].end);
        let prefix = initial.mega_path_vertex_instances.to_vec();

        // A genuinely new nonresident path uses the existing structural-append
        // lane. Resident first-use deliberately retains its individual lane;
        // this test must not change that separate admission policy.
        let mut added = rows[0].clone();
        added.id = ObjectId::new(1001);
        added.content = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-2.0, 0.125))
                .line_to(Vec2::new(2.0, 0.125)),
        )
        .into();
        append_fixture_row(&mut frame, added);
        let appended = preparer.prepare_incremental(
            &frame,
            &FrameChanges::with_structure(vec![], vec![1], vec![]),
        );
        let appended_range = ranges[31].end..appended.path_vertices.len();
        assert!(!appended_range.is_empty());
        assert_eq!(appended.stats.full_rebuilds, 0);
        assert_eq!(appended.stats.geometry_cache_misses, 1);
        assert_eq!(appended.stats.path_vertices_repacked, appended_range.len());
        assert_eq!(appended.stats.mega_path_count, 2);
        assert_eq!(
            appended.mega_path_vertex_instances.len(),
            appended_range.end
        );
        assert_eq!(
            &appended.mega_path_vertex_instances[..prefix.len()],
            &prefix
        );
        assert_eq!(
            appended.mega_path_instance_dirty_ranges,
            std::slice::from_ref(&appended_range)
        );
        assert!(appended
            .mega_path_indices
            .iter()
            .all(|&index| (index as usize) < appended.mega_path_vertex_instances.len()));

        // An already-allowed full rebuild may trim the logical suffix. Do not
        // claim this shrinks retained Vec capacity or a GPU high-water buffer.
        frame.presences[1] = false;
        let rebuilt = preparer.prepare(&frame);
        assert_eq!(rebuilt.stats.mega_path_count, 1);
        assert_eq!(rebuilt.mega_path_vertex_instances.len(), ranges[0].end);

        frame.presences[1] = true;
        let reentered = preparer.prepare(&frame);
        assert_eq!(reentered.stats.mega_path_count, 2);
        assert_eq!(
            reentered.mega_path_vertex_instances.len(),
            appended_range.end
        );
        let retained = reentered.mega_path_vertex_instances.to_vec();
        frame.objects[0].style.opacity = 0.25;
        let changed = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(changed.stats.full_rebuilds, 0);
        assert_eq!(changed.mega_path_instance_dirty_ranges, &ranges[..1]);
        assert_eq!(
            &changed.mega_path_vertex_instances[appended_range.clone()],
            &retained[appended_range]
        );
    }
}
