use std::collections::{HashMap, HashSet};

use noon_core::{Camera2DState, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D};
use noon_runtime::{ExecutionSlotId, FrameChanges, FrameObjectState, FrameState};
use serde::{Deserialize, Serialize};

pub const EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution";
pub const EXECUTION_TRANSPORT_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransportSlotId {
    pub slot: u32,
    pub generation: u32,
}

impl From<ExecutionSlotId> for TransportSlotId {
    fn from(value: ExecutionSlotId) -> Self {
        Self {
            slot: value.slot(),
            generation: value.generation(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransportObjectState {
    pub slot: TransportSlotId,
    pub order: u32,
    pub object: ObjectId,
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    pub render_geometry: Option<GeometryRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_transform: Option<Transform2D>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionDeltaEnvelope {
    pub channel: String,
    pub protocol_version: u32,
    pub session: u32,
    pub sequence: u64,
    pub snapshot: bool,
    pub time: f64,
    pub layout_generation: u64,
    #[serde(default)]
    pub camera: Camera2DState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<TransportSlotId>,
    pub objects: Vec<TransportObjectState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportApplyOutcome {
    Applied,
    DroppedStale,
}

#[derive(Debug)]
pub enum ExecutionTransportError {
    Json(serde_json::Error),
    InvalidChannel(String),
    UnsupportedVersion(u32),
    InvalidTime(f64),
    InvalidCameraState,
    InvalidRenderTransform(TransportSlotId),
    SequenceExhausted,
    SessionRequiresSnapshot { session: u32, sequence: u64 },
    SequenceGap { expected: u64, actual: u64 },
    StructuralDeltaRequiresSnapshot,
    DuplicateSlot(TransportSlotId),
    UnknownSlot(TransportSlotId),
    SlotIdentityChanged(TransportSlotId),
    InvalidOrder(u32),
    DuplicateObject(ObjectId),
}

impl std::fmt::Display for ExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => error.fmt(formatter),
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid execution transport channel {channel:?}")
            }
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported execution transport version {version}"
                )
            }
            Self::InvalidRenderTransform(slot) => write!(
                formatter,
                "invalid render transform for slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::InvalidTime(time) => write!(formatter, "invalid execution delta time {time}"),
            Self::InvalidCameraState => {
                formatter.write_str("invalid execution transport camera state")
            }
            Self::SequenceExhausted => {
                formatter.write_str("execution transport sequence exhausted")
            }
            Self::SessionRequiresSnapshot { session, sequence } => write!(
                formatter,
                "execution session {session} must begin with snapshot sequence 0, got {sequence}"
            ),
            Self::SequenceGap { expected, actual } => write!(
                formatter,
                "execution delta sequence gap: expected {expected}, got {actual}"
            ),
            Self::StructuralDeltaRequiresSnapshot => formatter
                .write_str("execution transport structural changes require a complete snapshot"),
            Self::DuplicateSlot(slot) => write!(
                formatter,
                "duplicate execution transport slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::UnknownSlot(slot) => write!(
                formatter,
                "unknown execution transport slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::SlotIdentityChanged(slot) => write!(
                formatter,
                "execution transport slot {}:{} changed object identity without a snapshot",
                slot.slot, slot.generation
            ),
            Self::InvalidOrder(order) => {
                write!(
                    formatter,
                    "invalid execution transport render order {order}"
                )
            }
            Self::DuplicateObject(object) => {
                write!(
                    formatter,
                    "duplicate execution transport object {}",
                    object.get()
                )
            }
        }
    }
}

impl std::error::Error for ExecutionTransportError {}

impl From<serde_json::Error> for ExecutionTransportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionFrameMirror {
    session: Option<u32>,
    next_sequence: u64,
    layout_generation: u64,
    slots: Vec<TransportSlotId>,
    slot_indices: HashMap<TransportSlotId, usize>,
    object_slots: HashMap<ObjectId, TransportSlotId>,
    camera: Camera2DState,
    frame: Option<FrameState>,
}

impl ExecutionFrameMirror {
    pub fn frame(&self) -> Option<&FrameState> {
        self.frame.as_ref()
    }

    pub const fn camera(&self) -> Camera2DState {
        self.camera
    }

