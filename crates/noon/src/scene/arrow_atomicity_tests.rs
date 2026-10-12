use super::*;
use crate::{arrow_authoring::publish_arrow_options, ManimArrowOptions};
use noon_core::SemanticObjectProperty;

fn running_scene() -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let seed = scene.circle(0.25).unwrap();
    scene.add(&seed).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    scene.owned_execution_mut().take_frame_changes();
    (scene, seed)
}

#[test]
fn arrow_variants_publish_detached_then_admit_without_staling_execution() {
    let (mut scene, seed) = running_scene();
    let seed_id = scene.owned_execution().execution_object_id(seed.node_id());
    for options in [
        ManimArrowOptions::arrow(-2.0, 0.0, 2.0, 0.0).unwrap(),
        ManimArrowOptions::vector(1.0, 1.0).unwrap(),
        ManimArrowOptions::double_arrow(-2.0, 1.0, 2.0, 1.0).unwrap(),
    ] {
        let before_revision = scene.revision();
        let before_frame = scene.owned_execution().frame().clone();
        let arrow = scene.manim_arrow(options).unwrap();
        assert_eq!(scene.revision().get(), before_revision.get() + 1);
        assert_eq!(
            scene
                .owned_execution()
                .publication_context()
                .scene_revision(),
            scene.revision()
        );
        assert_eq!(scene.owned_execution().frame(), &before_frame);
        assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
        let leaves = [
            Some(arrow.shaft()),
            Some(arrow.end_tip()),
            arrow.start_tip(),
        ];
        for object in leaves.into_iter().flatten() {
            assert!(scene
                .owned_execution()
                .execution_object_id(object.node_id())
                .is_none());
        }
        scene.add_many(&[arrow.family().into()]).unwrap();
        assert_eq!(
            scene.owned_execution().frame().objects.len(),
            before_frame.objects.len() + if arrow.start_tip().is_some() { 3 } else { 2 }
        );
        for object in leaves.into_iter().flatten() {
            assert!(scene
                .owned_execution()
                .execution_object_id(object.node_id())
                .is_some());
        }
        assert_eq!(
            scene.owned_execution().execution_object_id(seed.node_id()),
            seed_id
        );
        scene.owned_execution_mut().take_frame_changes();
    }
}

#[test]
fn running_arrow_rejects_stale_execution_before_import() {
    let (mut scene, seed) = running_scene();
    let mut external = SemanticMutationTransaction::new();
    external.set_property(seed.node_id(), SemanticObjectProperty::RotationZ, 0.5);
    external
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let before_revision = scene.revision();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let before_nodes = scene.integration_store().borrow().len();
    let before_context = scene.owned_execution().publication_context();
    let before_frame = scene.owned_execution().frame().clone();
    let error = scene
        .manim_arrow(ManimArrowOptions::double_arrow(-2.0, 0.0, 2.0, 0.0).unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::ExecutionPublication(
            crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
        )
    ));
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        before_resources
    );
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert_eq!(
        scene.owned_execution().publication_context(),
        before_context
    );
    assert_eq!(scene.owned_execution().frame(), &before_frame);
    assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
}

#[test]
fn arrow_scope_rolls_back_both_tips_on_real_late_lowering_failure() {
    let (mut scene, seed) = running_scene();
    // Retain an existing Arrow resource so rollback cannot simply reset the store.
    let retained = scene
        .manim_arrow(ManimArrowOptions::vector(0.0, 2.0).unwrap())
        .unwrap();
    scene.add_many(&[retained.family().into()]).unwrap();
    scene.owned_execution_mut().take_frame_changes();
    let store_rc = Rc::clone(scene.integration_store());
    let root = scene.root();
    let before_resources = store_rc.borrow().geometry_resources().stats();
    let before_nodes = store_rc.borrow().len();
    let before_members = store_rc.borrow().node(root).unwrap().members().to_vec();
    let before_revision = scene.revision();
    let before_frame = scene.owned_execution().frame().clone();
    let before_context = scene.owned_execution().publication_context();
    let before_seed = seed.state().unwrap();
    let before_retained = retained.end_tip().state().unwrap();
    let options = ManimArrowOptions::double_arrow(-2.0, 1.0, 2.0, 1.0).unwrap();
    let error = publish_arrow_options(
        options.clone(),
        &mut store_rc.borrow_mut(),
        |store, mut transaction| {
            assert_eq!(
                store.geometry_resources().len(),
                before_resources.live_resources + 2
            );
            // Finite semantic input which the renderer cannot represent: exercise the
            // actual shared publication preflight after both paths have been admitted.
            transaction.set_property(seed.node_id(), SemanticObjectProperty::RotationZ, f64::MAX);
            scene
                .owned_execution_mut()
                .apply_semantic_transaction_at_root(store, root, transaction)
                .map_err(AuthoringError::from)
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::ExecutionPublication(crate::ExecutionSessionPublicationError::Lowering(_))
    ));
    assert_eq!(
        store_rc.borrow().geometry_resources().stats(),
        before_resources
    );
    assert_eq!(store_rc.borrow().len(), before_nodes);
    assert_eq!(
        store_rc.borrow().node(root).unwrap().members(),
        before_members
    );
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(scene.owned_execution().frame(), &before_frame);
    assert_eq!(
        scene.owned_execution().publication_context(),
        before_context
    );
    assert_eq!(seed.state().unwrap(), before_seed);
    assert_eq!(retained.end_tip().state().unwrap(), before_retained);
    assert!(scene.owned_execution_mut().take_frame_changes().is_empty());

    let retry = scene.manim_arrow(options).unwrap();
    assert_eq!(
        store_rc.borrow().geometry_resources().len(),
        before_resources.live_resources + 2
    );
    scene.add_many(&[retry.family().into()]).unwrap();
    assert_eq!(
        scene.owned_execution().frame().objects.len(),
        before_frame.objects.len() + 3
    );
}
