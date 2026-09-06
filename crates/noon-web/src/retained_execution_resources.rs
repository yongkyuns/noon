use noon_core::{Camera2DState, ObjectContentRef, RetainedFamilyAnimationPlan};
use noon_runtime::{FrameChanges, FrameState, RetainedFamilyFrame, RetainedPlannedFamilyFrame};

use crate::{
    InstalledRetainedFamilyExecutionState, InstalledRetainedResources,
    RetainedExecutionDeltaEnvelope, RetainedExecutionFrameMirror, RetainedExecutionTransportError,
    RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionTransportError,
    RetainedResourceBundle, RetainedResourceTransportError, RetainedTransportApplyOutcome,
    TransportObjectContent,
};

/// Render-side retained execution mirror with renderer-local resource handles.
///
/// `RetainedExecutionFrameMirror` deliberately stays in wire-handle space so its
/// content-identity checks compare exactly what the engine sent. This layer owns the
/// installed resource arenas and a separate resolved frame. Snapshots remap every
/// text handle once; incrementals copy changed animated state and effective geometry
/// while keeping already-resolved renderer-local text handles unchanged.
#[derive(Clone, Debug)]
pub struct InstalledRetainedExecutionMirror {
    wire: RetainedExecutionFrameMirror,
    resources: InstalledRetainedResources,
    resolved: Option<FrameState>,
    family: InstalledRetainedFamilyExecutionState,
}

impl InstalledRetainedExecutionMirror {
    pub fn from_bundle_bytes(bytes: &[u8]) -> Result<Self, InstalledExecutionError> {
        let resources = RetainedResourceBundle::decode_binary(bytes)?.install()?;
        Ok(Self {
            wire: RetainedExecutionFrameMirror::with_installed_resources(
                resources.render_geometry_session(),
                resources.render_geometries(),
                resources.text_handle_remap(),
            ),
            resources,
            resolved: None,
            family: InstalledRetainedFamilyExecutionState::default(),
        })
    }

    pub fn resources(&self) -> &InstalledRetainedResources {
        &self.resources
    }

    pub fn frame(&self) -> Option<&FrameState> {
        self.resolved.as_ref()
    }

