//! Exercise provider-gated production code from an external test crate.
//! The library is compiled without cfg(test). No font or TeX process is needed.
#![cfg(any(feature = "native-text", feature = "latex"))]

use noon::{CompositeEntryHandle, MobjectTable, MobjectTarget, Scene, TableOptions};

#[test]
fn table_entries_preserve_family_roots_in_the_production_library() {
    let mut scene = Scene::new();
    let first = scene.circle(1.0).unwrap();
    let mut second = scene.circle(0.5).unwrap();
    second.shift(3.0, 0.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let separation = second.center().unwrap().0 - first.center().unwrap().0;

    let rows = vec![vec![MobjectTarget::from(&family)]];
    let table = MobjectTable::from_target_rows(&mut scene, rows).unwrap();
    assert_eq!(table.shape().unwrap(), (1, 1));
    assert_eq!(
        table.get_entry(0, 0).unwrap(),
        CompositeEntryHandle::Family(family)
    );
    assert_eq!(
        second.center().unwrap().0 - first.center().unwrap().0,
        separation
    );
}

#[test]
fn live_table_admission_keeps_one_session_and_rejects_duplicate_entries() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.circle(1.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    let authored = first.state().unwrap();
    let rejected = MobjectTable::from_rows_in_live_session(
        &mut scene.live(&mut session),
        vec![vec![first.clone(), first.clone()]],
        TableOptions::default(),
    );
    assert!(rejected.is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(first.state().unwrap(), authored);

    let table = MobjectTable::from_rows_in_live_session(
        &mut scene.live(&mut session),
        vec![vec![first.clone(), second.clone()]],
        TableOptions::default(),
    )
    .unwrap();
    assert_eq!(table.shape().unwrap(), (1, 2));
    assert_eq!(
        table.get_entries().unwrap(),
        vec![
            CompositeEntryHandle::Mobject(first),
            CompositeEntryHandle::Mobject(second),
        ]
    );
    assert_eq!(session.frame().time, 0.0);
}

#[cfg(feature = "latex")]
#[test]
fn matrix_queries_preserve_original_entries_without_compiling_tex() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.circle(1.0).unwrap();
    let left = scene.circle(0.25).unwrap();
    let right = scene.circle(0.25).unwrap();
    let row = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let entries = scene.family(&[(&row).into()]).unwrap();
    let root = scene
        .family(&[(&entries).into(), (&left).into(), (&right).into()])
        .unwrap();
    let matrix = noon::Matrix::from_family(root).unwrap();
    assert_eq!(matrix.shape().unwrap(), (1, 2));
    assert_eq!(
        matrix.entries().unwrap(),
        vec![
            CompositeEntryHandle::Mobject(first),
            CompositeEntryHandle::Mobject(second),
        ]
    );
    assert_eq!(matrix.left_bracket(), &left);
    assert_eq!(matrix.right_bracket(), &right);
}
