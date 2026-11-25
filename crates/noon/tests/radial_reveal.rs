//! Radial authoring/lifecycle follows the canonical attachment and animation path.
use noon::{
    effects::{
        DissolveDirection as Direction, DissolveStyle, EffectUpdate, NoiseDissolve, RadialEdge,
        RadialMask,
    },
    integration::scene_execution::{self, ExecutionSession},
    AnimationCompositionRequest as Request, AnimationOptions, MobjectTarget, RateFunction, Scene,
};
use noon_core::{SemanticMutationTransaction, SemanticNodeId};

fn timing() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn mask(session: &ExecutionSession, owner: SemanticNodeId) -> RadialMask {
    let id = session.execution_object_id(owner).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
        .effects
        .as_ref()
        .unwrap()
        .visibility()
        .unwrap()
        .definition
        .as_radial_mask()
        .unwrap()
}
fn request(
    target: MobjectTarget<'_>,
    direction: Direction,
    edge: Option<RadialEdge>,
    update: EffectUpdate,
) -> Request<'_> {
    Request::Dissolve {
        target,
        direction,
        style: DissolveStyle::radial(update, edge).unwrap(),
        options: timing().remover(false),
    }
}

#[test]
fn persistent_partial_edits_reuse_visibility_and_reject_other_kinds_atomically() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel
        .set_reveal_with_edge(
            EffectUpdate::default()
                .progress(0.3)
                .seed(7)
                .reveal_center([0.2, 0.7]),
            Some(RadialEdge::Burn),
        )
        .unwrap();
    let owner = panel.node_id();
    let id = scene
        .integration_store()
        .borrow()
        .visibility_effect(owner)
        .unwrap()
        .unwrap();
    for progress in [0.0, 0.6, 1.0, 0.25] {
        panel
            .set_reveal(EffectUpdate::default().progress(progress))
            .unwrap();
        let store = scene.integration_store().borrow();
        assert_eq!(store.visibility_effect(owner).unwrap(), Some(id));
        assert_eq!(store.node(owner).unwrap().effect_ids().len(), 1);
        let value = store
            .semantic_effect_state(id)
            .unwrap()
            .definition()
            .as_radial_mask()
            .unwrap();
        assert_eq!(
            (
                value.progress(),
                value.seed(),
                value.center(),
                value.rim_width()
            ),
            (progress, 7, [0.2, 0.7], RadialMask::burn().rim_width())
        );
    }
    let before = scene.revision();
    assert!(panel
        .set_reveal(EffectUpdate::default().reveal_axes([0.0, 0.5]))
        .is_err());
    assert_eq!(scene.revision(), before);
    panel
        .set_reveal_with_edge(
            EffectUpdate::default().reveal_rim_width(0.05),
            Some(RadialEdge::Clean),
        )
        .unwrap();
    let value = scene
        .integration_store()
        .borrow()
        .semantic_effect_state(id)
        .unwrap()
        .definition()
        .as_radial_mask()
        .unwrap();
    assert_eq!(
        (
            value.roughness(),
            value.rim_width(),
            value.seed(),
            value.center()
        ),
        (0.0, 0.05, 7, [0.2, 0.7])
    );
    panel.remove_reveal().unwrap();
    panel.remove_reveal().unwrap();
    assert!(scene
        .integration_store()
        .borrow()
        .semantic_effect_state(id)
        .is_err());
    let mut tx = SemanticMutationTransaction::new();
    tx.create_visibility_effect(owner, NoiseDissolve::default());
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let before = scene.revision();
    assert!(panel.set_reveal(EffectUpdate::default()).is_err());
    assert!(panel.remove_reveal().is_err());
    assert_eq!(scene.revision(), before);
}

