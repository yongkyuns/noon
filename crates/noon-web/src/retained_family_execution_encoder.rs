use crate::retained_resource_transport::residency::{ResourceResidency, StagedResourceResidency};
use noon_core::{
    Camera2DState, FontResourceLookup, GeometryRef, GeometryResource, GeometryResourceLookup,
    RetainedFamilyAnimationPlan, TextResourceHandle, TextResourceLookup,
};
use noon_runtime::{FrameChanges, RetainedFamilyFrame, RetainedPlannedFamilyFrame};

use std::{collections::HashMap, sync::Arc};

use crate::{
    RenderGeometryPreparation, RetainedExecutionDeltaEncoder, RetainedExecutionTransportError,
    RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionTransportError,
    RetainedResourceBundle, RetainedResourceInventory, RetainedResourceRetirements,
    RetainedResourceTransportError, TransportTextResourceHandle,
};

/// Sequence-owning producer for the additive retained family execution envelope.
///
/// Ordinary retained object state stays encoded by [`RetainedExecutionDeltaEncoder`].
/// This owner only attaches the already-evaluated family state and immutable member
/// plans, so sequencing, slot identity, and snapshot rules remain exactly the same as
/// the base retained transport.
#[derive(Clone, Debug)]
pub struct RetainedFamilyExecutionDeltaEncoder {
    retained: RetainedExecutionDeltaEncoder,
    plan_index_remap: HashMap<usize, u32>,
    published_plan_count: usize,
    observed_plan_count: usize,
    resources: RetainedResourceInventory,
    resource_roots: ResourceResidency,
    text_closures: HashMap<TransportTextResourceHandle, TextClosure>,
    geometry_references: HashMap<crate::TransportGeometryResourceHandle, usize>,
    font_references: HashMap<(String, u32), usize>,
    render_geometry_resources: HashMap<crate::TransportSlotId, u64>,
    render_geometry_generations: Vec<u32>,
    free_render_geometry_resources: Vec<u32>,
    next_render_geometry_resource: u32,
    published_rows: HashMap<crate::TransportSlotId, crate::RetainedTransportObjectState>,
}

#[derive(Clone, Debug)]
struct TextClosure {
    geometries: Vec<crate::TransportGeometryResourceHandle>,
    fonts: Vec<(String, u32)>,
}

#[derive(Debug)]
struct StagedPlanMappings {
    mappings: HashMap<usize, u32>,
    added_plan_indices: Vec<usize>,
    next_published_plan_count: usize,
}

/// Borrow the live arena during preparation. Reservations and generation changes
/// stay in a small overlay until all fallible resource capture has succeeded.
struct StagedRenderGeometryResources<'a> {
    installed: &'a HashMap<crate::TransportSlotId, u64>,
    generations: &'a [u32],
    free: &'a [u32],
    slot_changes: HashMap<crate::TransportSlotId, Option<u64>>,
    generation_changes: HashMap<u32, u32>,
    newly_free: Vec<u32>,
    reserved_free: usize,
    next: u32,
}

struct RenderGeometryChanges {
    slot_changes: HashMap<crate::TransportSlotId, Option<u64>>,
    generation_changes: HashMap<u32, u32>,
    newly_free: Vec<u32>,
    reserved_free: usize,
    next: u32,
}

impl<'a> StagedRenderGeometryResources<'a> {
    fn new(encoder: &'a RetainedFamilyExecutionDeltaEncoder) -> Self {
        Self {
            installed: &encoder.render_geometry_resources,
            generations: &encoder.render_geometry_generations,
            free: &encoder.free_render_geometry_resources,
            slot_changes: HashMap::new(),
            generation_changes: HashMap::new(),
            newly_free: Vec::new(),
            reserved_free: 0,
            next: encoder.next_render_geometry_resource,
        }
    }

    fn resource(&self, slot: &crate::TransportSlotId) -> Option<u64> {
        self.slot_changes
            .get(slot)
            .copied()
            .unwrap_or_else(|| self.installed.get(slot).copied())
    }

    fn generation(&self, index: u32) -> u32 {
        self.generation_changes
            .get(&index)
            .copied()
            .or_else(|| self.generations.get(index as usize).copied())
            .unwrap_or(0)
    }

    fn retire(
        &mut self,
        slot: crate::TransportSlotId,
    ) -> Result<Option<(u32, u32)>, RetainedResourceTransportError> {
        let Some(id) = self.resource(&slot) else {
            return Ok(None);
        };
        let (index, generation) = crate::retained_resource_transport::render_geometry_parts(id);
        let next = generation.checked_add(1).ok_or_else(|| {
            RetainedResourceTransportError::Encode(
                "retained render geometry generation exhausted".into(),
            )
        })?;
        self.slot_changes.insert(slot, None);
        self.generation_changes.insert(index, next);
        self.newly_free.push(index);
        Ok(Some((index, next)))
    }

    fn publish(
        &mut self,
        slot: crate::TransportSlotId,
    ) -> Result<u64, RetainedResourceTransportError> {
        let (index, generation) = if let Some(id) = self.resource(&slot) {
            let (index, generation) = crate::retained_resource_transport::render_geometry_parts(id);
            let next = generation.checked_add(1).ok_or_else(|| {
                RetainedResourceTransportError::Encode(
                    "retained render geometry generation exhausted".into(),
                )
            })?;
            self.generation_changes.insert(index, next);
            (index, next)
        } else {
            let index = if let Some(index) = self.newly_free.pop() {
                index
            } else if self.reserved_free < self.free.len() {
                let index = self.free[self.free.len() - 1 - self.reserved_free];
                self.reserved_free += 1;
                index
            } else {
                let index = self.next;
                self.next = self.next.checked_add(1).ok_or_else(|| {
                    RetainedResourceTransportError::Encode(
                        "retained render geometry resource index exhausted".into(),
                    )
                })?;
                index
            };
            (index, self.generation(index))
        };
        let id = crate::retained_resource_transport::render_geometry_id(index, generation);
        self.slot_changes.insert(slot, Some(id));
        Ok(id)
    }

