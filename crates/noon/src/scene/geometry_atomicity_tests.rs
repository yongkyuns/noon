use super::*;
use crate::{geometry_authoring::manim_triangle_path, ManimGeometryOptions};
use noon_core::{FrameEpoch, SemanticVec3, Vec2, VectorPath};

#[test]
fn geometry_is_detached_and_scene_owned_before_and_after_bootstrap() {
    let mut scene = Scene::new();
    let cold_revision = scene.revision();
    let cold = scene
        .geometry(ManimGeometryOptions::circle(0.5).unwrap())
        .unwrap();
    assert_eq!(scene.revision().get(), cold_revision.get() + 1);
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());

    scene.add_many(&[(&cold).into()]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let frame_len = scene.owned_execution().frame().objects.len();
    let running_revision = scene.revision();
    let running = scene
        .geometry(ManimGeometryOptions::square(0.75).unwrap())
        .unwrap();

    assert_eq!(scene.revision().get(), running_revision.get() + 1);
    assert_eq!(scene.owned_execution().frame().objects.len(), frame_len);
    assert!(scene
        .owned_execution()
        .execution_object_id(running.node_id())
        .is_none());
    scene.add_many(&[(&running).into()]).unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), frame_len + 1);
    assert!(scene
        .owned_execution()
        .execution_object_id(running.node_id())
        .is_some());
    assert_eq!(
        scene
            .owned_execution()
            .publication_context()
            .scene_revision(),
        scene.revision()
    );
}

#[test]
fn family_is_detached_and_scene_owned_before_and_after_bootstrap() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.square(0.75).unwrap();
    let detached = scene.rectangle(1.25, 0.5).unwrap();
    let cold_revision = scene.revision();
    let cold = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    assert_eq!(scene.revision().get(), cold_revision.get() + 1);
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());

    scene.add_many(&[(&cold).into()]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let frame_len = scene.owned_execution().frame().objects.len();
    let running_revision = scene.revision();
    let running = scene.family(&[(&detached).into()]).unwrap();

    assert_eq!(scene.revision().get(), running_revision.get() + 1);
    assert_eq!(scene.owned_execution().frame().objects.len(), frame_len);
    assert!(scene
        .owned_execution()
        .execution_object_id(running.node_id())
        .is_none());
    scene.add_many(&[(&running).into()]).unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), frame_len + 1);
    assert_eq!(
        scene
            .owned_execution()
            .publication_context()
            .scene_revision(),
        scene.revision()
    );
}

#[test]
fn authored_and_effective_queries_are_explicit_about_the_execution_boundary() {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    let tracker = scene.value_tracker(2.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::ZERO,
        )
        .unwrap();
    scene.bind_position(&object, &position).unwrap();
    let authored = scene.authored(&object).unwrap();
    assert_eq!(authored.transform.translation, SemanticVec3::ZERO);
    assert!(matches!(
        scene.effective(&object),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable
        ))
    ));

    scene.add(&object).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let effective = scene.effective(&object).unwrap();
    assert_eq!(scene.authored(&object).unwrap(), authored);
    assert_eq!(effective.transform.translation.x, 2.0);
    assert_ne!(
        effective.transform.translation.x,
        authored.transform.translation.x as f32
    );
    assert_eq!(effective.appearance, 1.0);
    assert_eq!(effective.publication.scene_revision(), scene.revision());
}

#[test]
fn running_geometry_rejects_stale_execution_before_importing_resources() {
    let mut scene = Scene::new();
    let seed = scene.circle(1.0).unwrap();
    scene.add(&seed).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);

    let mut external = noon_core::SemanticMutationTransaction::new();
    external.add_node(noon_core::SemanticNodeCreation::family());
    external
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let before_revision = scene.revision();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();

    let error = scene
        .geometry(ManimGeometryOptions::path(manim_triangle_path()).unwrap())
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
}

#[test]
fn geometry_publication_scope_rolls_back_path_after_late_runtime_error() {
    let mut scene = Scene::new();
    let seed = scene.circle(0.5).unwrap();
    scene.add(&seed).unwrap();

    // Retain one unrelated path so rollback must preserve an existing resource,
    // identity, authored state, and root membership rather than resetting storage.
    let retained = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::new(-2.0, 0.0))
                    .line_to(Vec2::new(-1.0, 1.0)),
            )
            .unwrap(),
        )
        .unwrap();
    scene.add(&retained).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);

    let before_revision = scene.revision();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let before_members = scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .to_vec();
    let before_seed = seed.state().unwrap();
    let before_retained = retained.state().unwrap();
    let before_context = scene.owned_execution().publication_context();
    let before_frame = scene.owned_execution().frame().clone();
    let exhausted = FrameEpoch::new(u64::MAX);

    // This is the exact import/publication helper used by Scene::geometry. The
    // callback models a fallible Runtime preflight that occurs after the path has
    // been interned but before the semantic transaction reaches its point of no
    // return. with_geometry_path must remove only that unpublished import.
    let error = {
        let store_rc = Rc::clone(scene.integration_store());
        let mut store = store_rc.borrow_mut();
        publish_geometry_options(
            ManimGeometryOptions::path(crate::geometry_authoring::manim_triangle_path()).unwrap(),
            &mut store,
            |store, _transaction| {
                assert_ne!(store.geometry_resources().stats(), before_resources);
                Err(AuthoringError::ExecutionPublication(
                    crate::ExecutionSessionPublicationError::Runtime(
                        noon_runtime::AuthoredPublicationError::FrameEpochExhausted(exhausted),
                    ),
                ))
            },
        )
        .unwrap_err()
    };

    assert_eq!(
        error,
        AuthoringError::ExecutionPublication(crate::ExecutionSessionPublicationError::Runtime(
            noon_runtime::AuthoredPublicationError::FrameEpochExhausted(exhausted),
        ))
    );
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        before_resources
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        before_members.as_slice()
    );
    assert_eq!(seed.state().unwrap(), before_seed);
    assert_eq!(retained.state().unwrap(), before_retained);
    assert_eq!(
        scene.owned_execution().publication_context(),
        before_context
    );
    assert_eq!(scene.owned_execution().frame(), &before_frame);
}