#[test]
fn in_out_in_preserves_one_generation_and_authored_edge_for_objects_and_scopes() {
    for composed in [false, true] {
        let mut scene = Scene::new();
        let left = scene.rectangle(2.0, 1.0).unwrap();
        let right = scene.circle(0.4).unwrap();
        let family = composed.then(|| scene.family(&[(&left).into(), (&right).into()]).unwrap());
        let target = family
            .as_ref()
            .map_or_else(|| (&left).into(), MobjectTarget::from);
        let owner = family
            .as_ref()
            .map_or(left.node_id(), |family| family.node_id());
        let mut session = scene_execution::lower(&scene).unwrap();
        let mut identity = None;
        for direction in [Direction::In, Direction::Out, Direction::In, Direction::Out] {
            let edge = (direction == Direction::Out).then_some(RadialEdge::Burn);
            let update = if edge.is_some() {
                EffectUpdate::default().seed(7)
            } else {
                EffectUpdate::default()
            };
            let segment = scene_execution::borrow_uninstalled(&scene, &mut session)
                .declare_and_activate_composition(
                    &request(target, direction, edge, update),
                    AnimationOptions::new(),
                )
                .unwrap();
            let id = scene
                .integration_store()
                .borrow()
                .visibility_effect(owner)
                .unwrap()
                .unwrap();
            assert_eq!(*identity.get_or_insert(id), id);
            for fraction in [0.25, 0.75, 0.25, 1.0] {
                session.seek(segment.start_time() + fraction).unwrap();
                let value = mask(&session, owner);
                assert_eq!(
                    value.progress(),
                    if direction == Direction::In {
                        1.0 - fraction
                    } else {
                        fraction
                    }
                );
                assert_eq!(value.hole(), direction == Direction::Out);
            }
            scene_execution::borrow_uninstalled(&scene, &mut session)
                .complete_segment(segment)
                .unwrap();
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .visibility_effect(owner)
                    .unwrap(),
                Some(id)
            );
            if session.frame().time > 1.0 {
                assert_eq!(mask(&session, owner).seed(), 7);
                assert_eq!(
                    mask(&session, owner).rim_width(),
                    RadialMask::burn().rim_width()
                );
            }
            if composed {
                for child in [&left, &right] {
                    assert!(
                        scene
                            .integration_store()
                            .borrow()
                            .node(child.node_id())
                            .unwrap()
                            .effect_ids()
                            .is_empty(),
                        "one scope mask, no per-child copies"
                    );
                }
            }
        }
    }
}

#[test]
fn sequence_flips_hole_only_at_equivalent_endpoint_and_rejects_partial_flip() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    let owner = panel.node_id();
    let mut session = scene_execution::lower(&scene).unwrap();
    let sequence = Request::Composition {
        kind: noon::SemanticAnimationCompositionKind::Sequence,
        children: vec![
            request(
                (&panel).into(),
                Direction::In,
                None,
                EffectUpdate::default(),
            ),
            request(
                (&panel).into(),
                Direction::Out,
                Some(RadialEdge::Burn),
                EffectUpdate::default().seed(7),
            ),
        ],
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
    };
    let segment = scene_execution::borrow_uninstalled(&scene, &mut session)
        .declare_and_activate_composition(&sequence, AnimationOptions::new())
        .unwrap();
    for (time, progress, hole) in [
        (0.5, 0.5, false),
        (1.5, 0.5, true),
        (0.25, 0.75, false),
        (2.0, 1.0, true),
    ] {
        session.seek(time).unwrap();
        let value = mask(&session, owner);
        assert_eq!((value.progress(), value.hole()), (progress, hole));
    }
    scene_execution::borrow_uninstalled(&scene, &mut session)
        .complete_segment(segment)
        .unwrap();
    // Timed direction reversal at a partially removed mask would teleport coverage.
    panel
        .set_reveal(EffectUpdate::default().progress(0.5).reveal_hole(true))
        .unwrap();
    let before = scene.revision();
    assert!(DissolveStyle::radial(EffectUpdate::default(), None)
        .unwrap()
        .resolve_definition(
            &RadialMask::new(EffectUpdate::default().progress(0.5).reveal_hole(true))
                .unwrap()
                .into(),
            Direction::In
        )
        .is_err());
    assert_eq!(scene.revision(), before);
}

