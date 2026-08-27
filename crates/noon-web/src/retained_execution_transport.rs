use std::collections::HashSet;

use noon_core::{
    Camera2DState, GeometryRef, ObjectContentRef, ObjectId, Style, TextResourceHandle,
    TextResourceId, Transform2D,
};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use serde::{Deserialize, Serialize};

use crate::TransportSlotId;

/// Resource-aware execution channel for the retained geometry/text runtime.
///
/// The legacy `noon.execution` v1 channel stays geometry-only. This channel makes
/// object content explicit so text can occupy the same identity/order stream as
/// geometry without a fake `GeometryRef` variant or placeholder object.
pub const RETAINED_EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution.retained";
pub const RETAINED_EXECUTION_TRANSPORT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransportTextResourceHandle {
    pub id: u64,
    pub version: u64,
}

impl From<TextResourceHandle> for TransportTextResourceHandle {
    fn from(value: TextResourceHandle) -> Self {
        Self {
            id: value.id.get(),
            version: value.version,
        }
    }
}

impl From<TransportTextResourceHandle> for TextResourceHandle {
    fn from(value: TransportTextResourceHandle) -> Self {
        Self {
            id: TextResourceId::new(value.id),
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
                text: (*text).into(),
            },
        }
    }
}

impl From<TransportObjectContent> for ObjectContentRef {
    fn from(value: TransportObjectContent) -> Self {
        match value {
            TransportObjectContent::Geometry { geometry } => Self::Geometry(geometry),
            TransportObjectContent::Text { text } => Self::Text(text.into()),
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
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    pub render_geometry: Option<GeometryRef>,
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
}

impl std::fmt::Display for RetainedExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid retained execution transport channel {channel:?}")
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
        }
    }
}

impl std::error::Error for RetainedExecutionTransportError {}

#[derive(Clone, Debug)]
pub struct RetainedExecutionDeltaEncoder {
    session: u32,
    next_sequence: u64,
    initialized: bool,
}

impl RetainedExecutionDeltaEncoder {
    pub const fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
            initialized: false,
        }
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFrameState,
        camera: Camera2DState,
    ) -> Result<RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        let objects = (0..frame.objects.len())
            .map(|index| transport_object(frame, index))
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        self.initialized = true;
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
        frame: &RetainedFrameState,
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
            return self.encode_snapshot(frame, camera).map(Some);
        }
        if changes.is_empty() {
            return Ok(None);
        }
        let objects = changes
            .object_indices()
            .iter()
            .copied()
            .map(|index| transport_object(frame, index))
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
    camera: Camera2DState,
    frame: Option<RetainedFrameState>,
}

impl RetainedExecutionFrameMirror {
    pub fn frame(&self) -> Option<&RetainedFrameState> {
        self.frame.as_ref()
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

        let changes = if delta.snapshot {
            self.apply_snapshot(&delta)?;
            FrameChanges::all()
        } else {
            self.apply_incremental(&delta)?
        };
        self.session = Some(delta.session);
        self.next_sequence = delta
            .sequence
            .checked_add(1)
            .ok_or(RetainedExecutionTransportError::SequenceExhausted)?;
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

        self.slots = objects.iter().map(|object| object.slot).collect();
        self.frame = Some(RetainedFrameState {
            time: delta.time,
            objects: objects.iter().map(frame_object).collect(),
            presences: objects.iter().map(|object| object.presence).collect(),
            reveals: objects.iter().map(|object| object.reveal).collect(),
            morphs: objects.iter().map(|object| object.morph).collect(),
            render_geometries: objects
                .iter()
                .map(|object| object.render_geometry.clone())
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
            .as_mut()
            .ok_or(RetainedExecutionTransportError::IncrementalBeforeSnapshot)?;
        let mut changed = Vec::with_capacity(delta.objects.len());
        let mut seen_slots = HashSet::with_capacity(delta.objects.len());
        for object in &delta.objects {
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            validate_object_state(object)?;
            let index = self
                .slots
                .iter()
                .position(|slot| *slot == object.slot)
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
            let content: ObjectContentRef = object.content.clone().into();
            if current.content != content {
                return Err(RetainedExecutionTransportError::ContentIdentityChanged(
                    object.slot,
                ));
            }
            frame.objects[index] = frame_object(object);
            frame.presences[index] = object.presence;
            frame.reveals[index] = object.reveal;
            frame.morphs[index] = object.morph;
            frame.render_geometries[index] = object.render_geometry.clone();
            changed.push(index);
        }
        Ok(FrameChanges::objects(changed))
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

fn validate_frame_shape(frame: &RetainedFrameState) -> Result<(), RetainedExecutionTransportError> {
    let count = frame.objects.len();
    if frame.presences.len() == count
        && frame.reveals.len() == count
        && frame.morphs.len() == count
        && frame.render_geometries.len() == count
    {
        Ok(())
    } else {
        Err(RetainedExecutionTransportError::FrameShapeMismatch)
    }
}

fn transport_object(
    frame: &RetainedFrameState,
    index: usize,
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
        presence: frame.presences[index],
        reveal: frame.reveals[index],
        morph: frame.morphs[index],
        render_geometry: frame.render_geometries[index].clone(),
    };
    validate_object_state(&state)?;
    Ok(state)
}

fn validate_object_state(
    object: &RetainedTransportObjectState,
) -> Result<(), RetainedExecutionTransportError> {
    if matches!(&object.content, TransportObjectContent::Text { .. })
        && object.render_geometry.is_some()
    {
        return Err(RetainedExecutionTransportError::TextRenderGeometry(
            object.slot,
        ));
    }
    Ok(())
}

fn frame_object(object: &RetainedTransportObjectState) -> RetainedFrameObjectState {
    RetainedFrameObjectState {
        id: object.object,
        content: object.content.clone().into(),
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Color, TextResourceId, Vec2};

    use super::*;

    fn mixed_frame() -> RetainedFrameState {
        let text = TextResourceHandle {
            id: TextResourceId::new(7),
            version: 3,
        };
        RetainedFrameState {
            time: 0.0,
            objects: vec![
                RetainedFrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                RetainedFrameObjectState {
                    id: ObjectId::new(12),
                    content: ObjectContentRef::Text(text),
                    transform: Transform2D::IDENTITY,
                    style: Style {
                        fill: Some(Color::WHITE),
                        ..Style::default()
                    },
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
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
        let mut mirror = RetainedExecutionFrameMirror::default();
        let (outcome, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap(), &frame);
    }

    #[test]
    fn incremental_transform_keeps_text_content_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
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
    fn incremental_content_swap_requires_snapshot() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(3);
        let initial = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
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
        frame.render_geometries[1] = Some(GeometryRef::circle(0.25));
        let mut encoder = RetainedExecutionDeltaEncoder::new(1);
        assert!(matches!(
            encoder.encode_snapshot(&frame, Camera2DState::default()),
            Err(RetainedExecutionTransportError::TextRenderGeometry(_))
        ));
    }
}
