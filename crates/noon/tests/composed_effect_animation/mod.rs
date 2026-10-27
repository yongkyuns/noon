//! Public family-target animation, identity and atomicity qualification.
use super::*;

fn qualify_family_opacity_target(start: f64) {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    for method in [false, true] {
        let mut scene = Scene::new();
        let a = scene.circle(0.4).unwrap();
        let b = scene.square(0.5).unwrap();
        let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        source.set_object_opacity(start).unwrap();
        scene.add_many(&[(&source).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let target = scene.live(&mut session).copy_family(&source).unwrap();
        let end = 0.234567890123;
        scene
            .live(&mut session)
            .set_family_object_opacity(target.root(), end)
            .unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let request = if method {
            AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &source,
                target_state: target.root(),
                options,
            }
        } else {
            AnimationCompositionRequest::FamilyTransformTo {
                source: &source,
                target_state: target.root(),
                options,
            }
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        session.advance_segment_to(segment, 0.5).unwrap();
        let scope = row(&session, source.node_id())
            .expect("family opacity animation needs its real composed controller");
        assert!(
            (f64::from(scope.style.opacity) - (start + end) * 0.5).abs() < 1e-6,
            "start={start} method={method}: family opacity must use the shared root clock; got {}",
            scope.style.opacity
        );
        session.advance_segment_to(segment, 1.0).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(
            source.own_style().unwrap().object_opacity,
            end,
            "completion preserves exact authored opacity"
        );
    }
}

#[test]
fn family_opacity_target_interpolates_an_existing_scope() {
    qualify_family_opacity_target(0.5);
}

#[test]
fn family_opacity_target_admits_the_default_full_opacity_scope() {
    qualify_family_opacity_target(1.0);
}

#[test]
fn parallel_family_opacity_targets_reject_conflicting_drivers_before_scope_admission() {
    use noon::{
        AnimationCompositionRequest, AnimationOptions, RateFunction,
        SemanticAnimationCompositionKind,
    };
    for start in [1.0, 0.5] {
        let mut scene = Scene::new();
        let leaf = scene.circle(0.4).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        family.set_object_opacity(start).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let a = scene.live(&mut session).copy_family(&family).unwrap();
        let b = scene.live(&mut session).copy_family(&family).unwrap();
        scene
            .live(&mut session)
            .set_family_object_opacity(a.root(), 0.25)
            .unwrap();
        scene
            .live(&mut session)
            .set_family_object_opacity(b.root(), 0.75)
            .unwrap();
        let before = session.frame().clone();
        let context = session.publication_context();
        let revision = scene.revision();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let result = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[
                    AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &family,
                        target_state: a.root(),
                        options,
                    },
                    AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &family,
                        target_state: b.root(),
                        options,
                    },
                ],
                AnimationOptions::new(),
                AnimationOptions::new(),
            );
        assert!(result.is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
        assert_eq!(family.own_style().unwrap().object_opacity, start);
        assert_eq!(row(&session, family.node_id()).is_some(), start != 1.0);
    }
}

