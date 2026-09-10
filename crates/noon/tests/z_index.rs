use noon::{ExecutionSession, LayoutAnchor, Scene};
use noon_compile::semantic_execution_object_id;
use noon_core::SemanticMutationTransaction;

fn order(session: &ExecutionSession) -> Vec<noon_core::ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect()
}

#[test]
fn authored_live_and_family_reorders_match_a_stable_reference() {
    let mut scene = Scene::new();
    let objects = (0..24)
        .map(|i| {
            let object = scene.square(1.).unwrap();
            object.set_z_index((i % 5) as f64 * 0.5).unwrap();
            object
        })
        .collect::<Vec<_>>();
    let members = objects.iter().map(Into::into).collect::<Vec<_>>();
    let family = scene.family(&members).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let rows = session.frame().objects.clone();
    let mut family_order = (0..objects.len()).collect::<Vec<_>>();
    for step in 0..72 {
        let changed = (step * 7) % objects.len();
        if step % 3 == 0 {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.reorder_member(family.node_id(), objects[changed].node_id(), None);
            scene.live(&mut session).apply(transaction).unwrap();
            family_order.retain(|&index| index != changed);
            family_order.push(changed);
        } else {
            scene
                .live(&mut session)
                .set_z_index(
                    &LayoutAnchor::from(&objects[changed]),
                    ((step % 7) as f64 - 3.) * 0.25,
                    true,
                )
                .unwrap();
        }
        let mut expected = family_order.clone();
        expected.sort_by(|&a, &b| {
            objects[a]
                .z_index()
                .unwrap()
                .partial_cmp(&objects[b].z_index().unwrap())
                .unwrap()
        });
        let expected = expected
            .iter()
            .map(|&i| semantic_execution_object_id(objects[i].node_id()))
            .collect::<Vec<_>>();
        assert_eq!(order(&session), expected, "step {step}");
        assert_eq!(session.frame().objects, rows);
        assert_eq!(session.last_patch_stats().full_seeks, 0);
        assert_eq!(session.last_patch_stats().full_group_rebuilds, 0);
    }
}

#[test]
fn family_priority_and_copies_preserve_roots_aliases_and_atomicity() {
    let mut scene = Scene::new();
    let a = scene.square(1.).unwrap();
    let b = scene.circle(1.).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    family.set_z_index(2.5, true).unwrap();
    family.set_z_index(-1.25, false).unwrap();
    assert_eq!(family.z_index().unwrap(), -1.25);
    assert_eq!(nested.z_index().unwrap(), 2.5);
    assert_eq!(a.z_index().unwrap(), 2.5);
    let copy = family.copy_family().unwrap();
    assert_eq!(copy.root().z_index().unwrap(), -1.25);
    assert_eq!(copy.family(&nested).unwrap().z_index().unwrap(), 2.5);
    assert_eq!(copy.mobject(&a).unwrap().z_index().unwrap(), 2.5);
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = order(&session);
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .set_z_index(&(&family).into(), f64::NAN, true)
        .is_err());
    assert_eq!(order(&session), before);
    assert_eq!(scene.revision(), revision);
    session.take_frame_changes();
    scene
        .live(&mut session)
        .set_z_index(&(&family).into(), 9., false)
        .unwrap();
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(a.z_index().unwrap(), 2.5);
    let foreign = Scene::new().square(1.).unwrap();
    assert!(scene
        .live(&mut session)
        .set_z_index(&(&foreign).into(), 5., true)
        .is_err());
}

#[test]
fn local_priority_changes_dirty_only_the_crossed_order_range() {
    let mut scene = Scene::new();
    let a = scene.square(1.).unwrap();
    let b = scene.square(1.).unwrap();
    let c = scene.square(1.).unwrap();
    scene
        .add_many(&[(&a).into(), (&b).into(), (&c).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    scene
        .live(&mut session)
        .set_z_index(&(&b).into(), 0.5, true)
        .unwrap();
    let changes = session.take_frame_changes();
    assert_eq!(changes.painter_order_range(), Some(1..3));
    assert!(changes.object_indices().is_empty());
    assert!(!changes.is_structural());
    assert_eq!(session.painter_order(), &[0, 2, 1]);
    scene
        .live(&mut session)
        .set_z_index(&(&b).into(), 1.5, true)
        .unwrap();
    assert!(session.take_frame_changes().is_empty());
    scene
        .live(&mut session)
        .set_z_index(&(&b).into(), 0., true)
        .unwrap();
    assert_eq!(session.painter_order(), &[0, 1, 2]);
}

#[test]
fn paired_priority_example_uses_the_retained_execution_session() {
    assert_eq!(
        noon::example_scenes::z_index::session()
            .unwrap()
            .frame()
            .objects
            .len(),
        3
    );
}

#[test]
fn later_admission_and_readmission_apply_current_priority_without_replacing_slots() {
    let mut scene = Scene::new();
    let a = scene.square(1.).unwrap();
    let b = scene.square(1.).unwrap();
    let c = scene.square(1.).unwrap();
    a.set_z_index(1.).unwrap();
    c.set_z_index(-2.).unwrap();
    scene.add_many(&[(&a).into(), (&b).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene.live(&mut session).add(&c).unwrap();
    assert_eq!(
        order(&session),
        [c.node_id(), b.node_id(), a.node_id()].map(semantic_execution_object_id)
    );
    let rows = session
        .frame()
        .objects
        .iter()
        .map(|row| row.id)
        .collect::<Vec<_>>();
    scene.live(&mut session).remove(&c).unwrap();
    scene
        .live(&mut session)
        .set_z_index(&(&c).into(), 3., true)
        .unwrap();
    scene.live(&mut session).add(&c).unwrap();
    assert_eq!(
        order(&session),
        [b.node_id(), a.node_id(), c.node_id()].map(semantic_execution_object_id)
    );
    assert_eq!(
        session
            .frame()
            .objects
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        rows
    );
}
