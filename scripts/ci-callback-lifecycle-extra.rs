

#[test]
fn pending_callback_excludes_lifecycle_admission_and_can_still_commit() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    let overlay = match execution.advance_to_callback_barrier(0.0).unwrap() {
        noon::integration::CallbackAdvance::HostRequired { overlay, .. } => overlay,
        noon::integration::CallbackAdvance::Ready(_) => panic!("expected an unpublished callback phase"),
    };
    let token = execution.pending_callback_token();
    let before = execution.publication_context();
    let frame = execution.frame().clone();
    let error = scene.live(&mut execution).declare_and_activate_composition(
        &Lifecycle::FadeIn.request(&target), AnimationOptions::new(),
    ).unwrap_err();
    assert!(matches!(error, noon::LiveSessionError::Activation(noon::ExecutionSessionAnimationError::RequiredCallbackPending)));
    assert_eq!(execution.pending_callback_token(), token);
    assert_eq!(execution.publication_context(), before);
    assert_eq!(scene.revision(), before.scene_revision());
    assert_eq!(execution.frame(), &frame);
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    execution.commit_required_callback_phase(overlay.finish()).unwrap();
    let segment = scene.live(&mut execution).declare_and_activate_composition(
        &Lifecycle::FadeIn.request(&target), AnimationOptions::new(),
    ).unwrap();
    callbacks.advance_segment_to(&mut execution, segment, 1.0).unwrap();
    scene.live(&mut execution).complete_segment(segment).unwrap();
    assert_eq!(execution.frame().objects[0].transform.translation.x, 1.0);
    assert!(scene.live(&mut execution).contains(&target).unwrap());
}

#[test]
fn later_same_store_failure_rolls_back_staged_admission_and_allows_retry() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    let attached = scene.square(0.5).unwrap();
    scene.add(&moving).unwrap();
    scene.add(&attached).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    callbacks.advance_to(&mut execution, 0.0).unwrap();
    let before = execution.publication_context();
    let frame = execution.frame().clone();
    let calls = times.borrow().clone();
    let nodes = scene.integration_store().borrow().len();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![Lifecycle::FadeIn.request(&target), Lifecycle::FadeIn.request(&attached)],
        options: options(),
    };
    let error = scene.live(&mut execution).declare_and_activate_composition(&request, AnimationOptions::new()).unwrap_err();
    assert!(error.to_string().contains("detached"), "{error}");
    assert_eq!(execution.publication_context(), before);
    assert_eq!(scene.revision(), before.scene_revision());
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(execution.frame(), &frame);
    assert_eq!(*times.borrow(), calls);
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    let segment = scene.live(&mut execution).declare_and_activate_composition(
        &Lifecycle::FadeIn.request(&target), AnimationOptions::new(),
    ).unwrap();
    callbacks.advance_segment_to(&mut execution, segment, 1.0).unwrap();
    scene.live(&mut execution).complete_segment(segment).unwrap();
    assert!(scene.live(&mut execution).contains(&target).unwrap());
    assert!(scene.live(&mut execution).contains(&attached).unwrap());
    assert_eq!(times.borrow().last().copied(), Some(1.0));
}

#[test]
fn unrelated_future_activation_is_not_skipped_by_lifecycle_progression() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(moving.node_id(), MOVE, 0.0);
    tx.apply(&mut scene.integration_store().borrow_mut()).unwrap();
    callbacks.add_updater(&mut scene.integration_store().borrow_mut(), moving.node_id(), MOVE, 0.5, None).unwrap();
    let mut execution = scene.execution_session().unwrap();
    callbacks.advance_to(&mut execution, 0.0).unwrap();
    assert!(times.borrow().is_empty());
    let segment = scene.live(&mut execution).declare_and_activate_composition(
        &Lifecycle::FadeIn.request(&target), AnimationOptions::new(),
    ).unwrap();
    callbacks.advance_segment_to(&mut execution, segment, 1.0).unwrap();
    assert_eq!(*times.borrow(), [0.5, 1.0]);
    assert_eq!(execution.frame().objects[0].transform.translation.x, 1.0);
    assert_eq!(execution.frame().objects[1].appearance, 1.0);
    scene.live(&mut execution).complete_segment(segment).unwrap();
}

#[test]
fn a_target_with_completed_updater_history_can_fade_out_without_restarting_it() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    scene.add(&target).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &target, &times);
    let mut execution = scene.execution_session().unwrap();
    let wait = scene.live(&mut execution).wait_segment(1.0).unwrap();
    callbacks.advance_segment_to(&mut execution, wait, 1.0).unwrap();
    scene.live(&mut execution).complete_segment(wait).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(target.node_id(), MOVE, 1.0);
    scene.live(&mut execution).apply(tx).unwrap();
    let calls = times.borrow().clone();
    let frozen = scene.live(&mut execution).effective(&target).unwrap().transform;
    let segment = scene.live(&mut execution).declare_and_activate_composition(
        &Lifecycle::FadeOut.request(&target), AnimationOptions::new(),
    ).unwrap();
    for time in [1.5, 2.0] {
        callbacks.advance_segment_to(&mut execution, segment, time).unwrap();
        assert_eq!(*times.borrow(), calls);
        assert_eq!(execution.frame().objects[0].transform, frozen);
    }
    scene.live(&mut execution).complete_segment(segment).unwrap();
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    assert_eq!(*times.borrow(), calls);
}
