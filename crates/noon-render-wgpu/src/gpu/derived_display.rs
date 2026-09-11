use bytemuck::Pod;

use crate::{
    DerivedDisplayPrimitive, DisplayPainterItem, PreparedDerivedDisplay, PreparedFrame,
    PreparedGeometryObjectOutcome, RenderPrimitive,
};

use super::{empty_instance_buffer, ensure_capacity, DrawStats, GpuRenderer, UploadStats};

#[derive(Debug)]
pub(super) struct DerivedDisplayGpu {
    circle_buffer: wgpu::Buffer,
    rectangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    circle_capacity_bytes: usize,
    rectangle_capacity_bytes: usize,
    line_capacity_bytes: usize,
}

impl DerivedDisplayGpu {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            circle_buffer: empty_instance_buffer(device, "Noon derived circle instances"),
            rectangle_buffer: empty_instance_buffer(device, "Noon derived rectangle instances"),
            line_buffer: empty_instance_buffer(device, "Noon derived line instances"),
            circle_capacity_bytes: 0,
            rectangle_capacity_bytes: 0,
            line_capacity_bytes: 0,
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

        let mut bytes_uploaded = 0;
        bytes_uploaded += upload_all(queue, &self.circle_buffer, &prepared.circles);
        bytes_uploaded += upload_all(queue, &self.rectangle_buffer, &prepared.rectangles);
        bytes_uploaded += upload_all(queue, &self.line_buffer, &prepared.lines);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MixedDrawItem {
    Stable {
        primitive: RenderPrimitive,
        instance_index: usize,
    },
    Derived {
        primitive: DerivedDisplayPrimitive,
        instance_index: usize,
    },
}

fn resolve_mixed_draw_items(
    stable: &PreparedFrame<'_>,
    derived: &PreparedDerivedDisplay,
) -> Vec<MixedDrawItem> {
    let mut resolved = Vec::with_capacity(derived.painter_items.len());
    for item in &derived.painter_items {
        match *item {
            DisplayPainterItem::Stable { object_index } => {
                match stable.observe_object(object_index as usize) {
                    Ok(object) => resolved.push(MixedDrawItem::Stable {
                        primitive: object.primitive,
                        instance_index: object.instance_index,
                    }),
                    Err(PreparedGeometryObjectOutcome::Absent)
                    | Err(PreparedGeometryObjectOutcome::Unsupported(_)) => {}
                }
            }
            DisplayPainterItem::Derived { occurrence_index } => {
                let slot = derived
                    .slot_for_occurrence(occurrence_index)
                    .expect("prepared derived painter item must retain its occurrence slot");
                resolved.push(MixedDrawItem::Derived {
                    primitive: slot.primitive,
                    instance_index: slot.instance_index,
                });
            }
        }
    }
    resolved
}

impl GpuRenderer {
    /// Encode one painter-coherent frame containing stable slots plus identity-free
    /// derived analytic occurrences. Derived buffers must have been uploaded with
    /// `upload_derived` for this exact publication.
    pub fn encode_with_derived(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(encoder, view, stable, clear_color, Some(derived), None)
    }