    pub fn session(&self) -> Option<u32> {
        self.session
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub const fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    pub fn frame_index_for_slot(&self, slot: TransportSlotId) -> Option<usize> {
        self.slot_indices.get(&slot).copied()
    }

    /// Returns the stable transport slot currently projected to a frame row.
    ///
    /// This is intentionally a read-only diagnostic seam: renderer-side
    /// instrumentation must be able to tie a packed instance back to the
    /// execution identity without deriving identity from a dense index.
    pub fn slot_for_frame_index(&self, frame_index: usize) -> Option<TransportSlotId> {
        self.slots.get(frame_index).copied()
    }

    pub fn live_object_count(&self) -> usize {
        self.slot_indices.len()
    }

    pub fn apply_json(
        &mut self,
        json: &str,
    ) -> Result<(TransportApplyOutcome, FrameChanges), ExecutionTransportError> {
        let delta: ExecutionDeltaEnvelope = serde_json::from_str(json)?;
        self.apply(delta)
    }

    pub fn apply(
        &mut self,
        delta: ExecutionDeltaEnvelope,
    ) -> Result<(TransportApplyOutcome, FrameChanges), ExecutionTransportError> {
        self.validate_envelope(&delta)?;

        if self.session != Some(delta.session) {
            if !delta.snapshot || delta.sequence != 0 {
                return Err(ExecutionTransportError::SessionRequiresSnapshot {
                    session: delta.session,
                    sequence: delta.sequence,
                });
            }
            self.reset_for_session(delta.session);
        }

        if delta.sequence < self.next_sequence {
            return Ok((TransportApplyOutcome::DroppedStale, FrameChanges::default()));
        }
        if delta.sequence > self.next_sequence {
            if !delta.snapshot {
                return Err(ExecutionTransportError::SequenceGap {
                    expected: self.next_sequence,
                    actual: delta.sequence,
                });
            }
            self.reset_for_session(delta.session);
            self.next_sequence = delta.sequence;
        }

        let changes = if delta.snapshot {
            self.apply_snapshot(&delta)?
        } else {
            self.apply_partial(&delta)?
        };
        self.camera = delta.camera;
        self.layout_generation = delta.layout_generation;
        self.next_sequence = delta
            .sequence
            .checked_add(1)
            .ok_or(ExecutionTransportError::SequenceExhausted)?;
        Ok((TransportApplyOutcome::Applied, changes))
    }

    fn validate_envelope(
        &self,
        delta: &ExecutionDeltaEnvelope,
    ) -> Result<(), ExecutionTransportError> {
        if delta.channel != EXECUTION_TRANSPORT_CHANNEL {
            return Err(ExecutionTransportError::InvalidChannel(
                delta.channel.clone(),
            ));
        }
        if delta.protocol_version != EXECUTION_TRANSPORT_VERSION {
            return Err(ExecutionTransportError::UnsupportedVersion(
                delta.protocol_version,
            ));
        }
        if !delta.time.is_finite() {
            return Err(ExecutionTransportError::InvalidTime(delta.time));
        }
        if !delta.camera.center.x.is_finite()
            || !delta.camera.center.y.is_finite()
            || !delta.camera.height.is_finite()
            || delta.camera.height <= 0.0
        {
            return Err(ExecutionTransportError::InvalidCameraState);
        }
        for object in &delta.objects {
            if let Some(transform) = object.render_transform {
                if !transform.translation.x.is_finite()
                    || !transform.translation.y.is_finite()
                    || !transform.scale.x.is_finite()
                    || !transform.scale.y.is_finite()
                    || !transform.rotation.is_finite()
                    || object.render_geometry.is_none()
                {
                    return Err(ExecutionTransportError::InvalidRenderTransform(object.slot));
                }
            }
        }
        Ok(())
    }

    fn reset_for_session(&mut self, session: u32) {
        self.session = Some(session);
        self.next_sequence = 0;
        self.layout_generation = 0;
        self.slots.clear();
        self.slot_indices.clear();
        self.object_slots.clear();
        self.camera = Camera2DState::default();
        self.frame = None;
    }

    fn apply_snapshot(
        &mut self,
        delta: &ExecutionDeltaEnvelope,
    ) -> Result<FrameChanges, ExecutionTransportError> {
        let count = delta.objects.len();
        let mut ordered = vec![None; count];
        let mut seen_slots = HashSet::with_capacity(count);
        let mut seen_objects = HashSet::with_capacity(count);
        for object in &delta.objects {
            if !seen_slots.insert(object.slot) {
                return Err(ExecutionTransportError::DuplicateSlot(object.slot));
            }
            if !seen_objects.insert(object.object) {
                return Err(ExecutionTransportError::DuplicateObject(object.object));
            }
            let order = object.order as usize;
            if order >= count || ordered[order].is_some() {
                return Err(ExecutionTransportError::InvalidOrder(object.order));
            }
            ordered[order] = Some(object.clone());
        }
        if !delta.removed.is_empty() {
            return Err(ExecutionTransportError::StructuralDeltaRequiresSnapshot);
        }

        self.slots.clear();
        self.slot_indices.clear();
        self.object_slots.clear();
        let mut frame = FrameState {
            family_animations: vec![None; count],
            family_animation_plan_indices: vec![None; count],
            time: delta.time,
            objects: Vec::with_capacity(count),
            presences: Vec::with_capacity(count),
            reveals: Vec::with_capacity(count),
            morphs: Vec::with_capacity(count),
            render_geometries: Vec::with_capacity(count),
            render_transforms: Vec::with_capacity(count),
        };
        for (index, object) in ordered.into_iter().enumerate() {
            let object = object.ok_or(ExecutionTransportError::InvalidOrder(index as u32))?;
            self.slots.push(object.slot);
            self.slot_indices.insert(object.slot, index);
            self.object_slots.insert(object.object, object.slot);
            push_frame_object(&mut frame, object);
        }
        self.frame = Some(frame);
        Ok(FrameChanges::all())
    }

    fn apply_partial(
        &mut self,
        delta: &ExecutionDeltaEnvelope,
    ) -> Result<FrameChanges, ExecutionTransportError> {
        let frame =
            self.frame
                .as_mut()
                .ok_or(ExecutionTransportError::SessionRequiresSnapshot {
                    session: delta.session,
                    sequence: delta.sequence,
                })?;
        frame.time = delta.time;

        let mut removed_indices = Vec::with_capacity(delta.removed.len());
        let mut seen_removed = HashSet::with_capacity(delta.removed.len());
        for &slot in delta.removed.iter() {
            if !seen_removed.insert(slot) {
                continue;
            }
            let index = self
                .slot_indices
                .remove(&slot)
                .ok_or(ExecutionTransportError::UnknownSlot(slot))?;
            let object = frame.objects[index].id;
            self.object_slots.remove(&object);
            frame.presences[index] = false;
            frame.render_geometries[index] = None;
            frame.render_transforms[index] = None;
            removed_indices.push(index);
        }

        let mut changed = Vec::with_capacity(delta.objects.len());
        let mut added_indices = Vec::new();
        let mut seen = HashSet::with_capacity(delta.objects.len());
        for object in &delta.objects {
            if !seen.insert(object.slot) {
                return Err(ExecutionTransportError::DuplicateSlot(object.slot));
            }
            if let Some(&index) = self.slot_indices.get(&object.slot) {
                if object.order as usize != index {
                    return Err(ExecutionTransportError::InvalidOrder(object.order));
                }
                if frame.objects[index].id != object.object {
                    return Err(ExecutionTransportError::SlotIdentityChanged(object.slot));
                }
                replace_frame_object(frame, index, object.clone());
                changed.push(index);
                continue;
            }

            if self.object_slots.contains_key(&object.object) {
                return Err(ExecutionTransportError::DuplicateObject(object.object));
            }
            let index = frame.objects.len();
            if object.order as usize != index {
                return Err(ExecutionTransportError::InvalidOrder(object.order));
            }
            self.slots.push(object.slot);
            self.slot_indices.insert(object.slot, index);
            self.object_slots.insert(object.object, object.slot);
            push_frame_object(frame, object.clone());
            added_indices.push(index);
            changed.push(index);
        }

        changed.extend_from_slice(&removed_indices);
        changed.sort_unstable();
        changed.dedup();
        Ok(FrameChanges::with_structure(
            changed,
            added_indices,
            removed_indices,
        ))
    }
}

fn push_frame_object(frame: &mut FrameState, object: TransportObjectState) {
    frame.objects.push(FrameObjectState {
        id: object.object,
        content: ObjectContentRef::Geometry(object.geometry),
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
        text_bounds: None,
    });
    frame.presences.push(object.presence);
    frame.reveals.push(object.reveal);
    frame.morphs.push(object.morph);
    frame
        .render_geometries
        .push(object.render_geometry.map(std::sync::Arc::new));
    frame.render_transforms.push(object.render_transform);
}

fn replace_frame_object(frame: &mut FrameState, index: usize, object: TransportObjectState) {
    frame.objects[index] = FrameObjectState {
        id: object.object,
        content: ObjectContentRef::Geometry(object.geometry),
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
        text_bounds: None,
    };
    frame.presences[index] = object.presence;
    frame.reveals[index] = object.reveal;
    frame.morphs[index] = object.morph;
    frame.render_geometries[index] = object.render_geometry.map(std::sync::Arc::new);
    frame.render_transforms[index] = object.render_transform;
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::Vec2;

    // An explicit external codec fixture, not a scene/engine authoring path.
    fn snapshot() -> ExecutionDeltaEnvelope {
        ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.into(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: 7,
            sequence: 0,
            snapshot: true,
            time: 0.0,
            layout_generation: 1,
            camera: Camera2DState::default(),
            removed: Vec::new(),
            objects: (0..2)
                .map(|slot| TransportObjectState {
                    slot: TransportSlotId {
                        slot,
                        generation: 0,
                    },
                    order: slot,
                    object: ObjectId::new(slot as u64),
                    geometry: GeometryRef::circle(1.0),
                    transform: Transform2D::default(),
                    style: Style::default(),
                    appearance: 1.0,
                    presence: true,
                    reveal: 1.0,
                    morph: 0.0,
                    render_geometry: None,
                    render_transform: None,
                })
                .collect(),
        }
    }

    #[test]
    fn snapshots_dirty_updates_and_removal_preserve_surviving_slots() {
        let initial = snapshot();
        let mut mirror = ExecutionFrameMirror::default();
        assert!(mirror.apply(initial.clone()).unwrap().1.is_all());
        assert_eq!(mirror.layout_generation(), initial.layout_generation);
        let mut delta = initial.clone();
        delta.snapshot = false;
        delta.sequence = 1;
        delta.time = 0.5;
        delta.objects.truncate(1);
        delta.objects[0].transform.translation.x = 2.0;
        let (_, changes) = mirror.apply(delta.clone()).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(
            mirror.frame().unwrap().objects[0].transform.translation.x,
            2.0
        );
        delta.sequence = 2;
        delta.objects.clear();
        delta.removed.push(initial.objects[0].slot);
        mirror.apply(delta).unwrap();
        assert!(!mirror.frame().unwrap().presences[0]);
        assert_eq!(
            mirror.frame_index_for_slot(initial.objects[1].slot),
            Some(1)
        );
    }

    #[test]
    fn stale_gap_and_new_session_rules_remain_enforced() {
        let initial = snapshot();
        let mut mirror = ExecutionFrameMirror::default();
        mirror.apply(initial.clone()).unwrap();
        assert_eq!(
            mirror.apply(initial.clone()).unwrap().0,
            TransportApplyOutcome::DroppedStale
        );
        let mut delta = initial.clone();
        delta.snapshot = false;
        delta.sequence = 2;
        assert!(matches!(
            mirror.apply(delta.clone()),
            Err(ExecutionTransportError::SequenceGap { .. })
        ));
        delta.session += 1;
        assert!(matches!(
            mirror.apply(delta),
            Err(ExecutionTransportError::SessionRequiresSnapshot { .. })
        ));
    }

    #[test]
    fn malformed_camera_and_missing_layout_generation_are_rejected() {
        let initial = snapshot();
        let mut value = serde_json::to_value(&initial).unwrap();
        value.as_object_mut().unwrap().remove("layout_generation");
        assert!(serde_json::from_value::<ExecutionDeltaEnvelope>(value).is_err());
        let mut mirror = ExecutionFrameMirror::default();
        mirror.apply(initial.clone()).unwrap();
        let before = mirror.frame().unwrap().clone();
        for camera in [
            Camera2DState {
                center: Vec2::new(f32::NAN, 0.0),
                ..Default::default()
            },
            Camera2DState {
                height: 0.0,
                ..Default::default()
            },
            Camera2DState {
                height: f32::INFINITY,
                ..Default::default()
            },
        ] {
            let mut delta = initial.clone();
            delta.sequence = 1;
            delta.camera = camera;
            assert!(matches!(
                mirror.apply(delta),
                Err(ExecutionTransportError::InvalidCameraState)
            ));
            assert_eq!(mirror.next_sequence(), 1);
            assert_eq!(mirror.frame().unwrap(), &before);
        }
    }
}
