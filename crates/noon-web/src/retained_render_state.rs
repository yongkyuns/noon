use noon_core::{Camera2DState, ObjectContentRef};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};

use crate::{
    InstalledRetainedResources, RetainedExecutionDeltaEnvelope, RetainedExecutionFrameMirror,
    RetainedExecutionTransportError, RetainedResourceBundle, RetainedResourceTransportError,
    RetainedTransportApplyOutcome, TransportTextResourceHandle,
};

/// Renderer-side retained state with transport identity kept separate from local
/// resource identity.
///
/// The transport mirror continues to validate engine-owned text handles exactly as
/// they arrived on `noon.execution.retained`. A parallel resolved frame replaces
/// only text handles with the render worker's locally installed resource handles.
/// Incremental deltas therefore resolve only changed objects; full O(n) remapping is
/// limited to snapshots and rewinds.
#[derive(Clone, Debug)]
pub struct ResolvedRetainedExecutionFrameMirror {
    transport: RetainedExecutionFrameMirror,
    resources: InstalledRetainedResources,
    frame: Option<RetainedFrameState>,
}

impl ResolvedRetainedExecutionFrameMirror {
    pub fn from_bundle(
        bundle: RetainedResourceBundle,
    ) -> Result<Self, ResolvedRetainedExecutionError> {
        Ok(Self::new(bundle.install()?))
    }

    pub fn from_bundle_binary(bytes: &[u8]) -> Result<Self, ResolvedRetainedExecutionError> {
        Self::from_bundle(RetainedResourceBundle::decode_binary(bytes)?)
    }

    pub fn new(resources: InstalledRetainedResources) -> Self {
        Self {
            transport: RetainedExecutionFrameMirror::default(),
            resources,
            frame: None,
        }
    }

    pub fn frame(&self) -> Option<&RetainedFrameState> {
        self.frame.as_ref()
    }

    pub const fn camera(&self) -> Camera2DState {
        self.transport.camera()
    }

    pub const fn resources(&self) -> &InstalledRetainedResources {
        &self.resources
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), ResolvedRetainedExecutionError> {
        let (outcome, changes) = self.transport.apply(delta)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }

        if self.frame.is_none() || changes.is_all() {
            self.resolve_snapshot()?;
        } else if !changes.is_empty() {
            self.resolve_incremental(&changes)?;
        } else if let (Some(source), Some(frame)) = (self.transport.frame(), self.frame.as_mut()) {
            frame.time = source.time;
        }
        Ok((outcome, changes))
    }

    pub fn apply_json(
        &mut self,
        json: &str,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), ResolvedRetainedExecutionError> {
        let delta: RetainedExecutionDeltaEnvelope = serde_json::from_str(json)
            .map_err(RetainedExecutionTransportError::from)?;
        self.apply(delta)
    }

    fn resolve_snapshot(&mut self) -> Result<(), ResolvedRetainedExecutionError> {
        let source = self
            .transport
            .frame()
            .expect("applied retained snapshot must create a transport frame");
        let mut frame = source.clone();
        for object in &mut frame.objects {
            self.resolve_object(object)?;
        }
        self.frame = Some(frame);
        Ok(())
    }

    fn resolve_incremental(
        &mut self,
        changes: &FrameChanges,
    ) -> Result<(), ResolvedRetainedExecutionError> {
        let source = self
            .transport
            .frame()
            .expect("applied retained delta must have a transport frame");
        let frame = self
            .frame
            .as_mut()
            .expect("resolved retained frame must exist after the first snapshot");
        frame.time = source.time;
        for &index in changes.object_indices() {
            let mut object = source.objects[index].clone();
            resolve_object_with_resources(&self.resources, &mut object)?;
            frame.objects[index] = object;
            frame.presences[index] = source.presences[index];
            frame.reveals[index] = source.reveals[index];
            frame.morphs[index] = source.morphs[index];
            frame.render_geometries[index] = source.render_geometries[index].clone();
        }
        Ok(())
    }

    fn resolve_object(
        &self,
        object: &mut RetainedFrameObjectState,
    ) -> Result<(), ResolvedRetainedExecutionError> {
        resolve_object_with_resources(&self.resources, object)
    }
}

