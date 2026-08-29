use std::collections::{HashMap, HashSet};

use noon_core::{Camera2DState, GeometryRef, ObjectId, Style, Transform2D};
use noon_runtime::{
    ExecutionSlotError, ExecutionSlotId, FrameChanges, FrameObjectState, FrameState,
};
use serde::{Deserialize, Serialize};

use crate::{ClockError, PlaybackClock, PlayerError, ReconcileOutcome, ScenePlayer};

pub const EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution";
pub const EXECUTION_TRANSPORT_VERSION: u32 = 2;

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
    Player(PlayerError),
    Clock(ClockError),
    ExecutionSlot(ExecutionSlotError),
    Json(serde_json::Error),
    InvalidChannel(String),
    UnsupportedVersion(u32),
    InvalidTime(f64),
    InvalidCameraState,
    InvalidCameraObject(ObjectId),
    InvalidViewportAspect(f32),
    SequenceExhausted,
    SessionRequiresSnapshot { session: u32, sequence: u64 },
    SequenceGap { expected: u64, actual: u64 },
    StructuralDeltaRequiresSnapshot,
    DuplicateSlot(TransportSlotId),
    UnknownSlot(TransportSlotId),
    SlotIdentityChanged(TransportSlotId),
    InvalidOrder(u32),
    DuplicateObject(ObjectId),
    SlotSpaceExhausted,
    SlotGenerationExhausted(TransportSlotId),
}

impl std::fmt::Display for ExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Player(error) => error.fmt(formatter),
            Self::Clock(error) => error.fmt(formatter),
            Self::ExecutionSlot(error) => error.fmt(formatter),
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
            Self::InvalidTime(time) => write!(formatter, "invalid execution delta time {time}"),
            Self::InvalidCameraState => {
                formatter.write_str("invalid execution transport camera state")
            }
            Self::InvalidCameraObject(object) => write!(
                formatter,
                "camera object {} is missing or not a supported 2D frame",
                object.get()
            ),
            Self::InvalidViewportAspect(aspect) => write!(
                formatter,
                "execution viewport aspect must be positive and finite, got {aspect}"
            ),
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
            Self::SlotSpaceExhausted => {
                formatter.write_str("execution transport slot space exhausted")
            }
            Self::SlotGenerationExhausted(slot) => write!(
                formatter,
                "execution transport slot {} generation exhausted at {}",
                slot.slot, slot.generation
            ),
        }
    }
}

impl std::error::Error for ExecutionTransportError {}

impl From<PlayerError> for ExecutionTransportError {
    fn from(value: PlayerError) -> Self {
        Self::Player(value)
    }
}

impl From<ClockError> for ExecutionTransportError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<ExecutionSlotError> for ExecutionTransportError {
    fn from(value: ExecutionSlotError) -> Self {
        Self::ExecutionSlot(value)
    }
}

impl From<serde_json::Error> for ExecutionTransportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct CameraRoleProbe {
    #[serde(default)]
    camera_object: Option<ObjectId>,
}

fn camera_object_from_scene_json(
    scene_json: &str,
) -> Result<Option<ObjectId>, ExecutionTransportError> {
    Ok(serde_json::from_str::<CameraRoleProbe>(scene_json)?.camera_object)
}

#[derive(Clone, Debug)]
pub struct ExecutionDeltaEncoder {
    session: u32,
    next_sequence: u64,
    initialized: bool,
    slot_orders: HashMap<TransportSlotId, u32>,
    next_order: u32,
    camera_object: Option<ObjectId>,
    layout_generation: u64,
}

