use noon::{
    AnimationOptions, Mobject, MobjectFamily, MobjectTarget, RateFunction, Scene, Vec2, VectorPath,
};
use noon_core::{SemanticMutationTransaction, SemanticNodeId};

fn attach(scene: &Scene, parent: SemanticNodeId, children: &[SemanticNodeId]) {
    let mut transaction = SemanticMutationTransaction::new();
    for &child in children {
        transaction.add_member(parent, child);
    }
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
}

fn curve(scene: &mut Scene, x: f32) -> Mobject {
    scene
        .path(
            VectorPath::new().move_to(Vec2::new(x, 1.0)).cubic_to(
                Vec2::new(x + 9.0, 5.0),
                Vec2::new(x + 7.0, -4.0),
                Vec2::new(x + 1.0, 2.0),
            ),
            Default::default(),
        )
        .unwrap()
}

fn aliased_tree(scene: &mut Scene) -> (Mobject, Mobject, Mobject, MobjectFamily) {
    let root = curve(scene, -3.0);
    let child = curve(scene, 4.0);
    let grandchild = curve(scene, 12.0);
    let nested = scene
        .family(&[(&child).into(), (&grandchild).into()])
        .unwrap();
    attach(scene, child.node_id(), &[grandchild.node_id()]);
    attach(scene, root.node_id(), &[child.node_id(), nested.node_id()]);
    (root, child, grandchild, nested)
}

fn own_points(objects: &[&Mobject]) -> Vec<(f64, f64)> {
    objects
        .iter()
        .flat_map(|object| object.path_query().unwrap().points().unwrap())
        .collect()
}

#[test]
fn ordinary_root_all_points_keep_own_content_and_every_nested_alias_occurrence() {
    let mut scene = Scene::new();
    let (root, child, grandchild, _) = aliased_tree(&mut scene);
    let revision = scene.revision();
    // Manim get_merged_array is recursive pre-order: C and D repeat per edge.
    let expected = own_points(&[&root, &child, &grandchild, &child, &grandchild, &grandchild]);
    assert_eq!(MobjectTarget::from(&root).all_points().unwrap(), expected);
    let query = MobjectTarget::from(&root);
    assert_eq!(query.length_over_dim(0).unwrap(), 24.0);
    assert_eq!(query.center_of_mass().unwrap(), (266.0 / 24.0, 1.0));
    assert_eq!(query.boundary_point(1.0, 0.0).unwrap(), (21.0, 5.0));
    assert_eq!(query.path_boundary_point(1.0, 0.0).unwrap(), (13.0, 2.0));
    assert_eq!(scene.revision(), revision);
}

#[test]
fn ordinary_root_reduction_tree_keeps_own_payload_separate_from_child_arrays() {
    let mut scene = Scene::new();
    let (root, child, grandchild, _) = aliased_tree(&mut scene);
    let tree = MobjectTarget::from(&root).point_reduction_tree().unwrap();
    assert_eq!(tree.points, own_points(&[&root]));
    assert_eq!(tree.children.len(), 2);
    assert_eq!(tree.children[0].points, own_points(&[&child]));
    assert_eq!(
        tree.children[0].children[0].points,
        own_points(&[&grandchild])
    );
    assert!(tree.children[1].points.is_empty());
    assert_eq!(tree.children[1].children.len(), 2);
    assert_eq!(
        tree.flattened_points(),
        own_points(&[&root, &child, &grandchild, &child, &grandchild, &grandchild])
    );
}

#[test]
fn vector_boundary_uses_unique_last_family_order_and_base_family_queries_keep_controls() {
    let mut scene = Scene::new();
    let (root, child, grandchild, _) = aliased_tree(&mut scene);
    let family = scene.family(&[(&root).into()]).unwrap();
    let expected = [&root, &child, &grandchild]
        .iter()
        .flat_map(|object| object.path_query().unwrap().anchors().unwrap())
        .collect::<Vec<_>>();
    let object = MobjectTarget::from(&root);
    let group = MobjectTarget::from(&family);
    assert_eq!(object.vector_boundary_points().unwrap(), expected);
    assert_eq!(object.path_boundary_points().unwrap(), expected);
    assert_eq!(group.vector_boundary_points().unwrap(), expected);
    assert_eq!(
        group.path_boundary_points().unwrap(),
        group.all_points().unwrap()
    );
    assert_eq!(expected.len(), 6);
    assert_eq!(group.all_points().unwrap().len(), 24);
}

#[test]
fn empty_own_array_does_not_hide_children_or_add_an_origin_boundary_point() {
    let mut scene = Scene::new();
    let root = scene.path(VectorPath::new(), Default::default()).unwrap();
    let child = curve(&mut scene, 4.0);
    attach(&scene, root.node_id(), &[child.node_id()]);
    let target = MobjectTarget::from(&root);
    assert_eq!(
        target.all_points().unwrap(),
        child.path_query().unwrap().points().unwrap()
    );
    assert_eq!(
        target.path_boundary_points().unwrap(),
        child.path_query().unwrap().anchors().unwrap()
    );
    let tree = target.point_reduction_tree().unwrap();
    assert!(tree.points.is_empty());
    assert_eq!(tree.children.len(), 1);
}

