use std::collections::{hash_map::Entry, BTreeMap, HashMap};

use bytemuck::Pod;

use crate::{
    DerivedDisplayPrimitive, DerivedPathGeometrySource, DisplayPainterItem, PreparedDerivedDisplay,
    PreparedDerivedDisplaySlot, PreparedFrame, PreparedGeometryObjectOutcome, RenderPrimitive,
};

use super::{
    empty_buffer, empty_instance_buffer, ensure_capacity, ensure_capacity_with_usage, DrawStats,
    GpuRenderer, ResolvedOrderedBatch, UploadStats,
};

#[derive(Debug)]
pub(super) struct DerivedDisplayGpu {
    circle_buffer: wgpu::Buffer,
    rectangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    path_vertex_buffer: wgpu::Buffer,
    path_index_buffer: wgpu::Buffer,
    path_instance_buffer: wgpu::Buffer,
    circle_capacity_bytes: usize,
    rectangle_capacity_bytes: usize,
    line_capacity_bytes: usize,
    path_vertex_capacity_bytes: usize,
    path_index_capacity_bytes: usize,
    path_instance_capacity_bytes: usize,
}

impl DerivedDisplayGpu {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            circle_buffer: empty_instance_buffer(device, "Noon derived circle instances"),
            rectangle_buffer: empty_instance_buffer(device, "Noon derived rectangle instances"),
            line_buffer: empty_instance_buffer(device, "Noon derived line instances"),
            path_vertex_buffer: empty_buffer(
                device,
                "Noon transient path vertices",
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ),
            path_index_buffer: empty_buffer(
                device,
                "Noon transient path indices",
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ),
            path_instance_buffer: empty_instance_buffer(device, "Noon transient path instances"),
            circle_capacity_bytes: 0,
            rectangle_capacity_bytes: 0,
            line_capacity_bytes: 0,
            path_vertex_capacity_bytes: 0,
            path_index_capacity_bytes: 0,
            path_instance_capacity_bytes: 0,
        }
    }

    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedDerivedDisplay,
    ) -> UploadStats {
        let circle_bytes = std::mem::size_of_val(prepared.circles.as_slice());
        let rectangle_bytes = std::mem::size_of_val(prepared.rectangles.as_slice());
        let line_bytes = std::mem::size_of_val(prepared.lines.as_slice());
        let path_vertex_bytes = std::mem::size_of_val(prepared.path_vertices.as_slice());
        let path_index_bytes = std::mem::size_of_val(prepared.path_indices.as_slice());
        let path_instance_bytes = std::mem::size_of_val(prepared.paths.as_slice());
        let mut buffer_reallocations = 0;

        buffer_reallocations += usize::from(ensure_capacity(
            device,
            &mut self.circle_buffer,
            &mut self.circle_capacity_bytes,
            circle_bytes,
            "Noon derived circle instances",
        ));
        buffer_reallocations += usize::from(ensure_capacity(
            device,
            &mut self.rectangle_buffer,
            &mut self.rectangle_capacity_bytes,
            rectangle_bytes,
            "Noon derived rectangle instances",
        ));
        buffer_reallocations += usize::from(ensure_capacity(
            device,
            &mut self.line_buffer,
            &mut self.line_capacity_bytes,
            line_bytes,
            "Noon derived line instances",
        ));
        buffer_reallocations += usize::from(ensure_capacity_with_usage(
            device,
            &mut self.path_vertex_buffer,
            &mut self.path_vertex_capacity_bytes,
            path_vertex_bytes,
            "Noon transient path vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        ));
        buffer_reallocations += usize::from(ensure_capacity_with_usage(
            device,
            &mut self.path_index_buffer,
            &mut self.path_index_capacity_bytes,
            path_index_bytes,
            "Noon transient path indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        ));
        buffer_reallocations += usize::from(ensure_capacity(
            device,
            &mut self.path_instance_buffer,
            &mut self.path_instance_capacity_bytes,
            path_instance_bytes,
            "Noon transient path instances",
        ));

        let mut bytes_uploaded = 0;
        bytes_uploaded += upload_all(queue, &self.circle_buffer, &prepared.circles);
        bytes_uploaded += upload_all(queue, &self.rectangle_buffer, &prepared.rectangles);
        bytes_uploaded += upload_all(queue, &self.line_buffer, &prepared.lines);
        bytes_uploaded += upload_all(queue, &self.path_vertex_buffer, &prepared.path_vertices);
        bytes_uploaded += upload_all(queue, &self.path_index_buffer, &prepared.path_indices);
        bytes_uploaded += upload_all(queue, &self.path_instance_buffer, &prepared.paths);
        UploadStats {
            bytes_uploaded,
            buffer_reallocations,
        }
    }
}

fn upload_all<T: Pod>(queue: &wgpu::Queue, buffer: &wgpu::Buffer, values: &[T]) -> usize {
    if values.is_empty() {
        return 0;
    }
    let bytes = bytemuck::cast_slice(values);
    queue.write_buffer(buffer, 0, bytes);
    bytes.len()
}

type ResolvedTransientAnchors =
    HashMap<RenderPrimitive, BTreeMap<usize, Vec<PreparedDerivedDisplaySlot>>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TransientResolutionStats {
    slot_index_entries: usize,
    slot_lookups: usize,
    anchor_lookups: usize,
    occurrences_resolved: usize,
}

/// Resolve each publication-local occurrence exactly once. Stable painter order is
/// deliberately not copied here: the retained ordered-batch stream remains the sole
/// authority and transient rows are spliced after their stable anchors during draw.
fn resolve_transient_anchors(
    stable: &PreparedFrame<'_>,
    derived: &PreparedDerivedDisplay,
) -> (ResolvedTransientAnchors, TransientResolutionStats) {
    let mut stats = TransientResolutionStats::default();
    let mut slots_by_occurrence = HashMap::with_capacity(derived.slots.len());
    for &slot in &derived.slots {
        slots_by_occurrence.insert(slot.occurrence_index, slot);
    }
    stats.slot_index_entries = slots_by_occurrence.len();

    let mut anchor_cache = HashMap::<u32, Option<(RenderPrimitive, usize)>>::new();
    let mut resolved = ResolvedTransientAnchors::new();
    for item in &derived.painter_items {
        let DisplayPainterItem::Derived { occurrence_index } = *item else {
            // Stable entries were emitted by the pre-locality implementation. The
            // retained prepared frame now draws those rows directly.
            continue;
        };
        stats.slot_lookups += 1;
        let slot = *slots_by_occurrence
            .get(&occurrence_index)
            .expect("prepared transient painter item must retain its occurrence slot");
        let anchor = match anchor_cache.entry(slot.anchor_object_index) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                stats.anchor_lookups += 1;
                let observation = match stable.observe_object(slot.anchor_object_index as usize) {
                    Ok(object) => Some((object.primitive, object.instance_index)),
                    Err(PreparedGeometryObjectOutcome::Absent)
                    | Err(PreparedGeometryObjectOutcome::Unsupported(_)) => None,
                };
                *entry.insert(observation)
            }
        };
        let Some((primitive, instance_index)) = anchor else {
            continue;
        };
        resolved
            .entry(primitive)
            .or_default()
            .entry(instance_index)
            .or_default()
            .push(slot);
        stats.occurrences_resolved += 1;
    }
    (resolved, stats)
}

impl GpuRenderer {
    /// Encode one painter-coherent frame containing stable slots plus identity-free
    /// transient analytic presentation occurrences. Transient buffers must have
    /// been uploaded for this exact publication.
    pub fn encode_with_transient_presentations(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        presentations: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            stable,
            clear_color,
            Some(presentations),
            None,
        )
    }

    /// Profiled variant of [`Self::encode_with_transient_presentations`] using the
    /// same host-owned two-entry timestamp query set as ordinary geometry encoding.
    pub fn encode_with_transient_presentations_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        presentations: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            stable,
            clear_color,
            Some(presentations),
            Some(query_set),
        )
    }

    /// Upload renderer-owned transient presentation instances without changing
    /// stable prepared-frame buffers or slot identity.
    pub fn upload_transient_presentations(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedDerivedDisplay,
    ) -> UploadStats {
        self.derived_display.upload(device, queue, prepared)
    }

    /// Migration entry point for the currently stacked B3 derived-display path.
    pub fn encode_with_derived(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_with_transient_presentations(encoder, view, stable, derived, clear_color)
    }

    /// Migration profiled entry point for the currently stacked B3 path.
    pub fn encode_with_derived_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_with_transient_presentations_profiled(
            encoder,
            view,
            stable,
            derived,
            clear_color,
            query_set,
        )
    }

    /// Migration upload entry point for the currently stacked B3 path.
    pub fn upload_derived(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedDerivedDisplay,
    ) -> UploadStats {
        self.upload_transient_presentations(device, queue, prepared)
    }

    pub(super) fn draw_with_derived<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        single_sample_analytics: bool,
    ) -> DrawStats {
        self.draw_with_derived_camera(
            pass,
            stable,
            derived,
            single_sample_analytics,
            &self.camera_bind_group,
        )
    }

    pub(super) fn draw_with_derived_camera<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        single_sample_analytics: bool,
        camera_bind_group: &'a wgpu::BindGroup,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, camera_bind_group, &[]);

        let (anchors, resolution) = resolve_transient_anchors(stable, derived);
        assert_eq!(
            resolution.occurrences_resolved, resolution.slot_lookups,
            "transient presentation anchor is absent from the prepared stable submission"
        );

        let mut inserted = 0usize;
        let mut pending = None::<ResolvedOrderedBatch>;
        for resolved in stable.ordered_render_batches() {
            let next = ResolvedOrderedBatch {
                batch: resolved.batch.clone(),
                mega: resolved.mega_path_batch.cloned(),
            };
            if pending.as_mut().is_some_and(|current| current.merge(&next)) {
                continue;
            }
            if let Some(current) = pending.replace(next) {
                let (drawn, inserted_now) = self.draw_stable_batch_with_transients(
                    pass,
                    stable,
                    derived,
                    &anchors,
                    &current,
                    single_sample_analytics,
                );
                stats.draw_calls += drawn.draw_calls;
                stats.instances_drawn += drawn.instances_drawn;
                inserted += inserted_now;
            }
        }
        if let Some(current) = pending {
            let (drawn, inserted_now) = self.draw_stable_batch_with_transients(
                pass,
                stable,
                derived,
                &anchors,
                &current,
                single_sample_analytics,
            );
            stats.draw_calls += drawn.draw_calls;
            stats.instances_drawn += drawn.instances_drawn;
            inserted += inserted_now;
        }

        assert_eq!(
            inserted, resolution.occurrences_resolved,
            "transient presentation anchor could not be located in retained painter batches"
        );
        stats
    }

    fn draw_stable_batch_with_transients<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        anchors: &ResolvedTransientAnchors,
        resolved: &ResolvedOrderedBatch,
        single_sample_analytics: bool,
    ) -> (DrawStats, usize) {
        let Some(insertions) = anchors.get(&resolved.batch.primitive) else {
            return (
                self.draw_resolved_ordered_batch(pass, stable, resolved, single_sample_analytics),
                0,
            );
        };
        let range_start = resolved.batch.instance_range.start as usize;
        let range_end = resolved.batch.instance_range.end as usize;
        let mut cursor = resolved.batch.instance_range.start;
        let mut stats = DrawStats::default();
        let mut inserted = 0usize;

        for (&anchor_instance, slots) in insertions.range(range_start..range_end) {
            let anchor = u32::try_from(anchor_instance)
                .expect("stable anchor instance count exceeds wgpu limits");
            let after_anchor = anchor
                .checked_add(1)
                .expect("stable anchor instance count exceeds wgpu limits");
            if cursor < after_anchor {
                let mut segment = resolved.clone();
                segment.batch.instance_range = cursor..after_anchor;
                let drawn = self.draw_resolved_ordered_batch(
                    pass,
                    stable,
                    &segment,
                    single_sample_analytics,
                );
                stats.draw_calls += drawn.draw_calls;
                stats.instances_drawn += drawn.instances_drawn;
            }
            for &slot in slots {
                let drawn = self.draw_transient_slot(pass, derived, slot, single_sample_analytics);
                stats.draw_calls += drawn.draw_calls;
                stats.instances_drawn += drawn.instances_drawn;
                inserted += 1;
            }
            cursor = after_anchor;
        }

        if cursor < resolved.batch.instance_range.end {
            let mut segment = resolved.clone();
            segment.batch.instance_range = cursor..resolved.batch.instance_range.end;
            let drawn =
                self.draw_resolved_ordered_batch(pass, stable, &segment, single_sample_analytics);
            stats.draw_calls += drawn.draw_calls;
            stats.instances_drawn += drawn.instances_drawn;
        }
        (stats, inserted)
    }

    fn draw_transient_slot<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        derived: &PreparedDerivedDisplay,
        slot: PreparedDerivedDisplaySlot,
        single_sample_analytics: bool,
    ) -> DrawStats {
        let circle_pipeline = if single_sample_analytics {
            &self.circle_pipeline_single_sample
        } else {
            &self.circle_pipeline
        };
        let rectangle_pipeline = if single_sample_analytics {
            &self.rectangle_pipeline_single_sample
        } else {
            &self.rectangle_pipeline
        };
        let line_pipeline = if single_sample_analytics {
            &self.line_pipeline_single_sample
        } else {
            &self.line_pipeline
        };
        match slot.primitive {
            DerivedDisplayPrimitive::Circle => draw_analytic(
                pass,
                circle_pipeline,
                &self.quad_buffer,
                &self.derived_display.circle_buffer,
                slot.instance_index,
            ),
            DerivedDisplayPrimitive::Rectangle => draw_analytic(
                pass,
                rectangle_pipeline,
                &self.quad_buffer,
                &self.derived_display.rectangle_buffer,
                slot.instance_index,
            ),
            DerivedDisplayPrimitive::Line => draw_analytic(
                pass,
                line_pipeline,
                &self.quad_buffer,
                &self.derived_display.line_buffer,
                slot.instance_index,
            ),
            DerivedDisplayPrimitive::Path { batch, source } => {
                let path = &derived.path_batches[batch];
                if path.index_range.is_empty() {
                    return DrawStats::default();
                }
                let (vertex_buffer, index_buffer) = match source {
                    DerivedPathGeometrySource::Transient => (
                        &self.derived_display.path_vertex_buffer,
                        &self.derived_display.path_index_buffer,
                    ),
                    DerivedPathGeometrySource::Retained => {
                        (&self.path_vertex_buffer, &self.path_index_buffer)
                    }
                };
                // Derived paths retain the full `PathVertex` format. The compact
                // pipeline is reserved for the stable ordinary/mega streams, so
                // binding it here would decode this buffer at the wrong stride.
                pass.set_pipeline(&self.full_path_pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.derived_display.path_instance_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                let start = u32::try_from(slot.instance_index)
                    .expect("transient path instance count exceeds wgpu limits");
                pass.draw_indexed(path.index_range.clone(), 0, start..start + 1);
                DrawStats {
                    draw_calls: 1,
                    instances_drawn: 1,
                }
            }
        };
        DrawStats {
            draw_calls: 1,
            instances_drawn: 1,
        }
    }
}

