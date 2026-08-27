use noon_core::{Camera2DState, ObjectContentRef};
use noon_runtime::{FrameChanges, RetainedFrameState};

use crate::{
    InstalledRetainedResources, RetainedExecutionDeltaEnvelope, RetainedExecutionFrameMirror,
    RetainedExecutionTransportError, RetainedResourceBundle, RetainedResourceTransportError,
    RetainedTransportApplyOutcome, TransportObjectContent,
};

/// Render-side retained execution mirror with renderer-local resource handles.
///
/// `RetainedExecutionFrameMirror` deliberately stays in wire-handle space so its
/// content-identity checks compare exactly what the engine sent. This layer owns the
/// installed resource arenas and a separate resolved frame. Snapshots remap every
/// text handle once; incrementals copy only changed animated state and keep the
/// already-resolved local content handle unchanged.
#[derive(Clone, Debug)]
pub struct InstalledRetainedExecutionMirror {
    wire: RetainedExecutionFrameMirror,
    resources: InstalledRetainedResources,
    resolved: Option<RetainedFrameState>,
}

impl InstalledRetainedExecutionMirror {
    pub fn from_bundle_bytes(bytes: &[u8]) -> Result<Self, InstalledExecutionError> {
        let resources = RetainedResourceBundle::decode_binary(bytes)?.install()?;
        Ok(Self {
            wire: RetainedExecutionFrameMirror::default(),
            resources,
            resolved: None,
        })
    }

    pub fn resources(&self) -> &InstalledRetainedResources {
        &self.resources
    }

    pub fn frame(&self) -> Option<&RetainedFrameState> {
        self.resolved.as_ref()
    }

    pub const fn camera(&self) -> Camera2DState {
        self.wire.camera()
    }

    pub fn apply_json(
        &mut self,
        json: &str,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        let delta: RetainedExecutionDeltaEnvelope = serde_json::from_str(json)?;
        self.apply(delta)
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
        if delta.snapshot {
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

    fn rebuild_resolved_snapshot(&mut self) -> Result<(), InstalledExecutionError> {
        let wire = self
            .wire
            .frame()
            .ok_or(InstalledExecutionError::MissingWireFrame)?;
        let mut resolved = wire.clone();
        for object in &mut resolved.objects {
            if let ObjectContentRef::Text(wire_handle) = object.content {
                let transport = wire_handle.into();
                let local = self.resources.resolve_text_handle(transport).ok_or(
                    InstalledExecutionError::UnknownTextResource {
                        id: transport.id,
                        version: transport.version,
                    },
                )?;
                object.content = ObjectContentRef::Text(local);
            }
        }
        self.resolved = Some(resolved);
        Ok(())
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

            // Content stays renderer-local. The wire mirror has already rejected any
            // incremental content/identity mutation for this slot.
            target.id = source.id;
            target.transform = source.transform;
            target.style = source.style;
            target.appearance = source.appearance;
            resolved.presences[index] = wire.presences[index];
            resolved.reveals[index] = wire.reveals[index];
            resolved.morphs[index] = wire.morphs[index];
            resolved.render_geometries[index] = wire.render_geometries[index].clone();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstalledExecutionError {
    Resource(RetainedResourceTransportError),
    Transport(RetainedExecutionTransportError),
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

impl From<serde_json::Error> for InstalledExecutionError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{ObjectId, SceneDefinition, Vec2};

    use super::*;
    use crate::{
        RetainedAuthoringDocument, RetainedAuthoringEnginePlayer, RetainedAuthoringTextObject,
        RetainedTypstAuthoringSpec,
    };

    fn engine() -> RetainedAuthoringEnginePlayer {
        let legacy = SceneDefinition::new();
        let document = RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(8),
                order: 0,
                text: RetainedTypstAuthoringSpec::new("*Hello*", false, 64.0).unwrap(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(21),
                order: 1,
                text: RetainedTypstAuthoringSpec::new("frac(x, 2)", true, 72.0).unwrap(),
            },
        ])
        .unwrap();
        RetainedAuthoringEnginePlayer::new(
            &noon_ir::encode_scene(&legacy).unwrap(),
            &document.to_json().unwrap(),
            4.0,
            17,
        )
        .unwrap()
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
