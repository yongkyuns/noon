use crate::retained_resource_transport::residency::{ResourceResidency, ResourceRoot};
use crate::retained_resource_transport::{render_geometry_id, RenderGeometrySlot};
use std::collections::HashMap;

use noon_core::{Camera2DState, RetainedFamilyAnimationPlan};
use noon_runtime::{FrameChanges, FrameState, RetainedFamilyFrame, RetainedPlannedFamilyFrame};

use crate::{
    InstalledRetainedFamilyExecutionState, InstalledRetainedResources,
    PreparedInstalledFamilyUpdate, RetainedExecutionDeltaEnvelope, RetainedExecutionFrameMirror,
    RetainedExecutionTransportError, RetainedFamilyExecutionDeltaEnvelope,
    RetainedFamilyExecutionTransportError, RetainedResourceBundle, RetainedResourceTransportError,
    RetainedTransportApplyOutcome, TransportObjectContent,
};

/// Render-side owner of one resolved retained frame and its installed resources.
/// The transport mirror resolves handles before committing a delta, so its frame
/// is also the frame consumed by family execution and the renderer.
#[derive(Clone, Debug)]
pub struct InstalledRetainedExecutionMirror {
    wire: RetainedExecutionFrameMirror,
    resources: InstalledRetainedResources,
    family: InstalledRetainedFamilyExecutionState,
    transient_presentations: Vec<noon_runtime::TransientPresentationOccurrence>,
    resource_roots: ResourceResidency,
}

impl InstalledRetainedExecutionMirror {
    pub fn from_bundle_bytes(bytes: &[u8]) -> Result<Self, InstalledExecutionError> {
        let resources = RetainedResourceBundle::decode_binary(bytes)?.install()?;
        let mut wire = RetainedExecutionFrameMirror::with_installed_resources(
            resources.render_geometry_session(),
            resources.render_geometries(),
            resources.text_handle_remap(),
        );
        wire.extend_installed_image_handles(&resources.image_handle_remap());
        wire.extend_installed_geometry_handles(&resources.geometry_handle_remap());
        Ok(Self {
            wire,
            resources,
            family: InstalledRetainedFamilyExecutionState::default(),
            transient_presentations: Vec::new(),
            resource_roots: ResourceResidency::default(),
        })
    }

    pub fn resources(&self) -> &InstalledRetainedResources {
        &self.resources
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn transport_mirror(&self) -> &RetainedExecutionFrameMirror {
        &self.wire
    }

    pub fn frame(&self) -> Option<&FrameState> {
        self.wire.frame()
    }

    pub fn painter_order(&self) -> &[u32] {
        self.wire.painter_order()
    }

    /// Reborrow the validated, installed worker frame as the same renderer
    /// publication contract consumed by direct native/WASM hosts. No second
    /// resource state or transport-to-scene re-lowering is created here.
    pub fn renderer_publication(
        &self,
        changes: &FrameChanges,
    ) -> Result<noon_runtime::RendererPublication<'_>, InstalledExecutionError> {
        let frame = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
        Ok(
            noon_runtime::RendererPublication::from_validated_transport_frame(
                self.wire.publication_context(),
                frame,
                changes.clone(),
                self.resources.texts(),
                self.resources.fonts(),
                self.resources.geometries(),
                self.resources.images(),
                self.family.plans(),
                self.family.active_indices(),
                self.wire.painter_order(),
            ),
        )
    }

