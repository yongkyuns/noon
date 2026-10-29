//! One retained renderer-publication encoder for live and prepared worker hosts.
//! This codec is used only at a genuine cross-context renderer boundary. Native
//! and single-context rendering continue to borrow RendererPublication directly.
use super::RetainedFamilyExecutionDeltaEncoder;
use crate::RetainedFamilyExecutionDeltaEnvelope;
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
        snapshot: bool,
        presentation_changed: bool,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, String> {
        let mut changes = publication.changes().clone();
        if changes.is_empty() && presentation_changed {
            changes = noon_runtime::FrameChanges::presentation_redraw();
        }
        let frame = publication.frame();
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
        debug_assert_eq!(delta.retained.publication_context, publication_context);
        delta
            .replace_transient_presentations(frame, publication.transient_presentations())
            .map_err(|error| error.to_string())?;
        Ok(Some(delta))
    }
}

impl RetainedFamilyExecutionDeltaEnvelope {
    /// The existing bounded metadata header plus unchanged retained body. Used
    /// only on the genuine worker channel, never between in-process Rust layers.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn encode_worker_carrier(&self) -> Result<String, String> {
        #[derive(serde::Serialize)]
        struct Header<'a> {
            channel: &'a str,
            session: u32,
            sequence: u64,
            snapshot: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            pointer_view: Option<crate::worker_pointer_presentation::PointerPresentationView>,
        }
        let header = Header {
            channel: &self.retained.channel,
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
