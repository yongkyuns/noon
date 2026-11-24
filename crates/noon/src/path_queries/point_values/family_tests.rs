use super::*;
use crate::Scene;

#[test]
fn recursive_xyz_queries_preserve_owned_roots_order_and_shared_aliases() {
    let mut scene = Scene::new();
    let first = SemanticVec3::new(0.125, 0.25, 0.123456789012345);
    let second = SemanticVec3::new(1.0, 2.0, 3.0);
    let root_point = SemanticVec3::new(4.0, 5.0, 6.25);
    let a = scene
        .path(
            VectorPath::from_manim_points_3d(vec![first]),
            Default::default(),
        )
        .unwrap();
    let b = scene
        .path(
            VectorPath::from_manim_points_3d(vec![second]),
            Default::default(),
        )
        .unwrap();
    let left = scene.family(&[(&a).into()]).unwrap();
    let right = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let root = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    root.set_points_3d(&[root_point]).unwrap();
    scene.add_many(&[(&root).into()]).unwrap();
    let expected = vec![root_point, first, first, second];
    assert_eq!(scene.all_points_3d((&root).into()).unwrap(), expected);
    assert_eq!(
        scene.all_points((&root).into()).unwrap(),
        expected
            .iter()
            .copied()
            .map(SemanticVec3::xy)
            .collect::<Vec<_>>()
    );
    let mut running = scene.into_execution().unwrap();
    assert_eq!(
        running.scene().all_points_3d((&root).into()).unwrap(),
        expected
    );
    let updated = SemanticVec3::new(7.0, 8.0, 9.125);
    running.scene_mut().set_points_3d(&a, &[updated]).unwrap();
    assert_eq!(
        running.scene().all_points_3d((&root).into()).unwrap(),
        [root_point, updated, updated, second]
    );
    assert_eq!(root.member_count().unwrap(), 2);
    assert_eq!(left.member_count().unwrap(), 1);
    assert_eq!(right.member_count().unwrap(), 2);
}

#[test]
fn recursive_xyz_queries_reject_foreign_and_stale_publications() {
    let mut scene = Scene::new();
    let root = scene.family(&[]).unwrap();
    scene.add_many(&[(&root).into()]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let foreign = Scene::new().family(&[]).unwrap();
    assert!(matches!(
        scene.all_points_3d((&foreign).into()),
        Err(AuthoringError::ForeignStore)
    ));
    root.set_draw_enabled(false).unwrap();
    assert!(matches!(
        scene.all_points_3d((&root).into()),
        Err(AuthoringError::ExecutionPublication(
            crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
        ))
    ));
}