    fn finish(self) -> RenderGeometryChanges {
        RenderGeometryChanges {
            slot_changes: self.slot_changes,
            generation_changes: self.generation_changes,
            newly_free: self.newly_free,
            reserved_free: self.reserved_free,
            next: self.next,
        }
    }
}

impl RenderGeometryChanges {
    fn commit(self, encoder: &mut RetainedFamilyExecutionDeltaEncoder) {
        for (slot, resource) in self.slot_changes {
            if let Some(resource) = resource {
                encoder.render_geometry_resources.insert(slot, resource);
            } else {
                encoder.render_geometry_resources.remove(&slot);
            }
        }
        encoder
            .render_geometry_generations
            .resize(self.next as usize, 0);
        for (index, generation) in self.generation_changes {
            encoder.render_geometry_generations[index as usize] = generation;
        }
        encoder
            .free_render_geometry_resources
            .truncate(encoder.free_render_geometry_resources.len() - self.reserved_free);
        encoder
            .free_render_geometry_resources
            .extend(self.newly_free);
        encoder.next_render_geometry_resource = self.next;
    }

    #[cfg(test)]
    fn touched_slots(&self) -> usize {
        self.slot_changes.len()
    }
}

impl RetainedFamilyExecutionDeltaEncoder {
    /// Compact only dense updates whose stable wire fields are unchanged. This
    /// runs after resource attachment so the comparison uses published IDs.
    pub(crate) fn compact_dense_rows(
        &mut self,
        envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
    ) {
        let retained = &mut envelope.retained;
        if retained.snapshot {
            self.published_rows.clear();
            return;
        }
        // Static snapshots and small scenes do not need a second row copy. A
        // dense incremental first seeds the cache without altering its wire.
        if self.published_rows.is_empty() && retained.objects.len() < 128 {
            return;
        }
        for slot in &retained.removed_slots {
            self.published_rows.remove(slot);
        }

        let eligible = !retained.snapshot
            && retained.objects.len() >= 128
            && retained.removed_slots.is_empty()
            && retained.painter_order.is_none()
            && envelope.family_plans.is_empty()
            && envelope.resource_additions.is_none()
            && envelope.resource_retirements.is_empty();
        let mut full = Vec::new();
        for row in std::mem::take(&mut retained.objects) {
            let previous = self.published_rows.get(&row.slot);
            let stable = previous.is_some_and(|previous| {
                previous.order == row.order
                    && previous.object == row.object
                    && previous.z_index == row.z_index
                    && previous.content == row.content
                    && previous.appearance == row.appearance
                    && previous.text_bounds == row.text_bounds
                    && previous.presence == row.presence
                    && previous.reveal == row.reveal
                    && previous.render_geometry == row.render_geometry
                    && previous.render_transform == row.render_transform
                    && previous.render_geometry_resource == row.render_geometry_resource
            });
            let patch = previous.and_then(|previous| {
                let transform = (previous.transform != row.transform).then_some(row.transform);
                let style = (previous.style != row.style).then_some(row.style);
                let morph = (previous.morph != row.morph).then_some(row.morph);
                (eligible && stable && (transform.is_some() || style.is_some() || morph.is_some()))
                    .then_some(crate::RetainedTransportObjectPatch {
                        slot: row.slot,
                        object: row.object,
                        transform,
                        style,
                        morph,
                    })
            });
            if let Some(patch) = patch {
                retained.object_patches.push(patch);
                self.published_rows.insert(row.slot, row);
            } else {
                self.published_rows.insert(row.slot, row.clone());
                full.push(row);
            }
        }
        retained.objects = full;
    }

    pub(crate) const fn session(&self) -> u32 {
        self.retained.session()
    }