    pub fn family_frame(&self) -> Result<Option<RetainedFamilyFrame<'_>>, InstalledExecutionError> {
        if self.family.plans().is_empty() {
            return Ok(None);
        }
        let frame = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
        Ok(Some(self.family.frame(frame)?))
    }

    pub fn planned_family_frame(
        &self,
    ) -> Result<Option<RetainedPlannedFamilyFrame<'_>>, InstalledExecutionError> {
        if self.family.plans().is_empty() {
            return Ok(None);
        }
        let frame = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
        Ok(Some(self.family.planned_frame(frame)?))
    }

    pub fn family_plans(&self) -> &[RetainedFamilyAnimationPlan] {
        self.family.plans()
    }

    pub fn active_family_animation_indices(&self) -> &std::collections::BTreeSet<usize> {
        self.family.active_indices()
    }

    pub fn transient_presentations(&self) -> &[noon_runtime::TransientPresentationOccurrence] {
        &self.transient_presentations
    }

    pub fn family_plan(
        &self,
    ) -> Result<Option<&RetainedFamilyAnimationPlan>, InstalledExecutionError> {
        Ok(self.family.single_plan()?)
    }

    pub const fn camera(&self) -> Camera2DState {
        self.wire.camera()
    }

    /// Exact source execution context carried by the currently installed frame.
    pub const fn publication_context(&self) -> noon_core::PublicationContext {
        self.wire.publication_context()
    }

    /// Camera3D is resolved from the transported effective object row. The 2D
    /// `camera()` remains the inspection/HUD camera only.
    pub fn camera_3d_object(&self) -> Option<&noon_runtime::FrameObjectState> {
        self.wire.camera_3d_object()
    }

    pub fn inset_2d_views(&self) -> &[noon_core::Inset2DViewState] {
        self.wire.inset_2d_views()
    }

    pub fn apply_json(
        &mut self,
        json: &str,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let delta: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(json)?;
        self.apply_family(delta)
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        self.apply_retained(delta, true)
    }

    fn apply_retained(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
        validate_snapshot_resources: bool,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let snapshot = delta.snapshot;
        let staged_roots = self.resource_roots.stage(&delta);
        if snapshot && validate_snapshot_resources {
            self.validate_snapshot_resources(&delta)?;
        }

        let (outcome, changes) = self.wire.apply(delta)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }

        self.resource_roots.commit(staged_roots);
        if snapshot {
            self.family = InstalledRetainedFamilyExecutionState::default();
            self.transient_presentations.clear();
        }
        Ok((outcome, changes))
    }

    pub fn apply_family(
        &mut self,
        mut delta: RetainedFamilyExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        if self.wire.session() == Some(delta.retained.session)
            && self
                .wire
                .applied_sequence()
                .is_some_and(|sequence| delta.retained.sequence <= sequence)
        {
            return Ok(self.wire.apply(delta.retained)?);
        }
        delta.validate()?;
        self.validate_resource_retirements(&delta)?;
        if let Some(bundle) = delta.resource_additions.take() {
            return self.apply_family_with_resource_additions(delta, bundle);
        }
        if delta.retained.snapshot {
            self.validate_snapshot_resources(&delta.retained)?;
        }
        let prepared_family = self.prepare_family_update(&delta, self.resources.texts())?;
        let prepared_transient = self.prepare_transient_presentations(&delta)?;

        let retirements = delta.resource_retirements.clone();
        let (outcome, changes) = self.apply(delta.retained)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }
        self.resources.retire(&retirements);
        self.wire
            .remove_installed_image_handles(retirements.images.iter());
        self.wire
            .remove_installed_text_handles(retirements.texts.iter());
        self.wire
            .remove_installed_geometry_handles(retirements.geometries.iter());
        self.family.commit_prepared(prepared_family);
        self.transient_presentations = prepared_transient;
        Ok((outcome, changes))
    }

    fn apply_family_with_resource_additions(
        &mut self,
        delta: RetainedFamilyExecutionDeltaEnvelope,
        bundle: RetainedResourceBundle,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let additions = self.resources.prepare_additions_with_render(bundle)?;
        self.validate_render_geometry_replacements(
            &delta.retained,
            additions.render_geometry_updates(),
        )?;
        let image_handles = additions.image_handle_remap();
        self.wire.extend_installed_image_handles(&image_handles);
        let text_handles = additions.text_handle_remap();
        let superseded_text_handles = additions.superseded_text_handles().to_vec();
        self.wire.extend_installed_text_handles(&text_handles);
        let geometry_handles = additions.geometry_handle_remap();
        self.wire
            .extend_installed_geometry_handles(&geometry_handles);

        let render_rollback = match (
            additions.render_geometry_session(),
            additions.render_geometry_updates(),
        ) {
            (Some(session), updates) => {
                match self
                    .wire
                    .stage_installed_render_geometries(session, updates)
                {
                    Ok(rollback) => Some(rollback),
                    Err(error) => {
                        self.wire
                            .remove_installed_image_handles(image_handles.keys());
                        self.wire.remove_installed_text_handles(text_handles.keys());
                        self.wire
                            .remove_installed_geometry_handles(geometry_handles.keys());
                        return Err(error.into());
                    }
                }
            }
            (None, _) => None,
        };

        let text_lookup = additions.text_lookup(&self.resources);
        let prepared_family = match self.prepare_family_update(&delta, &text_lookup) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.wire
                    .remove_installed_image_handles(image_handles.keys());
                self.wire.remove_installed_text_handles(text_handles.keys());
                self.wire
                    .remove_installed_geometry_handles(geometry_handles.keys());
                if let Some(rollback) = render_rollback {
                    self.wire.rollback_installed_render_geometries(rollback);
                }
                return Err(error);
            }
        };
        let prepared_transient = match self.prepare_transient_presentations(&delta) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.wire
                    .remove_installed_image_handles(image_handles.keys());
                self.wire.remove_installed_text_handles(text_handles.keys());
                self.wire
                    .remove_installed_geometry_handles(geometry_handles.keys());
                if let Some(rollback) = render_rollback {
                    self.wire.rollback_installed_render_geometries(rollback);
                }
                return Err(error);
            }
        };
        let retirements = delta.resource_retirements.clone();
        let applied = self.apply_retained(delta.retained, false);
        let (outcome, changes) = match applied {
            Ok(applied) => applied,
            Err(error) => {
                self.wire
                    .remove_installed_image_handles(image_handles.keys());
                self.wire.remove_installed_text_handles(text_handles.keys());
                self.wire
                    .remove_installed_geometry_handles(geometry_handles.keys());
                if let Some(rollback) = render_rollback {
                    self.wire.rollback_installed_render_geometries(rollback);
                }
                return Err(error);
            }
        };
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            self.wire
                .remove_installed_image_handles(image_handles.keys());
            self.wire.remove_installed_text_handles(text_handles.keys());
            self.wire
                .remove_installed_geometry_handles(geometry_handles.keys());
            if let Some(rollback) = render_rollback {
                self.wire.rollback_installed_render_geometries(rollback);
            }
            return Ok((outcome, changes));
        }

        self.resources.commit_additions_with_render(additions);
        self.resources.retire(&retirements);
        self.wire
            .remove_installed_image_handles(retirements.images.iter());
        self.wire
            .remove_installed_text_handles(retirements.texts.iter());
        self.wire
            .remove_installed_geometry_handles(retirements.geometries.iter());
        self.wire
            .remove_installed_text_handles(superseded_text_handles.iter());
        self.family.commit_prepared(prepared_family);
        self.transient_presentations = prepared_transient;
        Ok((outcome, changes))
    }

    fn validate_resource_retirements(
        &self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
    ) -> Result<(), InstalledExecutionError> {
        let staged = self.resource_roots.stage(&delta.retained);
        for image in &delta.resource_retirements.images {
            if self
                .resource_roots
                .references_after(&staged, ResourceRoot::Image(*image))
                > 0
            {
                return Err(RetainedResourceTransportError::RetiredLiveImage(*image).into());
            }
        }
        for text in &delta.resource_retirements.texts {
            if self
                .resource_roots
                .references_after(&staged, ResourceRoot::Text(*text))
                > 0
            {
                return Err(RetainedResourceTransportError::RetiredLiveText(*text).into());
            }
        }
        for geometry in &delta.resource_retirements.geometries {
            if self
                .resource_roots
                .references_after(&staged, ResourceRoot::Geometry(*geometry))
                > 0
            {
                return Err(RetainedResourceTransportError::RetiredLiveGeometry(*geometry).into());
            }
        }
        Ok(())
    }

    fn validate_render_geometry_replacements(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
        updates: &[(u32, RenderGeometrySlot)],
    ) -> Result<(), InstalledExecutionError> {
        let staged_roots = self.resource_roots.stage(delta);
        for (slot, _) in updates {
            let Some(previous) = self.resources.render_geometries().get(*slot as usize) else {
                continue;
            };
            if previous.geometry.is_some() {
                let old_id = render_geometry_id(*slot, previous.generation);
                if self
                    .resource_roots
                    .references_after(&staged_roots, ResourceRoot::RenderGeometry(old_id))
                    > 0
                {
                    return Err(
                        RetainedExecutionTransportError::InvalidRenderGeometryResource(old_id)
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn prepare_transient_presentations(
        &self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
    ) -> Result<Vec<noon_runtime::TransientPresentationOccurrence>, InstalledExecutionError> {
        let snapshot = delta.retained.snapshot;
        let mut next_indices = HashMap::with_capacity(delta.retained.objects.len());
        let mut next_row = if snapshot {
            0
        } else {
            self.wire
                .frame()
                .ok_or(InstalledExecutionError::MissingResolvedFrame)?
                .objects
                .len()
        };
        for object in &delta.retained.objects {
            let index = if snapshot {
                let index = object.order as usize;
                next_row = next_row.max(index + 1);
                index
            } else if let Some(index) = self.wire.frame_index_for_slot(object.slot) {
                index
            } else {
                let index = next_row;
                next_row += 1;
                index
            };
            next_indices.insert(object.object, index);
        }
        delta
            .transient_presentations
            .iter()
            .map(|object| {
                let index = next_indices
                    .get(&object.anchor)
                    .copied()
                    .or_else(|| {
                        (!snapshot)
                            .then(|| self.wire.frame_index_for_object(object.anchor))
                            .flatten()
                    })
                    .ok_or({
                        InstalledExecutionError::Family(
                            crate::RetainedFamilyExecutionTransportError::UnknownTransientAnchor(
                                object.anchor,
                            ),
                        )
                    })?;
                let anchor = u32::try_from(index).map_err(|_| {
                    InstalledExecutionError::Family(
                        crate::RetainedFamilyExecutionTransportError::InvalidTransientAnchorIndex(
                            index,
                        ),
                    )
                })?;
                Ok(object.install(anchor))
            })
            .collect()
    }

    fn prepare_family_update(
        &self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
        texts: &(impl noon_core::TextResourceLookup + ?Sized),
    ) -> Result<PreparedInstalledFamilyUpdate, InstalledExecutionError> {
        let current = self.wire.frame();
        let mut changed_objects = HashMap::with_capacity(delta.retained.objects.len());
        let mut next_indices = HashMap::with_capacity(delta.retained.objects.len());
        let added_plan_objects = delta
            .family_plans
            .iter()
            .flat_map(|plan| plan.bindings.iter().map(|binding| binding.object))
            .collect::<std::collections::HashSet<_>>();
        let mut next_row = if delta.retained.snapshot {
            0
        } else {
            current
                .ok_or(InstalledExecutionError::MissingResolvedFrame)?
                .objects
                .len()
        };
        for object in &delta.retained.objects {
            let index = if delta.retained.snapshot {
                let index = object.order as usize;
                next_row = next_row.max(index + 1);
                index
            } else if let Some(index) = self.wire.frame_index_for_slot(object.slot) {
                index
            } else {
                let index = next_row;
                next_row += 1;
                index
            };
            next_indices.insert(object.object, index);
            if added_plan_objects.contains(&object.object) {
                changed_objects.insert(
                    object.object,
                    self.wire.resolve_transport_object_state(object)?,
                );
            }
        }
        let frame_len = if delta.retained.snapshot {
            delta.retained.objects.len()
        } else {
            next_row
        };
        let snapshot = delta.retained.snapshot;
        Ok(self.family.prepare_with_lookup(
            delta,
            frame_len,
            texts,
            |object| {
                next_indices.get(&object).copied().or_else(|| {
                    (!snapshot)
                        .then(|| self.wire.frame_index_for_object(object))
                        .flatten()
                })
            },
            |object| {
                changed_objects.get(&object).or_else(|| {
                    (!snapshot)
                        .then(|| {
                            let index = self.wire.frame_index_for_object(object)?;
                            current?.objects.get(index)
                        })
                        .flatten()
                })
            },
        )?)
    }

    fn validate_snapshot_resources(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), InstalledExecutionError> {
        for object in &delta.objects {
            if let TransportObjectContent::Image { image, sampling } = object.content {
                if self
                    .resources
                    .resolve_image_handle(image, sampling)
                    .is_none()
                {
                    return Err(RetainedExecutionTransportError::UnknownImageResource(image).into());
                }
            }
            if let TransportObjectContent::Text { text } = object.content {
                if self.resources.resolve_text_handle(text).is_none() {
                    return Err(InstalledExecutionError::UnknownTextResource {
                        id: text.id,
                        version: text.version,
                    });
                }
            }
            if let TransportObjectContent::Geometry {
                resource: Some(geometry),
                ..
            } = object.content
            {
                if !self
                    .resources
                    .geometry_handle_remap()
                    .contains_key(&geometry)
                {
                    return Err(
                        RetainedExecutionTransportError::UnknownGeometryResource(geometry).into(),
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstalledExecutionError {
    Resource(RetainedResourceTransportError),
    Transport(RetainedExecutionTransportError),
    Family(RetainedFamilyExecutionTransportError),
    Json(String),
    UnknownTextResource { id: u64, version: u64 },
    MissingResolvedFrame,
}

impl std::fmt::Display for InstalledExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
            Self::Json(error) => write!(formatter, "invalid retained execution JSON: {error}"),
            Self::UnknownTextResource { id, version } => {
                write!(formatter, "unknown installed text resource {id}@{version}")
            }
            Self::MissingResolvedFrame => {
                formatter.write_str("retained execution has no renderer-local frame")
            }
        }
    }
}

impl std::error::Error for InstalledExecutionError {}

impl From<RetainedResourceTransportError> for InstalledExecutionError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

impl From<RetainedExecutionTransportError> for InstalledExecutionError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<RetainedFamilyExecutionTransportError> for InstalledExecutionError {
    fn from(value: RetainedFamilyExecutionTransportError) -> Self {
        Self::Family(value)
    }
}

impl From<serde_json::Error> for InstalledExecutionError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationMode, FamilyAnimationState, GeometryRef, MeshResource, ObjectId,
        RateFunction, Transform2D, Vec2, VectorPath,
    };

    use super::*;
    use crate::{
        RetainedFamilyExecutionObjectState, RetainedFamilyPlanTransport, SemanticExecutionPlayer,
    };

    fn engine() -> SemanticExecutionPlayer {
        let mut scene = noon::Scene::new();
        let hello = scene
            .text(noon::Text::new("Hello").with_font_size(64.0))
            .unwrap();
        let world = scene
            .text(noon::Text::new("World").with_font_size(72.0))
            .unwrap();
        scene.add_many(&[(&hello).into(), (&world).into()]).unwrap();
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 4.0, 17).unwrap()
    }

    fn geometry_engine() -> SemanticExecutionPlayer {
        let mut scene = noon::Scene::new();
        let first = scene.circle(1.0).unwrap();
        let second = scene.rectangle(1.0, 1.0).unwrap();
        scene
            .add_many(&[(&first).into(), (&second).into()])
            .unwrap();
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 4.0, 17).unwrap()
    }

    #[test]
    fn validated_worker_publication_borrows_exact_installed_frame_and_context() {
        let mut source = geometry_engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&source.resource_bundle_bytes())
                .unwrap();
        assert!(matches!(
            mirror.renderer_publication(&FrameChanges::all()),
            Err(InstalledExecutionError::MissingResolvedFrame)
        ));
        let (outcome, changes) = mirror
            .apply_json(&source.initial_delta_json().unwrap())
            .unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        let publication = mirror.renderer_publication(&changes).unwrap();
        assert_eq!(publication.frame(), mirror.frame().unwrap());
        assert_eq!(publication.context(), mirror.publication_context());
        assert_eq!(publication.painter_order(), mirror.painter_order());
        assert_eq!(publication.changes(), &changes);
        assert!(publication.active_family_animation_indices().is_empty());
        assert_eq!(publication.family_animation_plans().len(), 0);
    }

    #[test]
    fn actual_surface_player_binds_mesh_before_worker_install_and_keeps_motion_local() {
        let session = noon::example_scenes::spatial_surface::session().unwrap();
        let mut player = SemanticExecutionPlayer::from_session(session, 2.0, 81).unwrap();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
                .unwrap();
        let initial = player.initial_delta_json().unwrap();
        let envelope: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial).unwrap();
        let mesh_row = envelope
            .retained
            .objects
            .iter()
            .find(|row| {
                matches!(
                    row.content,
                    TransportObjectContent::Geometry {
                        geometry: GeometryRef::External(_),
                        ..
                    }
                )
            })
            .unwrap();
        let mesh_id = mesh_row.object;
        let TransportObjectContent::Geometry {
            resource: Some(source),
            ..
        } = mesh_row.content
        else {
            panic!("mesh source must be version-qualified before publication");
        };
        mirror.apply_json(&initial).unwrap();
        let local = mirror.resources().geometry_handle_remap()[&source];
        let resource = mirror.resources().geometries().get(local).unwrap();
        let noon_core::GeometryResource::Mesh(mesh) = resource else {
            panic!("mesh payload must stay an indexed mesh");
        };
        let retained = std::sync::Arc::clone(mesh);
        player.tick_delta_json(0.0).unwrap();
        let midpoint = player.tick_delta_json(500.0).unwrap().unwrap();
        let update: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(&midpoint).unwrap();
        assert_eq!(update.retained.objects.len(), 1);
        assert_eq!(update.retained.objects[0].object, mesh_id);
        assert!(update.resource_additions.is_none());
        mirror.apply_json(&midpoint).unwrap();
        let current = mirror
            .frame()
            .unwrap()
            .objects
            .iter()
            .find(|row| row.id == mesh_id)
            .unwrap();
        let angle = noon_core::SemanticRotation3D::from_axis_angle(
            noon_core::SemanticVec3::new(0.0, 0.0, 1.0),
            0.3,
        )
        .unwrap();
        for (actual, expected) in current
            .world_transform()
            .unwrap()
            .rotation
            .components()
            .into_iter()
            .zip(angle.components())
        {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        let noon_core::GeometryResource::Mesh(mesh) =
            mirror.resources().geometries().get(local).unwrap()
        else {
            panic!("motion must preserve mesh resource");
        };
        assert!(std::sync::Arc::ptr_eq(&retained, mesh));

        let mut invalid = envelope;
        let row = invalid
            .retained
            .objects
            .iter_mut()
            .find(|row| row.object == mesh_id)
            .unwrap();
        let TransportObjectContent::Geometry { resource, .. } = &mut row.content else {
            unreachable!()
        };
        *resource = None;
        let mut fresh =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
                .unwrap();
        assert!(
            fresh.apply_family(invalid).is_err(),
            "wire reference validation remains strict"
        );
        assert!(fresh.frame().is_none());
    }

    #[test]
    fn mesh_payload_is_resolved_by_exact_transport_handle_into_worker_frame() {
        let mut source_geometries = noon_core::GeometryResourceArena::new();
        let source = source_geometries.insert_mesh(
            MeshResource::new(
                vec![
                    noon_core::SemanticVec3::new(0.25, 0.5, -3.0),
                    noon_core::SemanticVec3::new(1.0, 0.0, 4.0),
                    noon_core::SemanticVec3::new(-2.0, 1.0, 1.5),
                ],
                None,
                vec![0, 1, 2],
            )
            .unwrap(),
        );
        let resources = RetainedResourceBundle::capture_additions_with_geometries(
            [],
            [source],
            &noon_core::TextResourceArena::new(),
            &source_geometries,
            &noon_core::FontResourceArena::new(),
            &crate::RetainedResourceInventory::default(),
        )
        .unwrap();
        let transport = crate::TransportGeometryResourceHandle::from(source);
        {
            let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(
                &resources.encode_binary().unwrap(),
            )
            .unwrap();
            let local_handle = mirror.resources.geometry_handle_remap()[&transport];
            let source_context = noon_core::PublicationContext::new(
                noon_core::SceneRevision::new(44),
                noon_core::ExecutionRevision::new(12),
                noon_core::FrameEpoch::new(7),
            );
            let delta = RetainedExecutionDeltaEnvelope {
                channel: crate::RETAINED_EXECUTION_TRANSPORT_CHANNEL.into(),
                protocol_version: crate::RETAINED_EXECUTION_TRANSPORT_VERSION,
                session: 77,
                sequence: 0,
                publication_context: source_context,
                snapshot: true,
                time: 0.0,
                camera: Camera2DState::default(),
                inset_2d_views: Vec::new(),
                objects: vec![crate::RetainedTransportObjectState {
                    slot: crate::TransportSlotId {
                        slot: 0,
                        generation: 0,
                    },
                    order: 0,
                    object: ObjectId::new(90),
                    glow: None,
                    z_index: 0.0,
                    content: TransportObjectContent::Geometry {
                        geometry: GeometryRef::External(source.id),
                        resource: Some(transport),
                    },
                    transform: Transform2D::IDENTITY,
                    spatial: Some(crate::TransportSpatialState {
                        translation: [1.0 / 3.0, -1.0e20, 5.5],
                        draw_kind: noon_compile::CompiledSpatialDrawKind::Mesh,
                        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
                        fixed_orientation_center: None,
                        rotation_wxyz: [0.9238795325112867, 0.0, 0.3826834323650898, 0.0],
                        scale: [0.5, 1.25, 2.0],
                        camera_projection: None,
                        material: crate::TransportSpatialMaterial::PointLit,
                        point_light: false,
                        cairo_path_appearance: None,
                    }),
                    style: noon_core::Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                    presence: true,
                    reveal: 1.0,
                    morph: 0.0,
                    render_geometry: None,
                    render_transform: None,
                    render_geometry_resource: None,
                }],
                object_patches: Vec::new(),
                removed_slots: Vec::new(),
                painter_order: None,
            };
            let spatial_auxiliary = |slot: u32,
                                     object: u64,
                                     translation: [f64; 3],
                                     camera_projection: Option<crate::TransportProjection3D>,
                                     point_light: bool| {
                crate::RetainedTransportObjectState {
                    slot: crate::TransportSlotId {
                        slot,
                        generation: 0,
                    },
                    order: slot,
                    object: ObjectId::new(object),
                    glow: None,
                    z_index: 0.0,
                    content: TransportObjectContent::Geometry {
                        geometry: GeometryRef::circle(0.1),
                        resource: None,
                    },
                    transform: Transform2D::IDENTITY,
                    spatial: Some(crate::TransportSpatialState {
                        translation,
                        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
                        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
                        fixed_orientation_center: None,
                        rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
                        scale: [1.0; 3],
                        camera_projection,
                        material: crate::TransportSpatialMaterial::Unlit,
                        point_light,
                        cairo_path_appearance: None,
                    }),
                    style: noon_core::Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                    presence: true,
                    reveal: 1.0,
                    morph: 0.0,
                    render_geometry: None,
                    render_transform: None,
                    render_geometry_resource: None,
                }
            };
            let mut delta = delta;
            delta.objects.push(spatial_auxiliary(
                1,
                91,
                [0.0, 0.0, 8.0],
                Some(crate::TransportProjection3D::Perspective {
                    vertical_fov_radians: 1.0,
                    near: 0.1,
                    far: 100.0,
                }),
                false,
            ));
            delta
                .objects
                .push(spatial_auxiliary(2, 92, [3.0, 4.0, 5.0], None, true));
            mirror
                .apply_family(RetainedFamilyExecutionDeltaEnvelope {
                    retained: delta,
                    family_states: Vec::new(),
                    family_plans: Vec::new(),
                    resource_additions: None,
                    resource_retirements: crate::RetainedResourceRetirements::default(),
                    transient_presentations: Vec::new(),
                    selection_overlay: None,
                    pointer_view: None,
                })
                .unwrap();
            assert_eq!(mirror.publication_context(), source_context);
            assert_eq!(
                mirror.frame().unwrap().objects[0].content.geometry(),
                Some(&GeometryRef::External(local_handle.id))
            );
            let source_world = mirror.frame().unwrap().objects[0]
                .world_transform()
                .expect("spatial mesh retains its source world transform");
            assert_eq!(source_world.translation.x, 1.0 / 3.0);
            assert_eq!(source_world.translation.y, -1.0e20);
            assert_eq!(source_world.scale.z, 2.0);
            assert_eq!(
                mirror.frame().unwrap().objects[0]
                    .spatial
                    .as_deref()
                    .unwrap()
                    .material,
                noon_core::SemanticSpatialMaterial::PointLit
            );
            assert_eq!(mirror.camera_3d_object().unwrap().id, ObjectId::new(91));
            let light = mirror.frame().unwrap().objects.iter().find(|object| {
                object
                    .spatial
                    .as_deref()
                    .is_some_and(|spatial| spatial.point_light)
            });
            assert_eq!(
                light.unwrap().world_transform().unwrap().translation,
                noon_core::SemanticVec3::new(3.0, 4.0, 5.0)
            );
            assert!(mirror.resources.geometries().get(local_handle).is_some());

            let replacement_source = source_geometries.insert_mesh(
                noon_core::MeshResource::new(
                    vec![
                        noon_core::SemanticVec3::new(-1.0, 2.0, 3.0),
                        noon_core::SemanticVec3::new(4.0, -5.0, 6.0),
                        noon_core::SemanticVec3::new(7.0, 8.0, -9.0),
                    ],
                    None,
                    vec![0, 2, 1],
                )
                .unwrap(),
            );
            let replacement_transport =
                crate::TransportGeometryResourceHandle::from(replacement_source);
            let additions = RetainedResourceBundle::capture_additions_with_geometries(
                [],
                [replacement_source],
                &noon_core::TextResourceArena::new(),
                &source_geometries,
                &noon_core::FontResourceArena::new(),
                &crate::RetainedResourceInventory::default(),
            )
            .unwrap();

            let replacement = RetainedFamilyExecutionDeltaEnvelope {
                retained: RetainedExecutionDeltaEnvelope {
                    channel: crate::RETAINED_EXECUTION_TRANSPORT_CHANNEL.into(),
                    protocol_version: crate::RETAINED_EXECUTION_TRANSPORT_VERSION,
                    session: 77,
                    sequence: 1,
                    publication_context: noon_core::PublicationContext::new(
                        noon_core::SceneRevision::new(44),
                        noon_core::ExecutionRevision::new(12),
                        noon_core::FrameEpoch::new(8),
                    ),
                    snapshot: false,
                    time: 0.5,
                    camera: Camera2DState::default(),
                    inset_2d_views: Vec::new(),
                    objects: vec![crate::RetainedTransportObjectState {
                        slot: crate::TransportSlotId {
                            slot: 0,
                            generation: 0,
                        },
                        order: 0,
                        object: ObjectId::new(90),
                        glow: None,
                        z_index: 0.0,
                        content: TransportObjectContent::Geometry {
                            geometry: GeometryRef::External(replacement_source.id),
                            resource: Some(replacement_transport),
                        },
                        transform: Transform2D::IDENTITY,
                        spatial: Some(crate::TransportSpatialState {
                            translation: [-2.0, 4.5, 7.25],
                            draw_kind: noon_compile::CompiledSpatialDrawKind::Mesh,
                            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
                            fixed_orientation_center: None,
                            rotation_wxyz: [0.8660254037844386, 0.0, 0.5, 0.0],
                            scale: [1.5, 0.75, 2.25],
                            camera_projection: None,
                            material: crate::TransportSpatialMaterial::Unlit,
                            point_light: false,
                            cairo_path_appearance: None,
                        }),
                        style: noon_core::Style::default(),
                        appearance: 1.0,
                        text_bounds: None,
                        presence: true,
                        reveal: 1.0,
                        morph: 0.0,
                        render_geometry: None,
                        render_transform: None,
                        render_geometry_resource: None,
                    }],
                    object_patches: Vec::new(),
                    removed_slots: Vec::new(),
                    painter_order: None,
                },
                family_states: Vec::new(),
                family_plans: Vec::new(),
                resource_additions: Some(additions),
                resource_retirements: crate::RetainedResourceRetirements {
                    geometries: vec![transport],
                    ..Default::default()
                },
                transient_presentations: Vec::new(),
                selection_overlay: None,
                pointer_view: None,
            };
            mirror.apply_family(replacement).unwrap();
            assert_eq!(
                mirror.publication_context().frame_epoch().get(),
                8,
                "resource and effective spatial replacement share one publication"
            );
            assert_eq!(mirror.resources().geometry_count(), 1);
            let replacement_local =
                mirror.resources.geometry_handle_remap()[&replacement_transport];
            let Some(noon_core::GeometryResource::Mesh(mesh)) =
                mirror.resources.geometries().get(replacement_local)
            else {
                panic!("incrementally installed mesh was not retained");
            };
            assert_eq!(mesh.positions()[1].y, -5.0);

            let path_replacement = RetainedFamilyExecutionDeltaEnvelope {
                retained: RetainedExecutionDeltaEnvelope {
                    channel: crate::RETAINED_EXECUTION_TRANSPORT_CHANNEL.into(),
                    protocol_version: crate::RETAINED_EXECUTION_TRANSPORT_VERSION,
                    session: 77,
                    sequence: 2,
                    publication_context: noon_core::PublicationContext::new(
                        noon_core::SceneRevision::new(44),
                        noon_core::ExecutionRevision::new(12),
                        noon_core::FrameEpoch::new(9),
                    ),
                    snapshot: false,
                    time: 1.0,
                    camera: Camera2DState::default(),
                    inset_2d_views: Vec::new(),
                    objects: vec![crate::RetainedTransportObjectState {
                        slot: crate::TransportSlotId {
                            slot: 0,
                            generation: 0,
                        },
                        order: 0,
                        object: ObjectId::new(90),
                        glow: None,
                        z_index: 0.0,
                        content: TransportObjectContent::Geometry {
                            geometry: GeometryRef::circle(1.0),
                            resource: None,
                        },
                        transform: Transform2D::IDENTITY,
                        spatial: None,
                        style: noon_core::Style::default(),
                        appearance: 1.0,
                        text_bounds: None,
                        presence: true,
                        reveal: 1.0,
                        morph: 0.0,
                        render_geometry: None,
                        render_transform: None,
                        render_geometry_resource: None,
                    }],
                    object_patches: Vec::new(),
                    removed_slots: Vec::new(),
                    painter_order: None,
                },
                family_states: Vec::new(),
                family_plans: Vec::new(),
                resource_additions: None,
                resource_retirements: crate::RetainedResourceRetirements {
                    geometries: vec![replacement_transport],
                    ..Default::default()
                },
                transient_presentations: Vec::new(),
                selection_overlay: None,
                pointer_view: None,
            };
            mirror.apply_family(path_replacement).unwrap();
            assert_eq!(mirror.resources().geometry_count(), 0);
        }
    }

    fn family_state(progress: f64) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn render_path(x: f32) -> std::sync::Arc<GeometryRef> {
        std::sync::Arc::new(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(x, 0.0))
                .line_to(Vec2::new(x + 1.0, 1.0)),
        ))
    }

    fn render_bundle(
        updates: Vec<(u32, u32, Option<std::sync::Arc<GeometryRef>>)>,
    ) -> RetainedResourceBundle {
        let mut bundle = RetainedResourceBundle::capture(
            [],
            &noon_core::TextResourceArena::new(),
            &noon_core::GeometryResourceArena::new(),
            &noon_core::FontResourceArena::new(),
        )
        .unwrap();
        bundle.set_render_geometry_updates(17, updates, Vec::new());
        bundle
    }

    fn family_snapshot(
        retained: RetainedExecutionDeltaEnvelope,
    ) -> RetainedFamilyExecutionDeltaEnvelope {
        let first_object = retained.objects[0].object;
        RetainedFamilyExecutionDeltaEnvelope {
            retained,
            family_states: vec![RetainedFamilyExecutionObjectState::new(
                first_object,
                Some(family_state(0.5)),
            )
            .unwrap()],
            family_plans: vec![RetainedFamilyPlanTransport::new(
                noon_core::SemanticNodeId::new(7, 2),
                vec![noon_core::FamilyAnimationLeafBinding::new(
                    noon_core::SemanticNodeId::new(7, 2),
                    first_object,
                )],
            )
            .unwrap()],
            resource_additions: None,
            resource_retirements: crate::RetainedResourceRetirements::default(),
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        }
    }

    #[test]
    fn snapshot_keeps_wire_identity_but_resolves_renderer_local_text_handles() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial = engine.initial_delta_json().unwrap();
        let wire: RetainedExecutionDeltaEnvelope = serde_json::from_str(&initial).unwrap();
        let wire_ids = wire
            .objects
            .iter()
            .map(|object| object.object)
            .collect::<Vec<_>>();
        let wire_text = match wire.objects[0].content {
            TransportObjectContent::Text { text } => text,
            TransportObjectContent::Image { .. } | TransportObjectContent::Geometry { .. } => {
                panic!("expected text")
            }
        };

        let (outcome, changes) = mirror.apply_json(&initial).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        let frame = mirror.frame().unwrap();
        assert_eq!(frame.objects[0].id, wire_ids[0]);
        assert_eq!(frame.objects[1].id, wire_ids[1]);

        let local = frame.objects[0].content.text().unwrap();
        assert_eq!(
            Some(local),
            mirror.resources().resolve_text_handle(wire_text)
        );
        assert!(mirror.resources().texts().get(local).is_some());
        assert!(mirror.family_frame().unwrap().is_none());
    }

    #[test]
    fn stale_resource_additions_are_dropped_before_duplicate_resource_validation() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial = engine.initial_delta_json().unwrap();
        mirror.apply_json(&initial).unwrap();
        let before = mirror.frame().unwrap().clone();
        let mut replay: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial).unwrap();
        replay.resource_additions =
            Some(RetainedResourceBundle::decode_binary(&engine.resource_bundle_bytes()).unwrap());
        let (outcome, changes) = mirror.apply_family(replay).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
        assert!(!changes.is_all());
        assert!(changes.object_indices().is_empty());
        assert_eq!(mirror.frame().unwrap(), &before);
    }

    #[test]
    fn render_arena_reuses_removed_slots_and_rejects_failed_or_stale_updates_atomically() {
        let mut engine = geometry_engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial_json = engine.initial_delta_json().unwrap();
        mirror.apply_json(&initial_json).unwrap();
        let initial: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial_json).unwrap();

        let mut sequence = initial.retained.sequence;
        let mut make_delta = |index: usize,
                              resource: Option<u64>,
                              bundle: RetainedResourceBundle| {
            sequence += 1;
            let mut delta = initial.clone();
            delta.retained.snapshot = false;
            delta.retained.sequence = sequence;
            delta.retained.objects = vec![initial.retained.objects[index].clone()];
            delta.retained.objects[0].render_transform = resource.map(|_| Transform2D::IDENTITY);
            delta.retained.objects[0].render_geometry_resource = resource;
            delta.retained.objects[0].render_geometry = None;
            delta.family_states.clear();
            delta.family_plans.clear();
            delta.resource_additions = Some(bundle);
            delta
        };

        let first = make_delta(
            0,
            Some(0),
            render_bundle(vec![(0, 0, Some(render_path(0.0)))]),
        );
        mirror.apply_family(first).unwrap();
        let unrelated = make_delta(
            1,
            Some(1),
            render_bundle(vec![(1, 0, Some(render_path(10.0)))]),
        );
        mirror.apply_family(unrelated).unwrap();
        let stable = mirror.resources().render_geometries()[1]
            .geometry
            .clone()
            .unwrap();

        for generation in 1..=16 {
            let id = crate::retained_resource_transport::render_geometry_id(0, generation);
            let valid = make_delta(
                0,
                Some(id),
                render_bundle(vec![(0, generation, Some(render_path(generation as f32)))]),
            );
            let mut invalid = valid.clone();
            invalid.retained.objects[0].render_geometry_resource = Some(id + 2);
            let previous = mirror.frame().unwrap().clone();
            assert!(mirror.apply_family(invalid).is_err());
            assert_eq!(mirror.frame().unwrap(), &previous);
            assert_eq!(
                mirror.resources().render_geometries()[0].generation,
                generation - 1
            );

            mirror.apply_family(valid.clone()).unwrap();
            let (outcome, _) = mirror.apply_family(valid).unwrap();
            assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
            assert_eq!(mirror.resources().render_geometries().len(), 2);
            assert!(std::sync::Arc::ptr_eq(
                mirror.resources().render_geometries()[1]
                    .geometry
                    .as_ref()
                    .unwrap(),
                &stable
            ));
        }

        let retirement = make_delta(0, None, render_bundle(vec![(0, 17, None)]));
        mirror.apply_family(retirement).unwrap();
        assert!(mirror.resources().render_geometries()[0].geometry.is_none());
        let reused = make_delta(
            0,
            Some(crate::retained_resource_transport::render_geometry_id(
                0, 17,
            )),
            render_bundle(vec![(0, 17, Some(render_path(99.0)))]),
        );
        mirror.apply_family(reused).unwrap();
        assert_eq!(mirror.resources().render_geometries().len(), 2);
        assert!(mirror.resources().render_geometries()[0].geometry.is_some());
    }

    #[test]
    fn shared_render_handle_cannot_be_retired_until_every_row_moves() {
        let mut engine = geometry_engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial_json = engine.initial_delta_json().unwrap();
        mirror.apply_json(&initial_json).unwrap();
        let initial: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial_json).unwrap();

        let mut shared = initial.clone();
        shared.retained.snapshot = false;
        shared.retained.sequence += 1;
        for object in &mut shared.retained.objects {
            object.render_geometry_resource = Some(0);
            object.render_geometry = None;
            object.render_transform = Some(Transform2D::IDENTITY);
        }
        shared.family_states.clear();
        shared.family_plans.clear();
        shared.resource_additions = Some(render_bundle(vec![(0, 0, Some(render_path(0.0)))]));
        mirror.apply_family(shared.clone()).unwrap();
        let old_frame = mirror.frame().unwrap().clone();
        assert!(std::sync::Arc::ptr_eq(
            old_frame.render_geometries[0].as_ref().unwrap(),
            old_frame.render_geometries[1].as_ref().unwrap(),
        ));

        let next = crate::retained_resource_transport::render_geometry_id(0, 1);
        let mut one_row = shared.clone();
        one_row.retained.sequence += 1;
        one_row.retained.objects.truncate(1);
        one_row.retained.objects[0].render_geometry_resource = Some(next);
        one_row.resource_additions = Some(render_bundle(vec![(0, 1, Some(render_path(2.0)))]));
        assert!(mirror.apply_family(one_row.clone()).is_err());
        assert_eq!(mirror.frame().unwrap(), &old_frame);
        assert_eq!(mirror.resources().render_geometries()[0].generation, 0);

        let mut one_retirement = one_row.clone();
        one_retirement.retained.objects[0].render_geometry_resource = None;
        one_retirement.retained.objects[0].render_transform = None;
        one_retirement.resource_additions = Some(render_bundle(vec![(0, 1, None)]));
        assert!(mirror.apply_family(one_retirement).is_err());
        assert_eq!(mirror.frame().unwrap(), &old_frame);

        let mut old_handle_retry = one_row.clone();
        old_handle_retry.retained.objects[0].render_geometry_resource = Some(0);
        old_handle_retry.resource_additions = None;
        mirror.apply_family(old_handle_retry).unwrap();
        assert_eq!(mirror.frame().unwrap(), &old_frame);

        let mut both_rows = shared;
        both_rows.retained.sequence += 2;
        for object in &mut both_rows.retained.objects {
            object.render_geometry_resource = Some(next);
        }
        both_rows.resource_additions = Some(render_bundle(vec![(0, 1, Some(render_path(2.0)))]));
        mirror.apply_family(both_rows).unwrap();
        assert_eq!(mirror.resources().render_geometries()[0].generation, 1);
        assert!(std::sync::Arc::ptr_eq(
            mirror.frame().unwrap().render_geometries[0]
                .as_ref()
                .unwrap(),
            mirror.frame().unwrap().render_geometries[1]
                .as_ref()
                .unwrap(),
        ));
    }

    #[test]
    fn family_snapshot_installs_local_plan_and_scheduler_state() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let retained: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        let family = family_snapshot(retained);
        let json = serde_json::to_string(&family).unwrap();

        let (outcome, changes) = mirror.apply_json(&json).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert!(mirror.family_plan().unwrap().is_some());
        assert_eq!(mirror.family_plans().len(), 1);
        assert_eq!(
            mirror.family_frame().unwrap().unwrap().family_animation(0),
            Some(family_state(0.5))
        );
        assert_eq!(
            mirror
                .planned_family_frame()
                .unwrap()
                .unwrap()
                .family_plan_index(0),
            Some(0)
        );
    }

    #[test]
    fn later_family_snapshot_previews_against_live_wire_sequence() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        mirror
            .apply_family(family_snapshot(initial.clone()))
            .unwrap();

        let mut later = initial;
        later.sequence = 1;
        later.time = 0.5;
        let (outcome, changes) = mirror.apply_family(family_snapshot(later)).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap().time, 0.5);
        assert!(mirror.family_plan().unwrap().is_some());
    }

    #[test]
    fn plain_snapshot_replaces_scene_and_clears_family_sidecar() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let retained: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        mirror
            .apply_family(family_snapshot(retained.clone()))
            .unwrap();
        assert!(mirror.family_plan().unwrap().is_some());

        let mut replacement = retained;
        replacement.session = replacement.session.wrapping_add(1);
        replacement.sequence = 0;
        replacement.time = 1.0;
        mirror.apply(replacement).unwrap();
        assert!(mirror.family_plan().unwrap().is_none());
        assert!(mirror.family_frame().unwrap().is_none());
    }

    #[test]
    fn invalid_family_snapshot_does_not_advance_base_mirror() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let retained: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        mirror.apply(retained.clone()).unwrap();
        assert_eq!(mirror.frame().unwrap().time, retained.time);

        let mut replacement = retained;
        replacement.session = replacement.session.wrapping_add(1);
        replacement.sequence = 0;
        replacement.time = 2.0;
        let invalid = RetainedFamilyExecutionDeltaEnvelope {
            retained: replacement,
            family_states: Vec::new(),
            family_plans: vec![RetainedFamilyPlanTransport::new(
                noon_core::SemanticNodeId::new(7, 2),
                vec![noon_core::FamilyAnimationLeafBinding::new(
                    noon_core::SemanticNodeId::new(7, 2),
                    ObjectId::new(u64::MAX),
                )],
            )
            .unwrap()],
            resource_additions: None,
            resource_retirements: crate::RetainedResourceRetirements::default(),
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        };
        assert!(mirror.apply_family(invalid).is_err());
        assert_ne!(mirror.frame().unwrap().time, 2.0);
    }

    #[test]
    fn invalid_base_incremental_does_not_commit_prepared_family_state() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        mirror
            .apply_family(family_snapshot(initial.clone()))
            .unwrap();

        let changed = initial.objects[0].clone();
        let invalid = RetainedFamilyExecutionDeltaEnvelope {
            retained: RetainedExecutionDeltaEnvelope {
                channel: initial.channel,
                protocol_version: initial.protocol_version,
                session: initial.session,
                sequence: 1,
                publication_context: initial.publication_context,
                snapshot: false,
                time: 2.0,
                camera: initial.camera,
                inset_2d_views: Vec::new(),
                objects: vec![changed.clone(), changed],
                object_patches: Vec::new(),
                removed_slots: Vec::new(),
                painter_order: None,
            },
            family_states: vec![RetainedFamilyExecutionObjectState::planned(
                initial.objects[0].object,
                Some(family_state(0.25)),
                Some(0),
            )
            .unwrap()],
            family_plans: Vec::new(),
            resource_additions: None,
            resource_retirements: crate::RetainedResourceRetirements::default(),
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        };

        assert!(mirror.apply_family(invalid).is_err());
        assert_ne!(mirror.frame().unwrap().time, 2.0);
        assert_eq!(
            mirror
                .planned_family_frame()
                .unwrap()
                .unwrap()
                .family_animation(0),
            Some(family_state(0.5))
        );
    }

    #[test]
    fn incremental_transform_updates_only_state_and_preserves_local_content_handle() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let initial_json = engine.initial_delta_json().unwrap();
        let initial: RetainedExecutionDeltaEnvelope = serde_json::from_str(&initial_json).unwrap();
        mirror.apply(initial.clone()).unwrap();
        let local_before = mirror.frame().unwrap().objects[0].content.text().unwrap();

        let mut changed = initial.objects[0].clone();
        changed.transform.translation = Vec2::new(2.0, -1.0);
        let delta = RetainedExecutionDeltaEnvelope {
            channel: initial.channel,
            protocol_version: initial.protocol_version,
            session: initial.session,
            sequence: 1,
            publication_context: initial.publication_context,
            snapshot: false,
            time: 0.5,
            camera: initial.camera,
            inset_2d_views: Vec::new(),
            objects: vec![changed],
            object_patches: Vec::new(),
            removed_slots: Vec::new(),
            painter_order: None,
        };
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        let frame = mirror.frame().unwrap();
        assert_eq!(frame.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(frame.objects[0].content.text().unwrap(), local_before);
        assert_eq!(frame.objects[1].id, initial.objects[1].object);
    }

    #[test]
    fn snapshot_with_uninstalled_wire_handle_is_rejected_before_mirror_mutation() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
                .unwrap();
        let mut initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        initial.objects[0].content = TransportObjectContent::Text {
            text: crate::TransportTextResourceHandle {
                arena: 0,
                id: u64::MAX,
                version: 0,
            },
        };
        assert!(matches!(
            mirror.apply(initial),
            Err(InstalledExecutionError::UnknownTextResource { .. })
        ));
        assert!(mirror.frame().is_none());
    }
}

#[cfg(test)]
mod morph_tests;

#[cfg(test)]
mod image_tests;
