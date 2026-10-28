use super::*;
use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{GeometryRef, ObjectId, Style, Transform2D, ViewColorProfile};
use noon_runtime::{ReplayLimits, SceneInstance};

#[test]
fn authored_camera_profile_reaches_initial_and_live_runtime_publications_atomically() {
    let mut scene = Scene::new();
    let camera = scene.camera_frame().unwrap();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
    scene.apply_semantic_transaction(tx).unwrap();
    assert_eq!(
        camera.state().unwrap().view_color_profile(),
        ViewColorProfile::LinearHdr
    );
    let camera_id = camera.node_id();
    let mut session = scene.execution_session().unwrap();
    let execution_id = session.execution_object_id(camera_id).unwrap();
    let circle_id = session.execution_object_id(circle.node_id()).unwrap();
    let initial = session.take_renderer_publication();
    assert_eq!(
        initial.view_color_profile(execution_id),
        Some(ViewColorProfile::LinearHdr)
    );
    assert_eq!(initial.view_color_profile(circle_id), None);
    let initial_context = initial.context();
    drop(initial);
    let before_rows = session.frame().objects.clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_view_color_profile(camera_id, ViewColorProfile::Compatibility);
    scene.live(&mut session).apply(tx).unwrap();
    assert_eq!(
        session.frame().objects,
        before_rows,
        "static view metadata never rewrites effective object rows"
    );
    assert_eq!(camera.node_id(), camera_id);
    let changed = session.take_renderer_publication();
    assert_eq!(
        changed.view_color_profile(execution_id),
        Some(ViewColorProfile::Compatibility)
    );
    assert_ne!(changed.context(), initial_context);
    assert_eq!(changed.changes().object_indices().len(), 1);
    let context = changed.context();
    drop(changed);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_view_color_profile(camera_id, ViewColorProfile::Compatibility);
    scene.live(&mut session).apply(tx).unwrap();
    let no_op = session.take_renderer_publication();
    assert_eq!(no_op.context(), context);
    assert!(no_op.changes().object_indices().is_empty());
    drop(no_op);
    let before_revision = scene.integration_store().borrow().scene_revision();
    let mut invalid = SemanticMutationTransaction::new();
    invalid.set_view_color_profile(camera_id, ViewColorProfile::LinearHdr);
    invalid.set_view_color_profile(circle.node_id(), ViewColorProfile::LinearHdr);
    assert!(scene.live(&mut session).apply(invalid).is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before_revision
    );
    assert_eq!(
        camera.state().unwrap().view_color_profile(),
        ViewColorProfile::Compatibility
    );
    let retained = session.take_renderer_publication();
    assert_eq!(retained.context(), context);
    assert_eq!(
        retained.view_color_profile(execution_id),
        Some(ViewColorProfile::Compatibility)
    );
}

#[test]
fn profile_retained_replay_uses_the_current_compiled_revision_without_a_second_state_map() {
    let id = ObjectId::new(1);
    let mut camera = CompiledObject::new(
        id,
        GeometryRef::rectangle(16.0, 9.0),
        Transform2D::IDENTITY,
        Style {
            opacity: 0.0,
            ..Style::default()
        },
    );
    camera.view_color_profile = Some(ViewColorProfile::Compatibility);
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![camera], &[]).unwrap());
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetViewColorProfile {
            object: id,
            profile: ViewColorProfile::LinearHdr,
        })
        .unwrap();
    runtime.advance_to(2.0).unwrap();
    runtime.seal_replay().unwrap();
    for (time, expected) in [
        (2.0, ViewColorProfile::LinearHdr),
        (0.0, ViewColorProfile::Compatibility),
        (1.5, ViewColorProfile::LinearHdr),
        (0.5, ViewColorProfile::Compatibility),
    ] {
        runtime.seek(time).unwrap();
        assert_eq!(runtime.view_color_profile(id), Some(expected));
        assert_eq!(
            runtime.take_renderer_publication().view_color_profile(id),
            Some(expected)
        );
    }
}

#[test]
fn pending_camera_profile_and_identity_commit_in_one_semantic_transaction() {
    let mut scene = Scene::new();
    let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT),
    });
    state.set_role(SemanticObjectRole::Camera2D);
    let mut tx = SemanticMutationTransaction::new();
    let pending = tx.create_node(SemanticNodeCreation::object(state));
    tx.add_member(scene.root(), pending);
    tx.set_view_color_profile(pending, ViewColorProfile::LinearHdr);
    let committed = scene.apply_semantic_transaction(tx).unwrap();
    let id = committed.resolve(pending).unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(id)
            .unwrap()
            .view_color_profile(),
        ViewColorProfile::LinearHdr
    );
    let mut session = scene.execution_session().unwrap();
    let object = session.execution_object_id(id).unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .view_color_profile(object),
        Some(ViewColorProfile::LinearHdr)
    );
}

#[test]
fn camera_view_profile_stays_with_receiver_role_and_clears_on_role_removal() {
    let mut scene = Scene::new();
    let camera = scene.camera_frame().unwrap();
    let circle = scene.circle(0.5).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
    scene.apply_semantic_transaction(tx).unwrap();
    let mut copied = camera
        .state()
        .unwrap()
        .with_visual_state_from(&circle.state().unwrap());
    assert_eq!(copied.role(), SemanticObjectRole::Camera2D);
    assert_eq!(copied.view_color_profile(), ViewColorProfile::LinearHdr);
    copied.set_role(SemanticObjectRole::Ordinary);
    assert_eq!(copied.view_color_profile(), ViewColorProfile::Compatibility);
}

#[test]
fn compiled_view_profile_preflight_is_atomic_and_accepts_a_staged_camera() {
    use noon_compile::ExecutionMutationTransaction;
    let camera_id = ObjectId::new(1);
    let ordinary_id = ObjectId::new(2);
    let mut camera = CompiledObject::new(
        camera_id,
        GeometryRef::rectangle(16.0, 9.0),
        Transform2D::IDENTITY,
        Style {
            opacity: 0.0,
            ..Style::default()
        },
    );
    camera.view_color_profile = Some(ViewColorProfile::Compatibility);
    let ordinary = CompiledObject::new(
        ordinary_id,
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    );
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(vec![camera.clone(), ordinary], &[]).unwrap(),
    );
    let context = runtime.take_renderer_publication().context();
    let rows = runtime.frame().objects.clone();
    let invalid = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetViewColorProfile {
            object: camera_id,
            profile: ViewColorProfile::LinearHdr,
        },
        ExecutionPatch::SetViewColorProfile {
            object: ordinary_id,
            profile: ViewColorProfile::LinearHdr,
        },
    ]);
    assert!(runtime.apply_execution_transaction(&invalid).is_err());
    assert_eq!(
        runtime.view_color_profile(camera_id),
        Some(ViewColorProfile::Compatibility)
    );
    assert_eq!(runtime.frame().objects, rows);
    assert_eq!(runtime.take_renderer_publication().context(), context);
    let mut empty = SceneInstance::new(
        CompiledScene::compile_objects(Vec::<CompiledObject>::new(), &[]).unwrap(),
    );
    let staged = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::CreateObject(camera),
        ExecutionPatch::SetViewColorProfile {
            object: camera_id,
            profile: ViewColorProfile::LinearHdr,
        },
    ]);
    empty.apply_execution_transaction(&staged).unwrap();
    assert_eq!(empty.frame().objects.len(), 1);
    assert_eq!(
        empty
            .take_renderer_publication()
            .view_color_profile(camera_id),
        Some(ViewColorProfile::LinearHdr)
    );
}
