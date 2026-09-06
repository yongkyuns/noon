use std::collections::{HashMap, HashSet};

use noon_compile::{
    CompilePatchError, CompiledScene, CompiledTransactionPreflightStats, ExecutionPatch,
};
use noon_core::{MutationTransaction, ObjectId, Property, Rect, ScenePatch, TrackId, Vec2};

use crate::{
    EvaluationError, ExecutionSpatialIndex, FrameChanges, FrameState, SceneInstance,
    SpatialIndexUpdateStats, SpatialQueryResult,
};

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

/// Compatibility frame-slot handle scoped to one compact layout generation.
///
/// `ExecutionSlotId` remains the durable object identity. Frame slots are an
/// order-preserving renderer/runtime projection and may be renumbered only by an
/// explicit compaction barrier. Carrying the layout generation prevents a stale
/// pre-compaction frame row from silently aliasing a different object afterward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameSlotId {
    index: u32,
    layout_generation: u64,
}

impl FrameSlotId {
    pub const fn new(index: u32, layout_generation: u64) -> Self {
        Self {
            index,
            layout_generation,
        }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn layout_generation(self) -> u64 {
        self.layout_generation
    }
}

/// Heuristic for deciding when an explicit maintenance checkpoint is worthwhile.
///
/// The normal mutation path never compacts automatically: doing so would renumber
/// source/painter-order frame rows in the middle of an edit. Callers can use this
/// policy to decide when to request the explicit generation barrier instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetiredSlotCompactionPolicy {
    pub min_retired_slots: usize,
    pub min_retired_percent: u8,
}

impl RetiredSlotCompactionPolicy {
    pub const fn new(min_retired_slots: usize, min_retired_percent: u8) -> Self {
        Self {
            min_retired_slots,
            min_retired_percent,
        }
    }

    pub fn recommends(self, live_slots: usize, slot_capacity: usize) -> bool {
        let retired = slot_capacity.saturating_sub(live_slots);
        if retired == 0 || retired < self.min_retired_slots || slot_capacity == 0 {
            return false;
        }
        let percent = usize::from(self.min_retired_percent.min(100));
        retired.saturating_mul(100) >= slot_capacity.saturating_mul(percent)
    }
}

impl Default for RetiredSlotCompactionPolicy {
    fn default() -> Self {
        Self::new(1_024, 25)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionCompactionStats {
    pub previous_layout_generation: u64,
    pub layout_generation: u64,
    pub frame_slots_before: usize,
    pub frame_slots_after: usize,
    pub frame_slots_reclaimed: usize,
    pub execution_slot_capacity: usize,
    /// Durable execution slots are never rewritten by compatibility compaction.
    pub execution_slots_rewritten: usize,
    /// Explicit compaction rebuilds the compatibility runtime once.
    pub runtime_rebuilds: usize,
    /// The rebuilt runtime is restored to the previous playhead once.
    pub full_seeks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionCompactionError {
    LayoutGenerationExhausted,
    SceneMismatch,
    NonCompactInput {
        live_slots: usize,
        slot_capacity: usize,
    },
}

impl std::fmt::Display for ExecutionCompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayoutGenerationExhausted => {
                formatter.write_str("frame-slot layout generation exhausted")
            }
            Self::SceneMismatch => formatter.write_str(
                "compaction input does not match the current live compiled scene",
            ),
            Self::NonCompactInput {
                live_slots,
                slot_capacity,
            } => write!(
                formatter,
                "compaction input still contains retired slots: {live_slots} live of {slot_capacity}",
            ),
        }
    }
}

impl std::error::Error for ExecutionCompactionError {}

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
    CapacityExhausted,
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
            Self::CapacityExhausted => formatter.write_str("execution slot capacity exhausted"),
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

struct ExecutionSlotPreflight<'a> {
    base: &'a ExecutionSlotTable,
    free_head: Option<u32>,
    next_new_slot: usize,
    touched_slots: HashMap<u32, ExecutionSlot>,
    object_overrides: HashMap<ObjectId, Option<ExecutionSlotId>>,
}

