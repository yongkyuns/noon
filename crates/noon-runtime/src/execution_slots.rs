use std::collections::HashMap;

use noon_compile::{CompilePatchError, CompiledScene, CompiledTransactionPreflightStats};
use noon_core::{MutationTransaction, ObjectId, Property, ScenePatch, TrackId};

use crate::{EvaluationError, FrameChanges, FrameState, SceneInstance};

/// Stable runtime identity independent of semantic IDs and dense frame indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExecutionSlotId {
    slot: u32,
    generation: u32,
}

impl ExecutionSlotId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub const fn slot(self) -> u32 {
        self.slot
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Debug)]
struct ExecutionSlot {
    generation: u32,
    object: Option<ObjectId>,
    next_free: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionSlotMutationStats {
    pub slots_written: usize,
    pub slots_reused: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSlotError {
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    GenerationExhausted(ExecutionSlotId),
}

impl std::fmt::Display for ExecutionSlotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateObject(id) => {
                write!(formatter, "duplicate execution object {}", id.get())
            }
            Self::UnknownObject(id) => write!(formatter, "unknown execution object {}", id.get()),
            Self::GenerationExhausted(id) => write!(
                formatter,
                "execution slot {} generation exhausted at {}",
                id.slot(),
                id.generation()
            ),
        }
    }
}

impl std::error::Error for ExecutionSlotError {}

/// Tombstoned/free-list runtime slot allocator.
///
/// The compatibility compiler may still expose dense object indices, but
/// those indices are no longer the identity consumers should retain across edits.
#[derive(Clone, Debug, Default)]
pub struct ExecutionSlotTable {
    slots: Vec<ExecutionSlot>,
    free_head: Option<u32>,
    object_slots: HashMap<ObjectId, ExecutionSlotId>,
    live_slots: usize,
    last_mutation: ExecutionSlotMutationStats,
}

