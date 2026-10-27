//! Visibility enrollment and lifecycle through the shared animation pipeline.
//! Uses the ordinary Scene transaction, lowering and runtime publication path.
use noon::{
    effects::{EffectUpdate, Glow, NoiseDissolve, Pixels},
    ExecutionSession, Scene,
};
use noon_core::{SemanticMutationTransaction, SemanticNodeId};

fn row(session: &ExecutionSession, owner: SemanticNodeId) -> &noon_runtime::FrameObjectState {
    let object = session.execution_object_id(owner).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap()
}

fn source_plan(
    session: &ExecutionSession,
    owner: SemanticNodeId,
) -> Option<&std::sync::Arc<noon_core::ComposedSource>> {
    match &row(session, owner).content {
        noon_core::ObjectContentRef::Composed(source) => Some(source),
        _ => None,
    }
}

fn linear() -> noon::AnimationOptions {
    noon::AnimationOptions::new()
        .run_time(1.0)
        .rate_func(noon::RateFunction::Linear)
}

fn mask(session: &ExecutionSession, owner: SemanticNodeId) -> NoiseDissolve {
    row(session, owner)
        .effects
        .as_ref()
        .unwrap()
        .definitions()
        .last()
        .unwrap()
        .as_noise_dissolve()
        .unwrap()
}

#[test]
fn dissolve_kept_out_then_in_reuses_one_visibility_and_releases_neutral_endpoint() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for composed in [false, true] {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        let family = composed.then(|| scene.family(&[(&source).into()]).unwrap());
        let target: noon::MobjectTarget<'_> = if composed {
            family.as_ref().unwrap().into()
        } else {
            (&source).into()
        };
        let owner = if composed {
            family.as_ref().unwrap().node_id()
        } else {
            source.node_id()
        };
        scene.add_many(&[target]).unwrap();
        let mut session = scene.execution_session().unwrap();
        for cycle in 0..3 {
            let out = noon::AnimationCompositionRequest::Dissolve {
                target,
                direction: Direction::Out,
                style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2)).unwrap(),
                options: linear().remover(false),
            };
            let segment = scene
                .live(&mut session)
                .declare_and_activate_composition(&out, noon::AnimationOptions::new())
                .unwrap();
            let id = scene
                .integration_store()
                .borrow()
                .visibility_effect(owner)
                .unwrap()
                .unwrap();
            assert_eq!(mask(&session, owner).progress(), 0.0, "cycle {cycle}");
            for alpha in [0.5, 0.25, 0.5, 1.0] {
                session.seek(segment.start_time() + alpha).unwrap();
                assert_eq!(mask(&session, owner).progress(), alpha);
                assert_eq!(mask(&session, owner).seed(), 7);
            }
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .visibility_effect(owner)
                    .unwrap(),
                Some(id)
            );
            assert_eq!(mask(&session, owner).progress(), 1.0);
            let input = noon::AnimationCompositionRequest::Dissolve {
                target,
                direction: Direction::In,
                style: DissolveStyle::default(),
                options: linear(),
            };
            let segment = scene
                .live(&mut session)
                .declare_and_activate_composition(&input, noon::AnimationOptions::new())
                .unwrap();
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .visibility_effect(owner)
                    .unwrap(),
                Some(id)
            );
            session
                .advance_segment_to(segment, segment.start_time() + 0.5)
                .unwrap();
            assert_eq!(mask(&session, owner).progress(), 0.5);
            assert_eq!(mask(&session, owner).softness(), 0.2);
            session
                .advance_segment_to(segment, segment.end_time())
                .unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .visibility_effect(owner)
                    .unwrap(),
                None
            );
        }
    }
}