impl<'a> ExecutionSlotPreflight<'a> {
    fn new(base: &'a ExecutionSlotTable) -> Self {
        Self {
            base,
            free_head: base.free_head,
            next_new_slot: base.slots.len(),
            touched_slots: HashMap::new(),
            object_overrides: HashMap::new(),
        }
    }

    fn slots_indexed(&self) -> usize {
        self.touched_slots.len()
    }

    fn slot_for_object(&self, object: ObjectId) -> Option<ExecutionSlotId> {
        match self.object_overrides.get(&object) {
            Some(slot) => *slot,
            None => self.base.object_slots.get(&object).copied(),
        }
    }

    fn slot_mut(&mut self, slot_index: u32) -> &mut ExecutionSlot {
        if !self.touched_slots.contains_key(&slot_index) {
            let slot = self.base.slots[slot_index as usize].clone();
            self.touched_slots.insert(slot_index, slot);
        }
        self.touched_slots
            .get_mut(&slot_index)
            .expect("touched slot was materialized")
    }

    fn insert_object(&mut self, object: ObjectId) -> Result<(), ExecutionSlotError> {
        if self.slot_for_object(object).is_some() {
            return Err(ExecutionSlotError::DuplicateObject(object));
        }

        let id = if let Some(slot_index) = self.free_head {
            let (generation, next_free) = {
                let slot = self.slot_mut(slot_index);
                (slot.generation, slot.next_free)
            };
            self.free_head = next_free;
            let slot = self.slot_mut(slot_index);
            slot.object = Some(object);
            slot.next_free = None;
            ExecutionSlotId::new(slot_index, generation)
        } else {
            let slot_index =
                u32::try_from(self.next_new_slot).expect("Noon execution slot space exhausted");
            self.next_new_slot += 1;
            self.touched_slots.insert(
                slot_index,
                ExecutionSlot {
                    generation: 0,
                    object: Some(object),
                    next_free: None,
                },
            );
            ExecutionSlotId::new(slot_index, 0)
        };
        self.object_overrides.insert(object, Some(id));
        Ok(())
    }

    fn remove_object(&mut self, object: ObjectId) -> Result<(), ExecutionSlotError> {
        let id = self
            .slot_for_object(object)
            .ok_or(ExecutionSlotError::UnknownObject(object))?;
        let next_generation = self
            .slot_mut(id.slot)
            .generation
            .checked_add(1)
            .ok_or(ExecutionSlotError::GenerationExhausted(id))?;
        let free_head = self.free_head;
        let slot = self.slot_mut(id.slot);
        debug_assert_eq!(slot.generation, id.generation);
        debug_assert_eq!(slot.object, Some(object));
        slot.object = None;
        slot.generation = next_generation;
        slot.next_free = free_head;
        self.free_head = Some(id.slot);
        self.object_overrides.insert(object, None);
        Ok(())
    }
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
    ) -> Result<usize, ExecutionSlotError> {
        let mut shadow = ExecutionSlotPreflight::new(self);
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
        Ok(shadow.slots_indexed())
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
    /// Unique execution slots materialized by sparse structural preflight.
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
    spatial_index: ExecutionSpatialIndex,
    last_spatial_update: SpatialIndexUpdateStats,
    last_delta: ExecutionDelta,
    layout_generation: u64,
}