impl ExecutionSlotTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_compiled(compiled: &CompiledScene) -> Self {
        let mut table = Self::new();
        for object in compiled.objects().iter().filter(|object| object.live) {
            table
                .insert_object(object.id)
                .expect("compiled scene object identities are unique");
        }
        table.last_mutation = ExecutionSlotMutationStats::default();
        table
    }

    pub fn insert_object(
        &mut self,
        object: ObjectId,
    ) -> Result<ExecutionSlotId, ExecutionSlotError> {
        if self.object_slots.contains_key(&object) {
            return Err(ExecutionSlotError::DuplicateObject(object));
        }
        let (slot_index, generation, reused) = if let Some(slot_index) = self.free_head {
            let slot = &mut self.slots[slot_index as usize];
            self.free_head = slot.next_free.take();
            (slot_index, slot.generation, true)
        } else {
            let slot_index =
                u32::try_from(self.slots.len()).expect("Noon execution slot space exhausted");
            self.slots.push(ExecutionSlot {
                generation: 0,
                object: None,
                next_free: None,
            });
            (slot_index, 0, false)
        };
        let id = ExecutionSlotId::new(slot_index, generation);
        self.slots[slot_index as usize].object = Some(object);
        self.object_slots.insert(object, id);
        self.live_slots += 1;
        self.last_mutation = ExecutionSlotMutationStats {
            slots_written: 1,
            slots_reused: usize::from(reused),
        };
        Ok(id)
    }

    pub fn remove_object(
        &mut self,
        object: ObjectId,
    ) -> Result<ExecutionSlotId, ExecutionSlotError> {
        let id = *self
            .object_slots
            .get(&object)
            .ok_or(ExecutionSlotError::UnknownObject(object))?;
        let next_generation = self.slots[id.slot as usize]
            .generation
            .checked_add(1)
            .ok_or(ExecutionSlotError::GenerationExhausted(id))?;
        let removed = self
            .object_slots
            .remove(&object)
            .expect("object existence was preflighted");
        debug_assert_eq!(removed, id);
        let slot = &mut self.slots[id.slot as usize];
        debug_assert_eq!(slot.generation, id.generation);
        debug_assert_eq!(slot.object, Some(object));
        slot.object = None;
        slot.generation = next_generation;
        slot.next_free = self.free_head;
        self.free_head = Some(id.slot);
        self.live_slots -= 1;
        self.last_mutation = ExecutionSlotMutationStats {
            slots_written: 1,
            slots_reused: 0,
        };
        Ok(id)
    }

    pub fn slot_for_object(&self, object: ObjectId) -> Option<ExecutionSlotId> {
        self.object_slots.get(&object).copied()
    }

    pub fn object_for_slot(&self, id: ExecutionSlotId) -> Option<ObjectId> {
        let slot = self.slots.get(id.slot as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.object)
            .flatten()
    }

    pub fn len(&self) -> usize {
        self.live_slots
    }

    pub fn is_empty(&self) -> bool {
        self.live_slots == 0
    }

    pub fn slot_capacity(&self) -> usize {
        self.slots.len()
    }

    pub const fn last_mutation_stats(&self) -> ExecutionSlotMutationStats {
        self.last_mutation
    }

    fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<(), ExecutionSlotError> {
        // Slot metadata is intentionally cheap to stage: it contains only IDs,
        // generations, and free-list links—not frame, geometry, or runtime state.
        let mut shadow = self.clone();
        for patch in transaction.mutations() {
            match patch {
                ScenePatch::CreateObject(object) => {
                    shadow.insert_object(object.id)?;
                }
                ScenePatch::RemoveObject(object) => {
                    shadow.remove_object(*object)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionEffects {
    pub property: bool,
    pub timeline: bool,
    pub structure: bool,
    pub render: bool,
    pub resources: bool,
    pub hierarchy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionChannelDelta {
    pub slot: ExecutionSlotId,
    pub property: Property,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionDelta {
    slots: Vec<ExecutionSlotId>,
    channels: Vec<ExecutionChannelDelta>,
    effects: ExecutionEffects,
}

impl ExecutionDelta {
    pub fn slots(&self) -> &[ExecutionSlotId] {
        &self.slots
    }

    pub fn channels(&self) -> &[ExecutionChannelDelta] {
        &self.channels
    }

    pub const fn effects(&self) -> ExecutionEffects {
        self.effects
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
            && self.channels.is_empty()
            && self.effects == ExecutionEffects::default()
    }

    fn push_slot(&mut self, slot: ExecutionSlotId) {
        if !self.slots.contains(&slot) {
            self.slots.push(slot);
        }
    }

    fn push_channel(&mut self, slot: ExecutionSlotId, property: Property) {
        let delta = ExecutionChannelDelta { slot, property };
        if !self.channels.contains(&delta) {
            self.channels.push(delta);
        }
        self.push_slot(slot);
    }

    fn merge_from(&mut self, other: &Self) {
        for slot in other.slots.iter().copied() {
            self.push_slot(slot);
        }
        for channel in other.channels.iter().copied() {
            self.push_channel(channel.slot, channel.property);
        }
        self.effects.property |= other.effects.property;
        self.effects.timeline |= other.effects.timeline;
        self.effects.structure |= other.effects.structure;
        self.effects.render |= other.effects.render;
        self.effects.resources |= other.effects.resources;
        self.effects.hierarchy |= other.effects.hierarchy;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionTransactionPreflightStats {
    pub compiled: CompiledTransactionPreflightStats,
    pub slots_indexed: usize,
    pub staged_runtime_clones: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionTransactionError {
    Compile(CompilePatchError),
    Slot(ExecutionSlotError),
}

impl std::fmt::Display for ExecutionTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(error) => write!(formatter, "compiled transaction failed: {error}"),
            Self::Slot(error) => write!(formatter, "execution slot transaction failed: {error}"),
        }
    }
}

impl std::error::Error for ExecutionTransactionError {}

impl From<CompilePatchError> for ExecutionTransactionError {
    fn from(value: CompilePatchError) -> Self {
        Self::Compile(value)
    }
}

impl From<ExecutionSlotError> for ExecutionTransactionError {
    fn from(value: ExecutionSlotError) -> Self {
        Self::Slot(value)
    }
}

#[derive(Clone, Debug, Default)]
struct PatchContext {
    primary_slot: Option<ExecutionSlotId>,
    old_track_channels: Vec<ExecutionChannelDelta>,
}

/// Transitional runtime adapter exposing stable execution identity while
/// the renderer/frame compatibility view remains dense.
#[derive(Clone, Debug)]
pub struct SlottedSceneInstance {
    inner: SceneInstance,
    slots: ExecutionSlotTable,
    last_delta: ExecutionDelta,
}

impl SlottedSceneInstance {
    pub fn new(compiled: CompiledScene) -> Self {
        let slots = ExecutionSlotTable::from_compiled(&compiled);
        Self {
            inner: SceneInstance::new(compiled),
            slots,
            last_delta: ExecutionDelta::default(),
        }
    }

    pub fn frame(&self) -> &FrameState {
        self.inner.frame()
    }

    pub fn scene_instance(&self) -> &SceneInstance {
        &self.inner
    }

    pub fn slot_table(&self) -> &ExecutionSlotTable {
        &self.slots
    }

    pub fn slot_for_object(&self, object: ObjectId) -> Option<ExecutionSlotId> {
        self.slots.slot_for_object(object)
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        &self.last_delta
    }

    pub fn take_execution_delta(&mut self) -> ExecutionDelta {
        std::mem::take(&mut self.last_delta)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.seek(time)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.advance_to(time)
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.inner.take_frame_changes()
    }

    pub fn contains_object(&self, object: ObjectId) -> bool {
        self.inner.contains_object(object)
    }

    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<ExecutionTransactionPreflightStats, ExecutionTransactionError> {
        let compiled = self.inner.preflight_transaction(transaction)?;
        self.slots.preflight_transaction(transaction)?;
        Ok(ExecutionTransactionPreflightStats {
            compiled,
            slots_indexed: self.slots.slot_capacity(),
            staged_runtime_clones: 0,
        })
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, ExecutionTransactionError> {
        self.preflight_transaction(transaction)?;
        let mut aggregate = ExecutionDelta::default();
        for patch in transaction.mutations() {
            self.apply_patch(patch)
                .expect("execution transaction was fully preflighted");
            aggregate.merge_from(&self.last_delta);
        }
        self.last_delta = aggregate;
        Ok(self.inner.frame())
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        let context = self.capture_context(patch);
        self.inner.apply_patch(patch)?;
        let mut delta = ExecutionDelta::default();

        match patch {
            ScenePatch::CreateObject(object) => {
                let slot = self
                    .slots
                    .insert_object(object.id)
                    .expect("compiled create succeeded after slot preflight");
                delta.push_slot(slot);
                delta.effects = ExecutionEffects {
                    structure: true,
                    render: true,
                    resources: true,
                    hierarchy: true,
                    ..ExecutionEffects::default()
                };
            }
            ScenePatch::RemoveObject(object) => {
                let slot = context
                    .primary_slot
                    .expect("compiled removal succeeded for a slotted object");
                self.slots
                    .remove_object(*object)
                    .expect("slot table mirrors compiled object identities");
                delta.push_slot(slot);
                for channel in context.old_track_channels {
                    delta.push_channel(channel.slot, channel.property);
                }
                delta.effects = ExecutionEffects {
                    timeline: !delta.channels.is_empty(),
                    structure: true,
                    render: true,
                    resources: true,
                    hierarchy: true,
                    ..ExecutionEffects::default()
                };
            }
            ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {
                let slot = self
                    .slots
                    .slot_for_object(*object)
                    .expect("compiled property patch succeeded for slotted object");
                delta.push_slot(slot);
                delta.effects.property = true;
                delta.effects.render = true;
            }
            ScenePatch::AddTrack(track) => {
                let slot = self
                    .slots
                    .slot_for_object(track.object)
                    .expect("compiled track target has a slot");
                delta.push_channel(slot, track.property);
                delta.effects.timeline = true;
                delta.effects.render = true;
            }
            ScenePatch::ReplaceTrack(track) => {
                for channel in context.old_track_channels {
                    delta.push_channel(channel.slot, channel.property);
                }
                let slot = self
                    .slots
                    .slot_for_object(track.object)
                    .expect("compiled replacement target has a slot");
                delta.push_channel(slot, track.property);
                delta.effects.timeline = true;
                delta.effects.render = true;
            }
            ScenePatch::RemoveTrack(_) => {
                for channel in context.old_track_channels {
                    delta.push_channel(channel.slot, channel.property);
                }
                delta.effects.timeline = true;
                delta.effects.render = true;
            }
        }

        self.last_delta = delta;
        Ok(self.inner.frame())
    }

    fn capture_context(&self, patch: &ScenePatch) -> PatchContext {
        let mut context = PatchContext::default();
        match patch {
            ScenePatch::RemoveObject(object)
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
                context.primary_slot = self.slots.slot_for_object(*object);
                if matches!(patch, ScenePatch::RemoveObject(_)) {
                    if let (Some(slot), Some(object_index)) = (
                        context.primary_slot,
                        self.inner.compiled.object_index(*object),
                    ) {
                        for property in EXECUTION_PROPERTIES {
                            if !self
                                .inner
                                .compiled
                                .track_group(object_index, property)
                                .is_empty()
                            {
                                push_context_channel(
                                    &mut context.old_track_channels,
                                    slot,
                                    property,
                                );
                            }
                        }
                    }
                }
            }
            ScenePatch::ReplaceTrack(track) => {
                self.capture_track(track.id, &mut context.old_track_channels);
            }
            ScenePatch::RemoveTrack(id) => {
                self.capture_track(*id, &mut context.old_track_channels);
            }
            ScenePatch::CreateObject(_) | ScenePatch::AddTrack(_) => {}
        }
        context
    }

    fn capture_track(&self, id: TrackId, channels: &mut Vec<ExecutionChannelDelta>) {
        let Some(track) = self
            .inner
            .compiled
            .tracks()
            .iter()
            .find(|track| track.id == id)
        else {
            return;
        };
        let object = &self.inner.compiled.objects()[track.object_index as usize];
        if let Some(slot) = self.slots.slot_for_object(object.id) {
            push_context_channel(channels, slot, track.property);
        }
    }
}

const EXECUTION_PROPERTIES: [Property; 8] = [
    Property::Presence,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Opacity,
    Property::Appearance,
    Property::Reveal,
    Property::Morph,
];

fn push_context_channel(
    channels: &mut Vec<ExecutionChannelDelta>,
    slot: ExecutionSlotId,
    property: Property,
) {
    let delta = ExecutionChannelDelta { slot, property };
    if !channels.contains(&delta) {
        channels.push(delta);
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{
        Easing, GeometryRef, ObjectId, Property, SceneDefinition, ScenePatch, TrackDefinition,
        TrackId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    #[test]
    fn removing_slot_ten_from_hundred_thousand_keeps_all_other_ids_stable() {
        let mut slots = ExecutionSlotTable::new();
        let mut ids = Vec::with_capacity(100_000);
        for index in 0..100_000u64 {
            ids.push(
                slots
                    .insert_object(ObjectId::new(index))
                    .expect("unique object"),
            );
        }
        let eleventh_before = ids[11];
        let last_before = ids[99_999];
        let removed = ids[10];

        assert_eq!(slots.remove_object(ObjectId::new(10)), Ok(removed));
        assert_eq!(
            slots.slot_for_object(ObjectId::new(11)),
            Some(eleventh_before)
        );
        assert_eq!(
            slots.slot_for_object(ObjectId::new(99_999)),
            Some(last_before)
        );
        assert_eq!(slots.last_mutation_stats().slots_written, 1);

        let reused = slots
            .insert_object(ObjectId::new(100_000))
            .expect("free slot is reusable");
        assert_eq!(reused.slot(), removed.slot());
        assert_eq!(reused.generation(), removed.generation() + 1);
        assert_eq!(slots.object_for_slot(removed), None);
        assert_eq!(slots.object_for_slot(reused), Some(ObjectId::new(100_000)));
    }

    #[test]
    fn generation_exhaustion_leaves_slot_table_unchanged() {
        let mut slots = ExecutionSlotTable::new();
        let object = ObjectId::new(7);
        let initial = slots.insert_object(object).expect("unique object");
        let exhausted = ExecutionSlotId::new(initial.slot(), u32::MAX);
        slots.slots[initial.slot() as usize].generation = u32::MAX;
        slots.object_slots.insert(object, exhausted);

        assert_eq!(
            slots.remove_object(object),
            Err(ExecutionSlotError::GenerationExhausted(exhausted))
        );
        assert_eq!(slots.slot_for_object(object), Some(exhausted));
        assert_eq!(slots.object_for_slot(exhausted), Some(object));
        assert_eq!(slots.len(), 1);
    }

    #[test]
    fn dense_compatibility_removal_does_not_change_execution_identity() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::rectangle(2.0, 1.0));
        let third = definition.add(GeometryRef::circle(0.5));
        let compiled = CompiledScene::compile(&definition).expect("valid scene");
        let mut live = SlottedSceneInstance::new(compiled);
        let second_slot = live.slot_for_object(second).unwrap();
        let third_slot = live.slot_for_object(third).unwrap();

        live.apply_patch(&ScenePatch::RemoveObject(first))
            .expect("valid removal");

        assert_eq!(live.slot_for_object(second), Some(second_slot));
        assert_eq!(live.slot_for_object(third), Some(third_slot));
        assert!(!live.frame().objects[0].live);
        assert!(!live.frame().is_present(0));
        assert_eq!(live.frame().objects[second_slot.slot() as usize].id, second);
        assert!(live.frame().objects[second_slot.slot() as usize].live);
        assert_eq!(live.frame().objects[third_slot.slot() as usize].id, third);
        assert!(live.frame().objects[third_slot.slot() as usize].live);
        assert_eq!(
            live.last_execution_delta().slots(),
            &[ExecutionSlotId::new(0, 0)]
        );
        assert!(live.last_execution_delta().effects().structure);
    }

    #[test]
    fn timeline_patch_reports_only_affected_slot_and_channel() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::rectangle(2.0, 1.0));
        let compiled = CompiledScene::compile(&definition).expect("valid scene");
        let mut live = SlottedSceneInstance::new(compiled);
        let first_slot = live.slot_for_object(first).unwrap();
        let second_slot = live.slot_for_object(second).unwrap();
        let track = TrackDefinition {
            id: TrackId::new(20),
            object: first,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(3.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };

        live.apply_patch(&ScenePatch::AddTrack(track))
            .expect("valid timeline edit");

        let delta = live.last_execution_delta();
        assert_eq!(delta.slots(), &[first_slot]);
        assert_eq!(
            delta.channels(),
            &[ExecutionChannelDelta {
                slot: first_slot,
                property: Property::Position,
            }]
        );
        assert!(!delta.slots().contains(&second_slot));
        assert!(delta.effects().timeline);
    }

    #[test]
    fn transaction_aggregates_bounded_execution_delta_without_runtime_clone() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile(&definition).expect("valid scene");
        let mut live = SlottedSceneInstance::new(compiled);
        let first_slot = live.slot_for_object(first).expect("first slot");
        let second_slot = live.slot_for_object(second).expect("second slot");
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object: first,
                transform: noon_core::Transform2D {
                    translation: Vec2::new(2.0, 0.0),
                    ..noon_core::Transform2D::IDENTITY
                },
            },
            ScenePatch::AddTrack(TrackDefinition {
                id: TrackId::new(20),
                object: first,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(3.0, 0.0),
                },
                timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
                time_map: noon_core::CompositionTimeMap::identity(),
            }),
        ]);
        let preflight = live
            .preflight_transaction(&transaction)
            .expect("transaction preflights");
        assert_eq!(preflight.staged_runtime_clones, 0);
        live.apply_transaction(&transaction)
            .expect("transaction commits locally");
        let delta = live.last_execution_delta();
        assert_eq!(delta.slots(), &[first_slot]);
        assert!(!delta.slots().contains(&second_slot));
        assert!(delta.effects().property);
        assert!(delta.effects().timeline);
    }
}
