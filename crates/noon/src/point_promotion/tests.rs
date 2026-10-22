use super::*;
use crate::{Color, ManimGeometryOptions, Scene, SemanticVec3, VectorPath};

fn points() -> [SemanticVec3; 4] {
    [
        SemanticVec3::new(-1., 0., 1.125),
        SemanticVec3::new(-0.5, 2., -3.5),
        SemanticVec3::new(0.5, -1., 4.75),
        SemanticVec3::new(1., 0., 2.25),
    ]
}

#[test]
fn promotion_preserves_xyz_draw_intent_members_and_metadata_aliases_in_one_revision() {
    for draw_enabled in [false, true] {
        let mut scene = Scene::new();
        let a = scene.circle(0.2).unwrap();
        let b = scene.square(0.3).unwrap();
        let source = scene
            .family_with_presentation(&[(&a).into(), (&b).into()], 3., Color::GREEN, draw_enabled)
            .unwrap();
        source.set_points_3d(&points()).unwrap();
        let old_members = source.member_view().unwrap();
        let root = source.node_id();
        let child_states = [a.state().unwrap(), b.state().unwrap()];
        let before_nodes = scene.integration_store().borrow().len();
        let before_revision = scene.revision();
        let copied = MobjectTarget::from(&source)
            .promote_own_points_with_references(&[(&a).into(), (&a).into()])
            .unwrap();
        let promoted = copied.family(&source).unwrap();
        assert_eq!(source.node_id(), root);
        assert_eq!(source.get_num_points().unwrap(), 0);
        assert_eq!(source.member_count().unwrap(), 3);
        assert_eq!(old_members.len().unwrap(), 2);
        assert_eq!(
            source.member_at(0).unwrap().unwrap().as_target().node_id(),
            a.node_id()
        );
        assert_eq!(
            source.member_at(1).unwrap().unwrap().as_target().node_id(),
            b.node_id()
        );
        assert_eq!(source.member_at(2).unwrap(), Some(copied.root().clone()));
        assert_eq!(
            promoted.own_path_query().unwrap().points_3d().unwrap(),
            points()
        );
        assert_eq!(promoted.member_count().unwrap(), 0);
        assert_eq!(promoted.draw_enabled().unwrap(), draw_enabled);
        assert_eq!(source.draw_enabled().unwrap(), draw_enabled);
        assert_eq!(promoted.own_color().unwrap(), source.own_color().unwrap());
        assert_eq!([a.state().unwrap(), b.state().unwrap()], child_states);
        assert_ne!(copied.mobject(&a).unwrap().node_id(), a.node_id());
        assert!(copied.mobject(&b).is_err());
        assert_eq!(scene.integration_store().borrow().len(), before_nodes + 2);
        assert_eq!(scene.revision(), before_revision.checked_next().unwrap());
    }
}

#[test]
fn repeated_empty_root_promotion_appends_once_and_preserves_captured_lists() {
    let mut scene = Scene::new();
    let child = scene.square(0.2).unwrap();
    let source = scene.family(&[(&child).into()]).unwrap();
    let before = scene.integration_store().borrow().len();
    for expected in [1, 2] {
        let old = source.member_view().unwrap();
        let copied = MobjectTarget::from(&source).promote_own_points().unwrap();
        assert_eq!(old.len().unwrap(), expected);
        assert_eq!(source.member_count().unwrap(), expected + 1);
        assert_eq!(copied.root().as_target().get_num_points().unwrap(), 0);
        assert_eq!(copied.root().member_count().unwrap(), 0);
        assert_eq!(scene.integration_store().borrow().len(), before + expected);
    }
}