#[test]
fn effective_root_queries_preserve_aliases_and_replay_at_each_coherent_child_pose() {
    let mut scene = Scene::new();
    let (root, child, grandchild, _) = aliased_tree(&mut scene);
    let mut target = grandchild.target_editor().unwrap();
    target.shift(0.0, 2.0).unwrap();
    scene.add(&root).unwrap();
    let animation = scene
        .declare_transform_to(
            &grandchild,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut execution)
        .play_animation(&animation)
        .unwrap();
    scene
        .live(&mut execution)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    for time in [1.0, 0.25, 1.5, 0.0, 1.0] {
        execution.seek(time).unwrap();
        let before = execution.publication_context();
        let expected = [&root, &child, &grandchild, &child, &grandchild, &grandchild]
            .iter()
            .flat_map(|object| {
                noon::integration::effective_path_query(
                    scene.integration_store(),
                    &execution,
                    object,
                )
                .unwrap()
                .points()
                .unwrap()
            })
            .collect::<Vec<_>>();
        let boundary = [&root, &child, &grandchild]
            .iter()
            .flat_map(|object| {
                noon::integration::effective_path_query(
                    scene.integration_store(),
                    &execution,
                    object,
                )
                .unwrap()
                .anchors()
                .unwrap()
            })
            .collect::<Vec<_>>();
        let points = noon::integration::effective_all_points(
            scene.integration_store(),
            &execution,
            (&root).into(),
        )
        .unwrap();
        assert_eq!(points, expected);
        assert_eq!(
            noon::integration::effective_point_reduction_tree(
                scene.integration_store(),
                &execution,
                (&root).into()
            )
            .unwrap()
            .flattened_points(),
            expected
        );
        assert_eq!(
            noon::integration::effective_vector_boundary_points(
                scene.integration_store(),
                &execution,
                (&root).into()
            )
            .unwrap(),
            boundary
        );
        assert_eq!(
            execution.publication_context(),
            before,
            "queries must not publish or change the coherent clock"
        );
    }
}

#[test]
fn shared_native_and_direct_wasm_example_observes_the_complete_copied_object() {
    let session = noon::example_scenes::object_membership::session().unwrap();
    assert_eq!(session.painter_order().len(), 3);
}

#[test]
fn empty_family_point_observations_validate_the_execution_publication() {
    fn snapshots(
        scene: &Scene,
        execution: &noon::ExecutionSession,
        family: &MobjectFamily,
    ) -> [Result<(), noon::AuthoringError>; 3] {
        let store = scene.integration_store();
        [
            noon::integration::effective_all_points(store, execution, family.into()).map(|_| ()),
            noon::integration::effective_vector_boundary_points(store, execution, family.into())
                .map(|_| ()),
            noon::integration::effective_point_reduction_tree(store, execution, family.into())
                .map(|_| ()),
        ]
    }

    let mut scene = Scene::new();
    let empty = scene.family(&[]).unwrap();
    let left = scene.family(&[(&empty).into()]).unwrap();
    let right = scene.family(&[(&empty).into()]).unwrap();
    let aliased = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    let mut circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let execution = scene.execution_session().unwrap();
    let publication = execution.publication_context();
    let revision = scene.revision();
    for family in [&empty, &aliased] {
        assert!(snapshots(&scene, &execution, family)
            .into_iter()
            .all(|r| r.is_ok()));
    }
    assert_eq!(scene.revision(), revision);
    assert_eq!(execution.publication_context(), publication);

    circle.set_object_opacity(0.5).unwrap();
    for family in [&empty, &aliased] {
        for result in snapshots(&scene, &execution, family) {
            assert!(matches!(
                result,
                Err(noon::AuthoringError::ExecutionPublication(
                    noon::ExecutionSessionPublicationError::StaleSceneRevision { .. }
                ))
            ));
        }
    }
    assert_eq!(execution.publication_context(), publication);

    let other = Scene::new();
    let foreign_execution = other.execution_session().unwrap();
    for result in snapshots(&scene, &foreign_execution, &empty) {
        assert!(matches!(
            result,
            Err(noon::AuthoringError::ExecutionPublication(
                noon::ExecutionSessionPublicationError::ForeignSemanticStore
            ))
        ));
    }
}

#[test]
fn inactive_family_aliases_do_not_prevent_removed_leaf_create() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let left = scene.family(&[(&circle).into()]).unwrap();
    let right = scene.family(&[(&circle).into()]).unwrap();
    let aliases = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    scene.add(&circle).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    assert!(scene
        .live(&mut execution)
        .declare_and_activate_create(&circle, AnimationOptions::new())
        .is_err());
    assert_eq!(execution.publication_context(), before);
    scene.live(&mut execution).remove(&circle).unwrap();
    assert!(execution.painter_order().is_empty());
    let reveal = scene
        .live(&mut execution)
        .declare_and_activate_create(
            &circle,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    scene
        .live(&mut execution)
        .advance_segment_to(reveal, 0.25)
        .unwrap();
    let current =
        noon::integration::effective_path_query(scene.integration_store(), &execution, &circle)
            .unwrap();
    assert_eq!(current.point_count(), 12);
    assert_eq!(execution.painter_order().len(), 1);
    assert_eq!(
        noon::integration::effective_vector_boundary_points(
            scene.integration_store(),
            &execution,
            (&aliases).into(),
        )
        .unwrap(),
        current.anchors().unwrap()
    );
    let store = scene.integration_store().borrow();
    assert!(store
        .is_direct_member(left.node_id(), circle.node_id())
        .unwrap());
    assert!(store
        .is_direct_member(right.node_id(), circle.node_id())
        .unwrap());
}