#[test]
fn dissolve_default_removes_scope_and_can_introduce_it_again() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for composed in [false, true] {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        let family = composed.then(|| scene.family(&[(&source).into()]).unwrap());
        let target: noon::MobjectTarget<'_> = if composed {
            family.as_ref().unwrap().into()
        } else {
            (&source).into()
        };
        let owner = if composed {
            family.as_ref().unwrap().node_id()
        } else {
            source.node_id()
        };
        scene.add_many(&[target]).unwrap();
        let mut session = scene.execution_session().unwrap();
        for direction in [Direction::Out, Direction::In] {
            let request = noon::AnimationCompositionRequest::Dissolve {
                target,
                direction,
                style: DissolveStyle::default(),
                options: linear(),
            };
            let segment = scene
                .live(&mut session)
                .declare_and_activate_composition(&request, noon::AnimationOptions::new())
                .unwrap();
            session
                .advance_segment_to(segment, segment.end_time())
                .unwrap();
            assert_eq!(
                mask(&session, owner).progress(),
                if direction == Direction::Out {
                    1.0
                } else {
                    0.0
                }
            );
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .visibility_effect(owner)
                    .unwrap(),
                None
            );
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .node(owner)
                    .unwrap()
                    .parents()
                    .is_empty(),
                direction == Direction::Out
            );
            let id = session.execution_object_id(source.node_id()).unwrap();
            let index = session
                .frame()
                .objects
                .iter()
                .position(|row| row.id == id)
                .unwrap();
            assert_eq!(
                session.frame().is_present(index),
                direction == Direction::In
            );
        }
    }
}

#[test]
fn dissolve_sequence_captures_prior_progress_and_holds_each_seed_without_interpolation() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for composed in [false, true] {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        let family = composed.then(|| scene.family(&[(&source).into()]).unwrap());
        let target: noon::MobjectTarget<'_> = family
            .as_ref()
            .map_or_else(|| (&source).into(), |family| family.into());
        let owner = family
            .as_ref()
            .map_or(source.node_id(), |family| family.node_id());
        scene.add_many(&[target]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let request = noon::AnimationCompositionRequest::Composition {
            kind: noon_core::SemanticAnimationCompositionKind::Sequence,
            children: vec![
                noon::AnimationCompositionRequest::Dissolve {
                    target,
                    direction: Direction::Out,
                    style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.1))
                        .unwrap(),
                    options: linear().remover(false),
                },
                noon::AnimationCompositionRequest::Dissolve {
                    target,
                    direction: Direction::In,
                    style: DissolveStyle::new(EffectUpdate::default().seed(u32::MAX).softness(0.4))
                        .unwrap(),
                    options: linear(),
                },
            ],
            options: noon::AnimationOptions::new().rate_func(noon::RateFunction::Linear),
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, noon::AnimationOptions::new())
            .unwrap();
        assert_eq!(segment.end_time() - segment.start_time(), 2.0);
        for (time, progress, seed, softness) in [
            (0.5, 0.5, 7, 0.1),
            (1.5, 0.5, u32::MAX, 0.4),
            (0.25, 0.25, 7, 0.1),
            (2.0, 0.0, u32::MAX, 0.4),
        ] {
            session.seek(time).unwrap();
            assert_eq!(
                mask(&session, owner),
                NoiseDissolve::new(seed, softness)
                    .unwrap()
                    .with_progress(progress)
                    .unwrap()
            );
        }
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert!(scene
            .integration_store()
            .borrow()
            .visibility_effect(owner)
            .unwrap()
            .is_none());
    }
}

#[test]
fn dissolve_composes_with_copied_motion_in_either_declaration_order() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for dissolve_first in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        source
            .add_effect(
                Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .source(noon_core::GlowSource::Silhouette),
                )
                .unwrap(),
                "halo",
            )
            .unwrap();
        let halo = source.get_effect("halo").unwrap().node_id();
        let mut destination = source.target_editor().unwrap();
        destination.shift(2.0, 0.0).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        let motion = noon::AnimationCompositionRequest::TransformTo(
            noon::TransformToRequest::new(&source, &destination, linear()).method_target(),
        );
        let dissolve = noon::AnimationCompositionRequest::Dissolve {
            target: (&source).into(),
            direction: Direction::Out,
            style: DissolveStyle::default(),
            options: linear().remover(false),
        };
        let request = noon::AnimationCompositionRequest::Composition {
            kind: noon_core::SemanticAnimationCompositionKind::Parallel,
            children: if dissolve_first {
                vec![dissolve, motion]
            } else {
                vec![motion, dissolve]
            },
            options: noon::AnimationOptions::new(),
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, noon::AnimationOptions::new())
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(mask(&session, source.node_id()).progress(), 0.5);
        assert_eq!(row(&session, source.node_id()).transform.translation.x, 1.0);
        assert_eq!(
            row(&session, source.node_id())
                .effects
                .as_ref()
                .unwrap()
                .attachments()
                .first(),
            Some(&halo)
        );
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(row(&session, source.node_id()).transform.translation.x, 2.0);
        assert_eq!(mask(&session, source.node_id()).progress(), 1.0);
    }
}