#[test]
fn family_opacity_uses_root_clock_with_member_lag_backward_seek_and_returning_completion() {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    for method in [false, true] {
        let mut scene = Scene::new();
        let a = scene.circle(0.4).unwrap();
        let b = scene.square(0.5).unwrap();
        let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        scene.add_many(&[(&source).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let target = scene.live(&mut session).copy_family(&source).unwrap();
        scene
            .live(&mut session)
            .set_family_object_opacity(target.root(), 0.25)
            .unwrap();
        scene
            .live(&mut session)
            .shift_family(target.root(), 2.0, 0.0)
            .unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let request = if method {
            AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &source,
                target_state: target.root(),
                options,
            }
        } else {
            AnimationCompositionRequest::FamilyTransformTo {
                source: &source,
                target_state: target.root(),
                options,
            }
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(
                &request,
                AnimationOptions::new()
                    .run_time(4.0)
                    .rate_func(RateFunction::ThereAndBack)
                    .lag_ratio(0.5),
            )
            .unwrap();
        assert_eq!(segment.end_time(), 4.0);
        for (time, opacity) in [(0.0, 1.0), (2.0, 0.25), (4.0, 1.0), (2.0, 0.25), (0.0, 1.0)] {
            session.seek(time).unwrap();
            assert_eq!(
                row(&session, source.node_id()).unwrap().style.opacity,
                opacity
            );
        }
        session.advance_segment_to(segment, 4.0).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(source.own_style().unwrap().object_opacity, 1.0);
        assert_eq!(row(&session, source.node_id()).unwrap().style.opacity, 1.0);
    }
}

#[test]
fn declared_opacity_scope_retires_when_its_last_declaration_is_cascade_removed() {
    use noon::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let leaf = scene.circle(0.4).unwrap();
    let source = scene.family(&[(&leaf).into()]).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let target = source.copy_family().unwrap();
    target.root().set_object_opacity(0.25).unwrap();
    let declaration = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::EffectTargetTo {
                target: source.node_id(),
                target_state: target.root().node_id(),
                method_target: true,
            },
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    assert_eq!(row(&session, source.node_id()).unwrap().style.opacity, 1.0);
    let segment = scene
        .live(&mut session)
        .play_animation(&declaration)
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(
        row(&session, source.node_id()).unwrap().style.opacity,
        0.625
    );
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene
        .live(&mut session)
        .set_family_object_opacity(&source, 1.0)
        .unwrap();
    assert!(
        row(&session, source.node_id()).is_some(),
        "live declaration retains the identity scope for replay"
    );
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(target.root().node_id());
    let before = scene.revision();
    scene.live(&mut session).apply(transaction).unwrap();
    assert_ne!(scene.revision(), before);
    assert!(
        declaration.options().is_err(),
        "the target dependency retires its declaration"
    );
    assert!(
        row(&session, source.node_id()).is_none(),
        "no declaration, opacity or effect still requires isolation"
    );
    assert!(row(&session, leaf.node_id()).is_some());
    assert_eq!(
        session.last_structural_publication_stats().exited_objects,
        1
    );
}

#[test]
fn family_effect_targets_interpolate_nested_columns_and_complete_exact_generations() {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    for nested in [false, true] {
        for method in [false, true] {
            let mut scene = Scene::new();
            let mut leaf = scene.circle(0.4).unwrap();
            leaf.add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
                "leaf",
            )
            .unwrap();
            let mut inner = scene.family(&[(&leaf).into()]).unwrap();
            inner
                .add_effect(
                    Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
                    "inner",
                )
                .unwrap();
            let mut family = if nested {
                scene.family(&[(&inner).into()]).unwrap()
            } else {
                inner.clone()
            };
            family
                .add_effect(
                    Blur::new(EffectUpdate::default().radius(Pixels(3.0))).unwrap(),
                    "root",
                )
                .unwrap();
            scene.add_many(&[(&family).into()]).unwrap();
            let leaf_effect = leaf.get_effect("leaf").unwrap();
            let inner_effect = inner.get_effect("inner").unwrap();
            let root_effect = family.get_effect("root").unwrap();
            let mut session = scene.execution_session().unwrap();
            let owner = session.execution_object_id(family.node_id()).unwrap();
            let source_plan = match &row(&session, family.node_id()).unwrap().content {
                noon_core::ObjectContentRef::Composed(source) => std::sync::Arc::clone(source),
                _ => panic!("expected composed source"),
            };
            let target = scene.live(&mut session).copy_family(&family).unwrap();
            let target_inner = target.family(&inner).unwrap();
            let target_leaf = target.mobject(&leaf).unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    target.root(),
                    "root",
                    EffectUpdate::default().radius(Pixels(7.0)),
                )
                .unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    &target_inner,
                    "inner",
                    EffectUpdate::default().radius(Pixels(6.0)),
                )
                .unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    &target_leaf,
                    "leaf",
                    EffectUpdate::default().radius(Pixels(5.0)),
                )
                .unwrap();
            scene
                .live(&mut session)
                .shift_family(target.root(), 2.0, 0.0)
                .unwrap();
            let options = AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear);
            let request = if method {
                AnimationCompositionRequest::FamilyMethodTargetTo {
                    source: &family,
                    target_state: target.root(),
                    options,
                }
            } else {
                AnimationCompositionRequest::FamilyTransformTo {
                    source: &family,
                    target_state: target.root(),
                    options,
                }
            };
            let segment = scene
                .live(&mut session)
                .declare_and_activate_composition(&request, AnimationOptions::new())
                .unwrap();
            session.advance_segment_to(segment, 0.5).unwrap();
            for (id, attachment, radius) in [
                (leaf.node_id(), leaf_effect.node_id(), 3.0),
                (inner.node_id(), inner_effect.node_id(), 4.0),
                (family.node_id(), root_effect.node_id(), 5.0),
            ] {
                assert_eq!(
                    row(&session, id)
                        .unwrap()
                        .effects
                        .as_ref()
                        .unwrap()
                        .get(attachment)
                        .unwrap()
                        .definition
                        .radius(),
                    Some(Pixels(radius).into())
                );
            }
            assert_eq!(
                row(&session, leaf.node_id())
                    .unwrap()
                    .transform
                    .translation
                    .x,
                1.0
            );
            assert_eq!(
                root_effect.authored_definition().unwrap().radius(),
                Some(Pixels(3.0).into())
            );
            match &row(&session, family.node_id()).unwrap().content {
                noon_core::ObjectContentRef::Composed(source) => {
                    assert!(std::sync::Arc::ptr_eq(&source_plan, source))
                }
                _ => panic!("expected composed source"),
            }
            session.advance_segment_to(segment, 1.0).unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                root_effect.authored_definition().unwrap().radius(),
                Some(Pixels(7.0).into())
            );
            assert_eq!(
                inner_effect.authored_definition().unwrap().radius(),
                Some(Pixels(6.0).into())
            );
            assert_eq!(
                leaf_effect.authored_definition().unwrap().radius(),
                Some(Pixels(5.0).into())
            );
            assert_eq!(session.execution_object_id(family.node_id()), Some(owner));
        }
    }
}

