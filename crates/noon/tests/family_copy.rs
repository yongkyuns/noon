use noon::{AnimationOptions, RateFunction, Scene};
use noon_core::{SemanticObjectProperty, SemanticVec3};

#[test]
fn authored_copy_preserves_dag_aliases_order_resources_and_source_state_in_one_commit() {
    let scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let nested = scene.family(&[(&leaf).into()]).unwrap();
    let source = scene.family(&[(&leaf).into(), (&nested).into()]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    let copied = source.copy_family().unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    let mut copy_leaf = copied.mobject(&leaf).unwrap();
    let copy_nested = copied.family(&nested).unwrap();
    assert_ne!(copy_leaf.node_id(), leaf.node_id());
    assert_ne!(copied.root().node_id(), source.node_id());
    let copied_state = copy_leaf.state().unwrap();
    let source_state = leaf.state().unwrap();
    assert_eq!(copied_state.content, source_state.content);
    assert_eq!(copied_state.transform, source_state.transform);
    assert_eq!(copied_state.style, source_state.style);
    assert_eq!(
        copied_state.presentation().z_index,
        source_state.presentation().z_index
    );
    assert_ne!(
        copied_state.presentation().insertion_order,
        source_state.presentation().insertion_order
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(copied.root().node_id())
            .unwrap(),
        vec![copy_leaf.node_id(), copy_nested.node_id()]
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(copy_nested.node_id())
            .unwrap(),
        vec![copy_leaf.node_id()]
    );
    copy_leaf.shift(2.0, 0.0).unwrap();
    assert_eq!(leaf.center().unwrap(), (0.0, 0.0));
    assert_eq!(copy_nested.layout().unwrap().center(), (2.0, 0.0));
    let empty = scene.family(&[]).unwrap();
    assert_eq!(
        empty.copy_family().unwrap().root().layout_bounds().unwrap(),
        None
    );
    assert!(copied.family(&empty).is_err());
    let foreign = Scene::new().square(1.0).unwrap();
    assert!(copied.mobject(&foreign).is_err());
    scene
        .integration_store()
        .borrow_mut()
        .remove_node(leaf.node_id())
        .unwrap();
    assert!(copied.mobject(&leaf).is_err());
    assert_eq!(copy_leaf.center().unwrap(), (2.0, 0.0));
}

#[test]
fn live_copy_obeys_completion_and_captures_completed_state_without_changing_source() {
    let mut scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let family = scene.family(&[(&leaf).into()]).unwrap();
    let mut target = leaf.target_editor().unwrap();
    target.shift(4.0, 0.0).unwrap();
    scene.add(&leaf).unwrap();
    let animation = scene
        .declare_transform_to(
            &leaf,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let halfway = live.effective(&leaf).unwrap();
    assert!(live.copy_family(&family).is_err());
    assert_eq!(live.effective(&leaf).unwrap(), halfway);
    live.advance_segment_to(segment, 2.0).unwrap();
    live.complete_segment(segment).unwrap();
    let before = live.effective(&leaf).unwrap();
    let copied = live.copy_family(&family).unwrap();
    let copy_leaf = copied.mobject(&leaf).unwrap();
    assert_eq!(copy_leaf.center().unwrap(), (4.0, 0.0));
    assert_eq!(leaf.center().unwrap(), (4.0, 0.0));
    assert_eq!(live.effective(&leaf).unwrap().transform, before.transform);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.publication.scene_revision().checked_next().unwrap()
    );
    live.shift(&copy_leaf, 1.0, 0.0).unwrap();
    assert_eq!(copy_leaf.center().unwrap(), (5.0, 0.0));
    assert_eq!(leaf.center().unwrap(), (4.0, 0.0));
}

#[test]
fn a_late_uncapturable_member_rejects_the_entire_live_copy() {
    let scene = Scene::new();
    let first = scene.square(0.5).unwrap();
    let reactive = scene.square(0.5).unwrap();
    let signal = scene
        .integration_store()
        .borrow_mut()
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    scene
        .integration_store()
        .borrow_mut()
        .bind_semantic_signal(
            signal,
            reactive.node_id(),
            SemanticObjectProperty::Translation,
        )
        .unwrap();
    let source = scene
        .family(&[(&first).into(), (&reactive).into()])
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    assert!(scene.live(&mut execution).copy_family(&source).is_err());
    assert_eq!(execution.publication_context(), before);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(first.center().unwrap(), (0.0, 0.0));
}

#[test]
fn detached_references_copy_atomically_without_changing_family_membership() {
    let scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let saved = leaf.target_editor().unwrap();
    let source = scene.family(&[(&leaf).into()]).unwrap();
    let referenced_family = scene.family(&[(&saved).into(), (&leaf).into()]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    let copied = source
        .copy_with_references(&[(&saved).into(), (&referenced_family).into(), (&leaf).into()])
        .unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    let copied_leaf = copied.mobject(&leaf).unwrap();
    let mut copied_saved = copied.mobject(&saved).unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_family_members_checked(copied.root().node_id())
            .unwrap(),
        vec![copied_leaf.node_id()]
    );
    assert_eq!(
        store
            .semantic_family_members_checked(copied.family(&referenced_family).unwrap().node_id())
            .unwrap(),
        vec![copied_saved.node_id(), copied_leaf.node_id()]
    );
    drop(store);
    copied_saved.shift(3.0, 0.0).unwrap();
    assert_eq!(saved.center().unwrap(), (0.0, 0.0));
    let foreign = Scene::new().square(1.0).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    assert!(source
        .copy_with_references(&[(&saved).into(), (&foreign).into()])
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
}