impl SlottedSceneInstance {
    pub fn new(compiled: CompiledScene) -> Self {
        let slots = ExecutionSlotTable::from_compiled(&compiled);
        let mut inner = SceneInstance::new(compiled);
        let live_slots = inner
            .compiled
            .objects()
            .iter()
            .enumerate()
            .filter(|&(_index, object)| object.live)
            .map(|(index, object)| {
                (
                    slots
                        .slot_for_object(object.id)
                        .expect("live object has slot"),
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut spatial_index = ExecutionSpatialIndex::default();
        let last_spatial_update = spatial_index.rebuild(inner.frame(), live_slots);
        let _ = inner.take_spatial_changes();
        Self {
            inner,
            slots,
            spatial_index,
            last_spatial_update,
            last_delta: ExecutionDelta::default(),
            layout_generation: 0,
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

    pub fn spatial_index(&self) -> &ExecutionSpatialIndex {
        &self.spatial_index
    }

    pub const fn last_spatial_update_stats(&self) -> SpatialIndexUpdateStats {
        self.last_spatial_update
    }

    pub fn hit_test(&self, point: Vec2) -> SpatialQueryResult {
        self.spatial_index.hit_test(point)
    }

    pub fn query_viewport(&self, bounds: Rect) -> SpatialQueryResult {
        self.spatial_index.query_rect(bounds)
    }

    pub fn slot_for_object(&self, object: ObjectId) -> Option<ExecutionSlotId> {
        self.slots.slot_for_object(object)
    }

    pub const fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    pub fn frame_slot_capacity(&self) -> usize {
        self.inner.compiled.objects().len()
    }

    pub fn retired_frame_slot_count(&self) -> usize {
        self.frame_slot_capacity()
            .saturating_sub(self.live_object_count())
    }

    pub fn compaction_recommended(&self, policy: RetiredSlotCompactionPolicy) -> bool {
        policy.recommends(self.live_object_count(), self.frame_slot_capacity())
    }

    pub fn frame_slot_for_execution_slot(&self, slot: ExecutionSlotId) -> Option<FrameSlotId> {
        let index = u32::try_from(self.frame_index_for_slot(slot)?).ok()?;
        Some(FrameSlotId::new(index, self.layout_generation))
    }

    pub fn execution_slot_for_frame_slot(&self, slot: FrameSlotId) -> Option<ExecutionSlotId> {
        if slot.layout_generation() != self.layout_generation {
            return None;
        }
        self.slot_for_frame_index(slot.index() as usize)
    }

    pub fn slot_for_frame_index(&self, frame_index: usize) -> Option<ExecutionSlotId> {
        if !self.inner.object_slot_is_live(frame_index) {
            return None;
        }
        let object = self.inner.frame().objects.get(frame_index)?.id;
        self.slots.slot_for_object(object)
    }

    pub fn frame_index_for_slot(&self, slot: ExecutionSlotId) -> Option<usize> {
        let object = self.slots.object_for_slot(slot)?;
        self.inner
            .compiled
            .object_index(object)
            .map(|index| index as usize)
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        &self.last_delta
    }

    pub fn take_execution_delta(&mut self) -> ExecutionDelta {
        std::mem::take(&mut self.last_delta)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.seek(time)?;
        self.sync_spatial_index();
        Ok(self.inner.frame())
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.advance_to(time)?;
        self.sync_spatial_index();
        Ok(self.inner.frame())
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.inner.take_frame_changes()
    }

    pub fn contains_object(&self, object: ObjectId) -> bool {
        self.inner.contains_object(object)
    }

    pub fn live_object_count(&self) -> usize {
        self.inner.compiled.live_object_count()
    }

    pub fn live_frame_indices(&self) -> Vec<usize> {
        self.inner
            .compiled
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(index, object)| object.live.then_some(index))
            .collect()
    }

    /// Reclaim retired compatibility frame slots at an explicit maintenance barrier.
    ///
    /// The supplied scene must be a compact recompilation of the same live semantic
    /// scene in the same source/painter order. Durable `ExecutionSlotId`s are kept
    /// intact; only compiled/frame row positions are rebuilt. The fresh runtime leaves
    /// `FrameChanges::all()` pending so renderer/worker consumers perform a deliberate
    /// full resynchronization rather than observing hidden row renumbering.
    pub fn compact_with_compiled(
        &mut self,
        compiled: CompiledScene,
    ) -> Result<ExecutionCompactionStats, ExecutionCompactionError> {
        let before = self.frame_slot_capacity();
        let live = self.live_object_count();
        if compiled.objects().len() != compiled.live_object_count() {
            return Err(ExecutionCompactionError::NonCompactInput {
                live_slots: compiled.live_object_count(),
                slot_capacity: compiled.objects().len(),
            });
        }
        if !compiled_scene_matches_live_projection(&self.inner.compiled, &compiled) {
            return Err(ExecutionCompactionError::SceneMismatch);
        }
        if before == live {
            return Ok(ExecutionCompactionStats {
                previous_layout_generation: self.layout_generation,
                layout_generation: self.layout_generation,
                frame_slots_before: before,
                frame_slots_after: before,
                frame_slots_reclaimed: 0,
                execution_slot_capacity: self.slots.slot_capacity(),
                ..ExecutionCompactionStats::default()
            });
        }

        let next_generation = self
            .layout_generation
            .checked_add(1)
            .ok_or(ExecutionCompactionError::LayoutGenerationExhausted)?;
        let time = self.inner.frame().time;
        let mut replacement = SceneInstance::new(compiled);
        replacement
            .seek(time)
            .expect("existing scene playhead must remain finite during compaction");
        replacement.publication = self.inner.publication_context();
        replacement.publish_execution_change();
        self.inner = replacement;
        self.layout_generation = next_generation;
        self.last_delta = ExecutionDelta::default();
        self.sync_spatial_index();

        let after = self.frame_slot_capacity();
        debug_assert_eq!(after, live);
        Ok(ExecutionCompactionStats {
            previous_layout_generation: next_generation - 1,
            layout_generation: next_generation,
            frame_slots_before: before,
            frame_slots_after: after,
            frame_slots_reclaimed: before - after,
            execution_slot_capacity: self.slots.slot_capacity(),
            execution_slots_rewritten: 0,
            runtime_rebuilds: 1,
            full_seeks: 1,
        })
    }

    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<ExecutionTransactionPreflightStats, ExecutionTransactionError> {
        let compiled = self.inner.compiled.preflight_transaction(transaction)?;
        let slots_indexed = self.slots.preflight_transaction(transaction)?;
        Ok(ExecutionTransactionPreflightStats {
            compiled,
            slots_indexed,
            staged_runtime_clones: 0,
        })
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, ExecutionTransactionError> {
        self.preflight_transaction(transaction)?;
        let mut aggregate = ExecutionDelta::default();
        let mut changed = false;
        for patch in final_legacy_value_writes(transaction) {
            if !self
                .inner
                .compiled
                .patch_changes_execution(&ExecutionPatch::decode(patch))
            {
                continue;
            }
            self.apply_patch_unpublished(patch)
                .expect("execution transaction was fully preflighted");
            changed = true;
            aggregate.merge_from(&self.last_delta);
        }
        self.last_delta = aggregate;
        if changed {
            self.inner.publish_execution_change();
        }
        Ok(self.inner.frame())
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        if !self
            .inner
            .compiled
            .patch_changes_execution(&ExecutionPatch::decode(patch))
        {
            self.last_delta = ExecutionDelta::default();
            return Ok(self.inner.frame());
        }
        self.apply_patch_unpublished(patch)?;
        self.inner.publish_execution_change();
        Ok(self.inner.frame())
    }

    fn apply_patch_unpublished(
        &mut self,
        patch: &ScenePatch,
    ) -> Result<&FrameState, CompilePatchError> {
        let context = self.capture_context(patch);
        self.inner
            .apply_patch_unpublished(&ExecutionPatch::decode(patch))?;
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
            ScenePatch::SetGeometry { object, .. }
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
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
        self.sync_spatial_index();
        Ok(self.inner.frame())
    }

    fn sync_spatial_index(&mut self) {
        let changes = self.inner.take_spatial_changes();
        if changes.is_empty() {
            self.last_spatial_update = SpatialIndexUpdateStats::default();
            return;
        }
        if changes.is_all() {
            let live_slots = self
                .inner
                .compiled
                .objects()
                .iter()
                .enumerate()
                .filter(|&(_index, object)| object.live)
                .map(|(index, object)| {
                    (
                        self.slots
                            .slot_for_object(object.id)
                            .expect("live compiled object has an execution slot"),
                        index,
                    )
                })
                .collect::<Vec<_>>();
            self.last_spatial_update = self.spatial_index.rebuild(self.inner.frame(), live_slots);
            return;
        }

        let mut stats = SpatialIndexUpdateStats::default();
        for &object_index in changes.object_indices() {
            let Some(object) = self.inner.frame().objects.get(object_index) else {
                continue;
            };
            if self.inner.object_slot_is_live(object_index) {
                if let Some(slot) = self.slots.slot_for_object(object.id) {
                    stats.merge_from(self.spatial_index.upsert_frame_slot(
                        self.inner.frame(),
                        slot,
                        object_index,
                        object_index as u64,
                    ));
                }
            } else {
                stats.merge_from(self.spatial_index.remove_object(object.id));
            }
        }
        self.last_spatial_update = stats;
    }

    fn capture_context(&self, patch: &ScenePatch) -> PatchContext {
        let mut context = PatchContext::default();
        match patch {
            ScenePatch::RemoveObject(object)
            | ScenePatch::SetGeometry { object, .. }
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
                context.primary_slot = self.slots.slot_for_object(*object);
                if matches!(patch, ScenePatch::RemoveObject(_)) {
                    if let Some(slot) = context.primary_slot {
                        for channel in self.inner.compiled.object_channels(*object) {
                            push_context_channel(
                                &mut context.old_track_channels,
                                slot,
                                channel.property,
                            );
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
        let Some(channel) = self.inner.compiled.channel_for_track(id) else {
            return;
        };
        let Some(object) = self.inner.compiled.track_object(id) else {
            return;
        };
        if let Some(slot) = self.slots.slot_for_object(object) {
            push_context_channel(channels, slot, channel.property);
        }
    }
}

fn final_legacy_value_writes(transaction: &MutationTransaction) -> Vec<&ScenePatch> {
    let mut final_writes = HashSet::new();
    let mut retained = Vec::with_capacity(transaction.mutations().len());
    for patch in transaction.mutations().iter().rev() {
        match patch {
            ScenePatch::SetGeometry { object, .. }
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
                if !final_writes.insert((*object, std::mem::discriminant(patch))) {
                    continue;
                }
            }
            _ => final_writes.clear(),
        }
        retained.push(patch);
    }
    retained.reverse();
    retained
}

fn compiled_scene_matches_live_projection(
    current: &CompiledScene,
    compact: &CompiledScene,
) -> bool {
    if current.live_object_count() != compact.live_object_count()
        || current.track_count() != compact.track_count()
        || current.resources() != compact.resources()
    {
        return false;
    }

    let current_objects = current.objects().iter().filter(|object| object.live);
    if !current_objects.zip(compact.objects()).all(|(left, right)| {
        left.id == right.id
            && left.content == right.content
            && left.text_bounds == right.text_bounds
            && left.base_transform == right.base_transform
            && left.base_style == right.base_style
            && left.dynamic == right.dynamic
            && right.live
    }) {
        return false;
    }

    current.tracks_iter().all(|track| {
        let Some(candidate) = compact.track(track.id) else {
            return false;
        };
        current.track_object(track.id) == compact.track_object(track.id)
            && track.id == candidate.id
            && track.property == candidate.property
            && track.values == candidate.values
            && track.timing == candidate.timing
            && track.time_map == candidate.time_map
            && track.transform_geometry_plan == candidate.transform_geometry_plan
    })
}

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
    fn structural_transaction_preflight_materializes_only_touched_slots() {
        let mut slots = ExecutionSlotTable::new();
        for index in 0..100_000u64 {
            slots
                .insert_object(ObjectId::new(index))
                .expect("unique object");
        }
        let object = ObjectId::new(10);
        let slot = slots.slot_for_object(object).expect("existing slot");
        let transaction = MutationTransaction::from_mutations([ScenePatch::RemoveObject(object)]);

        assert_eq!(slots.preflight_transaction(&transaction), Ok(1));
        assert_eq!(slots.slot_capacity(), 100_000);
        assert_eq!(slots.len(), 100_000);
        assert_eq!(slots.slot_for_object(object), Some(slot));
        assert_eq!(slots.object_for_slot(slot), Some(object));
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
        assert_eq!(live.frame().objects[0].id, first);
        assert!(!live.frame().presences[0]);
        assert_eq!(live.frame().objects[1].id, second);
        assert_eq!(live.frame().objects[2].id, third);
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
        assert_eq!(preflight.slots_indexed, 0);
        assert_eq!(preflight.staged_runtime_clones, 0);
        assert_eq!(preflight.compiled.staged_compiled_scene_clones, 0);
        live.apply_transaction(&transaction)
            .expect("transaction commits locally");
        let delta = live.last_execution_delta();
        assert_eq!(delta.slots(), &[first_slot]);
        assert!(!delta.slots().contains(&second_slot));
        assert!(delta.effects().property);
        assert!(delta.effects().timeline);
    }

    #[test]
    fn explicit_compaction_reclaims_frame_tombstones_without_rewriting_execution_slots() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::rectangle(2.0, 1.0));
        let third = definition.add(GeometryRef::circle(0.5));
        let compiled = CompiledScene::compile(&definition).expect("valid scene");
        let mut live = SlottedSceneInstance::new(compiled);
        live.seek(0.75).unwrap();

        let second_execution = live.slot_for_object(second).unwrap();
        let third_execution = live.slot_for_object(third).unwrap();
        let old_second_frame = live
            .frame_slot_for_execution_slot(second_execution)
            .unwrap();
        let execution_capacity = live.slot_table().slot_capacity();

        live.apply_patch(&ScenePatch::RemoveObject(first)).unwrap();
        definition
            .apply_patch(ScenePatch::RemoveObject(first))
            .unwrap();
        assert_eq!(live.frame_slot_capacity(), 3);
        assert_eq!(live.retired_frame_slot_count(), 1);

        let before_publication = live.inner.publication_context();
        let compact = CompiledScene::compile(&definition).unwrap();
        let stats = live.compact_with_compiled(compact).unwrap();
        let after_publication = live.inner.publication_context();
        assert_eq!(
            after_publication.scene_revision(),
            before_publication.scene_revision()
        );
        assert_eq!(
            after_publication.execution_revision(),
            before_publication
                .execution_revision()
                .checked_next()
                .unwrap()
        );
        assert_eq!(
            after_publication.frame_epoch(),
            before_publication.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(stats.frame_slots_before, 3);
        assert_eq!(stats.frame_slots_after, 2);
        assert_eq!(stats.frame_slots_reclaimed, 1);
        assert_eq!(stats.execution_slots_rewritten, 0);
        assert_eq!(stats.runtime_rebuilds, 1);
        assert_eq!(stats.full_seeks, 1);
        assert_eq!(live.layout_generation(), 1);
        assert_eq!(live.slot_table().slot_capacity(), execution_capacity);
        assert_eq!(live.slot_for_object(second), Some(second_execution));
        assert_eq!(live.slot_for_object(third), Some(third_execution));
        assert_eq!(live.frame().time, 0.75);
        assert_eq!(live.frame().objects.len(), 2);
        assert_eq!(live.frame().objects[0].id, second);
        assert_eq!(live.frame().objects[1].id, third);
        assert_eq!(live.execution_slot_for_frame_slot(old_second_frame), None);
        let new_second_frame = live
            .frame_slot_for_execution_slot(second_execution)
            .unwrap();
        assert_eq!(new_second_frame.index(), 0);
        assert_eq!(new_second_frame.layout_generation(), 1);
        assert_eq!(
            live.execution_slot_for_frame_slot(new_second_frame),
            Some(second_execution)
        );
        assert!(live.take_frame_changes().is_all());
        live.compact_with_compiled(CompiledScene::compile(&definition).unwrap())
            .unwrap();
        assert_eq!(live.inner.publication_context(), after_publication);
    }

    #[test]
    fn slotted_transaction_publishes_once_for_multiple_changed_objects() {
        let mut store = noon_core::SemanticStore::new();
        let nodes = [1.0, 2.0].map(|radius| {
            let object = store.insert_semantic_object(noon_core::SemanticObjectState::new(
                noon_core::StoredGeometry::Circle { radius },
            ));
            store.attach_to_scene(object).unwrap();
            object
        });
        let mut index = noon_compile::SemanticExecutionIndex::new();
        let lowered = noon_compile::lower_semantic_execution(&store, &mut index).unwrap();
        let [first, second] = nodes.map(|object| index.execution_object_id(object).unwrap());
        let mut live = SlottedSceneInstance::new(lowered.into_parts().0);
        let before = live.inner.publication_context();
        let transaction = MutationTransaction::from_mutations([first, second].map(|object| {
            ScenePatch::SetTransform {
                object,
                transform: noon_core::Transform2D {
                    translation: Vec2::ONE,
                    ..noon_core::Transform2D::IDENTITY
                },
            }
        }));
        live.apply_transaction(&transaction).unwrap();
        let after = live.inner.publication_context();
        assert_eq!(
            after.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(live.last_execution_delta().slots().len(), 2);
        live.take_frame_changes();
        live.apply_transaction(&transaction).unwrap();
        assert_eq!(live.inner.publication_context(), after);
        assert!(live.last_execution_delta().slots().is_empty());
        assert!(live.take_frame_changes().is_empty());
    }

    #[test]
    fn compaction_policy_requires_both_absolute_and_fractional_retirement() {
        let policy = RetiredSlotCompactionPolicy::new(1_000, 25);
        assert!(!policy.recommends(9_000, 10_000));
        assert!(policy.recommends(7_500, 10_000));
        assert!(!policy.recommends(900, 1_000));
        assert!(!policy.recommends(10_000, 10_000));
    }

    #[test]
    fn transform_patch_refits_one_spatial_leaf_without_rebuild() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(0.5));
        let compiled = CompiledScene::compile(&definition).unwrap();
        let mut live = SlottedSceneInstance::new(compiled);
        let slot = live.slot_for_object(object).unwrap();
        assert_eq!(live.hit_test(Vec2::ZERO).slots(), &[slot]);

        live.apply_patch(&ScenePatch::SetTransform {
            object,
            transform: noon_core::Transform2D {
                translation: Vec2::new(20.0, 0.0),
                ..noon_core::Transform2D::IDENTITY
            },
        })
        .unwrap();

        let stats = live.last_spatial_update_stats();
        assert_eq!(stats.full_rebuilds, 0);
        assert_eq!(stats.leaves_upserted, 1);
        assert!(live.hit_test(Vec2::ZERO).slots().is_empty());
        assert_eq!(live.hit_test(Vec2::new(20.0, 0.0)).slots(), &[slot]);
    }

    #[test]
    fn forward_timeline_motion_refits_only_active_object_leaf() {
        let mut definition = SceneDefinition::new();
        let moving = definition.add(GeometryRef::circle(0.5));
        let static_object = definition.add(GeometryRef::circle(0.5));
        definition
            .object_mut(static_object)
            .unwrap()
            .transform
            .translation = Vec2::new(0.0, 10.0);
        definition
            .animate_position(
                moving,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .unwrap();
        let mut live = SlottedSceneInstance::new(CompiledScene::compile(&definition).unwrap());
        live.advance_to(1.0).unwrap();
        let stats = live.last_spatial_update_stats();
        assert_eq!(stats.full_rebuilds, 0);
        assert_eq!(stats.leaves_upserted, 1);
        assert_eq!(live.hit_test(Vec2::new(5.0, 0.0)).slots().len(), 1);
    }

    #[test]
    fn structural_removal_retires_spatial_leaf_locally() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(0.5));
        let second = definition.add(GeometryRef::circle(0.5));
        definition.object_mut(second).unwrap().transform.translation = Vec2::new(10.0, 0.0);
        let mut live = SlottedSceneInstance::new(CompiledScene::compile(&definition).unwrap());
        assert_eq!(live.spatial_index().len(), 2);
        live.apply_patch(&ScenePatch::RemoveObject(first)).unwrap();
        let stats = live.last_spatial_update_stats();
        assert_eq!(stats.full_rebuilds, 0);
        assert_eq!(stats.leaves_removed, 1);
        assert_eq!(live.spatial_index().len(), 1);
        assert!(live.hit_test(Vec2::ZERO).slots().is_empty());
    }
}