#[test]
fn named_radial_values_use_normal_partial_target_animation_and_discrete_validation() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel.add_effect(RadialMask::burn(), "iris").unwrap();
    let attachment = panel.get_effect("iris").unwrap();
    scene.add(&panel).unwrap();
    let mut target = panel.target_editor().unwrap();
    target
        .set_effect(
            "iris",
            EffectUpdate::default()
                .progress(0.75)
                .reveal_center([0.2, 0.7])
                .reveal_rim_width(0.3),
        )
        .unwrap();
    let mut execution = scene.into_execution().unwrap();
    let segment = execution
        .live()
        .declare_and_activate_composition(
            &Request::TransformTo(
                noon::TransformToRequest::new(&panel, &target, timing()).method_target(),
            ),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [0.25, 0.75, 0.25, 1.0] {
        execution.seek(time).unwrap();
        let value = execution
            .scene()
            .effective_effect(&attachment)
            .unwrap()
            .as_radial_mask()
            .unwrap();
        assert!((value.progress() - 0.75 * time).abs() < 1e-12);
        assert!((value.center()[0] - (0.5 - 0.3 * time)).abs() < 1e-12);
        assert_eq!(value.roughness(), RadialMask::burn().roughness());
    }
    execution.live().complete_segment(segment).unwrap();
    let mut invalid = panel.target_editor().unwrap();
    invalid
        .set_effect("iris", EffectUpdate::default().reveal_hole(true))
        .unwrap();
    let frame = execution.session().frame().clone();
    assert!(execution
        .live()
        .declare_and_activate_composition(
            &Request::TransformTo(
                noon::TransformToRequest::new(&panel, &invalid, timing()).method_target()
            ),
            AnimationOptions::new()
        )
        .is_err());
    assert_eq!(execution.session().frame(), &frame);
}

#[test]
fn canonical_set_reveal_target_animates_only_explicit_fields_alongside_movement() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel
        .set_reveal_with_edge(
            EffectUpdate::default().progress(0.25).seed(7),
            Some(RadialEdge::Burn),
        )
        .unwrap();
    scene.add(&panel).unwrap();
    let owner = panel.node_id();
    let id = scene
        .integration_store()
        .borrow()
        .visibility_effect(owner)
        .unwrap()
        .unwrap();
    let mut target = panel.target_editor().unwrap();
    target.shift(0.5, 0.0).unwrap();
    target
        .set_reveal(EffectUpdate::default().progress(0.75).softness(0.03))
        .unwrap();
    let animation = scene
        .declare_transform_to(&panel, &target, timing())
        .unwrap();
    let mut session = scene_execution::lower(&scene).unwrap();
    let segment = scene_execution::borrow_uninstalled(&scene, &mut session)
        .play_animation(&animation)
        .unwrap();
    for time in [0.5, 0.25, 1.0] {
        session.seek(time).unwrap();
        let value = mask(&session, owner);
        assert_eq!(value.progress(), 0.25 + 0.5 * time);
        assert!((value.softness() - (0.025 + 0.005 * time)).abs() < 1e-12);
        assert_eq!(
            (value.seed(), value.rim_width()),
            (7, RadialMask::burn().rim_width())
        );
    }
    scene_execution::borrow_uninstalled(&scene, &mut session)
        .complete_segment(segment)
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .visibility_effect(owner)
            .unwrap(),
        Some(id)
    );
    assert_eq!(mask(&session, owner).progress(), 0.75);
}

