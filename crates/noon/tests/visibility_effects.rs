//! Visibility-role plumbing, before the high-level gesture is exposed.
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
