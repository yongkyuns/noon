use noon_compile::CompiledScene;
use noon_core::{GeometryRef, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch};
use noon_runtime::SlottedSceneInstance;

const WORKING_SET: usize = 128;
const REPLACEMENTS: usize = 1_000;
const REPLACEMENT_ID_BASE: u64 = 10_000_000;

#[test]
fn bounded_execution_working_set_reuses_slots_without_capacity_growth() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(WORKING_SET);
    for _ in 0..WORKING_SET {
        objects.push(definition.add(GeometryRef::circle(1.0)));
    }

    let compiled = CompiledScene::compile(&definition).expect("working-set scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);
    let mut slots = objects
        .iter()
        .copied()
        .map(|object| {
            live.slot_for_object(object)
                .expect("every initial object must own an execution slot")
        })
        .collect::<Vec<_>>();

    let baseline_capacity = live.slot_table().slot_capacity();
    assert_eq!(baseline_capacity, WORKING_SET);
    assert_eq!(live.slot_table().len(), WORKING_SET);

    for iteration in 0..REPLACEMENTS {
        let index = iteration % WORKING_SET;
        let untouched_index = (index + 1) % WORKING_SET;
        let removed_object = objects[index];
        let stale_slot = slots[index];
        let untouched_object = objects[untouched_index];
        let untouched_slot = slots[untouched_index];

        live.apply_patch(&ScenePatch::RemoveObject(removed_object))
            .expect("working-set removal must succeed");
        assert_eq!(live.slot_for_object(removed_object), None);
        assert_eq!(live.slot_table().object_for_slot(stale_slot), None);
        assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
        assert_eq!(live.slot_table().len(), WORKING_SET - 1);
        assert_eq!(live.slot_for_object(untouched_object), Some(untouched_slot));

        let replacement = ObjectId::new(REPLACEMENT_ID_BASE + iteration as u64);
        live.apply_patch(&ScenePatch::CreateObject(ObjectDefinition::new(
            replacement,
            GeometryRef::rectangle(1.0, 1.0),
        )))
        .expect("working-set replacement must succeed");

        let replacement_slot = live
            .slot_for_object(replacement)
            .expect("replacement must receive a reused execution slot");
        assert_eq!(replacement_slot.slot(), stale_slot.slot());
        assert_eq!(replacement_slot.generation(), stale_slot.generation() + 1);
        assert_eq!(live.slot_table().object_for_slot(stale_slot), None);
        assert_eq!(
            live.slot_table().object_for_slot(replacement_slot),
            Some(replacement)
        );
        assert_eq!(live.slot_for_object(untouched_object), Some(untouched_slot));
        assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
        assert_eq!(live.slot_table().len(), WORKING_SET);
        assert_eq!(live.live_object_count(), WORKING_SET);
        assert_eq!(live.slot_table().last_mutation_stats().slots_written, 1);
        assert_eq!(live.slot_table().last_mutation_stats().slots_reused, 1);

        objects[index] = replacement;
        slots[index] = replacement_slot;
    }

    assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
    assert_eq!(live.slot_table().len(), WORKING_SET);
    assert_eq!(live.live_object_count(), WORKING_SET);
    for (object, slot) in objects.into_iter().zip(slots) {
        assert_eq!(live.slot_for_object(object), Some(slot));
        assert_eq!(live.slot_table().object_for_slot(slot), Some(object));
    }
}