fn effect_radius(
    session: &ExecutionSession,
    owner: SemanticNodeId,
    attachment: SemanticNodeId,
) -> f64 {
    row(session, owner)
        .unwrap()
        .effects
        .as_ref()
        .unwrap()
        .get(attachment)
        .unwrap()
        .definition
        .radius()
        .unwrap()
        .value()
}

#[test]
fn family_scope_clock_preserves_play_overrides_member_lag_and_seek() {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.circle(0.3).unwrap();
    let mut family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    family
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let attachment = family.get_effect("soften").unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let target = scene.live(&mut session).copy_family(&family).unwrap();
    scene
        .live(&mut session)
        .shift_family(target.root(), 2.0, 0.0)
        .unwrap();
    scene
        .live(&mut session)
        .set_effect(
            target.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(6.0)),
        )
        .unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &family,
                target_state: target.root(),
                options: AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Smooth),
            },
            AnimationOptions::new()
                .run_time(4.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.5),
        )
        .unwrap();
    assert_eq!(segment.end_time(), 4.0);
    for (time, radius, ax, bx) in [
        (1.0, 3.0, 0.75, 0.0),
        (3.0, 5.0, 2.0, 1.25),
        (1.0, 3.0, 0.75, 0.0),
    ] {
        session.seek(time).unwrap();
        assert_eq!(
            effect_radius(&session, family.node_id(), attachment.node_id()),
            radius
        );
        assert_eq!(
            row(&session, a.node_id()).unwrap().transform.translation.x,
            ax
        );
        assert_eq!(
            row(&session, b.node_id()).unwrap().transform.translation.x,
            bx
        );
    }
    session.advance_segment_to(segment, 4.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        attachment.authored_definition().unwrap().radius(),
        Some(Pixels(6.0).into())
    );
}

#[test]
fn parallel_family_targets_reserve_exact_parameter_generations_and_rollback_conflicts() {
    use noon::{
        AnimationCompositionRequest, AnimationOptions, RateFunction,
        SemanticAnimationCompositionKind,
    };
    for conflict in [false, true] {
        let mut scene = Scene::new();
        let leaf = scene.circle(0.4).unwrap();
        let mut family = scene.family(&[(&leaf).into()]).unwrap();
        for (name, radius) in [("first", 1.0), ("second", 2.0)] {
            family
                .add_effect(
                    Blur::new(EffectUpdate::default().radius(Pixels(radius))).unwrap(),
                    name,
                )
                .unwrap();
        }
        scene.add_many(&[(&family).into()]).unwrap();
        let first = family.get_effect("first").unwrap();
        let second = family.get_effect("second").unwrap();
        let mut session = scene.execution_session().unwrap();
        let a = scene.live(&mut session).copy_family(&family).unwrap();
        let b = scene.live(&mut session).copy_family(&family).unwrap();
        scene
            .live(&mut session)
            .set_effect(
                a.root(),
                "first",
                EffectUpdate::default().radius(Pixels(5.0)),
            )
            .unwrap();
        // Same-valued explicit writes still compete for the exact lane.
        scene
            .live(&mut session)
            .set_effect(
                b.root(),
                if conflict { "first" } else { "second" },
                EffectUpdate::default().radius(Pixels(if conflict { 1.0 } else { 10.0 })),
            )
            .unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let before = session.frame().clone();
        let context = session.publication_context();
        let revision = scene.revision();
        let result = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[
                    AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &family,
                        target_state: a.root(),
                        options,
                    },
                    AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &family,
                        target_state: b.root(),
                        options,
                    },
                ],
                AnimationOptions::new(),
                AnimationOptions::new(),
            );
        if conflict {
            assert!(result.is_err());
            assert_eq!(session.frame(), &before);
            assert_eq!(session.publication_context(), context);
            assert_eq!(scene.revision(), revision);
        } else {
            let segment = result.unwrap();
            session.seek(0.5).unwrap();
            assert_eq!(
                effect_radius(&session, family.node_id(), first.node_id()),
                3.0
            );
            assert_eq!(
                effect_radius(&session, family.node_id(), second.node_id()),
                6.0
            );
            session.advance_segment_to(segment, 1.0).unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                first.authored_definition().unwrap().radius(),
                Some(Pixels(5.0).into())
            );
            assert_eq!(
                second.authored_definition().unwrap().radius(),
                Some(Pixels(10.0).into())
            );
        }
    }
}