    /// Profiled variant of [`Self::encode_with_derived`] using the same host-owned
    /// two-entry timestamp query set as ordinary geometry encoding.
    pub fn encode_with_derived_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            stable,
            clear_color,
            Some(derived),
            Some(query_set),
        )
    }

    /// Upload renderer-owned transient analytic instances without changing stable
    /// prepared-frame buffers or slot identity.
    pub fn upload_derived(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedDerivedDisplay,
    ) -> UploadStats {
        self.derived_display.upload(device, queue, prepared)
    }

    pub(super) fn draw_with_derived<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        stable: &PreparedFrame<'_>,
        derived: &PreparedDerivedDisplay,
        single_sample_analytics: bool,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, &self.camera_bind_group, &[]);

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

        for item in resolve_mixed_draw_items(stable, derived) {
            match item {
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Circle,
                    instance_index,
                } => draw_analytic(
                    pass,
                    circle_pipeline,
                    &self.quad_buffer,
                    &self.circle_buffer,
                    instance_index,
                ),
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Rectangle,
                    instance_index,
                } => draw_analytic(
                    pass,
                    rectangle_pipeline,
                    &self.quad_buffer,
                    &self.rectangle_buffer,
                    instance_index,
                ),
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Line,
                    instance_index,
                } => draw_analytic(
                    pass,
                    line_pipeline,
                    &self.quad_buffer,
                    &self.line_buffer,
                    instance_index,
                ),
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Path { batch },
                    instance_index,
                } => {
                    let path = &stable.path_batches[batch];
                    if path.index_range.is_empty() {
                        continue;
                    }
                    pass.set_pipeline(&self.path_pipeline);
                    pass.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, self.path_instance_buffer.slice(..));
                    pass.set_index_buffer(
                        self.path_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    let start = u32::try_from(instance_index)
                        .expect("stable path instance count exceeds wgpu limits");
                    pass.draw_indexed(path.index_range.clone(), 0, start..start + 1);
                }
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::MegaPath { .. },
                    ..
                } => unreachable!("stable object observation resolves retained Path slots"),
                MixedDrawItem::Derived {
                    primitive: DerivedDisplayPrimitive::Circle,
                    instance_index,
                } => draw_analytic(
                    pass,
                    circle_pipeline,
                    &self.quad_buffer,
                    &self.derived_display.circle_buffer,
                    instance_index,
                ),
                MixedDrawItem::Derived {
                    primitive: DerivedDisplayPrimitive::Rectangle,
                    instance_index,
                } => draw_analytic(
                    pass,
                    rectangle_pipeline,
                    &self.quad_buffer,
                    &self.derived_display.rectangle_buffer,
                    instance_index,
                ),
                MixedDrawItem::Derived {
                    primitive: DerivedDisplayPrimitive::Line,
                    instance_index,
                } => draw_analytic(
                    pass,
                    line_pipeline,
                    &self.quad_buffer,
                    &self.derived_display.line_buffer,
                    instance_index,
                ),
            }
            stats.draw_calls += 1;
            stats.instances_drawn += 1;
        }
        stats
    }
}

fn draw_analytic<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a wgpu::RenderPipeline,
    quad_buffer: &'a wgpu::Buffer,
    instance_buffer: &'a wgpu::Buffer,
    instance_index: usize,
) {
    let start = u32::try_from(instance_index).expect("analytic instance count exceeds wgpu limits");
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, quad_buffer.slice(..));
    pass.set_vertex_buffer(1, instance_buffer.slice(..));
    pass.draw(0..6, start..start + 1);
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledObject, CompiledScene};
    use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
    use noon_runtime::{DerivedDisplayObject, DerivedDisplayObjectState, SceneInstance};

    use super::*;
    use crate::{prepare_derived_display, FramePreparer};

    fn state(geometry: GeometryRef) -> DerivedDisplayObjectState {
        DerivedDisplayObjectState {
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
    fn mixed_draw_resolution_preserves_anchor_copy_order_without_new_stable_slots() {
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
        let derived_rows = [
            DerivedDisplayObject::new(0, 7, state(GeometryRef::circle(0.5))),
            DerivedDisplayObject::new(
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
            .with_derived_display_objects(&derived_rows)
            .unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let stable = preparer.prepare(publication.frame());

        assert_eq!(stable.slots.len(), 2);
        assert_eq!(
            resolve_mixed_draw_items(&stable, &derived),
            vec![
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Circle,
                    instance_index: 0,
                },
                MixedDrawItem::Derived {
                    primitive: DerivedDisplayPrimitive::Circle,
                    instance_index: 0,
                },
                MixedDrawItem::Stable {
                    primitive: RenderPrimitive::Rectangle,
                    instance_index: 0,
                },
                MixedDrawItem::Derived {
                    primitive: DerivedDisplayPrimitive::Line,
                    instance_index: 0,
                },
            ]
        );
    }
}