impl ExecutionDeltaEncoder {
    pub fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
            initialized: false,
            slot_orders: HashMap::new(),
            next_order: 0,
            camera_object: None,
            layout_generation: 0,
        }
    }

    pub const fn session(&self) -> u32 {
        self.session
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }

    pub fn set_camera_object(&mut self, camera_object: Option<ObjectId>) {
        self.camera_object = camera_object;
    }

    pub fn set_layout_generation(&mut self, layout_generation: u64) {
        self.layout_generation = layout_generation;
    }

    pub fn contains_slot(&self, slot: ExecutionSlotId) -> bool {
        self.slot_orders.contains_key(&TransportSlotId::from(slot))
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &FrameState,
        live_objects: &[(ExecutionSlotId, usize)],
    ) -> Result<ExecutionDeltaEnvelope, ExecutionTransportError> {
        self.validate_time(frame.time)?;
        let camera = self.camera_state(frame)?;
        self.slot_orders.clear();
        self.next_order = 0;
        let mut objects = Vec::with_capacity(live_objects.len());
        let mut seen = HashSet::with_capacity(live_objects.len());
        for &(slot, frame_index) in live_objects {
            let slot = TransportSlotId::from(slot);
            if !seen.insert(slot) {
                return Err(ExecutionTransportError::DuplicateSlot(slot));
            }
            let order = self.next_order;
            self.next_order = self
                .next_order
                .checked_add(1)
                .ok_or(ExecutionTransportError::SlotSpaceExhausted)?;
            self.slot_orders.insert(slot, order);
            objects.push(Self::object_state(frame, frame_index, slot, order)?);
        }
        let sequence = self.take_sequence()?;
        self.initialized = true;
        Ok(ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot: true,
            time: frame.time,
            layout_generation: self.layout_generation,
            camera,
            removed: Vec::new(),
            objects,
        })
    }

    pub fn encode_incremental(
        &mut self,
        frame: &FrameState,
        dirty_objects: &[(ExecutionSlotId, usize)],
        added_objects: &[(ExecutionSlotId, usize)],
        removed_slots: &[ExecutionSlotId],
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        self.validate_time(frame.time)?;
        if !self.initialized {
            return Err(ExecutionTransportError::StructuralDeltaRequiresSnapshot);
        }
        if dirty_objects.is_empty() && added_objects.is_empty() && removed_slots.is_empty() {
            return Ok(None);
        }
        let camera = self.camera_state(frame)?;

        let mut removed = Vec::with_capacity(removed_slots.len());
        let mut seen_removed = HashSet::with_capacity(removed_slots.len());
        for &slot in removed_slots {
            let slot = TransportSlotId::from(slot);
            if !seen_removed.insert(slot) {
                continue;
            }
            self.slot_orders
                .remove(&slot)
                .ok_or(ExecutionTransportError::UnknownSlot(slot))?;
            removed.push(slot);
        }

        let mut objects = Vec::with_capacity(dirty_objects.len() + added_objects.len());
        let mut emitted = HashSet::with_capacity(objects.capacity());
        for &(slot, frame_index) in added_objects {
            let slot = TransportSlotId::from(slot);
            if self.slot_orders.contains_key(&slot) || !emitted.insert(slot) {
                return Err(ExecutionTransportError::DuplicateSlot(slot));
            }
            let order = self.next_order;
            self.next_order = self
                .next_order
                .checked_add(1)
                .ok_or(ExecutionTransportError::SlotSpaceExhausted)?;
            self.slot_orders.insert(slot, order);
            objects.push(Self::object_state(frame, frame_index, slot, order)?);
        }
        for &(slot, frame_index) in dirty_objects {
            let slot = TransportSlotId::from(slot);
            if !emitted.insert(slot) {
                continue;
            }
            let order = *self
                .slot_orders
                .get(&slot)
                .ok_or(ExecutionTransportError::UnknownSlot(slot))?;
            objects.push(Self::object_state(frame, frame_index, slot, order)?);
        }

        let sequence = self.take_sequence()?;
        Ok(Some(ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot: false,
            time: frame.time,
            layout_generation: self.layout_generation,
            camera,
            removed,
            objects,
        }))
    }

    fn validate_time(&self, time: f64) -> Result<(), ExecutionTransportError> {
        if time.is_finite() {
            Ok(())
        } else {
            Err(ExecutionTransportError::InvalidTime(time))
        }
    }

    fn camera_state(&self, frame: &FrameState) -> Result<Camera2DState, ExecutionTransportError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object = frame
            .objects
            .iter()
            .find(|object| object.id == camera_object)
            .ok_or(ExecutionTransportError::InvalidCameraObject(camera_object))?;
        Camera2DState::from_frame_object(&object.geometry, object.transform)
            .ok_or(ExecutionTransportError::InvalidCameraObject(camera_object))
    }

    fn take_sequence(&mut self) -> Result<u64, ExecutionTransportError> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ExecutionTransportError::SequenceExhausted)?;
        Ok(sequence)
    }

    fn object_state(
        frame: &FrameState,
        index: usize,
        slot: TransportSlotId,
        order: u32,
    ) -> Result<TransportObjectState, ExecutionTransportError> {
        let object = frame
            .objects
            .get(index)
            .ok_or(ExecutionTransportError::SlotSpaceExhausted)?;
        Ok(TransportObjectState {
            slot,
            order,
            object: object.id,
            geometry: object.geometry.clone(),
            transform: object.transform,
            style: object.style,
            appearance: object.appearance,
            presence: frame.presences[index],
            reveal: frame.reveals[index],
            morph: frame.morphs[index],
            render_geometry: frame.render_geometries[index].clone(),
        })
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
            time: delta.time,
            objects: Vec::with_capacity(count),
            presences: Vec::with_capacity(count),
            reveals: Vec::with_capacity(count),
            morphs: Vec::with_capacity(count),
            render_geometries: Vec::with_capacity(count),
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
        for &slot in &delta.removed {
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
        geometry: object.geometry,
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
    });
    frame.presences.push(object.presence);
    frame.reveals.push(object.reveal);
    frame.morphs.push(object.morph);
    frame.render_geometries.push(object.render_geometry);
}