fn draw_analytic<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a wgpu::RenderPipeline,
    quad_buffer: &'a wgpu::Buffer,
    instance_buffer: &'a wgpu::Buffer,
    instance_index: usize,
) -> DrawStats {
    let start = u32::try_from(instance_index).expect("analytic instance count exceeds wgpu limits");
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, quad_buffer.slice(..));
    pass.set_vertex_buffer(1, instance_buffer.slice(..));
    pass.draw(0..6, start..start + 1);
    DrawStats {
        draw_calls: 1,
        instances_drawn: 1,
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledObject, CompiledScene};
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
    use noon_runtime::{
        SceneInstance, TransientPresentationOccurrence, TransientPresentationState,
    };

    use super::*;
    use crate::{
        prepare_derived_display, prepare_derived_display_visible_cached, FramePreparer,
        PathMeshPreload,
    };

    fn state(geometry: GeometryRef) -> TransientPresentationState {
        TransientPresentationState {
            z_index: 0.0,
            content: noon_core::ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        }
    }

    #[test]
    fn transient_path_upload_and_encode_uses_identity_free_path_buffers() {
        const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
        let objects = vec![CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        )];
        let compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
        let mut runtime = SceneInstance::new(compiled);
        let path = noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(-0.5, -0.5))
            .line_to(noon_core::Vec2::new(0.5, -0.5))
            .line_to(noon_core::Vec2::new(0.0, 0.5))
            .close();
        let mut path_state = state(GeometryRef::path(path));
        path_state.style.fill = Some(noon_core::Color::WHITE);
        path_state.style.stroke = None;
        let presentations = [TransientPresentationOccurrence::new(0, 7, path_state)];
        let publication = runtime
            .take_renderer_publication()
            .with_transient_presentations(&presentations)
            .unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let stable = preparer.prepare(publication.frame());

        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, FORMAT);
        renderer.set_viewport(&device, &queue, 32, 32);
        renderer.upload(&device, &queue, &stable);
        let uploaded = renderer.upload_transient_presentations(&device, &queue, &derived);
        assert!(uploaded.bytes_uploaded > 0);
        assert!(uploaded.buffer_reallocations >= 3);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon transient path test target"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode_with_transient_presentations(
            &mut encoder,
            &view,
            &stable,
            &derived,
            wgpu::Color::BLACK,
        );
        queue.submit(Some(encoder.finish()));

        assert_eq!(draw.draw_calls, 2);
        assert_eq!(draw.instances_drawn, 2);
    }

    #[test]
    fn resident_transient_path_uploads_instance_only() {
        const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
        let path = noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(-0.5, -0.5))
            .line_to(noon_core::Vec2::new(0.5, -0.5))
            .line_to(noon_core::Vec2::new(0.0, 0.5))
            .close();
        let geometry = GeometryRef::path(path);
        let style = Style::default();
        let objects = vec![CompiledObject::new(
            ObjectId::new(1),
            geometry.clone(),
            Transform2D::IDENTITY,
            style,
        )];
        let compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
        let mut runtime = SceneInstance::new(compiled);
        let presentations = [TransientPresentationOccurrence::new(
            0,
            7,
            state(geometry.clone()),
        )];
        let publication = runtime
            .take_renderer_publication()
            .with_transient_presentations(&presentations)
            .unwrap();
        let mut preparer = FramePreparer::for_individual_path_draws();
        preparer
            .preload_paths(&[PathMeshPreload {
                geometry: &geometry,
                style,
                transform: Transform2D::IDENTITY,
            }])
            .unwrap();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let derived =
            prepare_derived_display_visible_cached(&publication, &[0], &mut preparer).unwrap();
        assert!(derived.path_vertices.is_empty());
        assert!(derived.path_indices.is_empty());
        assert_eq!(derived.stats.resident_path_reuses, 1);
        let stable = preparer.prepare(publication.frame());

        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, FORMAT);
        renderer.set_viewport(&device, &queue, 32, 32);
        renderer.upload(&device, &queue, &stable);
        let uploaded = renderer.upload_transient_presentations(&device, &queue, &derived);
        assert_eq!(
            uploaded.bytes_uploaded,
            std::mem::size_of::<crate::PathInstance>()
        );
        assert_eq!(uploaded.buffer_reallocations, 1);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon resident transient path test target"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let draw = renderer.encode_with_transient_presentations(
            &mut encoder,
            &view,
            &stable,
            &derived,
            wgpu::Color::BLACK,
        );
        queue.submit(Some(encoder.finish()));
        assert_eq!(draw.draw_calls, 2);
        assert_eq!(draw.instances_drawn, 2);
    }

    #[test]
    fn transient_resolution_is_linear_in_occurrences_and_unique_anchors() {
        const COUNT: u32 = 4_096;
        let objects = vec![CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )];
        let compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
        let mut runtime = SceneInstance::new(compiled);
        let presentations = (0..COUNT)
            .map(|occurrence| {
                TransientPresentationOccurrence::new(
                    0,
                    occurrence,
                    state(GeometryRef::circle(0.25)),
                )
            })
            .collect::<Vec<_>>();
        let publication = runtime
            .take_renderer_publication()
            .with_transient_presentations(&presentations)
            .unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let stable = preparer.prepare(publication.frame());

        let (resolved, stats) = resolve_transient_anchors(&stable, &derived);
        assert_eq!(stats.slot_index_entries, COUNT as usize);
        assert_eq!(stats.slot_lookups, COUNT as usize);
        assert_eq!(stats.anchor_lookups, 1);
        assert_eq!(stats.occurrences_resolved, COUNT as usize);
        assert_eq!(
            resolved
                .get(&RenderPrimitive::Circle)
                .and_then(|by_instance| by_instance.get(&0))
                .map(Vec::len),
            Some(COUNT as usize)
        );
    }

    #[test]
    fn sparse_resolution_preserves_anchor_copy_order_without_new_stable_slots() {
        let objects = vec![
            CompiledObject::new(
                ObjectId::new(1),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            CompiledObject::new(
                ObjectId::new(2),
                GeometryRef::rectangle(1.0, 1.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
        ];
        let compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
        let mut runtime = SceneInstance::new(compiled);
        let presentations = [
            TransientPresentationOccurrence::new(0, 7, state(GeometryRef::circle(0.5))),
            TransientPresentationOccurrence::new(
                1,
                8,
                state(GeometryRef::line(
                    noon_core::Vec2::ZERO,
                    noon_core::Vec2::ONE,
                )),
            ),
        ];
        let publication = runtime
            .take_renderer_publication()
            .with_transient_presentations(&presentations)
            .unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let stable = preparer.prepare(publication.frame());

        assert_eq!(stable.slots.len(), 2);
        let (resolved, stats) = resolve_transient_anchors(&stable, &derived);
        assert_eq!(stats.slot_lookups, 2);
        assert_eq!(stats.anchor_lookups, 2);
        assert_eq!(
            resolved
                .get(&RenderPrimitive::Circle)
                .and_then(|by_instance| by_instance.get(&0))
                .map(|slots| slots[0].occurrence_index),
            Some(7)
        );
        assert_eq!(
            resolved
                .get(&RenderPrimitive::Rectangle)
                .and_then(|by_instance| by_instance.get(&0))
                .map(|slots| slots[0].occurrence_index),
            Some(8)
        );
    }
}
