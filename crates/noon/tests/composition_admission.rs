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
                let mut execution = scene.into_execution().unwrap();
                let options = AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::RushInto)
                    .remover(false)
                    .reverse_rate_function(reverse);
                let segment = if grouped {
                    execution
                        .live()
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
                    execution
                        .live()
                        .declare_and_activate_uncreate(&square, options)
                        .unwrap()
                };
                let mut samples = Vec::new();
                for time in [0.0, 0.25, 0.5, 1.0] {
                    execution.live().advance_segment_to(segment, time).unwrap();
                    let reveal = execution.session().frame().reveal(0);
                    let expected = RateFunction::RushInto.evaluate(if reverse {
                        1.0 - time as f32
                    } else {
                        time as f32
                    });
                    assert!((reveal - expected).abs() < 1e-6);
                    samples.push(reveal);
                }
                execution.live().complete_segment(segment).unwrap();
                assert!(execution.scene_mut().contains_direct(&square).unwrap());
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
    let mut execution = scene.into_execution().unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let segment = execution
        .live()
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
    assert!(execution.scene_mut().contains_direct(&entering).unwrap());
    execution.live().advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(execution.session().frame().reveal(0), 0.5);
    assert_eq!(execution.session().frame().reveal(1), 0.5);
    execution.live().advance_segment_to(segment, 1.0).unwrap();
    execution.live().complete_segment(segment).unwrap();
    assert!(!execution.scene_mut().contains_direct(&leaving).unwrap());
    assert!(execution.scene_mut().contains_direct(&entering).unwrap());
}

#[test]
fn rejected_composition_cannot_partially_admit_uncreate() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    let foreign = Scene::new().square(1.0).unwrap();
    let mut execution = scene.into_execution().unwrap();
    let before = execution.session().publication_context();
    let options = AnimationOptions::new();
    assert!(execution
        .live()
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
    assert_eq!(execution.session().publication_context(), before);
    assert_eq!(
        execution
            .scene()
            .integration_store()
            .borrow()
            .scene_revision(),
        before.scene_revision()
    );
    assert!(!execution.scene_mut().contains_direct(&target).unwrap());
}
