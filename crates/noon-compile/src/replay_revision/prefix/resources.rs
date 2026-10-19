//! Existing immutable resource closures, filtered to not-yet-delivered handles.
use super::{CompiledReplayRevision, CompiledScene};
use crate::{CompiledResourceError, CompiledResources};
use noon_core::{FontResourceKey, GeometryRef, ObjectContentRef};

impl CompiledScene {
    /// Capture dependency closures for the new forward suffix only. Payloads
    /// remain shared Arcs; no pixel/font/path byte copies or full arena scans.
    pub fn prefix_resource_delta(
        &self,
        revisions: &[CompiledReplayRevision],
        known: &CompiledResources,
    ) -> Result<CompiledResources, CompiledResourceError> {
        let mut delta = CompiledResources::default();
        for revision in revisions {
            for row in &revision.rows {
                let Some(object) = &row.object else {
                    continue;
                };
                match &object.content {
                    ObjectContentRef::Text(handle) => {
                        if known.texts.contains_key(handle) || delta.texts.contains_key(handle) {
                            continue;
                        }
                        let resource = self
                            .resources
                            .texts
                            .get(handle)
                            .ok_or(CompiledResourceError::MissingText(*handle))?
                            .clone();
                        delta.capture_text_payload(
                            *handle,
                            resource,
                            |face| {
                                let key = FontResourceKey::from_face(face);
                                let handle =
                                    *self.resources.font_handles.get(&key).ok_or_else(|| {
                                        CompiledResourceError::MissingFont(key.clone())
                                    })?;
                                let font = self
                                    .resources
                                    .fonts
                                    .get(&handle)
                                    .ok_or(CompiledResourceError::MissingFont(key))?
                                    .clone();
                                Ok((handle, font))
                            },
                            |handle| {
                                self.resources
                                    .geometries
                                    .get(&handle)
                                    .cloned()
                                    .ok_or(CompiledResourceError::MissingGeometry(handle))
                            },
                        )?;
                    }
                    ObjectContentRef::Image(image) => {
                        let handle = image.resource();
                        if known.images.contains_key(&handle) || delta.images.contains_key(&handle)
                        {
                            continue;
                        }
                        let resource = self
                            .resources
                            .images
                            .get(&handle)
                            .ok_or(CompiledResourceError::MissingImage(handle))?
                            .clone();
                        delta.images.insert(handle, resource);
                    }
                    ObjectContentRef::Geometry(GeometryRef::External(id)) => {
                        if known.geometry_handles.contains_key(id)
                            || delta.geometry_handles.contains_key(id)
                        {
                            continue;
                        }
                        let handle = *self
                            .resources
                            .geometry_handles
                            .get(id)
                            .ok_or(CompiledResourceError::ConflictingGeometryId(*id))?;
                        let resource = self
                            .resources
                            .geometries
                            .get(&handle)
                            .ok_or(CompiledResourceError::MissingGeometry(handle))?
                            .clone();
                        delta.geometry_handles.insert(*id, handle);
                        delta.geometries.insert(handle, resource);
                    }
                    ObjectContentRef::Geometry(_) => {}
                }
            }
        }
        known.preflight_merge(&delta)?;
        delta
            .fonts
            .retain(|handle, _| !known.fonts.contains_key(handle));
        delta
            .font_handles
            .retain(|face, _| !known.font_handles.contains_key(face));
        delta
            .geometries
            .retain(|handle, _| !known.geometries.contains_key(handle));
        delta
            .geometry_handles
            .retain(|id, _| !known.geometry_handles.contains_key(id));
        Ok(delta)
    }
}
impl CompiledResources {
    /// Count immutable dependency entries, not bytes or process/VRAM usage.
    pub fn prefix_payload_count(&self) -> usize {
        self.images.len() + self.texts.len() + self.fonts.len() + self.geometries.len()
    }
    /// Record delivered handles using the existing resource projection type.
    pub fn extend_prefix_knowledge(&mut self, delta: &Self) {
        self.merge(delta.clone());
    }
}