#[test]
fn dissolve_play_lifecycle_overrides_are_preflighted_before_nested_membership_changes() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    let family = scene.family(&[(&source).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let request = noon::AnimationCompositionRequest::Dissolve {
        target: (&source).into(),
        direction: Direction::Out,
        style: DissolveStyle::default(),
        options: linear(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, noon::AnimationOptions::new().remover(false))
        .unwrap();
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(mask(&session, source.node_id()).progress(), 1.0);
    let before = session.frame().clone();
    let context = session.publication_context();
    let request = noon::AnimationCompositionRequest::Dissolve {
        target: (&source).into(),
        direction: Direction::Out,
        style: DissolveStyle::default(),
        options: linear().remover(false),
    };
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(&request, noon::AnimationOptions::new().remover(true))
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(source.node_id())
            .unwrap()
            .parents(),
        &[family.node_id()]
    );
}

#[test]
fn dissolve_and_pulse_enroll_distinct_channels_in_one_atomic_stack() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let request = noon::AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Parallel,
        children: vec![
            noon::AnimationCompositionRequest::GlowPulse {
                target: &source,
                intensity: 2.0,
                options: noon::AnimationOptions::new().run_time(1.0),
            },
            noon::AnimationCompositionRequest::Dissolve {
                target: (&source).into(),
                direction: Direction::Out,
                style: DissolveStyle::default(),
                options: linear().remover(false),
            },
        ],
        options: noon::AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, noon::AnimationOptions::new())
        .unwrap();
    session.seek(0.5).unwrap();
    assert_eq!(mask(&session, source.node_id()).progress(), 0.5);
    assert_eq!(
        row(&session, source.node_id())
            .effects
            .as_ref()
            .unwrap()
            .definitions()[0]
            .as_glow()
            .unwrap()
            .intensity(),
        2.0
    );
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert!(source.get_effect("glow").is_err());
    assert_eq!(
        row(&session, source.node_id())
            .effects
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(mask(&session, source.node_id()).progress(), 1.0);
}

#[test]
fn returning_dissolve_completion_releases_the_actual_composed_endpoint() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for returning_leaf in [false, true] {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        let request = noon::AnimationCompositionRequest::Composition {
            kind: noon_core::SemanticAnimationCompositionKind::Parallel,
            children: vec![noon::AnimationCompositionRequest::Dissolve {
                target: (&source).into(),
                direction: Direction::Out,
                style: DissolveStyle::default(),
                options: linear().remover(false).rate_func(if returning_leaf {
                    noon::RateFunction::ThereAndBack
                } else {
                    noon::RateFunction::Linear
                }),
            }],
            options: noon::AnimationOptions::new().rate_func(if returning_leaf {
                noon::RateFunction::Linear
            } else {
                noon::RateFunction::ThereAndBack
            }),
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, noon::AnimationOptions::new())
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(mask(&session, source.node_id()).progress(), 1.0);
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        // Common exact-root finish settles children to their own terminal rate,
        // independently of a returning parent composition map.
        assert_eq!(
            mask(&session, source.node_id()).progress(),
            if returning_leaf { 0.0 } else { 1.0 }
        );
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .visibility_effect(source.node_id())
                .unwrap()
                .is_none(),
            returning_leaf
        );
        assert_eq!(
            row(&session, source.node_id()).effects.is_none(),
            returning_leaf
        );
    }
}

#[test]
fn dissolve_rejects_capacity_conflict_and_unsupported_options_without_publication() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    for failure in 0..4 {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        if failure == 0 {
            for index in 0..noon_core::MAX_EFFECT_STACK_LEN {
                source
                    .add_effect(noon::effects::Blur::default(), &format!("blur-{index}"))
                    .unwrap();
            }
        }
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_renderer_publication();
        let before = session.frame().clone();
        let context = session.publication_context();
        let nodes = scene.integration_store().borrow().len();
        let leaf = noon::AnimationCompositionRequest::Dissolve {
            target: (&source).into(),
            direction: Direction::Out,
            style: DissolveStyle::default(),
            options: match failure {
                2 => linear().path_arc(1.0),
                3 => linear().lag_ratio(0.5),
                _ => linear(),
            }
            .remover(false),
        };
        let request = if failure == 1 {
            noon::AnimationCompositionRequest::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![leaf.clone(), leaf],
                options: noon::AnimationOptions::new(),
            }
        } else {
            leaf
        };
        assert!(
            scene
                .live(&mut session)
                .declare_and_activate_composition(&request, noon::AnimationOptions::new())
                .is_err(),
            "failure {failure}"
        );
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert!(session.take_renderer_publication().changes().is_empty());
        assert!(scene
            .integration_store()
            .borrow()
            .visibility_effect(source.node_id())
            .unwrap()
            .is_none());
    }
}

