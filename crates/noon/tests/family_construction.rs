use noon::{semantic_family_leaf_ids, MobjectFamilyMember, Scene};

#[test]
fn nested_creation_and_membership_edits_preserve_identity_order_and_atomicity() {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.circle(0.2).unwrap();
    let empty = scene.family(&[]).unwrap();
    assert_eq!(empty.layout_bounds().unwrap(), None);
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let before = scene.store().borrow().scene_revision();
    let root = scene
        .family(&[(&first).into(), (&nested).into(), (&first).into()])
        .unwrap();
    assert_eq!(
        scene.store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    assert_eq!(
        scene
            .store()
            .borrow()
            .semantic_family_members_checked(root.node_id())
            .unwrap(),
        vec![first.node_id(), nested.node_id()]
    );
    assert_eq!(
        semantic_family_leaf_ids(&scene.store().borrow(), root.node_id()).unwrap(),
        vec![first.node_id(), first.node_id(), second.node_id()]
    );
    let revision = scene.store().borrow().scene_revision();
    assert!(!root.add((&first).into()).unwrap());
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert!(nested.add((&root).into()).is_err()); // Cycle rejected before commit.
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert!(root.remove((&nested).into()).unwrap());
    assert!(!root.remove((&nested).into()).unwrap());
    assert!(root.add((&nested).into()).unwrap());
    assert_eq!(
        semantic_family_leaf_ids(&scene.store().borrow(), root.node_id()).unwrap(),
        vec![first.node_id(), first.node_id(), second.node_id()]
    );
    assert_eq!(second.center().unwrap(), (0.0, 0.0));
}

#[test]
fn foreign_and_stale_members_cannot_create_or_edit_a_family() {
    let scene = Scene::new();
    let local = scene.square(1.0).unwrap();
    let root = scene.family(&[(&local).into()]).unwrap();
    let other = Scene::new();
    let foreign = other.square(1.0).unwrap();
    let stale = scene.square(1.0).unwrap();
    scene
        .store()
        .borrow_mut()
        .remove_node(stale.node_id())
        .unwrap();
    let revision = scene.store().borrow().scene_revision();
    for member in [MobjectFamilyMember::from(&foreign), (&stale).into()] {
        assert!(scene.family(&[(&local).into(), member]).is_err());
        assert!(root.add(member).is_err());
        assert!(root.remove(member).is_err());
    }
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert_eq!(
        semantic_family_leaf_ids(&scene.store().borrow(), root.node_id()).unwrap(),
        vec![local.node_id()]
    );
}

#[test]
fn live_creation_publishes_empty_and_nested_families_through_the_same_transaction() {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.circle(0.2).unwrap();
    let mut execution = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut execution);
        let empty = live.family(&[]).unwrap();
        let nested = live.family(&[(&first).into(), (&second).into()]).unwrap();
        let root = live.family(&[(&empty).into(), (&nested).into()]).unwrap();
        live.add_many(&[(&root).into()]).unwrap();
    }
    assert_eq!(execution.frame().objects.len(), 2);
    assert!(execution
        .frame()
        .objects
        .iter()
        .all(|object| object.geometry().is_some()));
}

#[test]
fn live_member_batch_rejects_a_late_cycle_without_partial_publication() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.circle(0.2).unwrap();
    let nested = scene.family(&[(&first).into()]).unwrap();
    let root = scene.family(&[(&nested).into()]).unwrap();
    scene.add_many(&[(&root).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let store = std::rc::Rc::clone(scene.store());
    {
        let mut live = scene.live(&mut execution);
        let wait = live.wait_segment(0.5).unwrap();
        live.advance_segment_to(wait, 0.5).unwrap();
        live.complete_segment(wait).unwrap();
        let before = live.effective(&first).unwrap();
        let revision = store.borrow().scene_revision();
        assert!(live
            .add_family_members(&nested, &[(&second).into(), (&root).into()])
            .is_err());
        assert_eq!(store.borrow().scene_revision(), revision);
        assert_eq!(live.effective(&first).unwrap(), before);
        assert_eq!(
            live.add_family_members(&nested, &[(&second).into(), (&second).into()])
                .unwrap(),
            vec![true, false]
        );
        assert_eq!(
            store.borrow().scene_revision(),
            revision.checked_next().unwrap()
        );
        assert_eq!(
            live.remove_family_members(&nested, &[(&second).into(), (&second).into()])
                .unwrap(),
            vec![true, false]
        );
        assert_eq!(
            live.add_family_members(&nested, &[(&second).into()])
                .unwrap(),
            vec![true]
        );
    }
    assert_eq!(execution.frame().objects.len(), 2);
}