#[test]
fn root_only_preparation_does_not_capture_existing_children_or_unrelated_objects() {
    let mut scene = Scene::new();
    let children = (0..50)
        .map(|_| scene.circle(0.1).unwrap())
        .collect::<Vec<_>>();
    let source = scene
        .family(&children.iter().map(Into::into).collect::<Vec<_>>())
        .unwrap();
    source.set_points_3d(&points()).unwrap();
    let _unrelated = scene.square(10.).unwrap();
    let mut captured = Vec::new();
    let mut effects = Vec::new();
    let before = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let _prepared = prepare_point_promotion::<AuthoringError>(
        (&source).into(),
        &[],
        |object, _| {
            captured.push(object.node_id());
            object.state()
        },
        |_, id, _, _| {
            effects.push(id);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(captured, [source.node_id()]);
    assert_eq!(effects, [source.node_id()]);
    assert_eq!(scene.revision(), before);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn ordinary_base_root_keeps_non_drawing_intent_and_rejects_foreign_metadata_atomically() {
    let mut scene = Scene::new();
    let mut options = ManimGeometryOptions::path(VectorPath::new()).unwrap();
    options.set_draw_enabled(false);
    let mut source = scene.geometry(options).unwrap();
    source.set_points_3d(&points()).unwrap();
    let id = source.node_id();
    let foreign = Scene::new().square(0.2).unwrap();
    let before = scene.revision();
    let count = scene.integration_store().borrow().len();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let state = source.state().unwrap();
    assert!(MobjectTarget::from(&source)
        .promote_own_points_with_references(&[(&foreign).into()])
        .is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    assert_eq!(source.state().unwrap(), state);
    let copied = MobjectTarget::from(&source).promote_own_points().unwrap();
    let promoted = copied.mobject(&source).unwrap();
    assert_eq!(source.node_id(), id);
    assert_eq!(source.get_num_points().unwrap(), 0);
    assert_eq!(
        promoted.path_query().unwrap().points_3d().unwrap(),
        points()
    );
    assert!(!promoted.draw_enabled().unwrap());
    assert_eq!(source.member_count().unwrap(), 1);
}

#[test]
fn opaque_content_rejects_without_source_loss_or_resource_and_node_admission() {
    let mut scene = Scene::new();
    let source = scene
        .image(crate::ImageMobjectOptions::rgba8(1, 1, vec![255, 0, 0, 255]).unwrap())
        .unwrap();
    let before = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let state = source.state().unwrap();
    assert!(MobjectTarget::from(&source).promote_own_points().is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    assert_eq!(source.state().unwrap(), state);
    assert_eq!(source.get_num_points().unwrap(), 4);
}

#[test]
fn structured_root_rejects_before_copy_capture_or_publication() {
    let mut scene = Scene::new();
    let graph = scene
        .graph([(0, (-1., 0.)), (1, (1., 0.))], [(0, 1)])
        .unwrap();
    let before = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let result = prepare_point_promotion::<AuthoringError>(
        graph.family().into(),
        &[],
        |_, _| panic!("structured root must reject before capture"),
        |_, _, _, _| panic!("structured root must reject before effect copy"),
    );
    assert!(matches!(
        result,
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::PointPromotionStructuredRoot
        ))
    ));
    assert_eq!(scene.revision(), before);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn live_promotion_keeps_effective_xyz_and_child_identity_in_one_publication() {
    let mut scene = Scene::new();
    let child = scene.square(0.2).unwrap();
    let source = scene.family(&[(&child).into()]).unwrap();
    source.set_points_3d(&points()).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .shift_3d(
            &source.own_object().unwrap().unwrap(),
            SemanticVec3::new(2., -1., 0.5),
        )
        .unwrap();
    let expected = source
        .effective_own_path_query(&session)
        .unwrap()
        .points_3d()
        .unwrap();
    let child_state = child.state().unwrap();
    let before = session.publication_context();
    let revision = scene.revision();
    let copied = scene
        .live(&mut session)
        .promote_own_points((&source).into(), &[])
        .unwrap();
    let promoted = copied.family(&source).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_ne!(session.publication_context(), before);
    assert_eq!(
        source
            .effective_own_path_query(&session)
            .unwrap()
            .point_count(),
        0
    );
    assert_eq!(
        promoted
            .effective_own_path_query(&session)
            .unwrap()
            .points_3d()
            .unwrap(),
        expected
    );
    assert_eq!(child.state().unwrap(), child_state);
    assert_eq!(
        source.member_at(0).unwrap().unwrap().as_target().node_id(),
        child.node_id()
    );
    assert_eq!(promoted.member_count().unwrap(), 0);
    let after = session.publication_context();
    let foreign = Scene::new().circle(0.1).unwrap();
    assert!(scene
        .live(&mut session)
        .promote_own_points((&source).into(), &[(&foreign).into()])
        .is_err());
    assert_eq!(session.publication_context(), after);
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
}
