use noon::{Scene, SceneMembershipRequest};
use noon_core::{
    ObjectId, SemanticMutationTransaction, SemanticSpatialCompositionDomain, SemanticTransform,
    SemanticTransform2_5D, SemanticVec3,
};

fn row_index(execution: &noon::ExecutionSession, object: ObjectId) -> usize {
    execution
        .frame()
        .objects
        .iter()
        .position(|row| row.id == object)
        .unwrap()
}

fn anchor_center(execution: &noon::ExecutionSession, object: ObjectId) -> SemanticVec3 {
    execution.frame().objects[row_index(execution, object)]
        .spatial
        .as_deref()
        .unwrap()
        .fixed_orientation_center
        .unwrap()
}

#[test]
fn removing_anchor_family_publishes_cleared_spatial_state_to_running_runtime() {
    let mut scene = Scene::new();
    let left = scene.square(2.0).unwrap();
    let right = scene.square(2.0).unwrap();
    let family = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    scene
        .edit_membership(SceneMembershipRequest::Add(&[(&family).into()]))
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let store_rc = scene.integration_store();

    let mut anchor = SemanticMutationTransaction::new();
    for member in [&left, &right] {
        anchor.set_spatial_composition_domain_with_anchor(
            member.node_id(),
            SemanticSpatialCompositionDomain::FixedOrientation,
            Some(family.node_id()),
        );
    }
    execution
        .apply_semantic_transaction(&mut store_rc.borrow_mut(), anchor)
        .unwrap();
    let left_object = execution.execution_object_id(left.node_id()).unwrap();
    let right_object = execution.execution_object_id(right.node_id()).unwrap();
    for object in [left_object, right_object] {
        let row = &execution.frame().objects[row_index(&execution, object)];
        let spatial = row.spatial.as_deref().unwrap();
        assert_eq!(spatial.spatial_anchor_family, Some(family.node_id()));
        assert!(spatial.fixed_orientation_center.is_some());
    }

    let before = execution.publication_context();
    let mut remove = SemanticMutationTransaction::new();
    // Keep the objects in the presented root while deleting their shared
    // anchor family, so this probes live center publication rather than tombstones.
    remove.add_member(scene.root(), left.node_id());
    remove.add_member(scene.root(), right.node_id());
    remove.remove_node(family.node_id());
    execution
        .apply_semantic_transaction(&mut store_rc.borrow_mut(), remove)
        .unwrap();
    assert_eq!(
        execution.publication_context().scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    for object in [left_object, right_object] {
        let index = row_index(&execution, object);
        assert!(execution.frame().is_present(index));
        let row = &execution.frame().objects[index];
        let spatial = row.spatial.as_deref().unwrap();
        assert_eq!(
            spatial.composition_domain,
            SemanticSpatialCompositionDomain::FixedOrientation
        );
        assert_eq!(spatial.spatial_anchor_family, None);
        assert!(spatial.fixed_orientation_center.is_some());
    }
}

#[test]
fn nested_anchor_membership_recomputes_only_its_shared_center() {
    let mut scene = Scene::new();
    let left = scene.square(2.0).unwrap();
    let right = scene.square(2.0).unwrap();
    let nested = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    let anchor = scene.family(&[(&nested).into()]).unwrap();
    let other_left = scene.square(2.0).unwrap();
    let other_right = scene.square(2.0).unwrap();
    let other_anchor = scene
        .family(&[(&other_left).into(), (&other_right).into()])
        .unwrap();
    scene
        .edit_membership(SceneMembershipRequest::Add(&[
            (&anchor).into(),
            (&other_anchor).into(),
        ]))
        .unwrap();
    let extra = scene.square(2.0).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let store_rc = scene.integration_store();

    let mut declare = SemanticMutationTransaction::new();
    for member in [&left, &right] {
        declare.set_spatial_composition_domain_with_anchor(
            member.node_id(),
            SemanticSpatialCompositionDomain::FixedOrientation,
            Some(anchor.node_id()),
        );
    }
    for member in [&other_left, &other_right] {
        declare.set_spatial_composition_domain_with_anchor(
            member.node_id(),
            SemanticSpatialCompositionDomain::FixedOrientation,
            Some(other_anchor.node_id()),
        );
    }
    execution
        .apply_semantic_transaction(&mut store_rc.borrow_mut(), declare)
        .unwrap();
    let left_id = execution.execution_object_id(left.node_id()).unwrap();
    let right_id = execution.execution_object_id(right.node_id()).unwrap();
    let other_id = execution.execution_object_id(other_left.node_id()).unwrap();
    let other_center = anchor_center(&execution, other_id);
    execution.take_frame_changes();

    let mut add = SemanticMutationTransaction::new();
    add.set_object_transform(
        extra.node_id(),
        SemanticTransform::from(SemanticTransform2_5D {
            translation: SemanticVec3::new(10.0, 0.0, 0.0),
            scale: SemanticVec3::new(1.0, 1.0, 1.0),
            rotation_z: 0.0,
        }),
    );
    add.add_member(nested.node_id(), extra.node_id());
    execution
        .apply_semantic_transaction(&mut store_rc.borrow_mut(), add)
        .unwrap();
    assert_eq!(anchor_center(&execution, left_id).x, 5.0);
    assert_eq!(anchor_center(&execution, right_id).x, 5.0);
    assert_eq!(anchor_center(&execution, other_id), other_center);
    let changed = execution.take_frame_changes().object_indices().to_vec();
    assert!(!changed.contains(&row_index(&execution, other_id)));

    let mut remove = SemanticMutationTransaction::new();
    remove.remove_member(nested.node_id(), extra.node_id());
    execution
        .apply_semantic_transaction(&mut store_rc.borrow_mut(), remove)
        .unwrap();
    assert_eq!(anchor_center(&execution, left_id).x, 0.0);
    assert_eq!(anchor_center(&execution, right_id).x, 0.0);
    assert_eq!(anchor_center(&execution, other_id), other_center);
    assert!(!execution
        .take_frame_changes()
        .object_indices()
        .contains(&row_index(&execution, other_id)));
}
