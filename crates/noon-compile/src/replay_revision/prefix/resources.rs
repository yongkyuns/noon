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
                        let handle = *self
                            .resources
                            .geometry_handles
                            .get(id)
                            .ok_or(CompiledResourceError::ConflictingGeometryId(*id))?;
                        if let Some(existing) = known
                            .geometry_handles
                            .get(id)
                            .or_else(|| delta.geometry_handles.get(id))
                        {
                            // A bare External(id) row cannot select different
                            // resource versions by authored time. Never reuse an
                            // older reader mapping after the source retargets it.
                            if *existing != handle {
                                return Err(CompiledResourceError::ConflictingGeometryId(*id));
                            }
                            continue;
                        }
                        // An unseen noninitial version may have earlier versions
                        // within this suffix; the row has no versioned handle.
                        // Reject rather than silently paint earlier frames with
                        // the producer's latest external geometry.
                        if handle.version != 0 {
                            return Err(CompiledResourceError::ConflictingGeometryId(*id));
                        }
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
    /// Logical payload bytes for this immutable dependency closure, once per handle.
    /// Uses the resource types' existing retained-byte estimates. Alias maps do
    /// not duplicate payloads; distinct handles sharing an allocation are charged
    /// conservatively. Excludes allocator overhead, spare capacity and GPU state.
    /// Call once for bootstrap and only on new suffix closures thereafter.
    /// Overflow, including a saturated resource estimate, fails closed.
    pub fn prefix_resource_bytes(&self) -> Option<usize> {
        self.images
            .values()
            .map(|resource| resource.retained_bytes())
            .chain(
                self.texts
                    .values()
                    .map(|resource| resource.retained_bytes()),
            )
            .chain(
                self.fonts
                    .values()
                    .map(|resource| resource.retained_bytes()),
            )
            .chain(
                self.geometries
                    .values()
                    .map(|resource| resource.retained_bytes()),
            )
            .try_fold(0_usize, |total, bytes| {
                (bytes != usize::MAX).then_some(())?;
                total.checked_add(bytes)
            })
    }

    /// Record delivered handles using the existing resource projection type.
    pub fn extend_prefix_knowledge(&mut self, delta: &Self) {
        self.merge(delta.clone());
    }
}

#[cfg(test)]
mod resource_bytes_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_revision::SavedRow;
    use crate::CompiledObject;
    use noon_core::{
        GeometryId, GeometryRef, GeometryResource, GeometryResourceArena, ObjectId, Style,
        Transform2D, Vec2, VectorPath,
    };
    use std::sync::Arc;

    fn path(width: f32) -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(width, 0.0))
    }

    fn revision(id: GeometryId) -> CompiledReplayRevision {
        CompiledReplayRevision {
            rows: vec![SavedRow {
                index: 0,
                object: Some(CompiledObject::new(
                    ObjectId::new(0),
                    GeometryRef::External(id),
                    Transform2D::IDENTITY,
                    Style::default(),
                )),
                order: None,
            }],
            channels: Vec::new(),
            tracks: Vec::new(),
            camera_motion_payloads: 0,
            path_motion_commands: 0,
        }
    }

    #[test]
    fn external_geometry_versions_fail_closed_without_historical_handle_provenance() {
        let mut arena = GeometryResourceArena::new();
        let original = arena.insert_path(path(1.0));
        let mut known = CompiledResources::default();
        known.capture_geometry_from_arena(&arena, original).unwrap();
        let replacement = arena
            .replace(
                original.id,
                GeometryResource::VectorPath(Arc::new(path(2.0))),
            )
            .unwrap();
        assert_ne!(replacement, original);

        let mut scene = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                ObjectId::new(0),
                GeometryRef::circle(0.5),
                Transform2D::IDENTITY,
                Style::default(),
            )],
            &[],
        )
        .unwrap();
        scene
            .resources
            .capture_geometry_from_arena(&arena, replacement)
            .unwrap();

        // Known ID, newer source version: never keep the reader's stale bytes.
        assert_eq!(
            scene.prefix_resource_delta(&[revision(original.id)], &known),
            Err(CompiledResourceError::ConflictingGeometryId(original.id))
        );
        // Unknown ID at a later version: no evidence of which version belongs
        // to each prior timestamp, even if the latest version is available.
        assert_eq!(
            scene.prefix_resource_delta(&[revision(original.id)], &CompiledResources::default()),
            Err(CompiledResourceError::ConflictingGeometryId(original.id))
        );

        // Unrelated fresh version-zero resources still deliver only the delta.
        let fresh = arena.insert_path(path(3.0));
        scene
            .resources
            .capture_geometry_from_arena(&arena, fresh)
            .unwrap();
        assert_eq!(
            scene
                .prefix_resource_delta(&[revision(fresh.id)], &known)
                .unwrap()
                .prefix_payload_count(),
            1
        );
    }
}
