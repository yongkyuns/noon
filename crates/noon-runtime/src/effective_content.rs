//! Runtime-owned content leases. The compiled object's authored content remains
//! the release baseline; only the effective frame row changes at publication.

use std::sync::Arc;

use noon_compile::{
    CompilePatchError, CompiledResourceError, CompiledResources, ExecutionMutationTransaction,
    ExecutionPatch,
};
use noon_core::{
    GeometryId, GeometryRef, GeometryResource, GeometryResourceArena, GeometryResourceHandle,
    GeometryResourceLookup, ObjectContentRef, ObjectId, Property, PublicationContext,
    RasterImageResource, RasterImageResourceArena, RasterImageResourceHandle, Rect,
    SemanticImageContent,
};

use crate::{RuntimeIdentity, SceneInstance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveContentLease {
    pub(crate) runtime: RuntimeIdentity,
    object: ObjectId,
    sequence: u64,
}

impl EffectiveContentLease {
    pub const fn object(self) -> ObjectId {
        self.object
    }
}

#[derive(Clone, Debug)]
pub struct PreparedEffectiveContentReplacement {
    lease: EffectiveContentLease,
    expected: PublicationContext,
    expected_version: Option<u64>,
    object_index: usize,
    content: ObjectContentRef,
    text_bounds: Option<Rect>,
    image_resource: Option<Arc<RasterImageResource>>,
    geometry_resource: Option<(GeometryId, GeometryResourceHandle, GeometryResource)>,
}

#[derive(Clone, Debug)]
pub(crate) struct EffectiveContentDriver {
    pub(crate) lease: EffectiveContentLease,
    version: u64,
    pub(crate) content: ObjectContentRef,
    pub(crate) text_bounds: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EffectiveContentError {
    Invalid(CompilePatchError),
    Resource(CompiledResourceError),
    UnknownObject(ObjectId),
    DriverConflict(ObjectId),
    UnsupportedExternalGeometry(ObjectId),
    GeometryResourceConflict(GeometryId),
    ActiveRenderOverride(ObjectId),
    GeometryDriverConflict(ObjectId),
    ReplaySealed,
    ForeignRuntime,
    StalePublication {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    StaleLease(ObjectId),
    SequenceExhausted,
    FrameEpochExhausted,
}

impl std::fmt::Display for EffectiveContentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "unknown effective content object {}",
                object.get()
            ),
            Self::DriverConflict(object) => write!(
                formatter,
                "object {} has another effective content driver",
                object.get()
            ),
            Self::UnsupportedExternalGeometry(object) => write!(
                formatter,
                "object {} requires an owned inline or versioned content resource",
                object.get()
            ),
            Self::GeometryResourceConflict(id) => write!(
                formatter,
                "external geometry ID {} is already owned by another resource",
                id.get()
            ),
            Self::ActiveRenderOverride(object) => write!(
                formatter,
                "object {} has an active render geometry override",
                object.get()
            ),
            Self::GeometryDriverConflict(object) => write!(
                formatter,
                "object {} has a geometry or family animation driver",
                object.get()
            ),
            Self::ReplaySealed => {
                formatter.write_str("sealed replay cannot acquire effective content")
            }
            Self::ForeignRuntime => {
                formatter.write_str("effective content lease belongs to another runtime")
            }
            Self::StalePublication { expected, actual } => write!(
                formatter,
                "effective content expected {expected:?}, found {actual:?}"
            ),
            Self::StaleLease(object) => write!(
                formatter,
                "effective content lease for object {} is stale",
                object.get()
            ),
            Self::SequenceExhausted => {
                formatter.write_str("effective content lease sequence exhausted")
            }
            Self::FrameEpochExhausted => {
                formatter.write_str("effective content frame epoch exhausted")
            }
        }
    }
}

impl std::error::Error for EffectiveContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            Self::Resource(error) => Some(error),
            _ => None,
        }
    }
}

impl SceneInstance {
    /// Observe the lease bound to this runtime incarnation. A cloned runtime
    /// has fresh identity, so a caller deliberately adopting that clone must
    /// obtain its lease rather than reuse a token from the source runtime.
    pub fn effective_content_lease(&self, object: ObjectId) -> Option<EffectiveContentLease> {
        let index = self.compiled.object_index(object)? as usize;
        self.effective_content_drivers
            .get(&index)
            .map(|driver| driver.lease)
    }