#[test]
fn declared_dissolve_is_inert_replayable_and_enrolls_at_activation() {
    use noon::effects::{DissolveDirection as Direction, DissolveStyle};
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    let input = scene
        .declare_dissolve(&source, Direction::In, DissolveStyle::default(), linear())
        .unwrap();
    let output = scene
        .declare_dissolve(&source, Direction::Out, DissolveStyle::default(), linear())
        .unwrap();
    let nodes = scene.integration_store().borrow().len();
    assert!(scene
        .integration_store()
        .borrow()
        .visibility_effect(source.node_id())
        .unwrap()
        .is_none());
    assert!(scene
        .integration_store()
        .borrow()
        .node(source.node_id())
        .unwrap()
        .parents()
        .is_empty());
    let mut session = scene.execution_session().unwrap();
    for _ in 0..3 {
        for declaration in [&input, &output] {
            let segment = scene
                .live(&mut session)
                .play_animation(declaration)
                .unwrap();
            session
                .advance_segment_to(segment, segment.end_time())
                .unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(scene.integration_store().borrow().len(), nodes);
        }
    }
}

#[test]
fn visibility_publishes_last_for_objects_and_composed_families_without_replacing_sources() {
    for composed in [false, true] {
        let mut scene = Scene::new();
        let leaf = scene.circle(0.4).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        let unrelated = scene.circle(0.2).unwrap();
        if composed {
            scene.add_many(&[(&family).into()]).unwrap();
        } else {
            scene.add(&leaf).unwrap();
        }
        scene.add(&unrelated).unwrap();
        let owner = if composed {
            family.node_id()
        } else {
            leaf.node_id()
        };
        let target = || -> noon::MobjectTarget<'_> {
            if composed {
                (&family).into()
            } else {
                (&leaf).into()
            }
        };
        let mut session = scene.execution_session().unwrap();
        let leaf_execution = session.execution_object_id(leaf.node_id()).unwrap();
        let unrelated_before = row(&session, unrelated.node_id()).clone();
        let mut tx = SemanticMutationTransaction::new();
        scene
            .integration_store()
            .borrow()
            .stage_visibility_mask(
                owner,
                NoiseDissolve::new(7, 0.08)
                    .unwrap()
                    .with_progress(1.0)
                    .unwrap(),
                &mut tx,
            )
            .unwrap();
        scene.live(&mut session).apply(tx).unwrap();
        let visibility = scene
            .integration_store()
            .borrow()
            .visibility_effect(owner)
            .unwrap()
            .unwrap();
        let owner_execution = session.execution_object_id(owner).unwrap();
        let original_source = source_plan(&session, owner).map(std::sync::Arc::clone);
        assert_eq!(composed, original_source.is_some());
        scene
            .live(&mut session)
            .add_effect(
                target(),
                Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .source(noon_core::GlowSource::Silhouette),
                )
                .unwrap(),
                "visibility",
            )
            .unwrap();
        let authored = if composed {
            family.get_effect("visibility").unwrap()
        } else {
            leaf.get_effect("visibility").unwrap()
        };
        assert_ne!(authored.node_id(), visibility);
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().attachments(),
            &[authored.node_id(), visibility]
        );
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().definitions()[1]
                .as_noise_dissolve()
                .unwrap()
                .progress(),
            1.0
        );
        assert_eq!(session.execution_object_id(owner), Some(owner_execution));
        assert_eq!(
            session.execution_object_id(leaf.node_id()),
            Some(leaf_execution)
        );
        for seed in 0..16 {
            session.take_renderer_publication();
            let mut tx = SemanticMutationTransaction::new();
            let definition = NoiseDissolve::new(seed, 0.4)
                .unwrap()
                .with_progress(f64::from(seed % 2))
                .unwrap();
            scene
                .integration_store()
                .borrow()
                .stage_visibility_mask(owner, definition, &mut tx)
                .unwrap();
            scene.live(&mut session).apply(tx).unwrap();
            assert_eq!(
                session
                    .take_renderer_publication()
                    .changes()
                    .object_indices()
                    .len(),
                1
            );
            assert_eq!(
                session
                    .last_structural_publication_stats()
                    .preparation
                    .object_states_lowered,
                0
            );
            assert_eq!(
                row(&session, owner)
                    .effects
                    .as_ref()
                    .unwrap()
                    .get(visibility)
                    .unwrap()
                    .definition,
                definition.into()
            );
            assert_eq!(row(&session, unrelated.node_id()), &unrelated_before);
            if let Some(original) = &original_source {
                assert!(std::sync::Arc::ptr_eq(
                    original,
                    source_plan(&session, owner).unwrap()
                ));
            }
        }
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(visibility);
        scene.live(&mut session).apply(remove).unwrap();
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().attachments(),
            &[authored.node_id()]
        );
        let before = session.frame().clone();
        let context = session.publication_context();
        let revision = scene.revision();
        let mut stale = SemanticMutationTransaction::new();
        stale.update_effect(authored.node_id(), EffectUpdate::default().intensity(2.0));
        stale.update_effect(visibility, EffectUpdate::default().progress(0.5));
        assert!(scene.live(&mut session).apply(stale).is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
    }
}