    pub fn family_frame(&self) -> Result<Option<RetainedFamilyFrame<'_>>, InstalledExecutionError> {
        if self.family.plans().is_empty() {
            return Ok(None);
        }
        let frame = self
            .resolved
            .as_ref()
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
            .resolved
            .as_ref()
            .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
        Ok(Some(self.family.planned_frame(frame)?))
    }

    pub fn family_plans(&self) -> &[RetainedFamilyAnimationPlan] {
        self.family.plans()
    }

    pub fn family_plan(
        &self,
    ) -> Result<Option<&RetainedFamilyAnimationPlan>, InstalledExecutionError> {
        Ok(self.family.single_plan()?)
    }

    pub const fn camera(&self) -> Camera2DState {
        self.wire.camera()
    }

    /// Decode the additive family envelope. Ordinary retained JSON is accepted
    /// unchanged because the flattened family fields default to empty.
    pub fn apply_json(
        &mut self,
        json: &str,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let delta: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(json)?;
        self.apply_family(delta)
    }

    /// Apply a base retained delta without family metadata.
    ///
    /// A successfully applied snapshot is authoritative for the whole retained scene,
    /// so it also clears any previously installed family sidecar. Incrementals preserve
    /// the sidecar because they cannot change retained identity/content shape.
    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let snapshot = delta.snapshot;
        if snapshot {
            self.validate_snapshot_resources(&delta)?;
        }

        let (outcome, changes) = self.wire.apply(delta)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }

        if changes.is_all() || self.resolved.is_none() {
            self.rebuild_resolved_snapshot()?;
        } else {
            self.apply_resolved_incremental(&changes)?;
        }
        if snapshot {
            self.family = InstalledRetainedFamilyExecutionState::default();
        }
        Ok((outcome, changes))
    }

    /// Transactionally apply retained execution plus generic family sidecar state.
    pub fn apply_family(
        &mut self,
        delta: RetainedFamilyExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        delta.validate()?;
        if delta.retained.snapshot {
            self.validate_snapshot_resources(&delta.retained)?;
        }

        // Family validation happens against the frame shape that will exist after the
        // retained delta, but live family state is not changed until the base mirror
        // accepts the sequence. Incrementals cannot change retained identity/content,
        // so the current resolved frame is sufficient for their sidecar validation.
        let mut next_family = self.family.clone();
        if delta.retained.snapshot {
            let preview = self.preview_resolved_snapshot(&delta.retained)?;
            next_family.apply(&delta, &preview, self.resources.texts())?;
        } else {
            let current = self
                .resolved
                .as_ref()
                .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
            next_family.apply(&delta, current, self.resources.texts())?;
        }

        let (outcome, changes) = self.apply(delta.retained)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }
        self.family = next_family;
        Ok((outcome, changes))
    }

    fn validate_snapshot_resources(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), InstalledExecutionError> {
        for object in &delta.objects {
            if let TransportObjectContent::Text { text } = object.content {
                if self.resources.resolve_text_handle(text).is_none() {
                    return Err(InstalledExecutionError::UnknownTextResource {
                        id: text.id,
                        version: text.version,
                    });
                }
            }
        }
        Ok(())
    }

    fn preview_resolved_snapshot(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<FrameState, InstalledExecutionError> {
        let mut wire = self.wire.clone();
        let (outcome, _) = wire.apply(delta.clone())?;
        debug_assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        let frame = wire
            .frame()
            .ok_or(InstalledExecutionError::MissingWireFrame)?;
        Ok(self.resolve_wire_frame(frame))
    }

    fn rebuild_resolved_snapshot(&mut self) -> Result<(), InstalledExecutionError> {
        let wire = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingWireFrame)?;
        self.resolved = Some(self.resolve_wire_frame(wire));
        Ok(())
    }

    fn resolve_wire_frame(&self, wire: &FrameState) -> FrameState {
        // The wire mirror resolves every transport key through this installed bundle
        // before it constructs a `FrameState`; cloned snapshots therefore retain only
        // checked renderer-local handles.
        wire.clone()
    }

    fn apply_resolved_incremental(
        &mut self,
        changes: &FrameChanges,
    ) -> Result<(), InstalledExecutionError> {
        let wire = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingWireFrame)?;
        let resolved = self
            .resolved
            .as_mut()
            .ok_or(InstalledExecutionError::MissingResolvedFrame)?;
        if resolved.objects.len() != wire.objects.len() {
            return Err(InstalledExecutionError::FrameShapeMismatch);
        }

        resolved.time = wire.time;
        for &index in changes.object_indices() {
            let source = wire
                .objects
                .get(index)
                .ok_or(InstalledExecutionError::InvalidObjectIndex(index))?;
            let target = resolved
                .objects
                .get_mut(index)
                .ok_or(InstalledExecutionError::InvalidObjectIndex(index))?;

            // Text handles stay renderer-local. Geometry snapshots may change during
            // Transform, and the wire mirror validates geometry-to-geometry updates.
            // Publish that effective content too, especially when a morph endpoint
            // clears its temporary render override.
            if let ObjectContentRef::Geometry(geometry) = &source.content {
                target.content = ObjectContentRef::Geometry(geometry.clone());
            }
            target.id = source.id;
            target.transform = source.transform;
            target.style = source.style;
            target.appearance = source.appearance;
            resolved.presences[index] = wire.presences[index];
            resolved.reveals[index] = wire.reveals[index];
            resolved.morphs[index] = wire.morphs[index];
            resolved.render_geometries[index] = wire.render_geometries[index].clone();
            resolved.render_transforms[index] = wire.render_transforms[index];
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
    MissingWireFrame,
    MissingResolvedFrame,
    FrameShapeMismatch,
    InvalidObjectIndex(usize),
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
            Self::MissingWireFrame => formatter.write_str("retained execution has no wire frame"),
            Self::MissingResolvedFrame => {
                formatter.write_str("retained execution has no renderer-local frame")
            }
            Self::FrameShapeMismatch => {
                formatter.write_str("wire and renderer-local retained frame shapes differ")
            }
            Self::InvalidObjectIndex(index) => {
                write!(
                    formatter,
                    "invalid renderer-local retained object index {index}"
                )
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
        FamilyAnimationMode, FamilyAnimationState, ObjectId, RateFunction, SceneDefinition, Vec2,
    };

    use super::*;
    use crate::{
        canonical_retained_scene_spec_json, CanonicalRetainedEnginePlayer,
        RetainedAuthoringDocument, RetainedAuthoringTextObject, RetainedFamilyExecutionObjectState,
        RetainedFamilyPlanTransport, RetainedTextAuthoringSpec,
    };

    fn native_text(source: &str, font_size: f32) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(
            source,
            noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
            font_size,
            -1.0,
        )
        .unwrap()
    }

    fn engine() -> CanonicalRetainedEnginePlayer {
        let legacy = SceneDefinition::new();
        let document = RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(8),
                order: 0,
                text: native_text("Hello", 64.0),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(21),
                order: 1,
                text: native_text("World", 72.0),
            },
        ])
        .unwrap();
        let legacy_json = noon_ir::encode_scene(&legacy).unwrap();
        let document_json = document.to_json().unwrap();
        let scene_spec_json =
            canonical_retained_scene_spec_json(&legacy_json, &document_json).unwrap();
        CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 17).unwrap()
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

    fn family_snapshot(
        retained: RetainedExecutionDeltaEnvelope,
    ) -> RetainedFamilyExecutionDeltaEnvelope {
        RetainedFamilyExecutionDeltaEnvelope {
            retained,
            family_states: vec![RetainedFamilyExecutionObjectState::new(
                ObjectId::new(8),
                Some(family_state(0.5)),
            )
            .unwrap()],
            family_plans: vec![RetainedFamilyPlanTransport::new(vec![ObjectId::new(8)]).unwrap()],
        }
    }

    #[test]
    fn snapshot_keeps_wire_identity_but_resolves_renderer_local_text_handles() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();
        let initial = engine.initial_delta_json().unwrap();
        let wire: RetainedExecutionDeltaEnvelope = serde_json::from_str(&initial).unwrap();
        let wire_text = match wire.objects[0].content {
            TransportObjectContent::Text { text } => text,
            TransportObjectContent::Geometry { .. } => panic!("expected text"),
        };

        let (outcome, changes) = mirror.apply_json(&initial).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        let frame = mirror.frame().unwrap();
        assert_eq!(frame.objects[0].id, ObjectId::new(8));
        assert_eq!(frame.objects[1].id, ObjectId::new(21));

        let local = frame.objects[0].content.text().unwrap();
        assert_eq!(
            Some(local),
            mirror.resources().resolve_text_handle(wire_text)
        );
        assert!(mirror.resources().texts().get(local).is_some());
        assert!(mirror.family_frame().unwrap().is_none());
    }

    #[test]
    fn family_snapshot_installs_local_plan_and_scheduler_state() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
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
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
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
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
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
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
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
            family_plans: vec![
                RetainedFamilyPlanTransport::new(vec![ObjectId::new(u64::MAX)]).unwrap(),
            ],
        };
        assert!(mirror.apply_family(invalid).is_err());
        assert_ne!(mirror.frame().unwrap().time, 2.0);
    }

    #[test]
    fn incremental_transform_updates_only_state_and_preserves_local_content_handle() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
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
            snapshot: false,
            time: 0.5,
            camera: initial.camera,
            objects: vec![changed],
        };
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        let frame = mirror.frame().unwrap();
        assert_eq!(frame.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(frame.objects[0].content.text().unwrap(), local_before);
        assert_eq!(frame.objects[1].id, ObjectId::new(21));
    }

    #[test]
    fn snapshot_with_uninstalled_wire_handle_is_rejected_before_mirror_mutation() {
        let mut engine = engine();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();
        let mut initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        initial.objects[0].content = TransportObjectContent::Text {
            text: crate::TransportTextResourceHandle {
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