fn replace_frame_object(frame: &mut FrameState, index: usize, object: TransportObjectState) {
    frame.objects[index] = FrameObjectState {
        id: object.object,
        geometry: object.geometry,
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
    };
    frame.presences[index] = object.presence;
    frame.reveals[index] = object.reveal;
    frame.morphs[index] = object.morph;
    frame.render_geometries[index] = object.render_geometry;
}

fn encode_player_delta(
    encoder: &mut ExecutionDeltaEncoder,
    player: &mut ScenePlayer,
    force_snapshot: bool,
) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
    encoder.set_layout_generation(player.layout_generation());
    let changes = player.take_frame_changes();
    let execution_delta = player.take_execution_delta();
    if force_snapshot || !encoder.is_initialized() || changes.is_all() {
        let live_objects = player
            .live_frame_indices()
            .into_iter()
            .filter_map(|frame_index| {
                player
                    .execution_slot_for_frame_index(frame_index)
                    .map(|slot| (slot, frame_index))
            })
            .collect::<Vec<_>>();
        return encoder
            .encode_snapshot(player.frame(), &live_objects)
            .map(Some);
    }

    let mut dirty_objects = Vec::new();
    let mut added_objects = Vec::new();
    for &frame_index in changes.object_indices() {
        let Some(slot) = player.execution_slot_for_frame_index(frame_index) else {
            continue;
        };
        if encoder.contains_slot(slot) {
            if !dirty_objects.iter().any(|(existing, _)| *existing == slot) {
                dirty_objects.push((slot, frame_index));
            }
        } else if !added_objects.iter().any(|(existing, _)| *existing == slot) {
            added_objects.push((slot, frame_index));
        }
    }

    let mut removed_slots = Vec::new();
    for &slot in execution_delta.slots() {
        match player.frame_index_for_execution_slot(slot) {
            Some(frame_index) if !encoder.contains_slot(slot) => {
                if !added_objects.iter().any(|(existing, _)| *existing == slot) {
                    added_objects.push((slot, frame_index));
                }
            }
            Some(_) => {}
            None if encoder.contains_slot(slot) => removed_slots.push(slot),
            None => {}
        }
    }

    encoder.encode_incremental(
        player.frame(),
        &dirty_objects,
        &added_objects,
        &removed_slots,
    )
}

#[derive(Debug)]
pub struct EngineScenePlayer {
    player: ScenePlayer,
    clock: PlaybackClock,
    encoder: ExecutionDeltaEncoder,
}

