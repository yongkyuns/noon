use noon::{semantic_family_leaf_ids, FamilyTranslation, Mobject, MobjectFamily, Scene};

fn nested_family() -> (Scene, MobjectFamily, [Mobject; 3]) {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.square(1.0).unwrap();
    let third = scene.square(1.0).unwrap();
    let nested = scene.family(&[&second, &third]).unwrap();
    let root = scene.family(&[&first]).unwrap();
    scene
        .store()
        .borrow_mut()
        .add_member(root.node_id(), nested.node_id())
        .unwrap();
    (scene, root, [first, second, third])
}

#[test]
fn nested_translation_commits_all_authoritative_leaves_once() {
    let (scene, root, members) = nested_family();
    let store = scene.store();
    assert_eq!(
        semantic_family_leaf_ids(&store.borrow(), root.node_id()).unwrap(),
        members.iter().map(Mobject::node_id).collect::<Vec<_>>()
    );
    let before = store.borrow().scene_revision();
    let translation =
        FamilyTranslation::begin(&store.borrow(), root.node_id(), 0.25, -0.5).unwrap();
    translation.apply(&mut store.borrow_mut()).unwrap();
    assert_eq!(
        store.borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    for member in members {
        assert_eq!(member.center().unwrap(), (0.25, -0.5));
    }
}

#[test]
fn stale_late_leaf_rejects_the_entire_translation() {
    let (scene, root, members) = nested_family();
    let store = scene.store();
    let translation = FamilyTranslation::begin(&store.borrow(), root.node_id(), 1.0, 0.0).unwrap();
    store
        .borrow_mut()
        .remove_node(members[2].node_id())
        .unwrap();
    let replacement = scene.circle(0.2).unwrap();
    let before = store.borrow().scene_revision();
    assert!(translation.apply(&mut store.borrow_mut()).is_err());
    assert_eq!(store.borrow().scene_revision(), before);
    assert_eq!(members[0].center().unwrap(), (0.0, 0.0));
    assert_eq!(members[1].center().unwrap(), (0.0, 0.0));
    assert_eq!(replacement.center().unwrap(), (0.0, 0.0));
}

#[test]
fn aliased_occurrences_accumulate_without_touching_other_objects() {
    let (scene, root, members) = nested_family();
    let store = scene.store();
    store
        .borrow_mut()
        .add_member(root.node_id(), members[2].node_id())
        .unwrap();
    let unrelated = scene.circle(0.2).unwrap();
    let before = store.borrow().scene_revision();
    assert!(FamilyTranslation::begin(&store.borrow(), root.node_id(), f64::NAN, 0.0).is_err());
    let translation =
        FamilyTranslation::begin(&store.borrow(), root.node_id(), 0.25, -0.5).unwrap();
    translation.apply(&mut store.borrow_mut()).unwrap();
    assert_eq!(
        store.borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    assert_eq!(members[0].center().unwrap(), (0.25, -0.5));
    assert_eq!(members[1].center().unwrap(), (0.25, -0.5));
    assert_eq!(members[2].center().unwrap(), (0.5, -1.0));
    assert_eq!(unrelated.center().unwrap(), (0.0, 0.0));
}
