use noon_compile::CompiledScene;
use noon_core::{GeometryRef, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch};

#[test]
fn removal_tombstones_slot_without_renumbering_unrelated_objects() {
    let mut scene = SceneDefinition::new();
    let ids = (0..100_000)
        .map(|_| scene.add(GeometryRef::circle(1.0)))
        .collect::<Vec<_>>();
    let mut compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let before_11 = compiled.object_index(ids[11]).expect("object 11");
    let before_last = compiled
        .object_index(*ids.last().unwrap())
        .expect("last object");
    let removed_slot = compiled.object_index(ids[10]).expect("object 10");

    compiled
        .apply_patch(&ScenePatch::RemoveObject(ids[10]))
        .expect("remove");
    assert_eq!(compiled.object_index(ids[11]), Some(before_11));
    assert_eq!(
        compiled.object_index(*ids.last().unwrap()),
        Some(before_last)
    );
    assert!(!compiled.objects()[removed_slot as usize].live);

    let replacement =
        ObjectDefinition::new(ObjectId::new(200_000), GeometryRef::rectangle(2.0, 1.0));
    compiled
        .apply_patch(&ScenePatch::CreateObject(replacement))
        .expect("create");
    assert_eq!(
        compiled.object_index(ObjectId::new(200_000)),
        Some(removed_slot)
    );
    assert!(compiled.objects()[removed_slot as usize].live);
}