    /// Prepare one content version. Initial acquisition pins the exact effective
    /// publication; a replacement of an existing lease names that lease and
    /// may commit after unrelated frame advances. This only versions the
    /// supplied content value: a producer whose value depends on authored time
    /// or other effective inputs must separately reject obsolete source samples.
    /// A new producer passes `None` and acquires ownership only at commit.
    pub fn prepare_effective_content_replacement(
        &self,
        object: ObjectId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
        lease: Option<EffectiveContentLease>,
    ) -> Result<PreparedEffectiveContentReplacement, EffectiveContentError> {
        self.prepare_effective_content_replacement_with_resources(
            object,
            content,
            text_bounds,
            lease,
            None,
            None,
        )
    }

    /// Prepare a producer-owned external geometry. GeometryRef stores a bare ID,
    /// so admission rejects IDs already visible from another resource owner.
    pub fn prepare_effective_geometry_replacement(
        &self,
        object: ObjectId,
        handle: GeometryResourceHandle,
        source: &GeometryResourceArena,
        lease: Option<EffectiveContentLease>,
    ) -> Result<PreparedEffectiveContentReplacement, EffectiveContentError> {
        let mut additions = CompiledResources::default();
        additions
            .capture_geometry_from_arena(source, handle)
            .map_err(EffectiveContentError::Resource)?;
        if self.geometry_resource_conflicts(object, handle, lease) {
            return Err(EffectiveContentError::GeometryResourceConflict(handle.id));
        }
        let resource = source
            .get(handle)
            .cloned()
            .ok_or(EffectiveContentError::Resource(
                CompiledResourceError::MissingGeometry(handle),
            ))?;
        let mut prepared = self.prepare_effective_content_replacement_with_resources(
            object,
            ObjectContentRef::Geometry(GeometryRef::External(handle.id)),
            None,
            lease,
            Some(additions),
            None,
        )?;
        prepared.geometry_resource = Some((handle.id, handle, resource));
        Ok(prepared)
    }

    /// Prepare a newly produced image against its producer-owned resource arena.
    /// The pixels are borrowed into an immutable prepared result; neither the
    /// runtime lookup nor the effective row changes until commit succeeds.
    pub fn prepare_effective_image_replacement(
        &self,
        object: ObjectId,
        content: SemanticImageContent,
        source: &RasterImageResourceArena,
        lease: Option<EffectiveContentLease>,
    ) -> Result<PreparedEffectiveContentReplacement, EffectiveContentError> {
        let mut additions = CompiledResources::default();
        let lowered = additions
            .capture_image_from_arena(source, content)
            .map_err(EffectiveContentError::Resource)?;
        let image_resource =
            source
                .get_shared(content.resource())
                .ok_or(EffectiveContentError::Resource(
                    CompiledResourceError::MissingImage(content.resource()),
                ))?;
        self.prepare_effective_content_replacement_with_resources(
            object,
            ObjectContentRef::Image(lowered),
            None,
            lease,
            Some(additions),
            Some(image_resource),
        )
    }

    fn prepare_effective_content_replacement_with_resources(
        &self,
        object: ObjectId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
        lease: Option<EffectiveContentLease>,
        resource_additions: Option<CompiledResources>,
        image_resource: Option<Arc<RasterImageResource>>,
    ) -> Result<PreparedEffectiveContentReplacement, EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        if lease.is_some_and(|lease| lease.runtime != self.identity) {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        let index = self
            .compiled
            .object_index(object)
            .map(|index| index as usize)
            .filter(|&index| self.object_slot_is_live(index))
            .ok_or(EffectiveContentError::UnknownObject(object))?;
        let existing = self.effective_content_drivers.get(&index);
        let (lease, expected_version) = match (existing, lease) {
            (None, None) => {
                let sequence = self.next_effective_content_sequence;
                sequence
                    .checked_add(1)
                    .ok_or(EffectiveContentError::SequenceExhausted)?;
                (
                    EffectiveContentLease {
                        runtime: self.identity,
                        object,
                        sequence,
                    },
                    None,
                )
            }
            (Some(driver), Some(lease)) if driver.lease == lease => {
                driver
                    .version
                    .checked_add(1)
                    .ok_or(EffectiveContentError::SequenceExhausted)?;
                (lease, Some(driver.version))
            }
            (Some(_), None) => return Err(EffectiveContentError::DriverConflict(object)),
            _ => return Err(EffectiveContentError::StaleLease(object)),
        };
        if expected_version.is_none() {
            if self.numeric_text.has_object_driver(index) {
                return Err(EffectiveContentError::DriverConflict(object));
            }
            // A resident lease can coexist with ordinary affine/style channels.
            // Geometry/family/Graph owners are excluded at acquisition, while
            // later authored ownership supersedes the lease at its patch.
            if [Property::Morph, Property::Transform]
                .into_iter()
                .any(|property| {
                    !self
                        .compiled
                        .channel_tracks(noon_compile::CompiledChannelKey::new(
                            index as u32,
                            property,
                        ))
                        .is_empty()
                })
                || self
                    .compiled
                    .family_animations()
                    .iter()
                    .any(|animation| animation.object_index as usize == index)
                || !self
                    .compiled
                    .graph_dependencies_for_changed_row(index as u32)
                    .is_empty()
            {
                return Err(EffectiveContentError::GeometryDriverConflict(object));
            }
        }
        if matches!(
            content,
            ObjectContentRef::Geometry(GeometryRef::External(_))
        ) && resource_additions.is_none()
        {
            return Err(EffectiveContentError::UnsupportedExternalGeometry(object));
        }
        if self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some()
        {
            return Err(EffectiveContentError::ActiveRenderOverride(object));
        }
        let patch = ExecutionPatch::SetContent {
            object,
            content: content.clone(),
            text_bounds,
        };
        let transaction = ExecutionMutationTransaction::from_mutations([patch]);
        if let Some(additions) = resource_additions.as_ref() {
            self.compiled
                .preflight_execution_transaction_with_resources(&transaction, additions)
                .map_err(EffectiveContentError::Invalid)?;
        } else {
            self.compiled
                .preflight_execution_transaction(&transaction)
                .map_err(EffectiveContentError::Invalid)?;
        }

        Ok(PreparedEffectiveContentReplacement {
            lease,
            expected: self.publication,
            expected_version,
            object_index: index,
            content,
            text_bounds,
            image_resource,
            geometry_resource: None,
        })
    }