fn resolve_object_with_resources(
    resources: &InstalledRetainedResources,
    object: &mut RetainedFrameObjectState,
) -> Result<(), ResolvedRetainedExecutionError> {
    let ObjectContentRef::Text(transport_handle) = object.content else {
        return Ok(());
    };
    let transport = TransportTextResourceHandle::from(transport_handle);
    let local = resources.resolve_text_handle(transport).ok_or_else(|| {
        ResolvedRetainedExecutionError::Resource(RetainedResourceTransportError::UnknownText(
            transport,
        ))
    })?;
    object.content = ObjectContentRef::Text(local);
    Ok(())
}

#[derive(Debug)]
pub enum ResolvedRetainedExecutionError {
    Transport(RetainedExecutionTransportError),
    Resource(RetainedResourceTransportError),
}

impl std::fmt::Display for ResolvedRetainedExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ResolvedRetainedExecutionError {}

impl From<RetainedExecutionTransportError> for ResolvedRetainedExecutionError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<RetainedResourceTransportError> for ResolvedRetainedExecutionError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

#[cfg(test)]
mod tests {
    use noon::{RetainedScene, Typst};
    use noon_core::{ObjectId, Style, Transform2D, Vec2};
    use noon_runtime::RetainedFrameObjectState;

    use crate::RetainedExecutionDeltaEncoder;

    use super::*;

    fn source_scene() -> (RetainedScene, noon_core::TextResourceHandle) {
        let mut scene = RetainedScene::new();
        scene.add_typst(Typst::new("A")).unwrap();
        scene.add_typst(Typst::new("B")).unwrap();
        let handle = scene.objects()[1].content.text().unwrap();
        (scene, handle)
    }

    fn transport_frame(handle: noon_core::TextResourceHandle) -> RetainedFrameState {
        RetainedFrameState {
            time: 0.0,
            objects: vec![RetainedFrameObjectState {
                id: ObjectId::new(41),
                content: ObjectContentRef::Text(handle),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
        }
    }

    fn resolved_mirror(
        scene: &RetainedScene,
        handle: noon_core::TextResourceHandle,
    ) -> ResolvedRetainedExecutionFrameMirror {
        let bundle = RetainedResourceBundle::capture(
            [handle],
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )
        .unwrap();
        ResolvedRetainedExecutionFrameMirror::from_bundle(bundle).unwrap()
    }

    #[test]
    fn snapshot_maps_transport_text_handle_to_render_local_handle() {
        let (scene, transport_handle) = source_scene();
        assert_eq!(transport_handle.id.get(), 1);
        let frame = transport_frame(transport_handle);
        let mut encoder = RetainedExecutionDeltaEncoder::new(7);
        let delta = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = resolved_mirror(&scene, transport_handle);

        let (outcome, changes) = mirror.apply(delta).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        let local_handle = mirror.frame().unwrap().objects[0].content.text().unwrap();
        assert_eq!(local_handle.id.get(), 0);
        assert_ne!(local_handle, transport_handle);
        assert_eq!(
            mirror.resources().texts().get(local_handle).unwrap().source.as_ref(),
            "B"
        );
    }

    #[test]
    fn incremental_delta_resolves_only_changed_object_and_keeps_local_identity() {
        let (scene, transport_handle) = source_scene();
        let frame = transport_frame(transport_handle);
        let mut encoder = RetainedExecutionDeltaEncoder::new(8);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = resolved_mirror(&scene, transport_handle);
        mirror.apply(initial).unwrap();
        let local_handle = mirror.frame().unwrap().objects[0].content.text().unwrap();

        let mut updated = frame.clone();
        updated.time = 0.5;
        updated.objects[0].transform.translation = Vec2::new(3.0, -1.0);
        let delta = encoder
            .encode_incremental(
                &updated,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        let resolved = mirror.frame().unwrap();
        assert_eq!(resolved.objects[0].content.text().unwrap(), local_handle);
        assert_eq!(
            resolved.objects[0].transform.translation,
            Vec2::new(3.0, -1.0)
        );
        assert_eq!(resolved.time, 0.5);
    }

    #[test]
    fn stale_transport_delta_does_not_mutate_resolved_frame() {
        let (scene, transport_handle) = source_scene();
        let frame = transport_frame(transport_handle);
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = resolved_mirror(&scene, transport_handle);
        mirror.apply(initial.clone()).unwrap();
        let before = mirror.frame().unwrap().clone();

        let (outcome, changes) = mirror.apply(initial).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
        assert!(changes.is_empty());
        assert_eq!(mirror.frame().unwrap(), &before);
    }
}
