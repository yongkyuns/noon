use noon_compile::CompiledScene;
use noon_core::{
    GeometryRef, MutationTransaction, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch,
};
use noon_runtime::SlottedSceneInstance;

const WORKING_SET: usize = 32;
const SWAPS: usize = 1_000;
const OBJECT_ID_BASE: u64 = 20_000_000;

#[test]
fn atomic_working_set_swaps_reuse_execution_slots_without_capacity_growth() {
    let mut definition = SceneDefinition::new();
    let mut current_objects = Vec::with_capacity(WORKING_SET);
    for _ in 0..WORKING_SET {
        current_objects.push(definition.add(GeometryRef::circle(1.0)));
    }

    let compiled = CompiledScene::compile(&definition).expect("working-set scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);
    let baseline_capacity = live.slot_table().slot_capacity();

    assert_eq!(baseline_capacity, WORKING_SET);
    assert_eq!(live.live_object_count(), WORKING_SET);
    assert_eq!(live.slot_table().len(), WORKING_SET);

    for swap in 0..SWAPS {
        let stale_slots = current_objects
            .iter()
            .copied()
            .map(|object| {
                live.slot_for_object(object)
                    .expect("every current object must own an execution slot")
            })
            .collect::<Vec<_>>();

        let mut next_objects = Vec::with_capacity(WORKING_SET);
        let mut mutations = Vec::with_capacity(WORKING_SET * 2);
        mutations.extend(
            current_objects
                .iter()
                .copied()
                .map(ScenePatch::RemoveObject),
        );

        for index in 0..WORKING_SET {
            let object = ObjectId::new(OBJECT_ID_BASE + (swap * WORKING_SET + index) as u64);
            let geometry = if swap % 2 == 0 {
                GeometryRef::rectangle(1.0, 1.0)
            } else {
                GeometryRef::circle(1.0)
            };
            mutations.push(ScenePatch::CreateObject(ObjectDefinition::new(
                object, geometry,
            )));
            next_objects.push(object);
        }

        let transaction = MutationTransaction::from_mutations(mutations);
        let preflight = live
            .preflight_transaction(&transaction)
            .expect("bounded whole-set replacement must preflight");
        assert_eq!(preflight.slots_indexed, WORKING_SET);
        assert_eq!(preflight.staged_runtime_clones, 0);

        live.apply_transaction(&transaction)
            .expect("bounded whole-set replacement must commit atomically");

        assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
        assert_eq!(live.slot_table().len(), WORKING_SET);
        assert_eq!(live.live_object_count(), WORKING_SET);

        for stale_slot in stale_slots {
            assert_eq!(
                live.slot_table().object_for_slot(stale_slot),
                None,
                "pre-swap handles must remain stale after full-set replacement",
            );
        }
        for object in next_objects.iter().copied() {
            let slot = live
                .slot_for_object(object)
                .expect("replacement object must receive a reused execution slot");
            assert_eq!(slot.generation(), (swap + 1) as u32);
            assert_eq!(live.slot_table().object_for_slot(slot), Some(object));
        }

        current_objects = next_objects;
    }

    assert_eq!(live.slot_table().slot_capacity(), baseline_capacity);
    assert_eq!(live.slot_table().len(), WORKING_SET);
    assert_eq!(live.live_object_count(), WORKING_SET);
}