    /// Commit exactly one prepared content version. Only the addressed row is
    /// invalidated; Graph-owned targets are excluded at acquisition.
    pub fn commit_effective_content_replacement(
        &mut self,
        prepared: PreparedEffectiveContentReplacement,
    ) -> Result<EffectiveContentLease, EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        self.validate_content_prepared(&prepared)?;
        let index = prepared.object_index;
        let geometry_resource_changed =
            prepared
                .geometry_resource
                .as_ref()
                .is_some_and(|(id, handle, _)| {
                    self.effective_geometries
                        .get(id)
                        .is_none_or(|(current, _, _)| current != handle)
                });
        let changed = self.frame.objects[index].content != prepared.content
            || self.frame.objects[index].text_bounds != prepared.text_bounds
            || geometry_resource_changed
            || self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some();
        let next_epoch = if changed {
            Some(
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or(EffectiveContentError::FrameEpochExhausted)?,
            )
        } else {
            None
        };
        let version = prepared.expected_version.map_or(0, |version| version + 1);
        let old_image = self
            .effective_content_drivers
            .get(&index)
            .and_then(|driver| driver.content.image())
            .map(|image| image.resource());
        let new_image = prepared.content.image().map(|image| image.resource());
        let old_geometry = self
            .effective_content_drivers
            .get(&index)
            .and_then(|driver| external_geometry_id(&driver.content));
        let new_geometry = external_geometry_id(&prepared.content);
        if old_image != new_image {
            if let Some(resource) = prepared.image_resource.as_ref() {
                self.retain_effective_image(
                    new_image.expect("prepared image has handle"),
                    resource,
                );
            }
            if let Some(handle) = old_image {
                self.release_effective_image(handle);
            }
        }
        if old_geometry != new_geometry || geometry_resource_changed {
            if let Some(id) = old_geometry {
                self.release_effective_geometry(id);
            }
            if let Some((id, handle, resource)) = prepared.geometry_resource.as_ref() {
                self.retain_effective_geometry(*id, *handle, resource);
            }
        }
        if prepared.expected_version.is_none() {
            self.next_effective_content_sequence += 1;
        }
        self.effective_content_drivers.insert(
            index,
            EffectiveContentDriver {
                lease: prepared.lease,
                version,
                content: prepared.content,
                text_bounds: prepared.text_bounds,
            },
        );
        self.invalidate_replay_domain();
        if let Some(next_epoch) = next_epoch {
            self.reapply_effective_content_driver(index);
            self.mark_changed(index);
            self.publication = self.publication.with_frame_epoch(next_epoch);
        }
        Ok(prepared.lease)
    }

    /// Release the exact producer and reveal the current authored/compiled
    /// content. A later authored edit made under the lease is therefore visible.
    pub fn release_effective_content(
        &mut self,
        lease: EffectiveContentLease,
    ) -> Result<(), EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        if lease.runtime != self.identity {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        let index = self
            .compiled
            .object_index(lease.object)
            .map(|index| index as usize)
            .filter(|&index| self.object_slot_is_live(index))
            .ok_or(EffectiveContentError::UnknownObject(lease.object))?;
        if self
            .effective_content_drivers
            .get(&index)
            .is_none_or(|driver| driver.lease != lease)
        {
            return Err(EffectiveContentError::StaleLease(lease.object));
        }
        let authored = &self.compiled.objects()[index];
        let changed = self.frame.objects[index].content != authored.content
            || self.frame.objects[index].text_bounds != authored.text_bounds
            || self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some();
        let next_epoch = if changed {
            Some(
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or(EffectiveContentError::FrameEpochExhausted)?,
            )
        } else {
            None
        };
        let authored_content = changed.then(|| authored.content.clone());
        let authored_text_bounds = authored.text_bounds;
        if let Some(driver) = self.effective_content_drivers.remove(&index) {
            if let Some(image) = driver.content.image() {
                self.release_effective_image(image.resource());
            }
            if let Some(id) = external_geometry_id(&driver.content) {
                self.release_effective_geometry(id);
            }
        }
        self.invalidate_replay_domain();
        if let Some(next_epoch) = next_epoch {
            self.frame.objects[index].content =
                authored_content.expect("changed content was copied");
            self.frame.objects[index].text_bounds = authored_text_bounds;
            self.frame.render_geometries[index] = None;
            self.frame.render_transforms[index] = None;
            self.mark_changed(index);
            self.publication = self.publication.with_frame_epoch(next_epoch);
        }
        Ok(())
    }

    fn validate_content_prepared(
        &self,
        prepared: &PreparedEffectiveContentReplacement,
    ) -> Result<(), EffectiveContentError> {
        if prepared.lease.runtime != self.identity {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        // A producer that already owns this object may finish preparation after
        // unrelated effective frames advance. Its lease version still orders
        // content results, while the current row supplies the transform/style
        // used for bounds and rendering. Initial acquisition remains pinned to
        // the exact frame because its sequence is reserved only at commit.
        let compatible = if prepared.expected_version.is_some() {
            prepared.expected.scene_revision() == self.publication.scene_revision()
                && prepared.expected.execution_revision() == self.publication.execution_revision()
        } else {
            prepared.expected == self.publication
        };
        if !compatible {
            return Err(EffectiveContentError::StalePublication {
                expected: prepared.expected,
                actual: self.publication,
            });
        }
        let index = prepared.object_index;
        if !self.object_slot_is_live(index) || self.frame.objects[index].id != prepared.lease.object
        {
            return Err(EffectiveContentError::UnknownObject(prepared.lease.object));
        }
        if self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some()
        {
            return Err(EffectiveContentError::ActiveRenderOverride(
                prepared.lease.object,
            ));
        }
        if let Some((id, handle, _)) = prepared.geometry_resource.as_ref() {
            // A held lease may outlive unrelated effective frame advances. A
            // different producer can therefore claim the same bare GeometryId
            // after preparation, and must not silently supply this row's path.
            if self.geometry_resource_conflicts(
                prepared.lease.object,
                *handle,
                Some(prepared.lease),
            ) {
                return Err(EffectiveContentError::GeometryResourceConflict(*id));
            }
        }
        match (
            self.effective_content_drivers.get(&index),
            prepared.expected_version,
        ) {
            (None, None) if prepared.lease.sequence == self.next_effective_content_sequence => {
                Ok(())
            }
            (Some(driver), Some(version))
                if driver.lease == prepared.lease && driver.version == version =>
            {
                Ok(())
            }
            _ => Err(EffectiveContentError::StaleLease(prepared.lease.object)),
        }
    }

    pub(crate) fn reapply_effective_content_driver(&mut self, index: usize) {
        if let Some(driver) = self.effective_content_drivers.get(&index) {
            self.frame.objects[index].content = driver.content.clone();
            self.frame.objects[index].text_bounds = driver.text_bounds;
            self.frame.render_geometries[index] = None;
            self.frame.render_transforms[index] = None;
        }
    }

    /// An authored geometry owner supersedes a content lease. This runs only
    /// after its patch has succeeded; the old producer token then becomes stale.
    pub(crate) fn retire_effective_content_driver(&mut self, index: usize) {
        if let Some(driver) = self.effective_content_drivers.remove(&index) {
            if let Some(image) = driver.content.image() {
                self.release_effective_image(image.resource());
            }
            if let Some(id) = external_geometry_id(&driver.content) {
                self.release_effective_geometry(id);
            }
            self.frame.objects[index].content = self.compiled.objects()[index].content.clone();
            self.frame.objects[index].text_bounds = self.compiled.objects()[index].text_bounds;
            self.mark_changed(index);
        }
    }

    fn retain_effective_image(
        &mut self,
        handle: RasterImageResourceHandle,
        resource: &Arc<RasterImageResource>,
    ) {
        let entry = self
            .effective_images
            .entry(handle)
            .or_insert_with(|| (Arc::clone(resource), 0));
        entry.1 += 1;
    }

    fn release_effective_image(&mut self, handle: RasterImageResourceHandle) {
        if let std::collections::btree_map::Entry::Occupied(mut entry) =
            self.effective_images.entry(handle)
        {
            if entry.get().1 == 1 {
                entry.remove();
            } else {
                entry.get_mut().1 -= 1;
            }
        }
    }

    fn retain_effective_geometry(
        &mut self,
        id: GeometryId,
        handle: GeometryResourceHandle,
        resource: &GeometryResource,
    ) {
        let entry = self
            .effective_geometries
            .entry(id)
            .or_insert_with(|| (handle, resource.clone(), 0));
        entry.2 += 1;
    }

    fn geometry_resource_conflicts(
        &self,
        object: ObjectId,
        handle: GeometryResourceHandle,
        lease: Option<EffectiveContentLease>,
    ) -> bool {
        if self
            .compiled
            .geometry_resources()
            .current_handle(handle.id)
            .is_some()
        {
            return true;
        }
        let Some((current, _, references)) = self.effective_geometries.get(&handle.id) else {
            return false;
        };
        if *current == handle {
            return false;
        }
        // GeometryRef carries only an ID. A version swap is safe only when the
        // same lease is the sole reader of the old version; sharing readers or
        // an unrelated producer would otherwise silently observe new payloads.
        let Some(lease) = lease else { return true };
        *references != 1
            || self.effective_content_lease(object) != Some(lease)
            || self
                .compiled
                .object_index(object)
                .and_then(|index| self.effective_content_drivers.get(&(index as usize)))
                .and_then(|driver| external_geometry_id(&driver.content))
                != Some(handle.id)
    }

    fn release_effective_geometry(&mut self, id: GeometryId) {
        if let std::collections::btree_map::Entry::Occupied(mut entry) =
            self.effective_geometries.entry(id)
        {
            if entry.get().2 == 1 {
                entry.remove();
            } else {
                entry.get_mut().2 -= 1;
            }
        }
    }
}