    pub fn new(session: u32) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::new(session),
            plan_index_remap: HashMap::new(),
            published_plan_count: 0,
            observed_plan_count: 0,
            resources: RetainedResourceInventory::default(),
            resource_roots: ResourceResidency::default(),
            text_closures: HashMap::new(),
            geometry_references: HashMap::new(),
            font_references: HashMap::new(),
            render_geometry_resources: HashMap::new(),
            render_geometry_generations: Vec::new(),
            free_render_geometry_resources: Vec::new(),
            next_render_geometry_resource: 0,
            published_rows: HashMap::new(),
        }
    }

    pub(crate) fn new_with_resources(session: u32, resources: &RetainedResourceBundle) -> Self {
        let mut encoder = Self::new(session);
        encoder.next_render_geometry_resource = u32::try_from(resources.render_geometry_count())
            .expect("retained render geometry resources exceed u32 transport index space");
        encoder.render_geometry_generations =
            vec![0; encoder.next_render_geometry_resource as usize];
        encoder.resources = resources.inventory();
        for closure in resources.text_closures() {
            encoder.register_text_closure(closure.text, closure.geometries, closure.fonts);
        }
        encoder
    }

    pub(crate) fn attach_resource_additions(
        &mut self,
        envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &(impl TextResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        images: &(impl noon_core::RasterImageResourceLookup + ?Sized),
    ) -> Result<(), RetainedResourceTransportError> {
        // Effective content may borrow an external path from the producer's
        // runtime lease. The receiver has a separate geometry arena, so encode
        // only touched paths as self-contained immutable content. Unchanged
        // objects are absent from an incremental delta.
        for object in &mut envelope.retained.objects {
            if let crate::TransportObjectContent::Geometry { geometry } = &mut object.content {
                resolve_external_geometry(geometry, geometries)?;
            }
            if let Some(geometry) = &mut object.render_geometry {
                resolve_external_geometry(geometry, geometries)?;
            }
        }
        let new_images = envelope
            .retained
            .objects
            .iter()
            .filter_map(|object| match object.content {
                crate::TransportObjectContent::Image { image, .. }
                    if !self.resources.contains_image(image) =>
                {
                    Some(noon_core::RasterImageResourceHandle::from(image))
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        let new_texts = text_handles
            .into_iter()
            .filter(|handle| {
                !self.resources.contains_text(
                    crate::TransportTextResourceHandle::from_source_handle(*handle),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();

        // The retained encoder emits a changed immutable geometry inline. Convert
        // only those changes into generation-qualified reusable arena slots.
        let mut staged = StagedRenderGeometryResources::new(self);
        let mut updates = std::collections::BTreeMap::<u32, (u32, Option<Arc<GeometryRef>>)>::new();
        let mut preparations = Vec::<RenderGeometryPreparation>::new();
        for slot in envelope.retained.removed_slots.iter().chain(
            envelope
                .retained
                .objects
                .iter()
                .filter(|object| object.render_transform.is_none())
                .map(|object| &object.slot),
        ) {
            if let Some((index, generation)) = staged.retire(*slot)? {
                updates.insert(index, (generation, None));
            }
        }
        for object in &mut envelope.retained.objects {
            if object.render_transform.is_none() {
                continue;
            }

            if let Some(geometry) = object.render_geometry.take() {
                // Resident GPU preloading is a one-time installation hint. A
                // later version of the same render-geometry slot travels as an
                // ordinary resource replacement and is tessellated into the
                // renderer's reusable per-row ranges on its next frame.
                let preload = envelope.retained.snapshot && staged.resource(&object.slot).is_none();
                let resource = staged.publish(object.slot)?;
                let (index, generation) =
                    crate::retained_resource_transport::render_geometry_parts(resource);
                if preload {
                    preparations.push(RenderGeometryPreparation {
                        resource: index,
                        style: object.style,
                        transform: object
                            .render_transform
                            .expect("render geometry publication has a render transform"),
                    });
                }
                updates.insert(index, (generation, Some(Arc::new(geometry))));
                object.render_geometry_resource = Some(resource);
            } else if object.render_geometry_resource.is_none() {
                if let Some(resource) = staged.resource(&object.slot) {
                    object.render_geometry_resource = Some(resource);
                }
            }
        }
        let pending_render = staged.finish();
        let staged_roots = self.resource_roots.stage(&envelope.retained);
        let retirements = self.resource_roots.retirements(&staged_roots);
        if new_texts.is_empty() && new_images.is_empty() && updates.is_empty() {
            envelope.resource_retirements = retirements;
            self.commit_resource_roots(staged_roots, &envelope.resource_retirements);
            pending_render.commit(self);
            return Ok(());
        }

        let mut additions = RetainedResourceBundle::capture_additions(
            new_texts,
            texts,
            geometries,
            fonts,
            &self.resources,
        )?;
        additions.capture_images(new_images, images)?;
        let closures = additions.text_closures().collect::<Vec<_>>();
        additions.retain_additions(&mut self.resources);
        for closure in closures {
            self.register_text_closure(closure.text, closure.geometries, closure.fonts);
        }
        if !updates.is_empty() {
            additions.set_render_geometry_updates(
                self.session(),
                updates
                    .into_iter()
                    .map(|(slot, (generation, geometry))| (slot, generation, geometry))
                    .collect(),
                preparations,
            );
        }
        debug_assert!(!additions.is_empty());
        envelope.resource_additions = Some(additions);
        envelope.resource_retirements = retirements;
        self.commit_resource_roots(staged_roots, &envelope.resource_retirements);
        pending_render.commit(self);
        Ok(())
    }

    fn register_text_closure(
        &mut self,
        text: TransportTextResourceHandle,
        geometries: Vec<crate::TransportGeometryResourceHandle>,
        fonts: Vec<(String, u32)>,
    ) {
        for geometry in &geometries {
            *self.geometry_references.entry(*geometry).or_default() += 1;
        }
        for font in &fonts {
            *self.font_references.entry(font.clone()).or_default() += 1;
        }
        self.text_closures
            .insert(text, TextClosure { geometries, fonts });
    }

    fn commit_resource_roots(
        &mut self,
        staged: StagedResourceResidency,
        retirements: &RetainedResourceRetirements,
    ) {
        self.resources.forget_root_retirements(retirements);
        for text in &retirements.texts {
            if let Some(TextClosure { geometries, fonts }) = self.text_closures.remove(text) {
                for geometry in geometries {
                    if let Some(count) = self.geometry_references.get_mut(&geometry) {
                        *count -= 1;
                        if *count == 0 {
                            self.geometry_references.remove(&geometry);
                            self.resources.forget_geometry(geometry);
                        }
                    }
                }
                for font in fonts {
                    if let Some(count) = self.font_references.get_mut(&font) {
                        *count -= 1;
                        if *count == 0 {
                            self.font_references.remove(&font);
                            self.resources.forget_font(&font);
                        }
                    }
                }
            }
        }
        self.resource_roots.commit(staged);
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        let envelope = RetainedFamilyExecutionDeltaEnvelope::snapshot(retained, frame, plans)?;
        self.plan_index_remap = (0..plans.len())
            .map(|index| (index, index as u32))
            .collect();
        self.published_plan_count = plans.len();
        self.observed_plan_count = plans.len();
        Ok(envelope)
    }

    pub fn encode_planned_snapshot(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        let envelope =
            RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(retained, frame, plans)?;
        self.compact_planned_snapshot(envelope, plans)
    }

    /// Encode an authoritative snapshot for the execution rows that still own a
    /// live slot. The family sidecar uses the same exact row selection as the base
    /// retained envelope, so retired rows cannot reappear through plan state.
    pub fn encode_planned_snapshot_indices(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let indices = indices.into_iter().collect::<Vec<_>>();
        let retained = self.retained.encode_snapshot_indices(
            frame.retained,
            camera,
            indices.iter().copied(),
        )?;
        let envelope = RetainedFamilyExecutionDeltaEnvelope::planned_snapshot_indices(
            retained, frame, plans, indices,
        )?;
        self.compact_planned_snapshot(envelope, plans)
    }

    /// Encode one sparse family-aware retained update.
    ///
    /// `plans` are normally used only by the initial snapshot. They are supplied here
    /// as well because the base retained encoder may legitimately promote an
    /// `FrameChanges::all()` update to an authoritative snapshot.
    pub fn encode_incremental(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        self.validate_plan_count(plans)?;
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let snapshot = retained.snapshot;
        let envelope = if snapshot {
            RetainedFamilyExecutionDeltaEnvelope::snapshot(retained, frame, plans)?
        } else {
            RetainedFamilyExecutionDeltaEnvelope::incremental(retained, frame, changes)?
        };
        Ok(Some(envelope))
    }

    pub fn encode_planned_incremental(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        self.validate_plan_count(plans)?;
        let staged = self.stage_plan_mappings(frame, plans, changes.object_indices())?;
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let snapshot = retained.snapshot;
        let mut envelope = if snapshot {
            let envelope =
                RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(retained, frame, plans)?;
            return self.compact_planned_snapshot(envelope, plans).map(Some);
        } else {
            let added_plans = staged
                .added_plan_indices
                .iter()
                .map(|&index| plans[index].clone())
                .collect::<Vec<_>>();
            RetainedFamilyExecutionDeltaEnvelope::planned_incremental_with_plans(
                retained,
                frame,
                changes,
                &added_plans,
            )?
        };
        remap_family_state_indices(&mut envelope, &self.plan_index_remap, &staged.mappings)?;
        self.commit_plan_mappings(staged, plans.len());
        Ok(Some(envelope))
    }

    /// Encode one sparse family-aware update with a compact painter-order splice.
    /// Newly admitted rows may begin an active family animation in this same atomic
    /// delta; their plan descriptor is appended before their state references it.
    pub fn encode_planned_incremental_with_painter_order(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
        painter_order: &[u32],
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        self.validate_plan_count(plans)?;
        // Validate before the base encoder advances its sequence. The final
        // family rows must follow that encoder's coalesced membership decision.
        self.stage_plan_mappings(frame, plans, changes.object_indices())?;
        let Some(retained) = self.retained.encode_incremental_with_painter_order(
            frame.retained,
            changes,
            camera,
            painter_order,
        )?
        else {
            return Ok(None);
        };
        // A row created and removed between publications never reaches the
        // consumer. Sending a family reset for it would reference an unknown
        // object. Use only the sparse rows emitted by the base transport.
        let published_objects = retained
            .objects
            .iter()
            .map(|object| object.object)
            .collect::<std::collections::HashSet<_>>();
        let family_changes = FrameChanges::objects(
            changes
                .object_indices()
                .iter()
                .copied()
                .filter(|&index| published_objects.contains(&frame.retained.objects[index].id))
                .collect(),
        );
        let staged = self.stage_plan_mappings(frame, plans, family_changes.object_indices())?;
        let added_plans = staged
            .added_plan_indices
            .iter()
            .map(|&index| plans[index].clone())
            .collect::<Vec<_>>();
        let mut envelope = RetainedFamilyExecutionDeltaEnvelope::planned_incremental_with_plans(
            retained,
            frame,
            &family_changes,
            &added_plans,
        )?;
        remap_family_state_indices(&mut envelope, &self.plan_index_remap, &staged.mappings)?;
        self.commit_plan_mappings(staged, plans.len());
        Ok(Some(envelope))
    }

    fn stage_plan_mappings(
        &self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        object_indices: &[usize],
    ) -> Result<StagedPlanMappings, RetainedFamilyExecutionTransportError> {
        let mut mappings = HashMap::new();
        let mut added_plan_indices = Vec::new();
        let mut next_published_plan_count = self.published_plan_count;
        for &object_index in object_indices {
            if frame.family_animation(object_index).is_none() {
                continue;
            }
            let object = &frame.retained.objects[object_index];
            let core_index = frame.family_plan_index(object_index).ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(object.id),
            )? as usize;
            if core_index >= plans.len() {
                return Err(RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: object.id,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                });
            }
            if self.plan_index_remap.contains_key(&core_index) || mappings.contains_key(&core_index)
            {
                continue;
            }
            mappings.insert(core_index, next_published_plan_count as u32);
            added_plan_indices.push(core_index);
            next_published_plan_count += 1;
        }
        Ok(StagedPlanMappings {
            mappings,
            added_plan_indices,
            next_published_plan_count,
        })
    }

    fn commit_plan_mappings(&mut self, staged: StagedPlanMappings, observed_plan_count: usize) {
        self.plan_index_remap.extend(staged.mappings);
        self.published_plan_count = staged.next_published_plan_count;
        self.observed_plan_count = observed_plan_count;
    }

    fn validate_plan_count(
        &self,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        if plans.len() < self.observed_plan_count {
            return Err(RetainedFamilyExecutionTransportError::PlanSetShrank {
                published: self.observed_plan_count,
                available: plans.len(),
            });
        }
        Ok(())
    }

    fn compact_planned_snapshot(
        &mut self,
        mut envelope: RetainedFamilyExecutionDeltaEnvelope,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let mut remap = HashMap::new();
        for entry in &envelope.family_states {
            if entry.state.family_animation.is_none() {
                continue;
            }
            let core_index = entry.family_plan_index.ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(entry.object),
            )? as usize;
            if core_index >= plans.len() {
                return Err(RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: entry.object,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                }
                .into());
            }
            let next = remap.len() as u32;
            remap.entry(core_index).or_insert(next);
        }
        envelope.family_plans = remap
            .iter()
            .map(|(&index, &wire)| {
                (
                    wire,
                    crate::RetainedFamilyPlanTransport::from_plan(&plans[index]),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        remap_family_state_indices(&mut envelope, &remap, &HashMap::new())?;
        envelope.validate()?;
        self.plan_index_remap = remap;
        self.published_plan_count = self.plan_index_remap.len();
        self.observed_plan_count = plans.len();
        Ok(envelope)
    }
}

fn resolve_external_geometry(
    geometry: &mut GeometryRef,
    resources: &(impl GeometryResourceLookup + ?Sized),
) -> Result<(), RetainedResourceTransportError> {
    let GeometryRef::External(id) = geometry else {
        return Ok(());
    };
    let handle = resources
        .current_handle(*id)
        .ok_or(RetainedResourceTransportError::UnknownGeometryId(*id))?;
    let GeometryResource::VectorPath(path) = resources
        .get(handle)
        .ok_or_else(|| RetainedResourceTransportError::UnknownGeometry(handle.into()))?;
    *geometry = GeometryRef::VectorPath(path.as_ref().clone());
    Ok(())
}

fn remap_family_state_indices(
    envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
    remap: &HashMap<usize, u32>,
    staged: &HashMap<usize, u32>,
) -> Result<(), RetainedFamilyExecutionTransportError> {
    for entry in &mut envelope.family_states {
        let Some(core_index) = entry.family_plan_index else {
            continue;
        };
        entry.family_plan_index = Some(
            remap
                .get(&(core_index as usize))
                .or_else(|| staged.get(&(core_index as usize)))
                .copied()
                .ok_or(RetainedFamilyExecutionTransportError::MissingPlanIndex(
                    entry.object,
                ))?,
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyExecutionEncodeError {
    Retained(RetainedExecutionTransportError),
    Family(RetainedFamilyExecutionTransportError),
}

impl std::fmt::Display for RetainedFamilyExecutionEncodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyExecutionEncodeError {}

impl From<RetainedExecutionTransportError> for RetainedFamilyExecutionEncodeError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedFamilyExecutionTransportError> for RetainedFamilyExecutionEncodeError {
    fn from(value: RetainedFamilyExecutionTransportError) -> Self {
        Self::Family(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationMode, FamilyAnimationState, FontResourceArena, GeometryRef,
        GeometryResourceArena, ObjectContentRef, ObjectId, RateFunction,
        RetainedFamilyAnimationPlanBuilder, SemanticStore, Style, TextResourceArena, Transform2D,
    };
    use noon_runtime::{FrameObjectState, FrameState};

    use super::*;

    fn state(progress: f64) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn fixture() -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let first = noon_runtime::FrameObjectState {
            z_index: 0.0,
            id: ObjectId::new(10),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            transform: noon_core::Transform2D::IDENTITY,
            style: noon_core::Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let second = noon_runtime::FrameObjectState {
            z_index: 0.0,
            id: ObjectId::new(11),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            transform: noon_core::Transform2D::IDENTITY,
            style: noon_core::Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let mut semantics = SemanticStore::new();
        let first_leaf = semantics.insert_authoring_object();
        let second_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, first_leaf).unwrap();
        semantics.add_member(family, second_leaf).unwrap();
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, family).unwrap();
        builder
            .accept_leaf(first_leaf, first.id, &first.content, &texts)
            .unwrap();
        builder
            .accept_leaf(second_leaf, second.id, &second.content, &texts)
            .unwrap();
        let plan = builder.finish().unwrap();

        let frame = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.5,
            objects: vec![
                FrameObjectState {
                    z_index: 0.0,
                    id: first.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    z_index: 0.0,
                    id: second.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        };
        (plan, frame, vec![Some(state(0.5)), Some(state(0.5))])
    }

    #[test]
    fn dense_row_compaction_preserves_changed_content_as_full_row() {
        let (_, mut frame, _) = fixture();
        let template = frame.objects[0].clone();
        frame.objects = (0..128)
            .map(|index| FrameObjectState {
                id: ObjectId::new(index + 1),
                ..template.clone()
            })
            .collect();
        frame.family_animations = vec![None; 128];
        frame.family_animation_plan_indices = vec![None; 128];
        frame.presences = vec![true; 128];
        frame.reveals = vec![1.0; 128];
        frame.morphs = vec![0.0; 128];
        frame.render_geometries = vec![None; 128];
        frame.render_transforms = vec![None; 128];

        let mut base = RetainedExecutionDeltaEncoder::new(1);
        let mut compactor = RetainedFamilyExecutionDeltaEncoder::new(1);
        let wrap = |retained| RetainedFamilyExecutionDeltaEnvelope {
            retained,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: None,
            resource_retirements: RetainedResourceRetirements::default(),
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        };
        let mut snapshot = wrap(
            base.encode_snapshot(&frame, Camera2DState::default())
                .unwrap(),
        );
        compactor.compact_dense_rows(&mut snapshot);
        assert_eq!(snapshot.retained.objects.len(), 128);
        assert!(snapshot.retained.object_patches.is_empty());

        frame.time = 0.5;
        frame.morphs.fill(0.3);
        let mut seed = wrap(
            base.encode_incremental(
                &frame,
                &FrameChanges::objects((0..128).collect()),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap(),
        );
        compactor.compact_dense_rows(&mut seed);
        assert_eq!(seed.retained.objects.len(), 128);
        assert!(seed.retained.object_patches.is_empty());

        frame.time = 0.6;
        frame.morphs.fill(0.4);
        frame.objects[0].content = ObjectContentRef::Geometry(GeometryRef::circle(4.0));
        let full = wrap(
            base.encode_incremental(
                &frame,
                &FrameChanges::objects((0..128).collect()),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap(),
        );
        let full_bytes = serde_json::to_vec(&full).unwrap().len();
        let mut compact = full.clone();
        compactor.compact_dense_rows(&mut compact);
        assert_eq!(compact.retained.objects.len(), 1);
        assert_eq!(compact.retained.object_patches.len(), 127);
        assert!(serde_json::to_vec(&compact).unwrap().len() < full_bytes);
    }

    #[test]
    fn initial_render_geometry_snapshot_carries_one_residency_hint() {
        let (_, mut frame, states) = fixture();
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::ZERO)
                .line_to(noon_core::Vec2::ONE),
        )));
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        let texts = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let images = noon_core::RasterImageResourceArena::new();
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(92);
        let mut snapshot = encoder
            .encode_snapshot(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();
        encoder
            .attach_resource_additions(&mut snapshot, [], &texts, &geometries, &fonts, &images)
            .unwrap();

        let resources = snapshot.resource_additions.unwrap();
        assert_eq!(resources.render_geometry_count(), 1);
        let addition = resources.render_geometry_addition().unwrap().unwrap();
        assert_eq!(addition.session, 92);
        assert_eq!(addition.preparations.len(), 1);

        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::new(2.0, 0.0))
                .line_to(noon_core::Vec2::new(3.0, 1.0)),
        )));
        let mut resnapshot = encoder
            .encode_snapshot(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();
        encoder
            .attach_resource_additions(&mut resnapshot, [], &texts, &geometries, &fonts, &images)
            .unwrap();
        assert!(resnapshot
            .resource_additions
            .as_ref()
            .unwrap()
            .render_geometry_addition()
            .unwrap()
            .unwrap()
            .preparations
            .is_empty());
    }

    #[test]
    fn unique_render_geometry_replacements_reuse_one_wire_slot() {
        let (_, mut frame, _) = fixture();
        let states = [None, None];
        let texts = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let images = noon_core::RasterImageResourceArena::new();
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(92);
        let mut initial = encoder
            .encode_snapshot(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();
        encoder
            .attach_resource_additions(&mut initial, [], &texts, &geometries, &fonts, &images)
            .unwrap();

        for generation in 0..64 {
            frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
                noon_core::VectorPath::new()
                    .move_to(noon_core::Vec2::new(generation as f32, 0.0))
                    .line_to(noon_core::Vec2::new(generation as f32 + 1.0, 1.0)),
            )));
            frame.render_transforms[0] = Some(Transform2D::IDENTITY);
            let mut delta = encoder
                .encode_incremental(
                    &RetainedFamilyFrame {
                        retained: &frame,
                        family_animations: &states,
                    },
                    &[],
                    &FrameChanges::objects(vec![0]),
                    Camera2DState::default(),
                )
                .unwrap()
                .unwrap();
            encoder
                .attach_resource_additions(&mut delta, [], &texts, &geometries, &fonts, &images)
                .unwrap();
            assert_eq!(delta.retained.objects.len(), 1);
            assert_eq!(
                delta.retained.objects[0].render_geometry_resource,
                Some(crate::retained_resource_transport::render_geometry_id(
                    0, generation
                ))
            );
            assert_eq!(
                delta
                    .resource_additions
                    .as_ref()
                    .unwrap()
                    .render_geometry_count(),
                1
            );
            assert!(delta
                .resource_additions
                .as_ref()
                .unwrap()
                .render_geometry_addition()
                .unwrap()
                .unwrap()
                .preparations
                .is_empty());
            assert_eq!(encoder.next_render_geometry_resource, 1);
        }

        frame.render_geometries[0] = None;
        frame.render_transforms[0] = None;
        let mut removal = encoder
            .encode_incremental(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        encoder
            .attach_resource_additions(&mut removal, [], &texts, &geometries, &fonts, &images)
            .unwrap();
        assert_eq!(
            removal
                .resource_additions
                .as_ref()
                .unwrap()
                .render_geometry_count(),
            0
        );
        assert_eq!(encoder.free_render_geometry_resources, vec![0]);

        // A failed unrelated resource capture must leave the retired slot
        // available for the next successfully published geometry.
        let missing_text = noon_core::TextResourceHandle {
            arena: 99,
            id: noon_core::TextResourceId::new(999),
            version: 1,
        };
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::new(100.0, 0.0))
                .line_to(noon_core::Vec2::new(101.0, 1.0)),
        )));
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        frame.objects[1].content = ObjectContentRef::Text(missing_text);
        let mut failed = encoder
            .encode_incremental(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                &FrameChanges::objects(vec![0, 1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        let mut retry = failed.clone();
        let previous_inventory = encoder.resources.clone();
        assert!(encoder
            .attach_resource_additions(
                &mut failed,
                [missing_text],
                &texts,
                &geometries,
                &fonts,
                &images,
            )
            .is_err());
        assert_eq!(encoder.resources, previous_inventory);
        assert_eq!(encoder.free_render_geometry_resources, vec![0]);
        assert!(encoder.render_geometry_resources.is_empty());
        assert_eq!(encoder.render_geometry_generations, vec![64]);

        retry.retained.objects[1].content = crate::TransportObjectContent::Geometry {
            geometry: GeometryRef::circle(2.0),
        };
        encoder
            .attach_resource_additions(&mut retry, [], &texts, &geometries, &fonts, &images)
            .unwrap();
        assert_eq!(
            retry.retained.objects[0].render_geometry_resource,
            Some(crate::retained_resource_transport::render_geometry_id(
                0, 64
            ))
        );
        assert_eq!(encoder.next_render_geometry_resource, 1);
        assert!(encoder.free_render_geometry_resources.is_empty());
    }

    #[test]
    fn one_change_stages_one_entry_with_a_hundred_thousand_live_render_slots() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(92);
        for index in 0..100_000 {
            encoder.render_geometry_resources.insert(
                crate::TransportSlotId {
                    slot: index,
                    generation: 0,
                },
                crate::retained_resource_transport::render_geometry_id(index, 0),
            );
        }
        encoder.render_geometry_generations = vec![0; 100_000];
        encoder.next_render_geometry_resource = 100_000;

        let slot = crate::TransportSlotId {
            slot: 42,
            generation: 0,
        };
        let mut staged = StagedRenderGeometryResources::new(&encoder);
        assert_eq!(
            staged.publish(slot).unwrap(),
            crate::retained_resource_transport::render_geometry_id(42, 1)
        );
        let changes = staged.finish();
        assert_eq!(changes.touched_slots(), 1);
        assert_eq!(changes.generation_changes.len(), 1);
        assert_eq!(changes.reserved_free, 0);
        assert!(changes.newly_free.is_empty());
        changes.commit(&mut encoder);
        assert_eq!(encoder.render_geometry_resources.len(), 100_000);
        assert_eq!(encoder.render_geometry_generations[42], 1);
        assert_eq!(encoder.next_render_geometry_resource, 100_000);
    }

    #[test]
    fn retired_text_closures_reintroduce_complete_payloads_without_inventory_growth() {
        let mut scene = noon::Scene::new();
        let first = scene
            .typst(noon::Typst::new("#line(length: 10pt) A"))
            .unwrap();
        let second = scene
            .typst(noon::Typst::new("#line(length: 10pt) B"))
            .unwrap();
        let first = first.state().unwrap().content.text().unwrap();
        let second = second.state().unwrap().content.text().unwrap();
        let owner = scene.integration_store();
        let store = owner.borrow();
        let images = noon_core::RasterImageResourceArena::new();
        let resources = RetainedResourceBundle::capture(
            [first, second],
            store.text_resources(),
            store.geometry_resources(),
            store.font_resources(),
        )
        .unwrap();
        assert!(resources.font_bytes() > 0);
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new_with_resources(47, &resources);
        assert_eq!(encoder.text_closures.len(), 2);
        assert!(!encoder.geometry_references.is_empty());
        assert!(!encoder.font_references.is_empty());
        let (_, mut frame, _) = fixture();
        frame.objects[0].content = ObjectContentRef::Text(first);
        frame.objects[1].content = ObjectContentRef::Text(second);
        let states = [None, None];
        let mut initial = encoder
            .encode_snapshot(
                &RetainedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();
        encoder
            .attach_resource_additions(
                &mut initial,
                [first, second],
                store.text_resources(),
                store.geometry_resources(),
                store.font_resources(),
                &images,
            )
            .unwrap();
        assert!(initial.resource_additions.is_none());

        for index in 0..2 {
            frame.objects[index].content = ObjectContentRef::Geometry(GeometryRef::circle(1.0));
            let mut delta = encoder
                .encode_incremental(
                    &RetainedFamilyFrame {
                        retained: &frame,
                        family_animations: &states,
                    },
                    &[],
                    &FrameChanges::objects(vec![index]),
                    Camera2DState::default(),
                )
                .unwrap()
                .unwrap();
            encoder
                .attach_resource_additions(
                    &mut delta,
                    [],
                    store.text_resources(),
                    store.geometry_resources(),
                    store.font_resources(),
                    &images,
                )
                .unwrap();
            assert_eq!(delta.resource_retirements.texts.len(), 1);
            if index == 0 {
                assert_eq!(encoder.text_closures.len(), 1);
                assert!(
                    !encoder.font_references.is_empty(),
                    "second text owns shared fonts"
                );
            }
        }
        assert_eq!(encoder.resources.counts(), [0; 4]);
        assert!(encoder.text_closures.is_empty());
        assert!(encoder.geometry_references.is_empty());
        assert!(encoder.font_references.is_empty());

        for cycle in 0..32 {
            frame.objects[0].content = ObjectContentRef::Text(first);
            let mut delta = encoder
                .encode_incremental(
                    &RetainedFamilyFrame {
                        retained: &frame,
                        family_animations: &states,
                    },
                    &[],
                    &FrameChanges::objects(vec![0]),
                    Camera2DState::default(),
                )
                .unwrap()
                .unwrap();
            // Failed capture leaves producer residency unchanged and permits exact retry.
            assert!(encoder
                .attach_resource_additions(
                    &mut delta,
                    [first],
                    &TextResourceArena::new(),
                    store.geometry_resources(),
                    store.font_resources(),
                    &images,
                )
                .is_err());
            assert_eq!(encoder.resources.counts(), [0; 4]);
            assert!(encoder.resource_roots.is_empty());
            encoder
                .attach_resource_additions(
                    &mut delta,
                    [first],
                    store.text_resources(),
                    store.geometry_resources(),
                    store.font_resources(),
                    &images,
                )
                .unwrap();
            let additions = delta.resource_additions.as_ref().unwrap();
            assert!(
                additions.font_bytes() > 0,
                "cycle {cycle} must resend retired fonts"
            );
            let installed = additions.clone().install().unwrap();
            assert!(installed
                .resolve_text_handle(TransportTextResourceHandle::from_source_handle(first))
                .is_some());
            assert_eq!(encoder.text_closures.len(), 1);

            frame.objects[0].content = ObjectContentRef::Geometry(GeometryRef::circle(1.0));
            let mut removal = encoder
                .encode_incremental(
                    &RetainedFamilyFrame {
                        retained: &frame,
                        family_animations: &states,
                    },
                    &[],
                    &FrameChanges::objects(vec![0]),
                    Camera2DState::default(),
                )
                .unwrap()
                .unwrap();
            encoder
                .attach_resource_additions(
                    &mut removal,
                    [],
                    store.text_resources(),
                    store.geometry_resources(),
                    store.font_resources(),
                    &images,
                )
                .unwrap();
            assert_eq!(encoder.resources.counts(), [0; 4]);
            assert!(encoder.text_closures.is_empty());
            assert!(encoder.geometry_references.is_empty());
            assert!(encoder.font_references.is_empty());
            assert!(encoder.resource_roots.is_empty());
        }
    }

    #[test]
    fn resource_backed_encoder_starts_after_installed_render_geometry_prefix() {
        let texts = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let mut resources =
            RetainedResourceBundle::capture([], &texts, &geometries, &fonts).unwrap();
        resources.set_render_geometries(
            29,
            vec![Arc::new(GeometryRef::circle(1.0))].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );

        let encoder = RetainedFamilyExecutionDeltaEncoder::new_with_resources(29, &resources);
        assert_eq!(encoder.next_render_geometry_resource, 1);
    }

    #[test]
    fn snapshot_and_incremental_share_base_sequence_and_sparse_family_indices() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(17);

        let snapshot = encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert!(snapshot.retained.snapshot);
        assert_eq!(snapshot.retained.sequence, 0);
        assert_eq!(snapshot.family_plans.len(), 1);
        assert_eq!(snapshot.family_states.len(), 2);

        let incremental = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(!incremental.retained.snapshot);
        assert_eq!(incremental.retained.sequence, 1);
        assert!(incremental.family_plans.is_empty());
        assert_eq!(incremental.family_states.len(), 1);
        assert_eq!(incremental.family_states[0].object, ObjectId::new(11));
    }

    #[test]
    fn planned_encoder_carries_sparse_plan_identity() {
        let (plan, frame, states) = fixture();
        let plan_indices = [Some(0), Some(0)];
        let family = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &plan_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(19);
        let snapshot = encoder
            .encode_planned_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert_eq!(snapshot.family_states[0].family_plan_index, Some(0));
        assert_eq!(snapshot.family_states[1].family_plan_index, Some(0));

        let incremental = encoder
            .encode_planned_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(incremental.family_states.len(), 1);
        assert_eq!(incremental.family_states[0].family_plan_index, Some(0));
    }

    #[test]
    fn planned_encoder_publishes_only_the_new_plan_suffix() {
        let (plan, frame, states) = fixture();
        let initial_indices = [Some(0), Some(0)];
        let initial = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &initial_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(20);
        encoder
            .encode_planned_snapshot(
                &initial,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        let plans = [plan.clone(), plan];
        let appended_indices = [Some(1), Some(0)];
        let appended = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &appended_indices,
        };
        let delta = encoder
            .encode_planned_incremental(
                &appended,
                &plans,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(!delta.retained.snapshot);
        assert_eq!(delta.family_plans.len(), 1);
        assert_eq!(delta.family_states.len(), 1);
        assert_eq!(delta.family_states[0].family_plan_index, Some(1));
    }

    #[test]
    fn sparse_new_rows_publish_active_family_state_with_appended_plan() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(24);
        let empty = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: Vec::new(),
            presences: Vec::new(),
            reveals: Vec::new(),
            morphs: Vec::new(),
            render_geometries: Vec::new(),
            render_transforms: Vec::new(),
        };
        encoder
            .encode_planned_snapshot(
                &RetainedPlannedFamilyFrame {
                    retained: &empty,
                    family_animations: &[],
                    family_plan_indices: &[],
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();

        let (plan, frame, states) = fixture();
        let plan_indices = [Some(0), Some(0)];
        let delta = encoder
            .encode_planned_incremental_with_painter_order(
                &RetainedPlannedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                    family_plan_indices: &plan_indices,
                },
                &[plan],
                &FrameChanges::with_structure(vec![0, 1], vec![0, 1], Vec::new())
                    .with_painter_order(0..2),
                Camera2DState::default(),
                &[0, 1],
            )
            .unwrap()
            .unwrap();

        assert!(!delta.retained.snapshot);
        assert_eq!(delta.retained.objects.len(), 2);
        assert_eq!(delta.family_plans.len(), 1);
        assert_eq!(delta.family_states.len(), 2);
        assert!(delta
            .family_states
            .iter()
            .all(|state| state.family_plan_index == Some(0)));
    }

    #[test]
    fn created_then_removed_rows_do_not_publish_unknown_family_objects() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(25);
        let (_, frame, _) = fixture();
        let states = [None, None];
        let planned = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &[None, None],
        };
        encoder
            .encode_planned_snapshot_indices(&planned, &[], Camera2DState::default(), [0])
            .unwrap();
        let delta = encoder
            .encode_planned_incremental_with_painter_order(
                &planned,
                &[],
                &FrameChanges::with_structure(vec![1], vec![1], vec![1]).with_painter_order(1..1),
                Camera2DState::default(),
                &[0],
            )
            .unwrap()
            .unwrap();
        assert!(!delta.retained.snapshot);
        assert!(delta.retained.objects.is_empty());
        assert!(delta.retained.removed_slots.is_empty());
        assert!(delta.family_states.is_empty());
        assert!(delta.family_plans.is_empty());
    }

    #[test]
    fn incremental_plan_mapping_stays_sparse_across_unpublished_history() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(25);
        let empty = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: Vec::new(),
            presences: Vec::new(),
            reveals: Vec::new(),
            morphs: Vec::new(),
            render_geometries: Vec::new(),
            render_transforms: Vec::new(),
        };
        encoder
            .encode_planned_snapshot(
                &RetainedPlannedFamilyFrame {
                    retained: &empty,
                    family_animations: &[],
                    family_plan_indices: &[],
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();

        let (plan, frame, states) = fixture();
        let plans = vec![plan; 1_024];
        let plan_indices = [Some(1_023), Some(1_023)];
        let delta = encoder
            .encode_planned_incremental_with_painter_order(
                &RetainedPlannedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                    family_plan_indices: &plan_indices,
                },
                &plans,
                &FrameChanges::with_structure(vec![0, 1], vec![0, 1], Vec::new())
                    .with_painter_order(0..2),
                Camera2DState::default(),
                &[0, 1],
            )
            .unwrap()
            .unwrap();

        assert_eq!(delta.family_plans.len(), 1);
        assert!(delta
            .family_states
            .iter()
            .all(|state| state.family_plan_index == Some(0)));
        assert_eq!(encoder.plan_index_remap.len(), 1);
        assert_eq!(encoder.published_plan_count, 1);
        assert_eq!(encoder.observed_plan_count, plans.len());
    }

    #[test]
    fn planned_snapshot_omits_inactive_historical_plan_descriptors() {
        let (plan, frame, _) = fixture();
        let states = [None, None];
        let plan_indices = [None, None];
        let inactive = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &plan_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(21);
        let snapshot = encoder
            .encode_planned_snapshot(
                &inactive,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert!(snapshot.family_plans.is_empty());
        assert!(snapshot
            .family_states
            .iter()
            .all(|entry| entry.family_plan_index.is_none()));
    }

    #[test]
    fn empty_incremental_does_not_consume_sequence() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(23);
        encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        assert!(encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::default(),
                Camera2DState::default(),
            )
            .unwrap()
            .is_none());
        let next = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(next.retained.sequence, 1);
    }

    #[test]
    fn all_changes_promote_to_snapshot_and_reinstall_plan() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(31);
        encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        let replacement = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::all(),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(replacement.retained.snapshot);
        assert_eq!(replacement.retained.sequence, 1);
        assert_eq!(replacement.family_plans.len(), 1);
        assert_eq!(replacement.family_states.len(), 2);
    }
}
