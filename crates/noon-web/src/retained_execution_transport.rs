use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use noon_core::{
    Camera2DState, GeometryRef, ObjectContentRef, ObjectId, Rect, Style, TextResourceHandle,
    Transform2D,
};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use serde::{Deserialize, Serialize};

use crate::TransportSlotId;

/// Resource-aware execution channel for the retained geometry/text runtime.
///
/// The legacy `noon.execution` v1 channel stays geometry-only. This channel makes
/// object content explicit so text can occupy the same identity/order stream as
/// geometry without a fake `GeometryRef` variant or placeholder object.
pub const RETAINED_EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution.retained";
pub const RETAINED_EXECUTION_TRANSPORT_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransportTextResourceHandle {
    pub id: u64,
    pub version: u64,
}

impl TransportTextResourceHandle {
    /// Encode a source handle as an opaque key scoped to the paired resource bundle.
    /// The worker must resolve this key through `InstalledRetainedResources`; it is
    /// deliberately not a serializable core arena handle.
    pub(crate) const fn from_source_handle(value: TextResourceHandle) -> Self {
        Self {
            id: value.id.get(),
            version: value.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportObjectContent {
    Geometry { geometry: GeometryRef },
    Text { text: TransportTextResourceHandle },
}

impl From<&ObjectContentRef> for TransportObjectContent {
    fn from(value: &ObjectContentRef) -> Self {
        match value {
            ObjectContentRef::Geometry(geometry) => Self::Geometry {
                geometry: geometry.clone(),
            },
            ObjectContentRef::Text(text) => Self::Text {
                text: TransportTextResourceHandle::from_source_handle(*text),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTransportObjectState {
    pub slot: TransportSlotId,
    pub order: u32,
    pub object: ObjectId,
    pub content: TransportObjectContent,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bounds: Option<Rect>,
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    pub render_geometry: Option<GeometryRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_transform: Option<Transform2D>,
    /// Index into the immutable geometry table installed with this session's bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_geometry_resource: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedExecutionDeltaEnvelope {
    pub channel: String,
    pub protocol_version: u32,
    pub session: u32,
    pub sequence: u64,
    pub snapshot: bool,
    pub time: f64,
    #[serde(default)]
    pub camera: Camera2DState,
    pub objects: Vec<RetainedTransportObjectState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedTransportApplyOutcome {
    Applied,
    DroppedStale,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedExecutionTransportError {
    InvalidChannel(String),
    UnsupportedVersion(u32),
    InvalidTime(f64),
    SequenceExhausted,
    SessionRequiresSnapshot { session: u32, sequence: u64 },
    SequenceGap { expected: u64, actual: u64 },
    IncrementalBeforeSnapshot,
    StructuralChangeRequiresSnapshot,
    FrameShapeMismatch,
    InvalidObjectIndex(usize),
    InvalidOrder(u32),
    DuplicateSlot(TransportSlotId),
    DuplicateObject(ObjectId),
    UnknownSlot(TransportSlotId),
    SlotIdentityChanged(TransportSlotId),
    ContentIdentityChanged(TransportSlotId),
    TextRenderGeometry(TransportSlotId),
    InvalidRenderGeometryResource(u32),
    AmbiguousRenderGeometry(TransportSlotId),
    MissingCompiledRenderResource(TransportSlotId),
    InvalidRenderTransform(TransportSlotId),
    UnknownTextResource(TransportTextResourceHandle),
}

impl std::fmt::Display for RetainedExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidChannel(channel) => {
                write!(
                    formatter,
                    "invalid retained execution transport channel {channel:?}"
                )
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported retained execution transport version {version}"
            ),
            Self::InvalidTime(time) => write!(formatter, "invalid retained frame time {time}"),
            Self::SequenceExhausted => {
                formatter.write_str("retained execution transport sequence exhausted")
            }
            Self::SessionRequiresSnapshot { session, sequence } => write!(
                formatter,
                "retained execution session {session} must begin with snapshot sequence 0, got {sequence}"
            ),
            Self::SequenceGap { expected, actual } => write!(
                formatter,
                "retained execution delta sequence gap: expected {expected}, got {actual}"
            ),
            Self::IncrementalBeforeSnapshot => formatter
                .write_str("retained execution transport requires a snapshot before incrementals"),
            Self::StructuralChangeRequiresSnapshot => formatter
                .write_str("retained execution structural changes require a complete snapshot"),
            Self::FrameShapeMismatch => formatter
                .write_str("retained frame object/property arrays have inconsistent lengths"),
            Self::InvalidObjectIndex(index) => {
                write!(formatter, "invalid retained frame object index {index}")
            }
            Self::InvalidOrder(order) => {
                write!(formatter, "invalid retained execution render order {order}")
            }
            Self::DuplicateSlot(slot) => write!(
                formatter,
                "duplicate retained execution slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::DuplicateObject(object) => write!(
                formatter,
                "duplicate retained execution object {}",
                object.get()
            ),
            Self::UnknownSlot(slot) => write!(
                formatter,
                "unknown retained execution slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::SlotIdentityChanged(slot) => write!(
                formatter,
                "retained execution slot {}:{} changed object identity without a snapshot",
                slot.slot, slot.generation
            ),
            Self::ContentIdentityChanged(slot) => write!(
                formatter,
                "retained execution slot {}:{} changed content identity without a snapshot",
                slot.slot, slot.generation
            ),
            Self::TextRenderGeometry(slot) => write!(
                formatter,
                "retained text slot {}:{} cannot carry transient render geometry",
                slot.slot, slot.generation
            ),
            Self::InvalidRenderGeometryResource(index) => write!(
                formatter,
                "unknown retained render geometry resource {index} in this session"
            ),
            Self::AmbiguousRenderGeometry(slot) => write!(
                formatter,
                "retained slot {}:{} carries both inline and resource geometry",
                slot.slot, slot.generation
            ),
            Self::MissingCompiledRenderResource(slot) => write!(
                formatter,
                "retained slot {}:{} has an unregistered compiled render geometry",
                slot.slot, slot.generation
            ),
            Self::InvalidRenderTransform(slot) => write!(
                formatter,
                "retained slot {}:{} has an invalid render transform",
                slot.slot, slot.generation
            ),
            Self::UnknownTextResource(handle) => write!(
                formatter,
                "unknown retained transport text resource {}@{}",
                handle.id, handle.version
            ),
        }
    }
}

impl std::error::Error for RetainedExecutionTransportError {}

#[derive(Clone, Debug)]
pub struct RetainedExecutionDeltaEncoder {
    session: u32,
    next_sequence: u64,
    initialized: bool,
    // Explicit worker-boundary projection: runtime tombstones have no wire row.
    snapshot_orders: Vec<Option<u32>>,
    // Retain the Arcs so pointer keys cannot be recycled during this encoder's lifetime.
    render_geometries: Option<Arc<[Arc<GeometryRef>]>>,
    render_geometry_indices: Option<HashMap<usize, u32>>,
}

impl RetainedExecutionDeltaEncoder {
    pub const fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
            initialized: false,
            snapshot_orders: Vec::new(),
            render_geometries: None,
            render_geometry_indices: None,
        }
    }

    pub(crate) fn with_render_geometries(
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
    ) -> Self {
        let indices = geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| {
                (
                    Arc::as_ptr(geometry) as usize,
                    u32::try_from(index).expect("compiled geometry table exceeds u32"),
                )
            })
            .collect();
        Self {
            render_geometries: Some(geometries),
            render_geometry_indices: Some(indices),
            ..Self::new(session)
        }
    }

    fn transport_object(
        &self,
        frame: &FrameState,
        index: usize,
    ) -> Result<RetainedTransportObjectState, RetainedExecutionTransportError> {
        let resource = frame
            .render_geometries
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|geometry| {
                self.render_geometry_indices
                    .as_ref()
                    .and_then(|indices| indices.get(&(Arc::as_ptr(geometry) as usize)))
                    .copied()
            });
        debug_assert!(resource.is_none_or(|index| {
            self.render_geometries
                .as_ref()
                .is_some_and(|items| (index as usize) < items.len())
        }));
        let object = transport_object(frame, index, resource)?;
        if self.render_geometry_indices.is_some()
            && object.render_transform.is_some()
            && object.render_geometry_resource.is_none()
        {
            return Err(
                RetainedExecutionTransportError::MissingCompiledRenderResource(object.slot),
            );
        }
        Ok(object)
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &FrameState,
        camera: Camera2DState,
    ) -> Result<RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError> {
        self.encode_snapshot_indices(frame, camera, 0..frame.objects.len())
    }

    /// Encode live runtime rows without copying or compacting the engine frame.
    /// Indices are supplied in painter order; subsequent deltas retain their
    /// original runtime slot and use the dense order established here.
    pub fn encode_snapshot_indices(
        &mut self,
        frame: &FrameState,
        camera: Camera2DState,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        let mut orders = vec![None; frame.objects.len()];
        let objects = indices
            .into_iter()
            .enumerate()
            .map(|(order, index)| {
                let mut object = self.transport_object(frame, index)?;
                if orders[index].is_some() {
                    return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
                }
                object.order = u32::try_from(order)
                    .map_err(|_| RetainedExecutionTransportError::InvalidObjectIndex(index))?;
                orders[index] = Some(object.order);
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        self.initialized = true;
        self.snapshot_orders = orders;
        Ok(RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot: true,
            time: frame.time,
            camera,
            objects,
        })
    }

    pub fn encode_incremental(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        if !self.initialized {
            return Err(RetainedExecutionTransportError::IncrementalBeforeSnapshot);
        }
        if changes.is_structural() {
            return Err(RetainedExecutionTransportError::StructuralChangeRequiresSnapshot);
        }
        if changes.is_all() {
            let indices = self
                .snapshot_orders
                .iter()
                .enumerate()
                .filter_map(|(index, order)| order.map(|_| index))
                .collect::<Vec<_>>();
            return self
                .encode_snapshot_indices(frame, camera, indices)
                .map(Some);
        }
        if changes.is_empty() {
            return Ok(None);
        }
        let objects = changes
            .object_indices()
            .iter()
            .copied()
            .map(|index| {
                let mut object = self.transport_object(frame, index)?;
                object.order = self
                    .snapshot_orders
                    .get(index)
                    .copied()
                    .flatten()
                    .ok_or(RetainedExecutionTransportError::UnknownSlot(object.slot))?;
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        Ok(Some(RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot: false,
            time: frame.time,
            camera,
            objects,
        }))
    }

    fn take_sequence(&mut self) -> Result<u64, RetainedExecutionTransportError> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RetainedExecutionTransportError::SequenceExhausted)?;
        Ok(sequence)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RetainedExecutionFrameMirror {
    session: Option<u32>,
    next_sequence: u64,
    slots: Vec<TransportSlotId>,
    slot_indices: HashMap<TransportSlotId, usize>,
    render_geometries: Arc<[Arc<GeometryRef>]>,
    resource_session: Option<u32>,
    text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    camera: Camera2DState,
    frame: Option<FrameState>,
}

impl RetainedExecutionFrameMirror {
    pub(crate) fn with_installed_resources(
        session: Option<u32>,
        geometries: Arc<[Arc<GeometryRef>]>,
        text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    ) -> Self {
        Self {
            resource_session: session,
            render_geometries: geometries,
            text_handles,
            ..Self::default()
        }
    }

    fn resolve_content(
        &self,
        content: &TransportObjectContent,
    ) -> Result<ObjectContentRef, RetainedExecutionTransportError> {
        match content {
            TransportObjectContent::Geometry { geometry } => {
                Ok(ObjectContentRef::Geometry(geometry.clone()))
            }
            TransportObjectContent::Text { text } => self
                .text_handles
                .get(text)
                .copied()
                .map(ObjectContentRef::Text)
                .ok_or(RetainedExecutionTransportError::UnknownTextResource(*text)),
        }
    }

    fn resolve_render_geometry(
        &self,
        object: &RetainedTransportObjectState,
        session: u32,
    ) -> Result<Option<Arc<GeometryRef>>, RetainedExecutionTransportError> {
        match object.render_geometry_resource {
            Some(index) if self.resource_session == Some(session) => self
                .render_geometries
                .get(index as usize)
                .cloned()
                .map(Some)
                .ok_or(RetainedExecutionTransportError::InvalidRenderGeometryResource(index)),
            Some(index) => {
                Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(index))
            }
            None => Ok(object.render_geometry.clone().map(Arc::new)),
        }
    }
    pub fn frame(&self) -> Option<&FrameState> {
        self.frame.as_ref()
    }

    pub const fn session(&self) -> Option<u32> {
        self.session
    }

    /// Sequence of the currently applied retained publication.
    pub fn applied_sequence(&self) -> Option<u64> {
        self.session.and_then(|_| self.next_sequence.checked_sub(1))
    }

    /// Resolve one durable transport slot without searching the frame.
    pub fn frame_index_for_slot(&self, slot: TransportSlotId) -> Option<usize> {
        self.slot_indices.get(&slot).copied()
    }

    pub const fn camera(&self) -> Camera2DState {
        self.camera
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), RetainedExecutionTransportError>
    {
        validate_envelope_header(&delta)?;

        match self.session {
            None => {
                if !delta.snapshot || delta.sequence != 0 {
                    return Err(RetainedExecutionTransportError::SessionRequiresSnapshot {
                        session: delta.session,
                        sequence: delta.sequence,
                    });
                }
            }
            Some(session) if session != delta.session => {
                if !delta.snapshot || delta.sequence != 0 {
                    return Err(RetainedExecutionTransportError::SessionRequiresSnapshot {
                        session: delta.session,
                        sequence: delta.sequence,
                    });
                }
            }
            Some(_) if delta.sequence < self.next_sequence => {
                return Ok((
                    RetainedTransportApplyOutcome::DroppedStale,
                    FrameChanges::default(),
                ));
            }
            Some(_) if delta.sequence != self.next_sequence => {
                return Err(RetainedExecutionTransportError::SequenceGap {
                    expected: self.next_sequence,
                    actual: delta.sequence,
                });
            }
            Some(_) => {}
        }

        let next_sequence = delta
            .sequence
            .checked_add(1)
            .ok_or(RetainedExecutionTransportError::SequenceExhausted)?;
        let changes = if delta.snapshot {
            self.apply_snapshot(&delta)?;
            FrameChanges::all()
        } else {
            self.apply_incremental(&delta)?
        };
        self.session = Some(delta.session);
        self.next_sequence = next_sequence;
        self.camera = delta.camera;
        if let Some(frame) = &mut self.frame {
            frame.time = delta.time;
        }
        Ok((RetainedTransportApplyOutcome::Applied, changes))
    }

    fn apply_snapshot(
        &mut self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), RetainedExecutionTransportError> {
        let mut objects = delta.objects.clone();
        objects.sort_by_key(|object| object.order);
        let mut seen_slots = HashSet::with_capacity(objects.len());
        let mut seen_objects = HashSet::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            let expected = u32::try_from(index)
                .map_err(|_| RetainedExecutionTransportError::InvalidOrder(object.order))?;
            if object.order != expected {
                return Err(RetainedExecutionTransportError::InvalidOrder(object.order));
            }
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            if !seen_objects.insert(object.object) {
                return Err(RetainedExecutionTransportError::DuplicateObject(
                    object.object,
                ));
            }
            validate_object_state(object)?;
        }

        let render_geometries = objects
            .iter()
            .map(|object| self.resolve_render_geometry(object, delta.session))
            .collect::<Result<Vec<_>, _>>()?;
        let frame_objects = objects
            .iter()
            .map(|object| {
                self.resolve_content(&object.content)
                    .map(|content| frame_object(object, content))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let slots = objects.iter().map(|object| object.slot).collect::<Vec<_>>();
        let slot_indices = slots
            .iter()
            .copied()
            .enumerate()
            .map(|(index, slot)| (slot, index))
            .collect();
        self.slots = slots;
        self.slot_indices = slot_indices;
        self.frame = Some(FrameState {
            time: delta.time,
            objects: frame_objects,
            presences: objects.iter().map(|object| object.presence).collect(),
            reveals: objects.iter().map(|object| object.reveal).collect(),
            morphs: objects.iter().map(|object| object.morph).collect(),
            render_geometries,
            render_transforms: objects
                .iter()
                .map(|object| object.render_transform)
                .collect(),
        });
        Ok(())
    }

    fn apply_incremental(
        &mut self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<FrameChanges, RetainedExecutionTransportError> {
        let frame = self
            .frame
            .as_ref()
            .ok_or(RetainedExecutionTransportError::IncrementalBeforeSnapshot)?;
        let mut updates = Vec::with_capacity(delta.objects.len());
        let mut seen_slots = HashSet::with_capacity(delta.objects.len());
        for object in &delta.objects {
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            validate_object_state(object)?;
            let index = self
                .slot_indices
                .get(&object.slot)
                .copied()
                .ok_or(RetainedExecutionTransportError::UnknownSlot(object.slot))?;
            let expected_order = u32::try_from(index)
                .map_err(|_| RetainedExecutionTransportError::InvalidOrder(object.order))?;
            if object.order != expected_order {
                return Err(RetainedExecutionTransportError::InvalidOrder(object.order));
            }
            let current = &frame.objects[index];
            if current.id != object.object {
                return Err(RetainedExecutionTransportError::SlotIdentityChanged(
                    object.slot,
                ));
            }
            let content = self.resolve_content(&object.content)?;
            if !incremental_content_identity_matches(&current.content, &content) {
                return Err(RetainedExecutionTransportError::ContentIdentityChanged(
                    object.slot,
                ));
            }
            let geometry = self.resolve_render_geometry(object, delta.session)?;
            updates.push((index, object, geometry, content));
        }
        // Validate all rows and resource references before mutating the live mirror.
        let frame = self.frame.as_mut().expect("validated retained frame");
        let mut changed = Vec::with_capacity(updates.len());
        for (index, object, geometry, content) in updates {
            frame.objects[index] = frame_object(object, content);
            frame.presences[index] = object.presence;
            frame.reveals[index] = object.reveal;
            frame.morphs[index] = object.morph;
            frame.render_geometries[index] = geometry;
            frame.render_transforms[index] = object.render_transform;
            changed.push(index);
        }
        Ok(FrameChanges::objects(changed))
    }
}

fn incremental_content_identity_matches(
    current: &ObjectContentRef,
    next: &ObjectContentRef,
) -> bool {
    match (current, next) {
        (ObjectContentRef::Geometry(_), ObjectContentRef::Geometry(_)) => true,
        (ObjectContentRef::Text(current), ObjectContentRef::Text(next)) => current == next,
        _ => false,
    }
}

fn validate_envelope_header(
    delta: &RetainedExecutionDeltaEnvelope,
) -> Result<(), RetainedExecutionTransportError> {
    if delta.channel != RETAINED_EXECUTION_TRANSPORT_CHANNEL {
        return Err(RetainedExecutionTransportError::InvalidChannel(
            delta.channel.clone(),
        ));
    }
    if delta.protocol_version != RETAINED_EXECUTION_TRANSPORT_VERSION {
        return Err(RetainedExecutionTransportError::UnsupportedVersion(
            delta.protocol_version,
        ));
    }
    validate_time(delta.time)
}

fn validate_time(time: f64) -> Result<(), RetainedExecutionTransportError> {
    if time.is_finite() {
        Ok(())
    } else {
        Err(RetainedExecutionTransportError::InvalidTime(time))
    }
}

fn validate_frame_shape(frame: &FrameState) -> Result<(), RetainedExecutionTransportError> {
    let count = frame.objects.len();
    if frame.presences.len() == count
        && frame.reveals.len() == count
        && frame.morphs.len() == count
        && frame.render_geometries.len() == count
        && frame.render_transforms.len() == count
    {
        Ok(())
    } else {
        Err(RetainedExecutionTransportError::FrameShapeMismatch)
    }
}

fn transport_object(
    frame: &FrameState,
    index: usize,
    render_geometry_resource: Option<u32>,
) -> Result<RetainedTransportObjectState, RetainedExecutionTransportError> {
    let object = frame
        .objects
        .get(index)
        .ok_or(RetainedExecutionTransportError::InvalidObjectIndex(index))?;
    let slot_index = u32::try_from(index)
        .map_err(|_| RetainedExecutionTransportError::InvalidObjectIndex(index))?;
    let state = RetainedTransportObjectState {
        slot: TransportSlotId {
            slot: slot_index,
            generation: 0,
        },
        order: slot_index,
        object: object.id,
        content: (&object.content).into(),
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
        text_bounds: object.text_bounds,
        presence: frame.presences[index],
        reveal: frame.reveals[index],
        morph: frame.morphs[index],
        render_geometry: if render_geometry_resource.is_none() {
            frame.render_geometries[index].as_deref().cloned()
        } else {
            None
        },
        render_transform: frame.render_transforms[index],
        render_geometry_resource,
    };
    validate_object_state(&state)?;
    Ok(state)
}

fn validate_object_state(
    object: &RetainedTransportObjectState,
) -> Result<(), RetainedExecutionTransportError> {
    if let Some(transform) = object.render_transform {
        if !transform.translation.x.is_finite()
            || !transform.translation.y.is_finite()
            || !transform.scale.x.is_finite()
            || !transform.scale.y.is_finite()
            || !transform.rotation.is_finite()
            || (object.render_geometry.is_none() && object.render_geometry_resource.is_none())
        {
            return Err(RetainedExecutionTransportError::InvalidRenderTransform(
                object.slot,
            ));
        }
    }
    if object.render_geometry.is_some() && object.render_geometry_resource.is_some() {
        return Err(RetainedExecutionTransportError::AmbiguousRenderGeometry(
            object.slot,
        ));
    }
    if matches!(&object.content, TransportObjectContent::Text { .. })
        && (object.render_geometry.is_some()
            || object.render_geometry_resource.is_some()
            || object.render_transform.is_some())
    {
        return Err(RetainedExecutionTransportError::TextRenderGeometry(
            object.slot,
        ));
    }
    Ok(())
}

fn frame_object(
    object: &RetainedTransportObjectState,
    content: ObjectContentRef,
) -> FrameObjectState {
    FrameObjectState {
        id: object.object,
        content,
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
        text_bounds: object.text_bounds,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Color, TextResourceId, Vec2};

    use super::*;

    fn test_mirror() -> RetainedExecutionFrameMirror {
        let handles = [
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(7),
                version: 3,
            },
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(8),
                version: 1,
            },
        ]
        .into_iter()
        .map(|handle| {
            (
                TransportTextResourceHandle::from_source_handle(handle),
                handle,
            )
        })
        .collect();
        RetainedExecutionFrameMirror::with_installed_resources(None, Arc::from([]), handles)
    }

    fn test_mirror_with_render_geometries(
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
    ) -> RetainedExecutionFrameMirror {
        let mut mirror = test_mirror();
        mirror.resource_session = Some(session);
        mirror.render_geometries = geometries;
        mirror
    }

    fn mixed_frame() -> FrameState {
        let text = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(7),
            version: 3,
        };
        FrameState {
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    id: ObjectId::new(12),
                    content: ObjectContentRef::Text(text),
                    transform: Transform2D::IDENTITY,
                    style: Style {
                        fill: Some(Color::WHITE),
                        ..Style::default()
                    },
                    appearance: 1.0,
                    text_bounds: None,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        }
    }

    #[test]
    fn mixed_geometry_and_text_snapshot_round_trips_in_one_order_stream() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let delta = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        assert_eq!(delta.objects.len(), 2);
        assert_eq!(delta.objects[0].order, 0);
        assert_eq!(delta.objects[1].order, 1);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        assert!(matches!(
            delta.objects[1].content,
            TransportObjectContent::Text { .. }
        ));

        let json = serde_json::to_string(&delta).unwrap();
        let decoded: RetainedExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        let mut mirror = test_mirror();
        let (outcome, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap(), &frame);
    }

    #[test]
    fn wire_text_requires_an_installed_resource_remap() {
        let frame = mixed_frame();
        let delta = RetainedExecutionDeltaEncoder::new(4)
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::UnknownTextResource(_))
        ));
        assert!(mirror.frame().is_none());
    }

    #[test]
    fn incremental_geometry_content_update_preserves_slot_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(8);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut updated = frame.clone();
        updated.time = 0.5;
        updated.objects[0].content = ObjectContentRef::Geometry(GeometryRef::rectangle(2.0, 1.0));
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
        assert_eq!(mirror.frame().unwrap(), &updated);
    }

    #[test]
    fn incremental_transform_keeps_text_content_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut updated = frame.clone();
        updated.time = 0.5;
        updated.objects[1].transform.translation = Vec2::new(2.0, -1.0);
        let delta = encoder
            .encode_incremental(
                &updated,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.objects.len(), 1);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Text { .. }
        ));

        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[1]);
        assert_eq!(mirror.frame().unwrap(), &updated);
    }

    #[test]
    fn incremental_text_resource_change_requires_snapshot() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(10);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut changed = frame.clone();
        changed.objects[1].content = ObjectContentRef::Text(TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(8),
            version: 1,
        });
        let delta = encoder
            .encode_incremental(
                &changed,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::ContentIdentityChanged(_))
        ));
    }

    #[test]
    fn incremental_content_swap_requires_snapshot() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(3);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut changed = frame.clone();
        changed.objects[1].content = ObjectContentRef::Geometry(GeometryRef::circle(0.5));
        let delta = encoder
            .encode_incremental(
                &changed,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::ContentIdentityChanged(_))
        ));
    }

    #[test]
    fn text_never_accepts_transient_geometry() {
        let mut frame = mixed_frame();
        frame.render_geometries[1] = Some(Arc::new(GeometryRef::circle(0.25)));
        let mut encoder = RetainedExecutionDeltaEncoder::new(1);
        assert!(matches!(
            encoder.encode_snapshot(&frame, Camera2DState::default()),
            Err(RetainedExecutionTransportError::TextRenderGeometry(_))
        ));
    }

    #[test]
    fn immutable_morph_resources_round_trip_without_inline_paths_and_survive_seek() {
        let path = Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        ));
        let resources: Arc<[Arc<GeometryRef>]> = vec![path.clone()].into();
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(path);
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        let mut encoder =
            RetainedExecutionDeltaEncoder::with_render_geometries(4, resources.clone());
        let mut mirror = test_mirror_with_render_geometries(4, resources);
        for (step, time) in [0.0, 0.25, 0.75, 0.0].into_iter().enumerate() {
            frame.time = time;
            frame.morphs[0] = time as f32;
            let delta = if step == 0 || step == 3 {
                encoder
                    .encode_snapshot(&frame, Camera2DState::default())
                    .unwrap()
            } else {
                encoder
                    .encode_incremental(
                        &frame,
                        &FrameChanges::objects(vec![0]),
                        Camera2DState::default(),
                    )
                    .unwrap()
                    .unwrap()
            };
            assert_eq!(delta.objects[0].render_geometry_resource, Some(0));
            assert!(delta.objects[0].render_geometry.is_none());
            let json = serde_json::to_string(&delta).unwrap();
            assert!(!json.contains("line_to"));
            mirror.apply(serde_json::from_str(&json).unwrap()).unwrap();
            assert_eq!(mirror.frame(), Some(&frame));
        }
    }

    #[test]
    fn unregistered_compiled_path_cannot_silently_fall_back_to_inline_transport() {
        let path = noon_core::VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0));
        let mut encoder = RetainedExecutionDeltaEncoder::with_render_geometries(
            4,
            vec![Arc::new(GeometryRef::path(path.clone()))].into(),
        );
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(path)));
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        assert!(matches!(
            encoder.encode_snapshot(&frame, Camera2DState::default()),
            Err(RetainedExecutionTransportError::MissingCompiledRenderResource(_))
        ));
    }

    #[test]
    fn invalid_later_resource_row_does_not_publish_earlier_rows_or_consume_sequence() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let mut mirror = test_mirror();
        mirror
            .apply(
                encoder
                    .encode_snapshot(&frame, Camera2DState::default())
                    .unwrap(),
            )
            .unwrap();
        let mut changed = frame.clone();
        changed.time = 0.5;
        changed.objects[1].transform.translation.x = 2.0;
        let mut valid = encoder
            .encode_incremental(
                &changed,
                &FrameChanges::objects(vec![1, 0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        valid.objects.swap(0, 1);
        let mut invalid = valid.clone();
        invalid.objects[1].render_geometry_resource = Some(99);
        assert!(matches!(
            mirror.apply(invalid),
            Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(99))
        ));
        assert_eq!(mirror.frame(), Some(&frame));
        mirror.apply(valid).unwrap();
        assert_eq!(mirror.frame(), Some(&changed));
    }

    #[test]
    fn geometry_resource_indices_are_scoped_to_the_installed_session() {
        let path = Arc::new(GeometryRef::path(noon_core::VectorPath::new()));
        let resources: Arc<[Arc<GeometryRef>]> = vec![path.clone()].into();
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(path);
        let mut encoder =
            RetainedExecutionDeltaEncoder::with_render_geometries(5, resources.clone());
        let delta = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror_with_render_geometries(4, resources);
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(0))
        ));
        assert!(mirror.frame().is_none());
    }
}