fn external_geometry_id(content: &ObjectContentRef) -> Option<GeometryId> {
    match content {
        ObjectContentRef::Geometry(GeometryRef::External(id)) => Some(*id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
    use noon_core::{
        CompositionTimeMap, GeometryRef, GeometryResource, GeometryResourceArena,
        GeometryResourceLookup, ObjectContentRef, ObjectId, Property, RasterImageResourceArena,
        RasterImageResourceLookup, RateFunction, SemanticImageContent, Style, TrackDefinition,
        TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
    };

    use super::{EffectiveContentError, SceneInstance};

    fn scene(count: usize, animate_last: bool) -> SceneInstance {
        let objects = (0..count)
            .map(|index| {
                CompiledObject::new(
                    ObjectId::new(index as u64),
                    GeometryRef::circle(1.0),
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .collect();
        let tracks = animate_last
            .then(|| TrackDefinition {
                id: TrackId::new(1),
                object: ObjectId::new((count - 1) as u64),
                property: Property::Opacity,
                values: TrackValues::Scalar { from: 1.0, to: 0.0 },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap())
    }

    #[test]
    fn external_geometry_is_published_and_retired_by_effective_content_leases() {
        let mut instance = scene(2, false);
        let mut source = GeometryResourceArena::new();
        let handle = source.insert_path(
            VectorPath::new()
                .move_to(Vec2::new(0.0, 0.0))
                .line_to(Vec2::new(2.0, 0.0)),
        );
        let first = instance
            .prepare_effective_geometry_replacement(ObjectId::new(0), handle, &source, None)
            .unwrap();
        let first_lease = instance
            .commit_effective_content_replacement(first)
            .unwrap();
        let second = instance
            .prepare_effective_geometry_replacement(ObjectId::new(1), handle, &source, None)
            .unwrap();
        let second_lease = instance
            .commit_effective_content_replacement(second)
            .unwrap();
        drop(source);

        let publication = instance.take_renderer_publication();
        assert!(publication.geometry_resources().get(handle).is_some());
        drop(publication);
        assert_eq!(
            instance.geometry_resources().current_handle(handle.id),
            Some(handle)
        );
        assert!(instance.geometry_resources().get(handle).is_some());
        assert_eq!(instance.effective_geometries.get(&handle.id).unwrap().2, 2);
        instance.release_effective_content(first_lease).unwrap();
        assert!(instance.geometry_resources().get(handle).is_some());
        assert_eq!(instance.effective_geometries.get(&handle.id).unwrap().2, 1);
        instance.release_effective_content(second_lease).unwrap();
        assert!(instance.geometry_resources().get(handle).is_none());
        assert!(!instance.effective_geometries.contains_key(&handle.id));
    }

    #[test]
    fn external_geometry_preparation_rejects_a_stale_initial_publication() {
        let mut instance = scene(1, false);
        let mut source = GeometryResourceArena::new();
        let handle = source.insert_path(VectorPath::new().move_to(Vec2::new(1.0, 1.0)));
        let prepared = instance
            .prepare_effective_geometry_replacement(ObjectId::new(0), handle, &source, None)
            .unwrap();
        instance.evaluate(1.0).unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(prepared),
            Err(EffectiveContentError::StalePublication { .. })
        ));
        assert!(instance.geometry_resources().get(handle).is_none());
    }

    #[test]
    fn sole_lease_can_replace_the_same_geometry_id_with_a_new_version() {
        let object = ObjectId::new(0);
        let mut instance = scene(1, false);
        let mut source = GeometryResourceArena::new();
        let first = source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        let prepared = instance
            .prepare_effective_geometry_replacement(object, first, &source, None)
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        instance.take_frame_changes();
        let before = instance.publication_context();

        let second = source
            .replace(
                first.id,
                GeometryResource::VectorPath(Arc::new(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(2.0, 0.0)),
                )),
            )
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_ne!(first, second);
        let prepared = instance
            .prepare_effective_geometry_replacement(object, second, &source, Some(lease))
            .unwrap();
        assert_eq!(
            instance.commit_effective_content_replacement(prepared),
            Ok(lease)
        );
        assert_ne!(instance.publication_context(), before);
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        assert_eq!(
            instance.geometry_resources().current_handle(first.id),
            Some(second)
        );
        assert!(instance.geometry_resources().get(first).is_none());
        assert!(instance.geometry_resources().get(second).is_some());
        instance.release_effective_content(lease).unwrap();
        assert!(instance.geometry_resources().get(second).is_none());
    }

    #[test]
    fn shared_geometry_id_cannot_swap_version_under_one_lease() {
        let mut instance = scene(2, false);
        let mut source = GeometryResourceArena::new();
        let first = source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        let mut leases = Vec::new();
        for id in 0..2 {
            let prepared = instance
                .prepare_effective_geometry_replacement(ObjectId::new(id), first, &source, None)
                .unwrap();
            leases.push(
                instance
                    .commit_effective_content_replacement(prepared)
                    .unwrap(),
            );
        }
        let second = source
            .replace(
                first.id,
                GeometryResource::VectorPath(Arc::new(
                    VectorPath::new().move_to(Vec2::new(1.0, 0.0)),
                )),
            )
            .unwrap();
        assert!(matches!(
            instance.prepare_effective_geometry_replacement(
                ObjectId::new(0), second, &source, Some(leases[0])
            ),
            Err(EffectiveContentError::GeometryResourceConflict(id)) if id == first.id
        ));
        assert_eq!(
            instance.geometry_resources().current_handle(first.id),
            Some(first)
        );
        instance.release_effective_content(leases[0]).unwrap();
        instance.release_effective_content(leases[1]).unwrap();
    }

    #[test]
    fn held_lease_rejects_geometry_id_claimed_by_another_producer_after_prepare() {
        let mut instance = scene(2, false);
        let inline = instance
            .prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let held_lease = instance
            .commit_effective_content_replacement(inline)
            .unwrap();
        let mut first_source = GeometryResourceArena::new();
        let first = first_source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        let mut second_source = GeometryResourceArena::new();
        let second = second_source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        assert_eq!(first.id, second.id);
        assert_ne!(first, second);

        let second_prepared = instance
            .prepare_effective_geometry_replacement(ObjectId::new(1), second, &second_source, None)
            .unwrap();
        let first_prepared = instance
            .prepare_effective_geometry_replacement(
                ObjectId::new(0),
                first,
                &first_source,
                Some(held_lease),
            )
            .unwrap();
        let second_lease = instance
            .commit_effective_content_replacement(second_prepared)
            .unwrap();
        assert_eq!(
            instance.commit_effective_content_replacement(first_prepared),
            Err(EffectiveContentError::GeometryResourceConflict(first.id))
        );
        assert_eq!(
            instance.geometry_resources().current_handle(first.id),
            Some(second)
        );
        assert_eq!(instance.effective_geometries.get(&first.id).unwrap().2, 1);
        instance.release_effective_content(held_lease).unwrap();
        instance.release_effective_content(second_lease).unwrap();
        assert!(instance.geometry_resources().get(second).is_none());
    }

    #[test]
    fn replacement_is_effective_only_and_release_reveals_latest_authored_content() {
        let mut instance = scene(1, false);
        let object = ObjectId::new(0);
        let before = instance.publication_context();
        let first = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let stale = first.clone();
        let lease = instance
            .commit_effective_content_replacement(first)
            .unwrap();
        let published = instance.publication_context();
        assert_eq!(published.scene_revision(), before.scene_revision());
        assert_eq!(published.execution_revision(), before.execution_revision());
        assert_ne!(published.frame_epoch(), before.frame_epoch());
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
        assert!(matches!(
            instance.commit_effective_content_replacement(stale),
            Err(EffectiveContentError::StalePublication { .. })
        ));

        instance
            .apply_execution_patch(&ExecutionPatch::SetContent {
                object,
                content: ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
                text_bounds: None,
            })
            .unwrap();
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
        instance.release_effective_content(lease).unwrap();
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );
        assert!(matches!(
            instance.release_effective_content(lease),
            Err(EffectiveContentError::StaleLease(_))
        ));
    }

    #[test]
    fn held_content_does_not_stage_or_dirty_unrelated_animated_rows() {
        let mut instance = scene(601, true);
        for index in 0..600 {
            let prepared = instance
                .prepare_effective_content_replacement(
                    ObjectId::new(index),
                    ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    None,
                    None,
                )
                .unwrap();
            instance
                .commit_effective_content_replacement(prepared)
                .unwrap();
        }
        instance.take_frame_changes();
        instance.take_spatial_changes();
        let prepared = instance.prepare_advance_to(1.0).unwrap();
        assert_eq!(prepared.staged_row_count(), 1);
        assert_eq!(prepared.prior_driver_rows(), 0);
        instance.advance_to(1.0).unwrap();
        assert_eq!(instance.take_frame_changes().object_indices(), &[600]);
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
    }

    #[test]
    fn invalid_replacement_and_geometry_driver_conflict_leave_frame_unchanged() {
        let mut instance = scene(1, false);
        let before = instance.frame().clone();
        let publication = instance.publication_context();
        assert!(instance
            .prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(f32::NAN)),
                None,
                None,
            )
            .is_err());
        assert_eq!(instance.frame(), &before);
        assert_eq!(instance.publication_context(), publication);

        let prepared = instance
            .prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(2),
                object: ObjectId::new(0),
                property: Property::Morph,
                values: TrackValues::Scalar { from: 0.0, to: 1.0 },
                timing: TrackTiming::new(1.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
        assert!(matches!(
            instance.release_effective_content(lease),
            Err(EffectiveContentError::StaleLease(_))
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
        assert!(matches!(
            instance.prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            ),
            Err(EffectiveContentError::GeometryDriverConflict(_))
        ));
    }

    #[test]
    fn cloned_runtime_rebinds_its_lease_without_accepting_the_source_token() {
        let mut original = scene(1, false);
        let object = ObjectId::new(0);
        let prepared = original
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let source_lease = original
            .commit_effective_content_replacement(prepared)
            .unwrap();
        let mut cloned = original.clone();
        assert!(matches!(
            cloned.release_effective_content(source_lease),
            Err(EffectiveContentError::ForeignRuntime)
        ));
        let cloned_lease = cloned.effective_content_lease(object).unwrap();
        cloned.release_effective_content(cloned_lease).unwrap();
        assert_eq!(
            cloned.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
        assert_eq!(
            original.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
    }

    #[test]
    fn leased_result_survives_unrelated_frame_but_not_authored_plan_change() {
        let mut instance = scene(2, true);
        let object = ObjectId::new(0);
        let first = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(first)
            .unwrap();
        let prepared = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
                None,
                Some(lease),
            )
            .unwrap();
        let superseded = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(5.0)),
                None,
                Some(lease),
            )
            .unwrap();
        instance.advance_to(1.0).unwrap();
        instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(superseded),
            Err(EffectiveContentError::StaleLease(_))
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );

        let stale = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(4.0)),
                None,
                Some(lease),
            )
            .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style: Style {
                    opacity: 0.6,
                    ..Style::default()
                },
            })
            .unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(stale),
            Err(EffectiveContentError::StalePublication { .. })
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );
    }

    #[test]
    fn prepared_image_is_invisible_until_commit_and_release_retires_its_resource() {
        let mut instance = scene(1, false);
        let mut source = RasterImageResourceArena::new();
        let object = ObjectId::new(0);
        let before = instance.publication_context();
        let handle = source.intern_rgba8(2, 1, vec![255; 8]).unwrap();
        let foreign = RasterImageResourceArena::new();
        assert!(matches!(
            instance.prepare_effective_image_replacement(
                object,
                SemanticImageContent::new(handle),
                &foreign,
                None,
            ),
            Err(EffectiveContentError::Resource(
                noon_compile::CompiledResourceError::MissingImage(_)
            ))
        ));
        let prepared = instance
            .prepare_effective_image_replacement(
                object,
                SemanticImageContent::new(handle),
                &source,
                None,
            )
            .unwrap();
        assert!(instance.raster_image_resources().get(handle).is_none());
        assert_eq!(instance.publication_context(), before);
        drop(source);
        let lease = instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        assert_eq!(
            instance
                .raster_image_resources()
                .get(handle)
                .unwrap()
                .rgba8(),
            &[255; 8]
        );
        assert_eq!(instance.effective_images.len(), 1);
        let publication = instance.take_renderer_publication();
        assert!(publication.raster_image_resources().get(handle).is_some());
        assert_eq!(
            publication.frame().objects[0]
                .content
                .image()
                .unwrap()
                .resource(),
            handle
        );
        instance.release_effective_content(lease).unwrap();
        assert!(instance.raster_image_resources().get(handle).is_none());
        assert!(instance.effective_images.is_empty());
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
    }

    #[test]
    fn stale_image_result_does_not_publish_or_accumulate_runtime_resources() {
        let mut instance = scene(2, true);
        let mut source = RasterImageResourceArena::new();
        let object = ObjectId::new(0);
        let first_handle = source.intern_rgba8(1, 1, vec![1; 4]).unwrap();
        let prepared = instance
            .prepare_effective_image_replacement(
                object,
                SemanticImageContent::new(first_handle),
                &source,
                None,
            )
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        let stale_handle = source.intern_rgba8(1, 1, vec![2; 4]).unwrap();
        let stale = instance
            .prepare_effective_image_replacement(
                object,
                SemanticImageContent::new(stale_handle),
                &source,
                Some(lease),
            )
            .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style: Style {
                    opacity: 0.5,
                    ..Style::default()
                },
            })
            .unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(stale),
            Err(EffectiveContentError::StalePublication { .. })
        ));
        assert!(instance
            .raster_image_resources()
            .get(stale_handle)
            .is_none());
        assert_eq!(instance.effective_images.len(), 1);
        assert!(instance
            .raster_image_resources()
            .get(first_handle)
            .is_some());

        for pixel in 3..35 {
            let handle = source.intern_rgba8(1, 1, vec![pixel; 4]).unwrap();
            let prepared = instance
                .prepare_effective_image_replacement(
                    object,
                    SemanticImageContent::new(handle),
                    &source,
                    Some(lease),
                )
                .unwrap();
            instance
                .commit_effective_content_replacement(prepared)
                .unwrap();
            assert_eq!(instance.effective_images.len(), 1);
            assert!(instance.raster_image_resources().get(handle).is_some());
        }
        drop(source);
        assert!(instance
            .raster_image_resources()
            .get(first_handle)
            .is_none());
        instance.release_effective_content(lease).unwrap();
        assert!(instance.effective_images.is_empty());
    }

    #[test]
    fn shared_effective_image_remains_live_until_its_last_lease_releases() {
        let mut instance = scene(2, false);
        let mut source = RasterImageResourceArena::new();
        let handle = source.intern_rgba8(1, 1, vec![7; 4]).unwrap();
        let leases = [ObjectId::new(0), ObjectId::new(1)].map(|object| {
            let prepared = instance
                .prepare_effective_image_replacement(
                    object,
                    SemanticImageContent::new(handle),
                    &source,
                    None,
                )
                .unwrap();
            instance
                .commit_effective_content_replacement(prepared)
                .unwrap()
        });
        drop(source);
        assert_eq!(instance.effective_images.get(&handle).unwrap().1, 2);
        instance.release_effective_content(leases[0]).unwrap();
        assert!(instance.raster_image_resources().get(handle).is_some());
        assert_eq!(instance.effective_images.get(&handle).unwrap().1, 1);
        instance.release_effective_content(leases[1]).unwrap();
        assert!(instance.raster_image_resources().get(handle).is_none());
    }
}
