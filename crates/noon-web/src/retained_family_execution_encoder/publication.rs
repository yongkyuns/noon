//! One retained renderer-publication encoder for live and prepared worker hosts.
//! This codec is used only at a genuine cross-context renderer boundary. Native
//! and single-context rendering continue to borrow RendererPublication directly.
use super::RetainedFamilyExecutionDeltaEncoder;
use crate::RetainedFamilyExecutionDeltaEnvelope;
#[cfg(any(target_arch = "wasm32", test))]
use crate::RetainedFamilyExecutionDeltaHeader;
use noon_core::Camera2DState;
use noon_runtime::RendererPublication;

impl RetainedFamilyExecutionDeltaEncoder {
    /// Encode only the selected effective publication, never retained future rows.
    /// The caller owns snapshot lifecycle, presentation acknowledgements and the
    /// decision to drain. Resources and sparse painter updates use the same codec
    /// for both source-owned sessions and independent prepared readers.
    pub(crate) fn encode_renderer_publication(
        &mut self,
        publication: &RendererPublication<'_>,
        camera: Camera2DState,
        spatial_inspection: Option<(usize, noon_core::SemanticCamera3D)>,
        viewer_indications: &[noon::ViewerIndication],
        snapshot: bool,
        presentation_changed: bool,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, String> {
        let mut changes = publication.changes().clone();
        let frame = publication.frame();
        if spatial_inspection != self.spatial_inspection {
            // Switching inspection off must republish the authored pose. A
            // removed or recycled non-camera slot is already handled by runtime changes.
            let index = spatial_inspection
                .or(self.spatial_inspection)
                .map(|(index, _)| index);
            if let Some(index) = index.filter(|&index| {
                frame
                    .objects
                    .get(index)
                    .is_some_and(|row| row.camera_projection().is_some())
            }) {
                changes.accumulate(noon_runtime::FrameChanges::objects(vec![index]));
            }
        }
        let indication_changed = self.viewer_indications != viewer_indications;
        if indication_changed {
            let indices = self
                .viewer_indications
                .iter()
                .chain(viewer_indications)
                .filter_map(|effect| publication.object_index(effect.object))
                .collect();
            changes.accumulate(noon_runtime::FrameChanges::objects(indices));
        }
        if changes.is_empty() && (presentation_changed || self.planar_camera != Some(camera)) {
            changes = noon_runtime::FrameChanges::presentation_redraw();
        }
        let publication_context = publication.context();
        let planned = publication.planned_family_frame();
        let plans = publication.family_animation_plans();
        let painter_order = publication.painter_order();

        let mut delta = if snapshot || changes.is_all() {
            let indices = painter_order
                .iter()
                .map(|&index| index as usize)
                .collect::<Vec<_>>();
            let text_handles = indices
                .iter()
                .filter_map(|&index| frame.objects[index].content.text())
                .collect::<Vec<_>>();
            let mut delta = self
                .encode_planned_snapshot_indices_with_context(
                    &planned,
                    plans,
                    camera,
                    publication_context,
                    indices,
                )
                .map_err(|e| e.to_string())?;
            self.attach_resource_additions(
                &mut delta,
                text_handles,
                publication.text_resources(),
                publication.geometry_resources(),
                publication.font_resources(),
                publication.raster_image_resources(),
            )
            .map_err(|error| error.to_string())?;
            delta
        } else if changes.is_structural() || changes.has_painter_order_change() {
            let text_handles = changes
                .object_indices()
                .iter()
                .filter_map(|&index| frame.objects.get(index)?.content.text())
                .collect::<Vec<_>>();
            let Some(mut delta) = self
                .encode_planned_incremental_with_painter_order_and_context(
                    &planned,
                    plans,
                    &changes,
                    camera,
                    publication_context,
                    painter_order,
                )
                .map_err(|e| e.to_string())?
            else {
                return Ok(None);
            };
            self.attach_resource_additions(
                &mut delta,
                text_handles,
                publication.text_resources(),
                publication.geometry_resources(),
                publication.font_resources(),
                publication.raster_image_resources(),
            )
            .map_err(|error| error.to_string())?;
            delta
        } else {
            let text_handles = changes
                .object_indices()
                .iter()
                .filter_map(|&index| frame.objects.get(index)?.content.text())
                .collect::<Vec<_>>();
            let Some(mut delta) = self
                .encode_planned_incremental_with_context(
                    &planned,
                    plans,
                    &changes,
                    camera,
                    publication_context,
                )
                .map_err(|e| e.to_string())?
            else {
                return Ok(None);
            };
            self.attach_resource_additions(
                &mut delta,
                text_handles,
                publication.text_resources(),
                publication.geometry_resources(),
                publication.font_resources(),
                publication.raster_image_resources(),
            )
            .map_err(|error| error.to_string())?;
            delta
        };
        delta.retained.primary_view = publication.primary_view().and_then(|camera| {
            publication.view_color_profile(camera).map(|color_profile| {
                crate::TransportPrimaryView {
                    camera,
                    color_profile,
                }
            })
        });
        // Reuse the normal camera slot. The renderer consumes the same pose
        // for world projection, depth ordering and fixed-orientation objects.
        if let Some((index, camera)) = spatial_inspection {
            let id = frame.objects[index].id;
            if let Some(row) = delta
                .retained
                .objects
                .iter_mut()
                .find(|row| row.object == id)
            {
                let spatial = row
                    .spatial
                    .as_mut()
                    .expect("spatial inspection targets a camera");
                spatial.translation = [camera.position.x, camera.position.y, camera.position.z];
                spatial.rotation_wxyz = camera.orientation.components();
                spatial.camera_projection = Some(camera.projection.into());
            }
        }
        if !viewer_indications.is_empty() {
            for row in &mut delta.retained.objects {
                if let Ok(index) =
                    viewer_indications.binary_search_by_key(&row.object, |effect| effect.object)
                {
                    compose_viewer_indicate(row, viewer_indications[index]);
                }
            }
        }
        debug_assert_eq!(delta.retained.publication_context, publication_context);
        delta
            .replace_transient_presentations(frame, publication.transient_presentations())
            .map_err(|error| error.to_string())?;
        self.planar_camera = Some(camera);
        self.spatial_inspection = spatial_inspection;
        if indication_changed {
            self.viewer_indications = viewer_indications.to_vec();
        }
        Ok(Some(delta))
    }
}

fn compose_viewer_indicate(
    row: &mut crate::RetainedTransportObjectState,
    effect: noon::ViewerIndication,
) {
    let scale_planar = |pose: &mut noon_core::Transform2D| {
        let scaled = pose.scale * effect.scale as f32;
        if scaled.x.is_finite() && scaled.y.is_finite() {
            pose.scale = scaled;
        }
    };
    if let Some(spatial) = row.spatial.as_mut().filter(|spatial| {
        spatial.composition_domain != noon_core::SemanticSpatialCompositionDomain::FixedFrame
    }) {
        let scaled = spatial.scale.map(|value| value * effect.scale);
        if scaled.iter().all(|value| value.is_finite()) {
            spatial.scale = scaled;
        }
    } else {
        scale_planar(&mut row.transform);
        if let Some(pose) = &mut row.render_transform {
            scale_planar(pose);
        }
    }

    let mix = |from, to| from * (1.0 - effect.strength) + to * effect.strength;
    let recolor = |color: noon_core::Color| {
        noon_core::Color::rgba(
            mix(color.red, effect.color.red),
            mix(color.green, effect.color.green),
            mix(color.blue, effect.color.blue),
            color.alpha,
        )
    };
    if row.fill_paint.is_none() {
        row.style.fill = row.style.fill.map(recolor);
    }
    row.style.stroke = row.style.stroke.map(recolor);
}

impl RetainedFamilyExecutionDeltaEnvelope {
    /// The existing bounded metadata header plus unchanged retained body. Used
    /// only on the genuine worker channel, never between in-process Rust layers.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn encode_worker_carrier(&self) -> Result<String, String> {
        let header = RetainedFamilyExecutionDeltaHeader {
            channel: self.retained.channel.clone(),
            session: self.retained.session,
            sequence: self.retained.sequence,
            snapshot: self.retained.snapshot,
            pointer_view: self.pointer_view,
        };
        let mut bytes = serde_json::to_vec(&header).map_err(|error| error.to_string())?;
        if bytes.len() > 512 {
            return Err("worker publication metadata exceeds its bounded header".into());
        }
        bytes.push(b'\n');
        serde_json::to_writer(&mut bytes, self).map_err(|error| error.to_string())?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }
}
