use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, RateFunction, Scene,
    SemanticAnimationCompositionKind as Kind,
};

#[test]
fn singleton_and_grouped_uncreate_share_reversal_and_membership() {
    for mounted in [false, true] {
        for reverse in [false, true] {
            let mut traces = Vec::new();
            for grouped in [false, true] {
                let mut scene = Scene::new();
                let square = scene.square(1.0).unwrap();
                if mounted {
                    scene.add(&square).unwrap();
                }
                let mut execution = scene.execution_session().unwrap();
                let options = AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::RushInto)
                    .remover(false)
                    .reverse_rate_function(reverse);
                let segment = if grouped {
                    scene
                        .live(&mut execution)
                        .declare_and_activate_composition(
                            &Request::Composition {
                                kind: Kind::Parallel,
                                children: vec![Request::Uncreate {
                                    target: &square,
                                    options,
                                }],
                                options: AnimationOptions::new().rate_func(RateFunction::Linear),
                            },
                            AnimationOptions::new(),
                        )
                        .unwrap()
                } else {
                    scene
                        .live(&mut execution)
                        .declare_and_activate_uncreate(&square, options)
                        .unwrap()
                };
                let mut samples = Vec::new();
                for time in [0.0, 0.25, 0.5, 1.0] {
                    scene
                        .live(&mut execution)
                        .advance_segment_to(segment, time)
                        .unwrap();
                    let reveal = execution.frame().reveal(0);
                    let expected = RateFunction::RushInto.evaluate(if reverse {
                        1.0 - time as f32
                    } else {
                        time as f32
                    });
                    assert!((reveal - expected).abs() < 1e-6);
                    samples.push(reveal);
                }
                scene
                    .live(&mut execution)
                    .complete_segment(segment)
                    .unwrap();
                assert!(scene.live(&mut execution).contains(&square).unwrap());
                traces.push(samples);
            }
            assert_eq!(traces[0], traces[1]);
        }
    }
}

#[test]
fn mixed_create_uncreate_publishes_one_coherent_completion() {
    let mut scene = Scene::new();
    let leaving = scene.square(1.0).unwrap();
    let entering = scene.square(0.5).unwrap();
    scene.add(&leaving).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Request::Composition {
                kind: Kind::Parallel,
                children: vec![
                    Request::Uncreate {
                        target: &leaving,
                        options,
                    },
                    Request::Create {
                        target: &entering,
                        options,
                    },
                ],
                options,
            },
            AnimationOptions::new(),
        )
        .unwrap();
    assert!(scene.live(&mut execution).contains(&entering).unwrap());
    scene
        .live(&mut execution)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(execution.frame().reveal(0), 0.5);
    assert_eq!(execution.frame().reveal(1), 0.5);
    scene
        .live(&mut execution)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
    assert!(!scene.live(&mut execution).contains(&leaving).unwrap());
    assert!(scene.live(&mut execution).contains(&entering).unwrap());
}

#[test]
fn rejected_composition_cannot_partially_admit_uncreate() {
    let scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    let foreign = Scene::new().square(1.0).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    let options = AnimationOptions::new();
    assert!(scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Request::Composition {
                kind: Kind::Parallel,
                children: vec![
                    Request::Uncreate {
                        target: &target,
                        options
                    },
                    Request::Create {
                        target: &foreign,
                        options
                    }
                ],
                options,
            },
            options,
        )
        .is_err());
    assert_eq!(execution.publication_context(), before);
    assert_eq!(
        scene.store().borrow().scene_revision(),
        before.scene_revision()
    );
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
}