#[test]
fn temporary_pulse_enrollment_preserves_existing_named_and_visibility_bindings() {
    for visibility_progress in [None, Some(0.0), Some(1.0)] {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        source
            .add_effect(
                noon::effects::Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
                "soften",
            )
            .unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        if let Some(progress) = visibility_progress {
            let mut tx = SemanticMutationTransaction::new();
            scene
                .integration_store()
                .borrow()
                .stage_visibility_mask(
                    source.node_id(),
                    NoiseDissolve::new(7, 0.08)
                        .unwrap()
                        .with_progress(progress)
                        .unwrap(),
                    &mut tx,
                )
                .unwrap();
            scene.live(&mut session).apply(tx).unwrap();
        }
        let original = row(&session, source.node_id())
            .effects
            .as_ref()
            .unwrap()
            .clone();
        let owner = session.execution_object_id(source.node_id()).unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(
                &noon::AnimationCompositionRequest::GlowPulse {
                    target: &source,
                    intensity: 2.0,
                    options: noon::AnimationOptions::new().run_time(1.0),
                },
                noon::AnimationOptions::new(),
            )
            .unwrap();
        let glow = source.get_effect("glow").unwrap();
        let active = row(&session, source.node_id()).effects.as_ref().unwrap();
        assert_eq!(active.len(), original.len() + 1);
        if visibility_progress.is_some() {
            assert_eq!(active.attachments().last(), original.attachments().last());
            assert_ne!(active.attachments().last(), Some(&glow.node_id()));
        }
        session.advance_segment_to(segment, 0.5).unwrap();
        assert_eq!(
            row(&session, source.node_id())
                .effects
                .as_ref()
                .unwrap()
                .get(glow.node_id())
                .unwrap()
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            2.0
        );
        for old in original.iter() {
            assert_eq!(
                row(&session, source.node_id())
                    .effects
                    .as_ref()
                    .unwrap()
                    .get(old.attachment),
                Some(old)
            );
        }
        session.advance_segment_to(segment, 1.0).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert!(source.get_effect("glow").is_err());
        assert_eq!(
            row(&session, source.node_id()).effects.as_ref().unwrap(),
            &original
        );
        assert_eq!(session.execution_object_id(source.node_id()), Some(owner));
    }
}

#[test]
fn pulse_enrollment_capacity_and_wrong_canonical_kind_reject_atomically() {
    for full in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        for index in 0..if full {
            noon_core::MAX_EFFECT_STACK_LEN
        } else {
            1
        } {
            let name = if full {
                format!("existing-{index}")
            } else {
                "glow".to_owned()
            };
            source
                .add_effect(noon::effects::Blur::default(), &name)
                .unwrap();
        }
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.frame().clone();
        let context = session.publication_context();
        let revision = scene.revision();
        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(
                &noon::AnimationCompositionRequest::GlowPulse {
                    target: &source,
                    intensity: 2.0,
                    options: noon::AnimationOptions::new().run_time(1.0)
                },
                noon::AnimationOptions::new(),
            )
            .is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
        if full {
            assert!(source.get_effect("glow").is_err());
        } else {
            assert!(source
                .get_effect("glow")
                .unwrap()
                .authored_definition()
                .unwrap()
                .as_blur()
                .is_some());
        }
        scene.live(&mut session).wait_segment(0.1).unwrap();
    }
}
