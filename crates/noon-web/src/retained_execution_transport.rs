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

/// One final-order splice over stable retained transport slots.
///
/// `slots` is the authoritative final segment beginning at `start`. Rows keep
/// their dense mirror indices; only the renderer's derived painter permutation
/// changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedPainterOrderDelta {
    pub start: u32,
    pub end: u32,
    pub slots: Vec<TransportSlotId>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_slots: Vec<TransportSlotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub painter_order: Option<RetainedPainterOrderDelta>,
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
            removed_slots: Vec::new(),
            painter_order: None,
        })
    }

    pub fn encode_incremental(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        self.encode_incremental_inner(frame, changes, camera, None)
    }

    /// Encode a sparse structural/order publication over stable worker rows.
    /// Only dirty/new object rows, removed slot identities, and the affected
    /// final painter segment cross the genuine worker boundary.
    pub fn encode_incremental_with_painter_order(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
        painter_order: &[u32],
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        self.encode_incremental_inner(frame, changes, camera, Some(painter_order))
    }

    fn encode_incremental_inner(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
        painter_order: Option<&[u32]>,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        if !self.initialized {
            return Err(RetainedExecutionTransportError::IncrementalBeforeSnapshot);
        }
        if changes.is_structural() && painter_order.is_none() {
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
        let removed_indices = changes
            .removed_indices()
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let removed_slots = changes
            .removed_indices()
            .iter()
            .copied()
            .map(|index| {
                self.transport_object(frame, index)
                    .map(|object| object.slot)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (order_updates, painter_order) = if let Some(order) = painter_order {
            let delta = changes
                .painter_order_range()
                .map(|range| {
                    let end = range.end.min(order.len());
                    let mut seen = HashSet::with_capacity(end.saturating_sub(range.start));
                    let mut updates = Vec::with_capacity(end.saturating_sub(range.start));
                    let slots = order[range.start.min(end)..end]
                        .iter()
                        .enumerate()
                        .map(|(offset, &index)| {
                            let index = index as usize;
                            if !seen.insert(index) {
                                return Err(RetainedExecutionTransportError::InvalidObjectIndex(
                                    index,
                                ));
                            }
                            let rank = u32::try_from(range.start + offset).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(index)
                            })?;
                            updates.push((index, rank));
                            self.transport_object(frame, index)
                                .map(|object| object.slot)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok::<_, RetainedExecutionTransportError>((
                        updates,
                        RetainedPainterOrderDelta {
                            start: u32::try_from(range.start).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(range.start)
                            })?,
                            end: u32::try_from(range.end).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(range.end)
                            })?,
                            slots,
                        },
                    ))
                })
                .transpose()?;
            match delta {
                Some((updates, delta)) => (updates, Some(delta)),
                None => (Vec::new(), None),
            }
        } else {
            (Vec::new(), None)
        };
        let order_update_map = order_updates.iter().copied().collect::<HashMap<_, _>>();
        let objects = changes
            .object_indices()
            .iter()
            .copied()
            .filter(|index| !removed_indices.contains(index))
            .map(|index| {
                let mut object = self.transport_object(frame, index)?;
                object.order = order_update_map
                    .get(&index)
                    .copied()
                    .or_else(|| self.snapshot_orders.get(index).copied().flatten())
                    .ok_or(RetainedExecutionTransportError::UnknownSlot(object.slot))?;
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        for index in &removed_indices {
            if let Some(order) = self.snapshot_orders.get_mut(*index) {
                *order = None;
            }
        }
        if self.snapshot_orders.len() < frame.objects.len() {
            self.snapshot_orders.resize(frame.objects.len(), None);
        }
        for (index, rank) in order_updates {
            self.snapshot_orders[index] = Some(rank);
        }
        Ok(Some(RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot: false,
            time: frame.time,
            camera,
            objects,
            removed_slots,
            painter_order,
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
    object_indices: HashMap<ObjectId, usize>,
    render_geometries: Arc<[Arc<GeometryRef>]>,
    resource_session: Option<u32>,
    text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    camera: Camera2DState,
    frame: Option<FrameState>,
    painter_order: Vec<u32>,
    painter_ranks: Vec<Option<u32>>,
}

struct PreparedPainterOrder {
    range: std::ops::Range<usize>,
    old_end: usize,
    segment: Vec<u32>,
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

    pub(crate) fn extend_installed_text_handles(
        &mut self,
        additions: &HashMap<TransportTextResourceHandle, TextResourceHandle>,
    ) {
        self.text_handles.extend(
            additions
                .iter()
                .map(|(&transport, &local)| (transport, local)),
        );
    }

    pub(crate) fn remove_installed_text_handles<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a TransportTextResourceHandle>,
    ) {
        for handle in handles {
            self.text_handles.remove(handle);
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

    /// Resolve one semantic execution identity without searching the dense frame.
    pub(crate) fn frame_index_for_object(&self, object: ObjectId) -> Option<usize> {
        self.object_indices.get(&object).copied()
    }

    /// Resolve the authored fields of one sparse transport row without mutating the mirror.
    pub(crate) fn resolve_transport_object_state(
        &self,
        object: &RetainedTransportObjectState,
    ) -> Result<FrameObjectState, RetainedExecutionTransportError> {
        validate_object_state(object)?;
        let content = self.resolve_content(&object.content)?;
        Ok(frame_object(object, content))
    }

    pub const fn camera(&self) -> Camera2DState {
        self.camera
    }

    /// Dense mirror-row indices in authoritative engine painter order.
    pub fn painter_order(&self) -> &[u32] {
        &self.painter_order
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
        let object_indices = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.object, index))
            .collect();
        self.slots = slots;
        self.slot_indices = slot_indices;
        self.object_indices = object_indices;
        self.painter_order = (0..objects.len() as u32).collect();
        self.painter_ranks = (0..objects.len() as u32).map(Some).collect();
        self.frame = Some(FrameState {
            family_animations: vec![None; objects.len()],
            family_animation_plan_indices: vec![None; objects.len()],
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
        let mut added_slot_indices = HashMap::new();
        let mut added_objects = HashSet::new();
        let mut next_slot_count = self.slots.len();
        for object in &delta.objects {
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            validate_object_state(object)?;
            let content = self.resolve_content(&object.content)?;
            let geometry = self.resolve_render_geometry(object, delta.session)?;
            let (index, added) = if let Some(&index) = self.slot_indices.get(&object.slot) {
                let current = &frame.objects[index];
                if current.id != object.object {
                    return Err(RetainedExecutionTransportError::SlotIdentityChanged(
                        object.slot,
                    ));
                }
                if !incremental_content_identity_matches(&current.content, &content) {
                    return Err(RetainedExecutionTransportError::ContentIdentityChanged(
                        object.slot,
                    ));
                }
                (index, false)
            } else {
                if self.object_indices.contains_key(&object.object)
                    || !added_objects.insert(object.object)
                {
                    return Err(RetainedExecutionTransportError::DuplicateObject(
                        object.object,
                    ));
                }
                let index = next_slot_count;
                next_slot_count += 1;
                added_slot_indices.insert(object.slot, index);
                (index, true)
            };
            updates.push((index, added, object, geometry, content));
        }
        let mut seen_removed = HashSet::with_capacity(delta.removed_slots.len());
        let _validated_removed_indices = delta
            .removed_slots
            .iter()
            .map(|slot| {
                if !seen_removed.insert(*slot) || seen_slots.contains(slot) {
                    return Err(RetainedExecutionTransportError::DuplicateSlot(*slot));
                }
                self.slot_indices
                    .get(slot)
                    .copied()
                    .ok_or(RetainedExecutionTransportError::UnknownSlot(*slot))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let painter_update = self.validate_painter_order_delta(
            delta.painter_order.as_ref(),
            &added_slot_indices,
            &seen_removed,
            &seen_slots,
        )?;
        let segment_ranks = painter_update
            .as_ref()
            .map(|PreparedPainterOrder { range, segment, .. }| {
                segment
                    .iter()
                    .enumerate()
                    .map(|(offset, &index)| (index, range.start + offset))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        for (index, _, object, _, _) in &updates {
            let rank = segment_ranks
                .get(&(*index as u32))
                .copied()
                .or_else(|| {
                    self.painter_ranks
                        .get(*index)
                        .copied()
                        .flatten()
                        .map(|rank| rank as usize)
                })
                .ok_or(RetainedExecutionTransportError::InvalidOrder(object.order))?;
            if object.order as usize != rank {
                return Err(RetainedExecutionTransportError::InvalidOrder(object.order));
            }
        }
        let added_indices = updates
            .iter()
            .filter_map(|(index, added, _, _, _)| {
                (*added || self.painter_ranks.get(*index).copied().flatten().is_none())
                    .then_some(*index)
            })
            .collect::<Vec<_>>();
        let removed_indices = _validated_removed_indices;
        // Validate all rows and resource references before mutating the live mirror.
        let frame = self.frame.as_mut().expect("validated retained frame");
        let mut changed = Vec::with_capacity(updates.len());
        for (index, is_added, object, geometry, content) in updates {
            if is_added {
                debug_assert_eq!(index, frame.objects.len());
                self.slots.push(object.slot);
                self.slot_indices.insert(object.slot, index);
                self.object_indices.insert(object.object, index);
                frame.objects.push(frame_object(object, content));
                frame.presences.push(object.presence);
                frame.reveals.push(object.reveal);
                frame.morphs.push(object.morph);
                frame.render_geometries.push(geometry);
                frame.render_transforms.push(object.render_transform);
            } else {
                frame.objects[index] = frame_object(object, content);
                frame.presences[index] = object.presence;
                frame.reveals[index] = object.reveal;
                frame.morphs[index] = object.morph;
                frame.render_geometries[index] = geometry;
                frame.render_transforms[index] = object.render_transform;
            }
            changed.push(index);
        }
        let painter_range = painter_update.map(
            |PreparedPainterOrder {
                 range,
                 old_end,
                 segment,
             }| {
                for &index in &self.painter_order[range.start..old_end] {
                    self.painter_ranks[index as usize] = None;
                }
                if self.painter_ranks.len() < frame.objects.len() {
                    self.painter_ranks.resize(frame.objects.len(), None);
                }
                let next_end = range.start + segment.len();
                self.painter_order.splice(range.start..old_end, segment);
                for (rank, &index) in self.painter_order[range.start..next_end].iter().enumerate() {
                    self.painter_ranks[index as usize] = Some((range.start + rank) as u32);
                }
                range
            },
        );
        let mut changes = FrameChanges::with_structure(changed, added_indices, removed_indices);
        if let Some(range) = painter_range {
            changes = changes.with_painter_order(range);
        }
        Ok(changes)
    }

    fn validate_painter_order_delta(
        &self,
        delta: Option<&RetainedPainterOrderDelta>,
        added_slot_indices: &HashMap<TransportSlotId, usize>,
        removed: &HashSet<TransportSlotId>,
        updated: &HashSet<TransportSlotId>,
    ) -> Result<Option<PreparedPainterOrder>, RetainedExecutionTransportError> {
        if delta.is_none() {
            if !removed.is_empty() || !added_slot_indices.is_empty() {
                return Err(RetainedExecutionTransportError::StructuralChangeRequiresSnapshot);
            }
            return Ok(None);
        }
        let delta = delta.expect("checked painter delta");
        let start = delta.start as usize;
        let end = delta.end as usize;
        if end < start {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        let old_end = end.min(self.painter_order.len());
        if start > old_end {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.start));
        }
        let old_segment = self.painter_order[start..old_end]
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut segment = Vec::with_capacity(delta.slots.len());
        let mut seen = HashSet::with_capacity(delta.slots.len());
        for slot in &delta.slots {
            if removed.contains(slot) || !seen.insert(*slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(*slot));
            }
            let index = self
                .slot_indices
                .get(slot)
                .or_else(|| added_slot_indices.get(slot))
                .copied()
                .ok_or(RetainedExecutionTransportError::UnknownSlot(*slot))?;
            segment.push(index as u32);
        }
        let segment_set = segment.iter().copied().collect::<HashSet<_>>();
        if removed.iter().any(|slot| {
            self.slot_indices
                .get(slot)
                .is_none_or(|index| !old_segment.contains(&(*index as u32)))
        }) {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        if old_segment.iter().any(|index| {
            !segment_set.contains(index) && !removed.contains(&self.slots[*index as usize])
        }) {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        let mut added = added_slot_indices
            .values()
            .map(|&index| index as u32)
            .collect::<HashSet<_>>();
        for slot in updated {
            if let Some(&index) = self.slot_indices.get(slot) {
                if self.painter_ranks.get(index).copied().flatten().is_none() {
                    added.insert(index as u32);
                }
            }
        }
        if segment
            .iter()
            .any(|index| !old_segment.contains(index) && !added.contains(index))
            || added.iter().any(|index| !segment_set.contains(index))
        {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        Ok(Some(PreparedPainterOrder {
            range: start..end,
            old_end,
            segment,
        }))
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
            family_animations: vec![None; 2],
            family_animation_plan_indices: vec![None; 2],
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
    fn compact_painter_splice_reorders_and_replaces_without_resending_live_rows() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(19);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let reorder = encoder
            .encode_incremental_with_painter_order(
                &frame,
                &FrameChanges::painter_order(0..2),
                Camera2DState::default(),
                &[1, 0],
            )
            .unwrap()
            .unwrap();
        assert!(reorder.objects.is_empty());
        assert!(reorder.removed_slots.is_empty());
        assert_eq!(reorder.painter_order.as_ref().unwrap().slots.len(), 2);
        let (_, changes) = mirror.apply(reorder).unwrap();
        assert_eq!(changes.painter_order_range(), Some(0..2));
        assert_eq!(mirror.painter_order(), &[1, 0]);

        let mut replaced = frame.clone();
        replaced.objects.push(FrameObjectState {
            id: ObjectId::new(13),
            content: ObjectContentRef::Geometry(GeometryRef::rectangle(3.0, 1.0)),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            text_bounds: None,
        });
        replaced.presences.push(true);
        replaced.reveals.push(1.0);
        replaced.morphs.push(0.0);
        replaced.render_geometries.push(None);
        replaced.render_transforms.push(None);
        let structural =
            FrameChanges::with_structure(vec![0, 2], vec![2], vec![0]).with_painter_order(0..2);
        let replace = encoder
            .encode_incremental_with_painter_order(
                &replaced,
                &structural,
                Camera2DState::default(),
                &[2, 1],
            )
            .unwrap()
            .unwrap();
        assert_eq!(replace.objects.len(), 1);
        assert_eq!(replace.removed_slots.len(), 1);
        let mut malformed = replace.clone();
        malformed.painter_order.as_mut().unwrap().slots[1] =
            malformed.painter_order.as_ref().unwrap().slots[0];
        let before_frame = mirror.frame().unwrap().clone();
        let before_order = mirror.painter_order().to_vec();
        assert!(matches!(
            mirror.apply(malformed),
            Err(RetainedExecutionTransportError::DuplicateSlot(_))
        ));
        assert_eq!(mirror.frame().unwrap(), &before_frame);
        assert_eq!(mirror.painter_order(), before_order);

        let (_, changes) = mirror.apply(replace).unwrap();
        assert_eq!(changes.added_indices(), &[2]);
        assert_eq!(changes.removed_indices(), &[0]);
        assert_eq!(changes.painter_order_range(), Some(0..2));
        assert_eq!(mirror.painter_order(), &[2, 1]);
        assert_eq!(mirror.frame().unwrap().objects[2].id, ObjectId::new(13));
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
