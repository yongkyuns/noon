use noon_compile::{CompilePatchError, CompiledScene};
use noon_core::{
    GeometryRef, MutationTransaction, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch,
};
use noon_runtime::{ExecutionTransactionError, SlottedSceneInstance};

const REPLACEMENTS: u32 = 1_000;
const REPLACEMENT_ID_BASE: u64 = 1_000_000;

#[test]
fn execution_slot_capacity_plateaus_across_one_thousand_replacements() {
    let mut definition = SceneDefinition::new();
    let initial = definition.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&definition).expect("scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);

    let mut current_object = initial;
    let mut current_slot = live
        .slot_for_object(current_object)
        .expect("initial object must have a durable execution slot");
    let stable_slot_index = current_slot.slot();
    let baseline_capacity = live.slot_table().slot_capacity();
    assert_eq!(baseline_capacity, 1);

    for generation in 1..=REPLACEMENTS {
        let stale_slot = current_slot;
        live.apply_patch(&ScenePatch::RemoveObject(current_object))
            .expect("bounded replacement removal must succeed");
        assert_eq!(live.slot_for_object(current_object), None);
        assert_eq!(live.slot_table().object_for_slot(stale_slot), None);

        let next_object = ObjectId::new(REPLACEMENT_ID_BASE + u64::from(generation));
        live.apply_patch(&ScenePatch::CreateObject(ObjectDefinition::new(
            next_object,
            GeometryRef::circle(1.0),
        )))
        .expect("bounded replacement creation must succeed");

        let next_slot = live
            .slot_for_object(next_object)
            .expect("replacement must receive a durable execution slot");
        assert_eq!(next_slot.slot(), stable_slot_index);
        assert_eq!(next_slot.generation(), generation);
        assert_eq!(live.slot_table().object_for_slot(stale_slot), None);
        assert_eq!(
            live.slot_table().object_for_slot(next_slot),
            Some(next_object)
        );
        assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
        assert_eq!(live.slot_table().len(), 1);
        assert_eq!(live.slot_table().last_mutation_stats().slots_written, 1);
        assert_eq!(live.slot_table().last_mutation_stats().slots_reused, 1);

        current_object = next_object;
        current_slot = next_slot;
    }

    assert_eq!(current_slot.generation(), REPLACEMENTS);
    assert_eq!(live.live_object_count(), 1);
    assert_eq!(live.slot_table().len(), 1);
    assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
}

#[test]
fn rejected_structural_transaction_does_not_consume_slot_generation() {
    let mut definition = SceneDefinition::new();
    let current_object = definition.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&definition).expect("scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);

    let current_slot = live
        .slot_for_object(current_object)
        .expect("initial object must have a durable execution slot");
    let duplicate = ObjectId::new(REPLACEMENT_ID_BASE);
    let invalid = MutationTransaction::from_mutations([
        ScenePatch::RemoveObject(current_object),
        ScenePatch::CreateObject(ObjectDefinition::new(duplicate, GeometryRef::circle(0.5))),
        ScenePatch::CreateObject(ObjectDefinition::new(
            duplicate,
            GeometryRef::rectangle(2.0, 1.0),
        )),
    ]);

    assert_eq!(
        live.apply_transaction(&invalid),
        Err(ExecutionTransactionError::Compile(
            CompilePatchError::DuplicateObject(duplicate)
        ))
    );
    assert_eq!(live.slot_for_object(current_object), Some(current_slot));
    assert_eq!(live.slot_for_object(duplicate), None);
    assert_eq!(
        live.slot_table().object_for_slot(current_slot),
        Some(current_object)
    );
    assert_eq!(live.slot_table().len(), 1);
    assert_eq!(live.slot_table().slot_capacity(), 1);

    let replacement = ObjectId::new(REPLACEMENT_ID_BASE + 1);
    let valid = MutationTransaction::from_mutations([
        ScenePatch::RemoveObject(current_object),
        ScenePatch::CreateObject(ObjectDefinition::new(replacement, GeometryRef::circle(0.5))),
    ]);
    live.apply_transaction(&valid)
        .expect("valid structural replacement must commit");

    let replacement_slot = live
        .slot_for_object(replacement)
        .expect("replacement must receive the freed execution slot");
    assert_eq!(replacement_slot.slot(), current_slot.slot());
    assert_eq!(replacement_slot.generation(), current_slot.generation() + 1);
    assert_eq!(live.slot_table().object_for_slot(current_slot), None);
    assert_eq!(
        live.slot_table().object_for_slot(replacement_slot),
        Some(replacement)
    );
    assert_eq!(live.slot_table().slot_capacity(), 1);
}
