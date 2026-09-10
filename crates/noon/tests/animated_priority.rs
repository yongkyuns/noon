use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, RateFunction, Scene,
    TransformToRequest,
};

#[test]
fn method_priority_is_discrete_exact_and_persists_with_returning_motion() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let cover = scene.square(1.0).unwrap();
    scene.add(&source).unwrap();
    scene.add(&cover).unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_z_index(1.0000000000000002).unwrap();
    target.shift(2.0, 0.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let request = Request::TransformTo(
        TransformToRequest::new(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::ThereAndBack),
        )
        .method_target(),
    );
    let segment = live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    assert_eq!(live.effective(&source).unwrap().z_index, 0.0);
    assert_eq!(
        live.effective(&source).unwrap().transform.translation.x,
        2.0
    );
    live.advance_segment_to(segment, 2.0).unwrap();
    assert_eq!(live.effective(&source).unwrap().z_index, 1.0000000000000002);
    assert_eq!(live.z_index(&(&source).into()).unwrap(), 1.0000000000000002);
    assert_eq!(
        live.effective(&source).unwrap().transform.translation.x,
        0.0
    );
    live.complete_segment(segment).unwrap();
    assert_eq!(source.z_index().unwrap(), 1.0000000000000002);
    live.set_z_index(&(&source).into(), -2.0, true).unwrap();
    assert_eq!(live.effective(&source).unwrap().z_index, -2.0);
    assert_eq!(session.painter_order(), &[0, 1]);
    session.seek(0.5).unwrap();
    assert_eq!(session.frame().objects[0].z_index, 0.0);
}

#[test]
fn ordinary_transform_ignores_target_priority() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    scene.add(&source).unwrap();
    let target = source.target_editor().unwrap();
    target.set_z_index(9.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let request = Request::TransformTo(TransformToRequest::new(
        &source,
        &target,
        AnimationOptions::new(),
    ));
    let segment = live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    assert_eq!(source.z_index().unwrap(), 0.0);
    assert_eq!(live.effective(&source).unwrap().z_index, 0.0);
}

#[test]
fn paired_example_retains_each_completed_priority_interval() {
    let mut session = noon::example_scenes::animated_priority::session().unwrap();
    for (time, expected) in [
        (0.25, vec![0, 1]),
        (0.6, vec![1, 0]),
        (1.0, vec![1, 0]),
        (1.4, vec![0, 1]),
        (2.5, vec![0, 1]),
        (3.125, vec![1, 0]),
    ] {
        session.seek(time).unwrap();
        assert_eq!(session.painter_order(), expected);
    }
}

#[test]
fn nested_sequence_captures_prior_priority_and_reconciles_only_final_authored_value() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let cover = scene.square(1.0).unwrap();
    scene.add(&source).unwrap();
    scene.add(&cover).unwrap();
    let first = source.target_editor().unwrap();
    first.set_z_index(2.0).unwrap();
    let last = source.target_editor().unwrap();
    last.set_z_index(-1.0).unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let request = Request::Composition {
        kind: noon::SemanticAnimationCompositionKind::Sequence,
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
        children: vec![
            Request::Wait { duration: 0.5 },
            Request::TransformTo(TransformToRequest::new(&source, &first, options).method_target()),
            Request::TransformTo(TransformToRequest::new(&source, &last, options).method_target()),
        ],
    };
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    live.advance_segment_to(segment, 1.75).unwrap();
    assert_eq!(live.effective(&source).unwrap().z_index, 2.0);
    assert_eq!(live.z_index(&(&source).into()).unwrap(), 2.0);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    assert_eq!(source.z_index().unwrap(), -1.0);
    session.seek(1.75).unwrap();
    assert_eq!(session.frame().objects[0].z_index, 2.0);
}