#[test]
fn unrelated_effects_do_not_change_unequal_family_transform_admission() {
    use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let a = scene.circle(0.4).unwrap();
    let b = scene.circle(0.4).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let mut endpoint = scene.circle(0.4).unwrap();
    endpoint.set_translation(3.0, 0.0).unwrap();
    let target = scene.family(&[(&endpoint).into()]).unwrap();
    let mut unrelated = scene.circle(0.2).unwrap();
    unrelated
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "outside",
        )
        .unwrap();
    scene
        .add_many(&[(&family).into(), (&unrelated).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let effects = row(&session, unrelated.node_id()).unwrap().effects.clone();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyTransformTo {
                source: &family,
                target_state: &target,
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(
        row(&session, a.node_id()).unwrap().transform.translation.x,
        1.5
    );
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(row(&session, unrelated.node_id()).unwrap().effects, effects);
}

#[test]
fn sequential_family_targets_capture_prior_scope_endpoint_and_seek_deterministically() {
    use noon::{
        AnimationCompositionRequest, AnimationOptions, RateFunction,
        SemanticAnimationCompositionKind,
    };
    let mut scene = Scene::new();
    let leaf = scene.circle(0.4).unwrap();
    let mut family = scene.family(&[(&leaf).into()]).unwrap();
    family
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let attachment = family.get_effect("soften").unwrap();
    let mut session = scene.execution_session().unwrap();
    let first = scene.live(&mut session).copy_family(&family).unwrap();
    let last = scene.live(&mut session).copy_family(&family).unwrap();
    scene
        .live(&mut session)
        .set_effect(
            first.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(6.0)),
        )
        .unwrap();
    // This explicit return to the original value must consume the preceding
    // completed radius, rather than being treated as an unchanged copied target.
    scene
        .live(&mut session)
        .set_effect(
            last.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(2.0)),
        )
        .unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let request = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Sequence,
        children: vec![
            AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &family,
                target_state: first.root(),
                options,
            },
            AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &family,
                target_state: last.root(),
                options,
            },
        ],
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    assert_eq!(segment.end_time(), 2.0);
    for (time, radius) in [(0.5, 4.0), (1.0, 6.0), (1.5, 4.0), (0.5, 4.0), (2.0, 2.0)] {
        session.seek(time).unwrap();
        assert_eq!(
            effect_radius(&session, family.node_id(), attachment.node_id()),
            radius
        );
    }
    session.advance_segment_to(segment, 2.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        attachment.authored_definition().unwrap().radius(),
        Some(Pixels(2.0).into())
    );
}

#[test]
fn inert_family_effect_declaration_captures_effective_values_and_rejects_rebound_names() {
    use noon::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let leaf = scene.circle(0.4).unwrap();
    let mut family = scene.family(&[(&leaf).into()]).unwrap();
    family
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let target = family.copy_family().unwrap();
    target
        .root()
        .clone()
        .set_effect("soften", EffectUpdate::default().radius(Pixels(6.0)))
        .unwrap();
    let declaration = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::EffectTargetTo {
                target: family.node_id(),
                target_state: target.root().node_id(),
                method_target: true,
            },
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let attachment = family.get_effect("soften").unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .set_effect(
            &family,
            &attachment,
            EffectUpdate::default().radius(Pixels(4.0)),
        )
        .unwrap();
    let segment = scene
        .live(&mut session)
        .play_animation(&declaration)
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(
        effect_radius(&session, family.node_id(), attachment.node_id()),
        5.0
    );
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene
        .live(&mut session)
        .remove_effect(&family, &attachment)
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(
            &family,
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let replacement = family.get_effect("soften").unwrap();
    assert_ne!(replacement.node_id(), attachment.node_id());
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .play_animation(&declaration)
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
}
