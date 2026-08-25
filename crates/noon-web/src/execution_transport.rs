use std::collections::{HashMap, HashSet};

use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
use noon_runtime::{
    ExecutionSlotError, ExecutionSlotId, ExecutionSlotTable, FrameChanges, FrameObjectState,
    FrameState,
};
use serde::{Deserialize, Serialize};

use crate::{ClockError, PlaybackClock, PlayerError, ReconcileOutcome, ScenePlayer};

pub const EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution";
pub const EXECUTION_TRANSPORT_VERSION: u32 = 1;

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

#[derive(Clone, Debug)]
pub struct ExecutionDeltaEncoder {
    session: u32,
    next_sequence: u64,
    initialized: bool,
    slots: ExecutionSlotTable,
    previous_order: Vec<ObjectId>,
    object_orders: HashMap<ObjectId, u32>,
}

impl ExecutionDeltaEncoder {
    pub fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
            initialized: false,
            slots: ExecutionSlotTable::new(),
            previous_order: Vec::new(),
            object_orders: HashMap::new(),
        }
    }

    pub const fn session(&self) -> u32 {
        self.session
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn encode(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        let live_indices = (0..frame.objects.len()).collect::<Vec<_>>();
        self.encode_live(frame, changes, &live_indices)
    }

    pub fn encode_live(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        live_indices: &[usize],
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        if !frame.time.is_finite() {
            return Err(ExecutionTransportError::InvalidTime(frame.time));
        }

        let structural = self.sync_slots(frame, live_indices)?;
        let snapshot = !self.initialized || structural || changes.is_all();
        if !snapshot && changes.is_empty() {
            return Ok(None);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ExecutionTransportError::SequenceExhausted)?;
        let indices = if snapshot {
            live_indices.to_vec()
        } else {
            changes.object_indices().to_vec()
        };
        let mut objects = Vec::with_capacity(indices.len());
        for (snapshot_order, index) in indices.into_iter().enumerate() {
            let object = frame
                .objects
                .get(index)
                .ok_or(ExecutionTransportError::SlotSpaceExhausted)?;
            let order = if snapshot {
                u32::try_from(snapshot_order)
                    .map_err(|_| ExecutionTransportError::SlotSpaceExhausted)?
            } else {
                *self.object_orders.get(&object.id).ok_or_else(|| {
                    ExecutionTransportError::UnknownSlot(
                        self.slots
                            .slot_for_object(object.id)
                            .map(TransportSlotId::from)
                            .unwrap_or(TransportSlotId {
                                slot: u32::MAX,
                                generation: 0,
                            }),
                    )
                })?
            };
            objects.push(self.object_state(frame, index, order)?);
        }
        self.initialized = true;

        Ok(Some(ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot,
            time: frame.time,
            removed: Vec::new(),
            objects,
        }))
    }

    pub fn encode_json(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.encode(frame, changes)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    pub fn encode_live_json(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        live_indices: &[usize],
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.encode_live(frame, changes, live_indices)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    fn sync_slots(
        &mut self,
        frame: &FrameState,
        live_indices: &[usize],
    ) -> Result<bool, ExecutionTransportError> {
        let order = live_indices
            .iter()
            .map(|index| {
                frame
                    .objects
                    .get(*index)
                    .map(|object| object.id)
                    .ok_or(ExecutionTransportError::SlotSpaceExhausted)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let current = order.iter().copied().collect::<HashSet<_>>();
        if current.len() != order.len() {
            if let Some(duplicate) = order.iter().copied().find(|object| {
                order
                    .iter()
                    .filter(|candidate| **candidate == *object)
                    .count()
                    > 1
            }) {
                return Err(ExecutionTransportError::DuplicateObject(duplicate));
            }
        }

        let removed = self
            .previous_order
            .iter()
            .copied()
            .filter(|object| !current.contains(object))
            .collect::<Vec<_>>();
        let mut structural = !removed.is_empty() || self.previous_order != order;
        for object in removed {
            self.slots.remove_object(object)?;
        }

        for object in &order {
            if self.slots.slot_for_object(*object).is_some() {
                continue;
            }
            structural = true;
            self.slots.insert_object(*object)?;
        }
        self.previous_order = order;
        self.object_orders.clear();
        for (order, object) in self.previous_order.iter().copied().enumerate() {
            let order =
                u32::try_from(order).map_err(|_| ExecutionTransportError::SlotSpaceExhausted)?;
            self.object_orders.insert(object, order);
        }
        Ok(structural)
    }

    fn object_state(
        &self,
        frame: &FrameState,
        index: usize,
        order: u32,
    ) -> Result<TransportObjectState, ExecutionTransportError> {
        let object = &frame.objects[index];
        let slot: TransportSlotId = self
            .slots
            .slot_for_object(object.id)
            .expect("frame object was synchronized to an execution slot")
            .into();
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
    slots: Vec<TransportSlotId>,
    slot_indices: HashMap<TransportSlotId, usize>,
    frame: Option<FrameState>,
}

impl ExecutionFrameMirror {
    pub fn frame(&self) -> Option<&FrameState> {
        self.frame.as_ref()
    }

    pub fn session(&self) -> Option<u32> {
        self.session
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
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
        Ok(())
    }

    fn reset_for_session(&mut self, session: u32) {
        self.session = Some(session);
        self.next_sequence = 0;
        self.slots.clear();
        self.slot_indices.clear();
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
            push_frame_object(&mut frame, object);
        }
        self.frame = Some(frame);
        Ok(FrameChanges::all())
    }

    fn apply_partial(
        &mut self,
        delta: &ExecutionDeltaEnvelope,
    ) -> Result<FrameChanges, ExecutionTransportError> {
        if !delta.removed.is_empty() {
            return Err(ExecutionTransportError::StructuralDeltaRequiresSnapshot);
        }
        let frame =
            self.frame
                .as_mut()
                .ok_or(ExecutionTransportError::SessionRequiresSnapshot {
                    session: delta.session,
                    sequence: delta.sequence,
                })?;
        frame.time = delta.time;
        let mut changed = Vec::with_capacity(delta.objects.len());
        let mut seen = HashSet::with_capacity(delta.objects.len());
        for object in &delta.objects {
            if !seen.insert(object.slot) {
                return Err(ExecutionTransportError::DuplicateSlot(object.slot));
            }
            let index = *self
                .slot_indices
                .get(&object.slot)
                .ok_or(ExecutionTransportError::UnknownSlot(object.slot))?;
            if object.order as usize != index {
                return Err(ExecutionTransportError::InvalidOrder(object.order));
            }
            if frame.objects[index].id != object.object {
                return Err(ExecutionTransportError::SlotIdentityChanged(object.slot));
            }
            replace_frame_object(frame, index, object.clone());
            changed.push(index);
        }
        Ok(FrameChanges::objects(changed))
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
        Ok(Self {
            player: ScenePlayer::from_scene_json(scene_json)?,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            encoder: ExecutionDeltaEncoder::new(session),
        })
    }

    pub fn initial_delta_json(&mut self) -> Result<String, ExecutionTransportError> {
        let changes = self.player.take_frame_changes();
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &changes, &live_indices)?
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

    pub fn replace_scene_delta_json(
        &mut self,
        json: &str,
    ) -> Result<String, ExecutionTransportError> {
        self.player.replace_scene_json(json)?;
        self.clock.reset();
        self.force_snapshot_json()
    }

    pub fn reconcile_scene_delta_json(
        &mut self,
        json: &str,
    ) -> Result<(ReconcileOutcome, Option<String>), ExecutionTransportError> {
        let outcome = self.player.reconcile_scene_json(json)?;
        let delta = self.take_delta_json()?;
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
        let changes = self.player.take_frame_changes();
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &changes, &live_indices)
    }

    fn force_snapshot_json(&mut self) -> Result<String, ExecutionTransportError> {
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &FrameChanges::all(), &live_indices)?
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
    use noon_core::{GeometryRef, ObjectId, SceneDefinition, ScenePatch, TrackTiming, Vec2};
    use noon_ir::{encode_patch_batch, encode_scene, PatchBatch};

    use super::*;

    fn encode_current(
        encoder: &mut ExecutionDeltaEncoder,
        player: &mut ScenePlayer,
    ) -> ExecutionDeltaEnvelope {
        let changes = player.take_frame_changes();
        let live_indices = player.live_frame_indices();
        encoder
            .encode_live(player.frame(), &changes, &live_indices)
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

    #[test]
    fn snapshot_then_dirty_delta_round_trip() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(7);
        let mut mirror = ExecutionFrameMirror::default();

        let initial_changes = player.take_frame_changes();
        let initial = encoder
            .encode(player.frame(), &initial_changes)
            .unwrap()
            .unwrap();
        assert!(initial.snapshot);
        let (outcome, changes) = mirror.apply(initial).unwrap();
        assert_eq!(outcome, TransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap(), player.frame());

        player.advance_to(0.5).unwrap();
        let changes = player.take_frame_changes();
        let delta = encoder.encode(player.frame(), &changes).unwrap().unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        let (outcome, changes) = mirror.apply(delta).unwrap();
        assert_eq!(outcome, TransportApplyOutcome::Applied);
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.frame().unwrap(), player.frame());
    }

    #[test]
    fn structural_change_forces_snapshot_and_preserves_surviving_slot() {
        let mut player = ScenePlayer::from_scene_json(&scene_json()).unwrap();
        let mut encoder = ExecutionDeltaEncoder::new(2);
        let initial = encode_current(&mut encoder, &mut player);
        let surviving_slot = initial.objects[1].slot;

        let batch = PatchBatch::new(0, vec![ScenePatch::RemoveObject(ObjectId::new(0))]);
        player
            .apply_patch_batch_json(&encode_patch_batch(&batch).unwrap())
            .unwrap();
        let delta = encode_current(&mut encoder, &mut player);
        assert!(delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].slot, surviving_slot);
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
}