impl EngineScenePlayer {
    pub fn new(
        scene_json: &str,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, ExecutionTransportError> {
        let camera_object = camera_object_from_scene_json(scene_json)?;
        let mut encoder = ExecutionDeltaEncoder::new(session);
        encoder.set_camera_object(camera_object);
        Ok(Self {
            player: ScenePlayer::from_scene_json(scene_json)?,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            encoder,
        })
    }

    pub fn initial_delta_json(&mut self) -> Result<String, ExecutionTransportError> {
        encode_player_delta(&mut self.encoder, &mut self.player, true)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, ExecutionTransportError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.player.advance_to(scene_time)?;
        self.take_delta_json()
    }

    pub fn viewport_visibility_json(&self, aspect: f32) -> Result<String, ExecutionTransportError> {
        let camera = self.encoder.camera_state(self.player.frame())?;
        let bounds = camera
            .viewport_bounds(aspect)
            .ok_or(ExecutionTransportError::InvalidViewportAspect(aspect))?;
        serde_json::to_string(&self.player.viewport_visibility(
            bounds.min.x,
            bounds.min.y,
            bounds.max.x,
            bounds.max.y,
        ))
        .map_err(ExecutionTransportError::from)
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), ExecutionTransportError> {
        self.clock.set_loop_duration(duration)?;
        Ok(())
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn resume(&mut self) {
        self.clock.resume();
    }

    pub fn seek_delta_json(
        &mut self,
        scene_time: f64,
    ) -> Result<Option<String>, ExecutionTransportError> {
        let mut clock = self.clock.clone();
        clock.seek(scene_time)?;
        self.player.advance_to(scene_time)?;
        self.clock = clock;
        self.take_delta_json()
    }

    pub const fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    pub fn apply_patch_batch_delta_json(
        &mut self,
        json: &str,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.player.apply_patch_batch_json(json)?;
        self.take_delta_json()
    }

    pub fn apply_host_patch_batch_delta_json(
        &mut self,
        json: &str,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.player.apply_host_patch_batch_json(json)?;
        self.take_delta_json()
    }

    pub fn snapshot_delta_json(&mut self) -> Result<String, ExecutionTransportError> {
        self.force_snapshot_json()
    }

    pub fn compact_retired_slots_delta_json(
        &mut self,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.player.compact_retired_slots()?;
        self.take_delta_json()
    }

    pub fn replace_scene_delta_json(
        &mut self,
        json: &str,
    ) -> Result<String, ExecutionTransportError> {
        let camera_object = camera_object_from_scene_json(json)?;
        self.player.replace_scene_json(json)?;
        self.encoder.set_camera_object(camera_object);
        self.clock.reset();
        self.force_snapshot_json()
    }

    pub fn reconcile_scene_delta_json(
        &mut self,
        json: &str,
    ) -> Result<(ReconcileOutcome, Option<String>), ExecutionTransportError> {
        let camera_object = camera_object_from_scene_json(json)?;
        let camera_changed = camera_object != self.encoder.camera_object();
        let outcome = self.player.reconcile_scene_json(json)?;
        self.encoder.set_camera_object(camera_object);
        let delta = if camera_changed {
            Some(self.force_snapshot_json()?)
        } else {
            self.take_delta_json()?
        };
        Ok((outcome, delta))
    }

    pub fn scene_json(&self) -> Result<String, ExecutionTransportError> {
        self.player
            .scene_json()
            .map_err(ExecutionTransportError::from)
    }

    pub const fn next_patch_sequence(&self) -> u64 {
        self.player.next_sequence()
    }

    pub fn time(&self) -> f64 {
        self.player.frame().time
    }

    fn take_delta_json(&mut self) -> Result<Option<String>, ExecutionTransportError> {
        encode_player_delta(&mut self.encoder, &mut self.player, false)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    fn force_snapshot_json(&mut self) -> Result<String, ExecutionTransportError> {
        encode_player_delta(&mut self.encoder, &mut self.player, true)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::EngineScenePlayer;

    #[wasm_bindgen(js_name = EngineScenePlayer)]
    pub struct WasmEngineScenePlayer {
        inner: EngineScenePlayer,
    }

    #[wasm_bindgen(js_class = EngineScenePlayer)]
    impl WasmEngineScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            scene_json: &str,
            loop_duration_seconds: f64,
            session: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                inner: EngineScenePlayer::new(scene_json, loop_duration_seconds, session)
                    .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = initialDeltaJson)]
        pub fn initial_delta_json(&mut self) -> Result<String, JsValue> {
            self.inner.initial_delta_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = tickDeltaJson)]
        pub fn tick_delta_json(&mut self, timestamp_ms: f64) -> Result<Option<String>, JsValue> {
            self.inner.tick_delta_json(timestamp_ms).map_err(js_error)
        }

        #[wasm_bindgen(js_name = viewportVisibilityJson)]
        pub fn viewport_visibility_json(&self, aspect: f32) -> Result<String, JsValue> {
            self.inner.viewport_visibility_json(aspect).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setLoopDurationSeconds)]
        pub fn set_loop_duration_seconds(&mut self, duration: f64) -> Result<(), JsValue> {
            self.inner.set_loop_duration(duration).map_err(js_error)
        }

