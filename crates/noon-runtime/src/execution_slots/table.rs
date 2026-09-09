use std::collections::HashMap;

use noon_compile::CompiledScene;
use noon_core::ObjectId;

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
/// Execution rows are derived indices; durable slots preserve identity across
/// local membership changes without renumbering unrelated objects.
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
}

#[cfg(test)]
mod tests {
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
}
