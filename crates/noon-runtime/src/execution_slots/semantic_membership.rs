use noon_core::ObjectId;

use super::{ExecutionSlotError, ExecutionSlotMutationStats, ExecutionSlotPreflight, ExecutionSlotTable};

impl ExecutionSlotTable {
    /// Apply one net execution-membership batch atomically to the durable slot table.
    ///
    /// Exits are applied before entries so newly visible objects may reuse freshly
    /// retired tombstones. The existing sparse preflight shadow validates the entire
    /// batch first; after preflight succeeds, every primitive slot mutation is
    /// guaranteed to succeed and no rollback clone of the full table is required.
    pub fn apply_object_membership_changes(
        &mut self,
        exited: &[ObjectId],
        entered: &[ObjectId],
    ) -> Result<ExecutionSlotMutationStats, ExecutionSlotError> {
        let mut shadow = ExecutionSlotPreflight::new(self);
        for object in exited {
            shadow.remove_object(*object)?;
        }
        for object in entered {
            shadow.insert_object(*object)?;
        }
        drop(shadow);

        let mut stats = ExecutionSlotMutationStats::default();
        for object in exited {
            self.remove_object(*object)
                .expect("membership batch was fully preflighted");
            stats.slots_written += 1;
        }
        for object in entered {
            let capacity_before = self.slot_capacity();
            let slot = self
                .insert_object(*object)
                .expect("membership batch was fully preflighted");
            stats.slots_written += 1;
            stats.slots_reused += usize::from(slot.slot() as usize < capacity_before);
        }
        self.last_mutation = stats;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::ObjectId;

    use super::*;

    #[test]
    fn membership_batch_reuses_retired_slot_without_churning_other_ids() {
        let mut slots = ExecutionSlotTable::new();
        let first = ObjectId::new(10);
        let second = ObjectId::new(20);
        let replacement = ObjectId::new(30);
        let first_slot = slots.insert_object(first).unwrap();
        let second_slot = slots.insert_object(second).unwrap();

        let stats = slots
            .apply_object_membership_changes(&[first], &[replacement])
            .unwrap();
        let replacement_slot = slots.slot_for_object(replacement).unwrap();

        assert_eq!(stats.slots_written, 2);
        assert_eq!(stats.slots_reused, 1);
        assert_eq!(replacement_slot.slot(), first_slot.slot());
        assert_eq!(replacement_slot.generation(), first_slot.generation() + 1);
        assert_eq!(slots.slot_for_object(second), Some(second_slot));
        assert_eq!(slots.object_for_slot(first_slot), None);
        assert_eq!(slots.object_for_slot(replacement_slot), Some(replacement));
        assert_eq!(slots.last_mutation_stats(), stats);
    }

    #[test]
    fn failed_membership_preflight_leaves_slot_table_unchanged() {
        let mut slots = ExecutionSlotTable::new();
        let existing = ObjectId::new(10);
        let duplicate = ObjectId::new(20);
        let existing_slot = slots.insert_object(existing).unwrap();
        let capacity = slots.slot_capacity();

        assert_eq!(
            slots.apply_object_membership_changes(&[existing], &[duplicate, duplicate]),
            Err(ExecutionSlotError::DuplicateObject(duplicate))
        );
        assert_eq!(slots.slot_for_object(existing), Some(existing_slot));
        assert_eq!(slots.object_for_slot(existing_slot), Some(existing));
        assert_eq!(slots.slot_for_object(duplicate), None);
        assert_eq!(slots.slot_capacity(), capacity);
        assert_eq!(slots.len(), 1);
    }

    #[test]
    fn empty_membership_batch_is_a_noop() {
        let mut slots = ExecutionSlotTable::new();
        let object = ObjectId::new(5);
        let slot = slots.insert_object(object).unwrap();

        let stats = slots.apply_object_membership_changes(&[], &[]).unwrap();

        assert_eq!(stats, ExecutionSlotMutationStats::default());
        assert_eq!(slots.slot_for_object(object), Some(slot));
        assert_eq!(slots.last_mutation_stats(), stats);
    }
}
