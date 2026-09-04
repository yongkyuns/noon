use std::collections::HashMap;

use noon_core::ObjectId;

use super::{ExecutionSlotError, ExecutionSlotMutationStats, ExecutionSlotTable};

/// Apply one net execution-membership batch atomically to the durable slot table.
///
/// Exits are committed before entries so newly visible objects may reuse freshly
/// retired tombstones. The whole batch is validated first using only stable object
/// identity/generation lookups, so no full-table clone or rollback pass is needed.
pub fn apply_execution_slot_membership_changes(
    slots: &mut ExecutionSlotTable,
    exited: &[ObjectId],
    entered: &[ObjectId],
) -> Result<ExecutionSlotMutationStats, ExecutionSlotError> {
    preflight_membership_changes(slots, exited, entered)?;

    let mut stats = ExecutionSlotMutationStats::default();
    for object in exited {
        slots
            .remove_object(*object)
            .expect("membership batch was fully preflighted");
        stats.slots_written += 1;
    }
    for object in entered {
        let capacity_before = slots.slot_capacity();
        let slot = slots
            .insert_object(*object)
            .expect("membership batch was fully preflighted");
        stats.slots_written += 1;
        stats.slots_reused += usize::from((slot.slot() as usize) < capacity_before);
    }
    Ok(stats)
}

fn preflight_membership_changes(
    slots: &ExecutionSlotTable,
    exited: &[ObjectId],
    entered: &[ObjectId],
) -> Result<(), ExecutionSlotError> {
    let mut live_overrides = HashMap::<ObjectId, bool>::new();

    for object in exited {
        let live = live_overrides
            .get(object)
            .copied()
            .unwrap_or_else(|| slots.slot_for_object(*object).is_some());
        if !live {
            return Err(ExecutionSlotError::UnknownObject(*object));
        }

        let slot = slots
            .slot_for_object(*object)
            .expect("exit is live in the base table before any committed batch mutation");
        if slot.generation() == u32::MAX {
            return Err(ExecutionSlotError::GenerationExhausted(slot));
        }
        live_overrides.insert(*object, false);
    }

    for object in entered {
        let live = live_overrides
            .get(object)
            .copied()
            .unwrap_or_else(|| slots.slot_for_object(*object).is_some());
        if live {
            return Err(ExecutionSlotError::DuplicateObject(*object));
        }
        live_overrides.insert(*object, true);
    }

    Ok(())
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

        let stats =
            apply_execution_slot_membership_changes(&mut slots, &[first], &[replacement]).unwrap();
        let replacement_slot = slots.slot_for_object(replacement).unwrap();

        assert_eq!(stats.slots_written, 2);
        assert_eq!(stats.slots_reused, 1);
        assert_eq!(replacement_slot.slot(), first_slot.slot());
        assert_eq!(replacement_slot.generation(), first_slot.generation() + 1);
        assert_eq!(slots.slot_for_object(second), Some(second_slot));
        assert_eq!(slots.object_for_slot(first_slot), None);
        assert_eq!(slots.object_for_slot(replacement_slot), Some(replacement));
    }

    #[test]
    fn failed_membership_preflight_leaves_slot_table_unchanged() {
        let mut slots = ExecutionSlotTable::new();
        let existing = ObjectId::new(10);
        let duplicate = ObjectId::new(20);
        let existing_slot = slots.insert_object(existing).unwrap();
        let capacity = slots.slot_capacity();

        assert_eq!(
            apply_execution_slot_membership_changes(
                &mut slots,
                &[existing],
                &[duplicate, duplicate],
            ),
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

        let stats = apply_execution_slot_membership_changes(&mut slots, &[], &[]).unwrap();

        assert_eq!(stats, ExecutionSlotMutationStats::default());
        assert_eq!(slots.slot_for_object(object), Some(slot));
    }
}