#[test]
fn paired_owned_rust_programs_reach_exact_boundaries_with_mixed_motion_and_fade() {
    use noon::{LiveProgramStatus, RustHostCallbackTable};
    for (mut program, duration, owners) in [
        (
            noon::example_scenes::radial_reveal::program().unwrap(),
            3.75,
            1,
        ),
        (
            noon::example_scenes::radial_reveal_composed::program().unwrap(),
            2.5,
            2,
        ),
    ] {
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        for (time, progress) in [
            (0.5, 0.5),
            (1.75, 0.5),
            (duration, if duration == 3.75 { 0.0 } else { 1.0 }),
        ] {
            for _ in 0..32 {
                if program.status() == LiveProgramStatus::Finished {
                    break;
                }
                match program.drive_to(&mut callbacks, time).unwrap() {
                    LiveProgramStatus::PublicationPending(_) => {
                        let context = program.take_renderer_publication().context();
                        program.admit_publication(context).unwrap();
                        program.resume().unwrap();
                    }
                    LiveProgramStatus::ReadyToResume => {
                        program.resume().unwrap();
                    }
                    LiveProgramStatus::Awaiting(_) | LiveProgramStatus::Finished => break,
                    other => panic!("unexpected radial program state {other:?}"),
                }
            }
            let masks: Vec<_> = program
                .session()
                .frame()
                .objects
                .iter()
                .filter_map(|row| {
                    row.effects
                        .as_ref()?
                        .visibility()?
                        .definition
                        .as_radial_mask()
                })
                .collect();
            assert_eq!(masks.len(), owners);
            for mask in masks {
                assert_eq!(mask.progress(), progress);
            }
        }
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, duration);
    }
}

#[test]
fn explicit_same_value_visibility_writes_conflict_and_saved_copy_restores_fresh_binding() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel
        .set_reveal_with_edge(
            EffectUpdate::default().progress(0.25).seed(7),
            Some(RadialEdge::Burn),
        )
        .unwrap();
    let owner = panel.node_id();
    let old = scene
        .integration_store()
        .borrow()
        .visibility_effect(owner)
        .unwrap()
        .unwrap();
    let saved = panel.target_editor().unwrap();
    let mut a = panel.target_editor().unwrap();
    let mut b = panel.target_editor().unwrap();
    a.set_reveal(EffectUpdate::default().progress(0.75))
        .unwrap();
    b.set_reveal(EffectUpdate::default().progress(0.25))
        .unwrap();
    scene.add(&panel).unwrap();
    let mut session = scene_execution::lower(&scene).unwrap();
    let frame = session.frame().clone();
    let revision = scene.revision();
    let request = Request::Composition {
        kind: noon::SemanticAnimationCompositionKind::Parallel,
        children: vec![
            Request::TransformTo(
                noon::TransformToRequest::new(&panel, &a, timing()).method_target(),
            ),
            Request::TransformTo(
                noon::TransformToRequest::new(&panel, &b, timing()).method_target(),
            ),
        ],
        options: AnimationOptions::new(),
    };
    assert!(scene_execution::borrow_uninstalled(&scene, &mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(session.frame(), &frame);
    assert_eq!(scene.revision(), revision);
    panel
        .set_reveal(
            EffectUpdate::default()
                .progress(1.0)
                .reveal_center([0.2, 0.7]),
        )
        .unwrap();
    panel.become_handle(&saved, Default::default()).unwrap();
    let store = scene.integration_store().borrow();
    let restored = store.visibility_effect(owner).unwrap().unwrap();
    assert_ne!(restored, old);
    assert!(store.semantic_effect_state(old).is_err());
    let definition = store
        .semantic_effect_state(restored)
        .unwrap()
        .definition()
        .as_radial_mask()
        .unwrap();
    assert_eq!(
        (
            definition.progress(),
            definition.center(),
            definition.seed()
        ),
        (0.25, [0.5, 0.5], 7)
    );
    let saved_id = store.visibility_effect(saved.node_id()).unwrap().unwrap();
    assert_ne!(restored, saved_id);
    assert_eq!(
        store
            .semantic_effect_state(saved_id)
            .unwrap()
            .definition()
            .as_radial_mask(),
        Some(definition)
    );
}
