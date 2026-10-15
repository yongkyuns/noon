//! Direct-Rust counterpart of the unchanged CPython/Pyodide source corpus.
//! Calls the same ordinary composition and completion boundaries as both bindings.
use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest, Vec2,
};

#[test]
fn python_host_sequential_completion_and_same_handle_reentry() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let identity = circle.node_id();
    let mut execution = scene.execution_session().unwrap();
    let options = AnimationOptions::new()
        .run_time(0.25)
        .rate_func(RateFunction::Linear);
    for (x, y) in [(1.0, 0.0), (0.0, 1.0)] {
        let mut live = scene.live(&mut execution);
        let target = live.target_editor(&circle).unwrap();
        live.shift(&target, x, y).unwrap();
        let request = Request::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new().lag_ratio(0.0),
            children: vec![Request::TransformTo(
                TransformToRequest::new(&circle, &target, options).method_target(),
            )],
        };
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        let start = live.effective(&circle).unwrap().transform.translation;
        live.advance_segment_to(segment, segment.end_time() - 0.125)
            .unwrap();
        let mid = live.effective(&circle).unwrap().transform.translation;
        assert_eq!(mid, start + Vec2::new(x as f32 * 0.5, y as f32 * 0.5));
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert!(!live.segment_state(segment).is_complete());
        live.complete_segment(segment).unwrap();
        assert!(live.segment_state(segment).is_complete());
    }
    let mut live = scene.live(&mut execution);
    assert_eq!(
        live.effective(&circle).unwrap().transform.translation,
        Vec2::new(1.0, 1.0)
    );
    live.remove(&circle).unwrap();
    live.add(&circle).unwrap();
    assert_eq!(circle.node_id(), identity);
    let wait = live.wait_segment(0.5).unwrap();
    live.advance_segment_to(wait, wait.end_time()).unwrap();
    live.complete_segment(wait).unwrap();
    assert_eq!(
        live.effective(&circle).unwrap().transform.translation,
        Vec2::new(1.0, 1.0)
    );
}

#[test]
fn python_host_conflicting_admission_cannot_partially_publish() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let first = live.target_editor(&circle).unwrap();
    live.shift(&first, 1.0, 0.0).unwrap();
    let second = live.target_editor(&circle).unwrap();
    live.shift(&second, 0.0, 1.0).unwrap();
    let options = AnimationOptions::new().run_time(0.25);
    let request = Request::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        options: AnimationOptions::new(),
        children: vec![
            Request::TransformTo(TransformToRequest::new(&circle, &first, options)),
            Request::TransformTo(TransformToRequest::new(&circle, &second, options)),
        ],
    };
    let before = live.effective(&circle).unwrap();
    assert!(live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(live.effective(&circle).unwrap(), before);
    // Failure does not poison a later valid operation.
    let wait = live.wait_segment(0.25).unwrap();
    live.advance_segment_to(wait, wait.end_time()).unwrap();
    live.complete_segment(wait).unwrap();
}
