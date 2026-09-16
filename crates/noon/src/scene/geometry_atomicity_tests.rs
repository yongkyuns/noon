use super::*;
use crate::ManimGeometryOptions;
use noon_core::{FrameEpoch, Vec2, VectorPath};

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
