use noon::{MobjectFamily, Scene, SemanticNodeId};

fn members(family: &MobjectFamily) -> Vec<SemanticNodeId> {
    family
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap()
}

#[test]
fn incoming_duplicates_keep_the_last_occurrence_and_readds_move_to_the_tail() {
    let scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let c = scene.square(1.0).unwrap();
    let family = scene
        .family(&[(&a).into(), (&b).into(), (&a).into()])
        .unwrap();
    assert_eq!(members(&family), [b.node_id(), a.node_id()]);
    let revision = scene.revision();
    assert_eq!(
        family
            .add_many(&[(&c).into(), (&b).into(), (&c).into()])
            .unwrap(),
        [false, false, true]
    );
    assert_eq!(members(&family), [a.node_id(), b.node_id(), c.node_id()]);
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert!(!family.add((&a).into()).unwrap());
    assert_eq!(members(&family), [b.node_id(), c.node_id(), a.node_id()]);
    let revision = scene.revision();
    family.add((&a).into()).unwrap();
    assert_eq!(scene.revision(), revision);
    family.remove((&a).into()).unwrap();
    family.add((&a).into()).unwrap();
    assert_eq!(members(&family), [b.node_id(), c.node_id(), a.node_id()]);
}

#[test]
fn invalid_late_member_does_not_publish_an_earlier_reorder() {
    let scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let parent = scene.family(&[(&family).into()]).unwrap();
    let foreign = Scene::new().square(1.0).unwrap();
    let revision = scene.revision();
    assert!(family.add_many(&[(&a).into(), (&parent).into()]).is_err());
    assert!(family.add_many(&[(&a).into(), (&foreign).into()]).is_err());
    assert_eq!(members(&family), [a.node_id(), b.node_id()]);
    assert_eq!(scene.revision(), revision);
}

#[test]
fn live_reorder_keeps_alias_identity_and_updates_painter_order() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let c = scene.square(1.0).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene
        .family(&[(&a).into(), (&nested).into(), (&c).into()])
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = a.state().unwrap();
    {
        let mut live = scene.live(&mut session);
        live.add_family_members(&family, &[(&a).into(), (&nested).into()])
            .unwrap();
    }
    assert_eq!(
        members(&family),
        [c.node_id(), a.node_id(), nested.node_id()]
    );
    let ids: Vec<_> = session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect();
    assert_eq!(
        ids,
        [c.node_id(), a.node_id(), b.node_id()].map(noon_compile::semantic_execution_object_id)
    );
    assert_eq!(a.state().unwrap(), before);
}

#[test]
fn one_readd_stages_one_local_edit_even_in_a_large_family() {
    let scene = Scene::new();
    let objects: Vec<_> = (0..1000).map(|_| scene.square(1.0).unwrap()).collect();
    let inputs: Vec<_> = objects.iter().map(Into::into).collect();
    let family = scene.family(&inputs).unwrap();
    family.add((&objects[0]).into()).unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .last_mutation_stats()
            .slots_written,
        1
    );
    assert_eq!(members(&family).last(), Some(&objects[0].node_id()));
}

#[test]
fn paired_example_uses_the_normal_execution_session() {
    noon::example_scenes::family_membership_order::session().unwrap();
}
