use noon_compile::CompiledScene;
use noon_core::{GeometryRef, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch};
use noon_runtime::SlottedSceneInstance;

const TEMPORARY_OBJECTS: usize = 4_096;
const SURVIVORS: usize = 8;
const REPLACEMENTS: usize = 1_000;
const REPLACEMENT_ID_BASE: u64 = 20_000_000;

#[test]
fn large_temporary_scene_releases_live_slots_to_small_working_set() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(TEMPORARY_OBJECTS);
    for _ in 0..TEMPORARY_OBJECTS {
        objects.push(definition.add(GeometryRef::circle(1.0)));
    }

    let compiled = CompiledScene::compile(&definition).expect("temporary scene must compile");
    let mut live = SlottedSceneInstance::new(compiled);
    let survivor_handles = objects[..SURVIVORS]
        .iter()
        .copied()
        .map(|object| {
            (
                object,
                live.slot_for_object(object)
                    .expect("every initial object must own an execution slot"),
            )
        })
        .collect::<Vec<_>>();
    let retired_handle = live
        .slot_for_object(objects[SURVIVORS])
        .expect("retired object must initially own an execution slot");
    let plateau_capacity = live.slot_table().slot_capacity();

    for object in objects.iter().copied().skip(SURVIVORS) {
        live.apply_patch(&ScenePatch::RemoveObject(object))
            .expect("temporary object removal must succeed");
    }

    assert_eq!(live.live_object_count(), SURVIVORS);
    assert_eq!(live.slot_table().len(), SURVIVORS);
    assert_eq!(live.slot_table().slot_capacity(), plateau_capacity);
    assert_eq!(live.slot_table().object_for_slot(retired_handle), None);
    for (object, slot) in &survivor_handles {
        assert_eq!(live.slot_for_object(*object), Some(*slot));
        assert_eq!(live.slot_table().object_for_slot(*slot), Some(*object));
    }

    for iteration in 0..REPLACEMENTS {
        let object = ObjectId::new(REPLACEMENT_ID_BASE + iteration as u64);
        live.apply_patch(&ScenePatch::CreateObject(ObjectDefinition::new(
            object,
            GeometryRef::rectangle(1.0, 1.0),
        )))
        .expect("post-release object creation must succeed");
        assert_eq!(live.slot_table().slot_capacity(), plateau_capacity);
        assert_eq!(live.slot_table().len(), SURVIVORS + 1);
        assert_eq!(live.live_object_count(), SURVIVORS + 1);
        assert_eq!(live.slot_table().last_mutation_stats().slots_written, 1);
        assert_eq!(live.slot_table().last_mutation_stats().slots_reused, 1);

        live.apply_patch(&ScenePatch::RemoveObject(object))
            .expect("post-release object removal must succeed");
        assert_eq!(live.slot_table().slot_capacity(), plateau_capacity);
        assert_eq!(live.slot_table().len(), SURVIVORS);
        assert_eq!(live.live_object_count(), SURVIVORS);
    }

    for (object, slot) in survivor_handles {
        assert_eq!(live.slot_for_object(object), Some(slot));
        assert_eq!(live.slot_table().object_for_slot(slot), Some(object));
    }
}