        pub fn pause(&mut self) {
            self.inner.pause();
        }

        pub fn resume(&mut self) {
            self.inner.resume();
        }

        #[wasm_bindgen(js_name = seekDeltaJson)]
        pub fn seek_delta_json(&mut self, scene_time: f64) -> Result<Option<String>, JsValue> {
            self.inner.seek_delta_json(scene_time).map_err(js_error)
        }

        #[wasm_bindgen(js_name = isPlaying)]
        pub fn is_playing(&self) -> bool {
            self.inner.is_playing()
        }

        #[wasm_bindgen(js_name = applyPatchBatchDeltaJson)]
        pub fn apply_patch_batch_delta_json(
            &mut self,
            json: &str,
        ) -> Result<Option<String>, JsValue> {
            self.inner
                .apply_patch_batch_delta_json(json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = applyHostPatchBatchDeltaJson)]
        pub fn apply_host_patch_batch_delta_json(
            &mut self,
            json: &str,
        ) -> Result<Option<String>, JsValue> {
            self.inner
                .apply_host_patch_batch_delta_json(json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = snapshotDeltaJson)]
        pub fn snapshot_delta_json(&mut self) -> Result<String, JsValue> {
            self.inner.snapshot_delta_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = compactRetiredSlotsDeltaJson)]
        pub fn compact_retired_slots_delta_json(&mut self) -> Result<Option<String>, JsValue> {
            self.inner
                .compact_retired_slots_delta_json()
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = replaceSceneDeltaJson)]
        pub fn replace_scene_delta_json(&mut self, json: &str) -> Result<String, JsValue> {
            self.inner.replace_scene_delta_json(json).map_err(js_error)
        }

        #[wasm_bindgen(js_name = reconcileSceneDeltaJson)]
        pub fn reconcile_scene_delta_json(&mut self, json: &str) -> Result<JsValue, JsValue> {
            let (outcome, delta) = self
                .inner
                .reconcile_scene_delta_json(json)
                .map_err(js_error)?;
            let incremental = matches!(outcome, crate::ReconcileOutcome::Incremental { .. });
            let value = serde_json::json!({
                "incremental": incremental,
                "delta": delta,
            });
            Ok(JsValue::from_str(&value.to_string()))
        }

        #[wasm_bindgen(js_name = sceneJson)]
        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextPatchSequence)]
        pub fn next_patch_sequence(&self) -> u64 {
            self.inner.next_patch_sequence()
        }

        pub fn time(&self) -> f64 {
            self.inner.time()
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, ObjectId, SceneDefinition, ScenePatch, TrackTiming, Vec2,
        DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH,
    };
    use noon_ir::{encode_patch_batch, encode_scene, PatchBatch};

    use super::*;

    fn encode_current(
        encoder: &mut ExecutionDeltaEncoder,
        player: &mut ScenePlayer,
    ) -> ExecutionDeltaEnvelope {
        encode_player_delta(encoder, player, false)
            .unwrap()
            .unwrap()
    }

    fn scene_json() -> String {
        let mut scene = SceneDefinition::new();
        let moving = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                moving,
                Vec2::ZERO,
                Vec2::new(4.0, 0.0),
                TrackTiming::new(0.0, 2.0, noon_core::Easing::Linear),
            )
            .unwrap();
        scene.add(GeometryRef::rectangle(2.0, 1.0));
        encode_scene(&scene).unwrap()
    }

    fn camera_scene_json() -> String {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let frame = scene.add(GeometryRef::rectangle(
            DEFAULT_FRAME_WIDTH,
            DEFAULT_FRAME_HEIGHT,
        ));
        scene.object_mut(frame).unwrap().style.opacity = 0.0;
        assert!(scene.set_camera_object(frame));
        scene
            .animate_position(
                frame,
                Vec2::ZERO,
                Vec2::new(3.0, -1.0),
                TrackTiming::new(0.0, 2.0, noon_core::Easing::Linear),
            )
            .unwrap();
        encode_scene(&scene).unwrap()
    }

    fn camera_visibility_scene_json() -> String {
        let mut scene = SceneDefinition::new();
        let target = scene.add(GeometryRef::circle(1.0));
        scene.object_mut(target).unwrap().transform.translation = Vec2::new(6.0, 0.0);
        let frame = scene.add(GeometryRef::rectangle(
            DEFAULT_FRAME_WIDTH,
            DEFAULT_FRAME_HEIGHT,
        ));
        scene.object_mut(frame).unwrap().style.opacity = 0.0;
        assert!(scene.set_camera_object(frame));
        scene
            .animate_position(
                frame,
                Vec2::ZERO,
                Vec2::new(6.0, 0.0),
                TrackTiming::new(0.0, 2.0, noon_core::Easing::Linear),
            )
            .unwrap();
        encode_scene(&scene).unwrap()
    }

    #[test]
    fn snapshot_then_dirty_delta_round_trip() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(7);
        let mut mirror = ExecutionFrameMirror::default();

        let initial = encode_current(&mut encoder, &mut player);
        assert!(initial.snapshot);
        assert_eq!(initial.protocol_version, 2);
        assert_eq!(initial.layout_generation, player.layout_generation());
        let (outcome, changes) = mirror.apply(initial).unwrap();
        assert_eq!(outcome, TransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.layout_generation(), player.layout_generation());
        assert_eq!(mirror.frame().unwrap(), player.frame());

        player.advance_to(0.5).unwrap();
        let delta = encode_current(&mut encoder, &mut player);
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        let (outcome, changes) = mirror.apply(delta).unwrap();
        assert_eq!(outcome, TransportApplyOutcome::Applied);
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.frame().unwrap(), player.frame());
    }

    #[test]
    fn layout_generation_is_required_on_wire() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(31);
        let initial = encode_current(&mut encoder, &mut player);
        let mut value = serde_json::to_value(initial).unwrap();
        value.as_object_mut().unwrap().remove("layout_generation");

        assert!(serde_json::from_value::<ExecutionDeltaEnvelope>(value).is_err());
    }

    #[test]
    fn camera_state_uses_same_evaluated_transform_as_scene_objects() {
        let mut player = EngineScenePlayer::new(&camera_scene_json(), 4.0, 19).unwrap();
        let initial: ExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        assert_eq!(initial.camera, Camera2DState::default());

        let delta_json = player.tick_delta_json(0.0).unwrap();
        assert!(delta_json.is_none());
        let delta_json = player.tick_delta_json(1_000.0).unwrap().unwrap();
        let delta: ExecutionDeltaEnvelope = serde_json::from_str(&delta_json).unwrap();
        assert!((delta.camera.center.x - 1.5).abs() < 1.0e-5);
        assert!((delta.camera.center.y + 0.5).abs() < 1.0e-5);
        assert_eq!(delta.camera.height, DEFAULT_FRAME_HEIGHT);

        let mut mirror = ExecutionFrameMirror::default();
        mirror.apply(initial).unwrap();
        mirror.apply(delta).unwrap();
        assert!((mirror.camera().center.x - 1.5).abs() < 1.0e-5);
    }

    #[test]
    fn engine_visibility_uses_post_advance_camera_and_render_aspect() {
        let mut player = EngineScenePlayer::new(&camera_visibility_scene_json(), 4.0, 37).unwrap();
        player.initial_delta_json().unwrap();

        let narrow: crate::ExecutionVisibilityEnvelope =
            serde_json::from_str(&player.viewport_visibility_json(1.0).unwrap()).unwrap();
        assert_eq!(narrow.time, 0.0);
        assert!(!narrow.slots.iter().any(|slot| slot.slot == 0));

        let wide: crate::ExecutionVisibilityEnvelope =
            serde_json::from_str(&player.viewport_visibility_json(2.0).unwrap()).unwrap();
        assert!(wide.slots.iter().any(|slot| slot.slot == 0));

        player.seek_delta_json(2.0).unwrap();
        let moved: crate::ExecutionVisibilityEnvelope =
            serde_json::from_str(&player.viewport_visibility_json(1.0).unwrap()).unwrap();
        assert_eq!(moved.time, 2.0);
        assert!(moved.slots.iter().any(|slot| slot.slot == 0));
        assert!(matches!(
            player.viewport_visibility_json(0.0),
            Err(ExecutionTransportError::InvalidViewportAspect(aspect)) if aspect == 0.0
        ));
    }

    #[test]
    fn structural_change_is_one_slot_delta_and_preserves_surviving_slot() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(2);
        let initial = encode_current(&mut encoder, &mut player);
        let surviving_slot = initial.objects[1].slot;

        let batch = PatchBatch::new(0, vec![ScenePatch::RemoveObject(ObjectId::new(0))]);
        player
            .apply_patch_batch_json(&encode_patch_batch(&batch).unwrap())
            .unwrap();
        let delta = encode_current(&mut encoder, &mut player);
        assert!(!delta.snapshot);
        assert!(delta.objects.is_empty());
        assert_eq!(delta.removed, vec![initial.objects[0].slot]);

        let mut mirror = ExecutionFrameMirror::default();
        mirror.apply(initial).unwrap();
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.removed_indices(), &[0]);
        assert_eq!(mirror.live_object_count(), 1);
        assert_eq!(mirror.frame().unwrap().objects.len(), 2);
        assert!(!mirror.frame().unwrap().presences[0]);
        assert_eq!(mirror.frame_index_for_slot(surviving_slot), Some(1));
        assert_eq!(mirror.slots[1], surviving_slot);
    }

    #[test]
    fn stale_delta_is_dropped_and_gap_requires_snapshot() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(4);
        let initial = encode_current(&mut encoder, &mut player);
        let mut mirror = ExecutionFrameMirror::default();
        mirror.apply(initial.clone()).unwrap();
        assert_eq!(
            mirror.apply(initial).unwrap().0,
            TransportApplyOutcome::DroppedStale
        );

        player.advance_to(0.25).unwrap();
        let mut delta = encode_current(&mut encoder, &mut player);
        delta.sequence += 1;
        assert!(matches!(
            mirror.apply(delta),
            Err(ExecutionTransportError::SequenceGap { .. })
        ));
    }

    #[test]
    fn new_session_requires_sequence_zero_snapshot() {
        let mut mirror = ExecutionFrameMirror::default();
        let delta = ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: 9,
            sequence: 3,
            snapshot: false,
            time: 0.0,
            layout_generation: 0,
            camera: Camera2DState::default(),
            removed: Vec::new(),
            objects: Vec::new(),
        };
        assert!(matches!(
            mirror.apply(delta),
            Err(ExecutionTransportError::SessionRequiresSnapshot {
                session: 9,
                sequence: 3
            })
        ));
    }

    #[test]
    fn retiming_execution_clock_preserves_phase_and_patch_sequence() {
        let mut player = EngineScenePlayer::new(&scene_json(), 4.0, 13).unwrap();
        player.initial_delta_json().unwrap();
        player.tick_delta_json(100.0).unwrap();
        player.tick_delta_json(1_600.0).unwrap();
        assert_eq!(player.time(), 1.5);

        let patch = PatchBatch::new(0, Vec::new());
        player
            .apply_patch_batch_delta_json(&encode_patch_batch(&patch).unwrap())
            .unwrap();
        assert_eq!(player.next_patch_sequence(), 1);

        player.set_loop_duration(3.0).unwrap();
        assert_eq!(player.time(), 1.5);
        assert_eq!(player.next_patch_sequence(), 1);

        player.tick_delta_json(3_100.0).unwrap();
        assert_eq!(player.time(), 0.0);
        assert_eq!(player.next_patch_sequence(), 1);
    }

    #[test]
    fn playback_controls_preserve_session_and_patch_identity() {
        let mut player = EngineScenePlayer::new(&scene_json(), 4.0, 23).unwrap();
        let initial: ExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        player.tick_delta_json(100.0).unwrap();
        player.tick_delta_json(1_100.0).unwrap();
        assert_eq!(player.time(), 1.0);

        player.pause();
        assert!(!player.is_playing());
        assert!(player.tick_delta_json(5_100.0).unwrap().is_none());
        assert_eq!(player.time(), 1.0);

        let seek: ExecutionDeltaEnvelope =
            serde_json::from_str(&player.seek_delta_json(0.25).unwrap().expect("seek delta"))
                .unwrap();
        assert_eq!(seek.session, initial.session);
        assert_eq!(seek.time, 0.25);
        assert_eq!(player.next_patch_sequence(), 0);
        assert_eq!(player.time(), 0.25);
        assert!(player.tick_delta_json(8_100.0).unwrap().is_none());
        assert_eq!(player.time(), 0.25);

        player.resume();
        assert!(player.is_playing());
        assert!(player.tick_delta_json(8_100.0).unwrap().is_none());
        player.tick_delta_json(8_600.0).unwrap();
        assert_eq!(player.time(), 0.75);
    }

    #[test]
    fn exact_endpoint_seek_is_not_implicitly_wrapped() {
        let mut player = EngineScenePlayer::new(&scene_json(), 4.0, 29).unwrap();
        player.initial_delta_json().unwrap();
        player.tick_delta_json(100.0).unwrap();

        player.seek_delta_json(4.0).unwrap();
        assert_eq!(player.time(), 4.0);
        assert!(player.tick_delta_json(100.0).unwrap().is_none());
        assert_eq!(player.time(), 4.0);
        player.tick_delta_json(350.0).unwrap();
        assert_eq!(player.time(), 0.25);
    }

    #[test]
    fn compaction_resynchronizes_transport_with_same_execution_slots() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(11);
        let mut mirror = ExecutionFrameMirror::default();

        let initial = encode_current(&mut encoder, &mut player);
        let surviving_slot = initial.objects[1].slot;
        let initial_generation = initial.layout_generation;
        mirror.apply(initial.clone()).unwrap();

        let batch = PatchBatch::new(0, vec![ScenePatch::RemoveObject(ObjectId::new(0))]);
        player
            .apply_patch_batch_json(&encode_patch_batch(&batch).unwrap())
            .unwrap();
        let removal = encode_current(&mut encoder, &mut player);
        assert!(!removal.snapshot);
        mirror.apply(removal).unwrap();
        assert_eq!(mirror.frame().unwrap().objects.len(), 2);
        assert_eq!(mirror.live_object_count(), 1);
        assert_eq!(mirror.frame_index_for_slot(surviving_slot), Some(1));

        let stats = player.compact_retired_slots().unwrap();
        assert_eq!(stats.frame_slots_reclaimed, 1);
        let compacted = encode_current(&mut encoder, &mut player);
        assert!(compacted.snapshot);
        assert!(compacted.layout_generation > initial_generation);
        assert_eq!(compacted.layout_generation, player.layout_generation());
        assert_eq!(compacted.objects.len(), 1);
        assert_eq!(compacted.objects[0].slot, surviving_slot);
        assert_eq!(compacted.objects[0].order, 0);

        let (_, changes) = mirror.apply(compacted).unwrap();
        assert!(changes.is_all());
        assert_eq!(mirror.layout_generation(), player.layout_generation());
        assert_eq!(mirror.live_object_count(), 1);
        assert_eq!(mirror.frame().unwrap().objects.len(), 1);
        assert_eq!(mirror.frame_index_for_slot(surviving_slot), Some(0));
        assert_eq!(mirror.slots, vec![surviving_slot]);
        assert_eq!(mirror.frame().unwrap(), player.frame());
    }
}
