//! Fail after resource import, not merely at constructor argument preflight.
use super::*;
use crate::{ExecutionSession, ExecutionSessionPublicationError, LiveSessionError, Scene};
use noon_core::{FrameEpoch, GeometryResourceHandle};

fn path_options() -> ManimGeometryOptions {
    ManimGeometryOptions::path(
        VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 1.0)),
    )
    .unwrap()
}

fn fixture() -> (Scene, Mobject, Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let retained = scene.geometry(path_options()).unwrap();
    let sentinel = scene.circle(0.25).unwrap();
    scene.add(&retained).unwrap();
    scene.add(&sentinel).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.seek(0.25).unwrap();
    session.take_frame_changes();
    (scene, retained, sentinel, session)
}

fn resource(object: &Mobject) -> GeometryResourceHandle {
    match object.state().unwrap().content.geometry().unwrap() {
        StoredGeometry::Resource(handle) => handle,
        _ => panic!("expected a retained path"),
    }
}

#[test]
fn geometry_publication_scope_rolls_back_a_real_late_lowering_failure() {
    let (scene, retained, sentinel, mut session) = fixture();
    let store_rc = Rc::clone(scene.integration_store());
    let root = scene.root();
    let before_stats = store_rc.borrow().geometry_resources().stats();
    let before_members = store_rc.borrow().node(root).unwrap().members().to_vec();
    let before_context = session.publication_context();
    let before_frame = session.frame().clone();
    let before_sentinel = sentinel.state().unwrap();
    let before_retained = retained.state().unwrap();
    let before_id = session.execution_object_id(sentinel.node_id());
    let retained_resource = resource(&retained);
    let mut imported = None;

    // The same scoped materialization used by the live constructor encloses an
    // actual compiler preflight. The existing visible object's finite but
    // unrenderable rotation fails lowering after the path is already available.
    let error = path_options()
        .with_state(&mut store_rc.borrow_mut(), |store, state| {
            let StoredGeometry::Resource(handle) = state.content.geometry().unwrap() else {
                panic!("the test must reach path admission");
            };
            imported = Some(handle);
            assert!(store.geometry_resources().get(handle).is_some());
            assert_eq!(
                store.geometry_resources().len(),
                before_stats.live_resources + 1
            );
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(SemanticNodeCreation::object(state));
            transaction.set_property(
                sentinel.node_id(),
                SemanticObjectProperty::RotationZ,
                f64::MAX,
            );
            session
                .apply_semantic_transaction_at_root(store, root, transaction)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
        .unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::ExecutionPublication(ExecutionSessionPublicationError::Lowering(_))
    ));
    let rejected = imported.expect("failure must occur after import");
    assert!(store_rc
        .borrow()
        .geometry_resources()
        .get(rejected)
        .is_none());
    assert!(store_rc
        .borrow()
        .geometry_resources()
        .get(retained_resource)
        .is_some());
    assert_eq!(store_rc.borrow().geometry_resources().stats(), before_stats);
    assert_eq!(
        store_rc.borrow().node(root).unwrap().members(),
        before_members
    );
    assert_eq!(scene.revision(), before_context.scene_revision());
    assert_eq!(session.publication_context(), before_context);
    assert_eq!(session.frame(), &before_frame);
    assert_eq!(sentinel.state().unwrap(), before_sentinel);
    assert_eq!(retained.state().unwrap(), before_retained);
    assert_eq!(session.execution_object_id(sentinel.node_id()), before_id);
    assert!(session.take_frame_changes().is_empty());

    // Retry through the public constructor. A released generation must not
    // revive, and admission must reuse the successful import rather than create
    // another copy or replace the runtime's unrelated object identity.
    let curve = scene
        .live(&mut session)
        .create_manim_geometry(path_options())
        .unwrap();
    let retry_resource = resource(&curve);
    assert_ne!(retry_resource, rejected);
    assert!(store_rc
        .borrow()
        .geometry_resources()
        .get(rejected)
        .is_none());
    assert_eq!(session.execution_object_id(curve.node_id()), None);
    assert_eq!(session.frame().objects, before_frame.objects);
    assert!(session.take_frame_changes().is_empty());
    scene.live(&mut session).add(&curve).unwrap();
    assert_eq!(resource(&curve), retry_resource);
    assert_eq!(
        store_rc.borrow().geometry_resources().len(),
        before_stats.live_resources + 1
    );
    assert_eq!(session.execution_object_id(sentinel.node_id()), before_id);
    assert_eq!(curve.path_query().unwrap().start().unwrap(), (-1.0, 0.0));
    assert_eq!(curve.path_query().unwrap().end().unwrap(), (1.0, 1.0));
}

#[test]
fn geometry_publication_scope_preserves_injected_runtime_errors_and_existing_paths() {
    let (scene, retained, sentinel, mut session) = fixture();
    let store_rc = Rc::clone(scene.integration_store());
    let before_stats = store_rc.borrow().geometry_resources().stats();
    let before_revision = scene.revision();
    let before_context = session.publication_context();
    let before_frame = session.frame().clone();
    let original = resource(&retained);
    let expected = AuthoringError::ExecutionPublication(ExecutionSessionPublicationError::Runtime(
        noon_runtime::AuthoredPublicationError::FrameEpochExhausted(FrameEpoch::new(u64::MAX)),
    ));
    let mut rejected_handles = Vec::new();
    for _ in 0..3 {
        let result: Result<(), AuthoringError> =
            path_options().with_state(&mut store_rc.borrow_mut(), |store, state| {
                let StoredGeometry::Resource(handle) = state.content.geometry().unwrap() else {
                    panic!("expected imported path");
                };
                assert!(store.geometry_resources().get(handle).is_some());
                assert_eq!(
                    store.geometry_resources().len(),
                    before_stats.live_resources + 1
                );
                rejected_handles.push(handle);
                Err(expected.clone())
            });
        assert_eq!(result, Err(expected.clone()));
        assert_eq!(store_rc.borrow().geometry_resources().stats(), before_stats);
        assert!(store_rc
            .borrow()
            .geometry_resources()
            .get(original)
            .is_some());
    }
    for (index, handle) in rejected_handles.iter().enumerate() {
        assert!(!rejected_handles[..index].contains(handle));
        assert!(store_rc
            .borrow()
            .geometry_resources()
            .get(*handle)
            .is_none());
    }
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(session.publication_context(), before_context);
    assert_eq!(session.frame(), &before_frame);
    assert!(session.take_frame_changes().is_empty());
    retained.validate().unwrap();
    sentinel.validate().unwrap();
}

#[test]
fn live_geometry_preflight_still_rejects_stale_owners_without_import() {
    let (scene, retained, sentinel, mut session) = fixture();
    let store_rc = Rc::clone(scene.integration_store());
    let before_stats = store_rc.borrow().geometry_resources().stats();
    // An explicit out-of-band edit must not be repaired by the constructor.
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(sentinel.node_id(), SemanticObjectProperty::RotationZ, 0.5);
    transaction.apply(&mut store_rc.borrow_mut()).unwrap();
    let before_revision = scene.revision();
    let before_context = session.publication_context();
    assert!(matches!(
        scene
            .live(&mut session)
            .create_manim_geometry(path_options()),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::StaleSceneRevision { .. }
        ))
    ));
    assert_eq!(store_rc.borrow().geometry_resources().stats(), before_stats);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(session.publication_context(), before_context);
    retained.validate().unwrap();
}
