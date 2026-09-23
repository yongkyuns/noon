//! Every authoring entrypoint admits animation content below persistent foreground.
use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, ExecutionSession, FocusOnOptions,
    Mobject, RateFunction, Scene, SemanticAnimationCompositionKind as Kind,
};
use noon_compile::semantic_execution_object_id;
use noon_core::{ObjectId, SemanticFadeDirection, SemanticNodeId};

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn display(scene: &Scene) -> Vec<SemanticNodeId> {
    scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
}
fn foreground(scene: &Scene) -> Vec<SemanticNodeId> {
    scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .foreground_members()
        .to_vec()
}
fn painter(session: &ExecutionSession) -> Vec<ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&i| session.frame().objects[i as usize].id)
        .collect()
}
fn id(object: &Mobject) -> ObjectId {
    semantic_execution_object_id(object.node_id())
}

fn check_admission(path: u8) {
    let mut scene = Scene::new();
    let back = scene.square(3.0).unwrap();
    let front = scene.square(1.0).unwrap();
    let a = scene.square(2.0).unwrap();
    let b = scene.circle(0.7).unwrap();
    let later = scene.circle(0.3).unwrap();
    let family = if path >= 5 {
        Some(scene.family(&[(&a).into(), (&b).into()]).unwrap())
    } else {
        None
    };
    scene.add(&back).unwrap();
    scene.add_foreground_many(&[(&front).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.revision();
    let segment = match path {
        0 => scene
            .live(&mut session)
            .declare_and_activate_create(&a, options())
            .unwrap(),
        1 => scene
            .live(&mut session)
            .declare_and_activate_fade(&a, SemanticFadeDirection::In, options())
            .unwrap(),
        2 => scene
            .live(&mut session)
            .declare_and_activate_affine_lifecycle(
                &a,
                noon::AffineLifecycleDirection::IntroduceFrom,
                noon::AffineLifecycleEndpoint::Point {
                    x: -1.0,
                    y: 0.0,
                    rotation_offset: 0.0,
                    point_color: None,
                },
                options(),
            )
            .unwrap(),
        3 => scene
            .live(&mut session)
            .declare_and_activate_create_parallel(&[(&a, options()), (&b, options())], options())
            .unwrap(),
        4 => scene
            .live(&mut session)
            .declare_and_activate_composition(
                &Request::Composition {
                    kind: Kind::Parallel,
                    children: vec![
                        Request::Create {
                            target: &a,
                            options: options(),
                        },
                        Request::Fade {
                            target: &b,
                            direction: SemanticFadeDirection::In,
                            endpoint: noon::FadeEndpoint::default(),
                            options: options(),
                        },
                    ],
                    options: options(),
                },
                options(),
            )
            .unwrap(),
        5 => scene
            .live(&mut session)
            .declare_and_activate_family_fade(
                family.as_ref().unwrap(),
                SemanticFadeDirection::In,
                options(),
            )
            .unwrap(),
        6 => scene
            .live(&mut session)
            .declare_and_activate_composition(
                &Request::FamilyReveal {
                    target: family.as_ref().unwrap(),
                    reverse: false,
                    options: options(),
                },
                options(),
            )
            .unwrap(),
        _ => unreachable!(),
    };
    let expected = if path < 3 {
        vec![id(&back), id(&a), id(&front)]
    } else {
        vec![id(&back), id(&a), id(&b), id(&front)]
    };
    assert_eq!(painter(&session), expected, "activation path {path}");
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    let authored = scene.revision();
    let membership = display(&scene);
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, t)
            .unwrap();
        assert_eq!(painter(&session), expected, "path {path}, time {t}");
        assert_eq!(scene.revision(), authored);
        assert_eq!(display(&scene), membership);
        assert_eq!(foreground(&scene), [front.node_id()]);
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(painter(&session), expected);
    scene.live(&mut session).add(&later).unwrap();
    let mut after = expected;
    after.insert(after.len() - 1, id(&later));
    assert_eq!(painter(&session), after);
    assert_eq!(foreground(&scene), [front.node_id()]);
}
#[test]
fn foreground_admission_direct_create() {
    check_admission(0);
}
#[test]
fn foreground_admission_direct_fade() {
    check_admission(1);
}
#[test]
fn foreground_admission_direct_affine() {
    check_admission(2);
}
#[test]
fn foreground_admission_parallel_create() {
    check_admission(3);
}
#[test]
fn foreground_admission_recursive_composition() {
    check_admission(4);
}
#[test]
fn foreground_admission_family_fade() {
    check_admission(5);
}
#[test]
fn foreground_admission_family_reveal() {
    check_admission(6);
}

#[test]
fn foreground_admission_preserves_pending_focus_positions_and_cleanup() {
    let mut scene = Scene::new();
    let front = scene.square(1.0).unwrap();
    let a = scene.square(2.0).unwrap();
    let b = scene.circle(0.7).unwrap();
    scene.add_foreground_many(&[(&front).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &Request::Composition {
                kind: Kind::Parallel,
                children: vec![
                    Request::Create {
                        target: &a,
                        options: options(),
                    },
                    Request::FocusOn {
                        focus: FocusOnOptions::new((0.0, 0.0)),
                        options: options(),
                    },
                    Request::Create {
                        target: &b,
                        options: options(),
                    },
                    Request::FocusOn {
                        focus: FocusOnOptions::new((1.0, 0.0)),
                        options: options(),
                    },
                ],
                options: options(),
            },
            options(),
        )
        .unwrap();
    let order = painter(&session);
    assert_eq!(order.len(), 5);
    assert_eq!([order[0], order[2], order[4]], [id(&a), id(&b), id(&front)]);
    assert_ne!(order[1], order[3]);
    for t in [0.0, 0.5, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, t)
            .unwrap();
        assert_eq!(painter(&session), order);
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(painter(&session), [id(&a), id(&b), id(&front)]);
    assert_eq!(foreground(&scene), [front.node_id()]);
}

#[test]
fn foreground_admission_rejection_preserves_membership_publication_and_dirty_state() {
    let mut scene = Scene::new();
    let front = scene.square(1.0).unwrap();
    let a = scene.square(2.0).unwrap();
    scene.add_foreground_many(&[(&front).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    let frame = session.frame().clone();
    session.take_frame_changes();
    let result = scene.live(&mut session).declare_and_activate_composition(
        &Request::Composition {
            kind: Kind::Parallel,
            children: vec![
                Request::FocusOn {
                    focus: FocusOnOptions::new((0.0, 0.0)),
                    options: options(),
                },
                Request::Create {
                    target: &a,
                    options: options().run_time(f64::NAN),
                },
            ],
            options: options(),
        },
        options(),
    );
    assert!(result.is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame(), &frame);
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(display(&scene), [front.node_id()]);
    assert_eq!(foreground(&scene), [front.node_id()]);
}
