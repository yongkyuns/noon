//! Coherent effective capture and failure atomicity through LiveSession.
use super::*;
use noon::{AnimationCompositionRequest, AnimationOptions, RateFunction};

#[test]
fn live_family_restore_is_one_local_publication_and_pending_failure_preserves_handles() {
    let mut scene = Scene::new();
    let a = scene.square(0.5).unwrap();
    let b = scene.circle(0.3).unwrap();
    let mut source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    source.set_object_opacity(0.5).unwrap();
    let saved = source.copy_family().unwrap();
    scene
        .set_effect(
            saved.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(4.0)),
        )
        .unwrap();
    saved.root().set_object_opacity(0.25).unwrap();
    let unrelated = scene.circle(0.1).unwrap();
    scene
        .add_many(&[(&source).into(), (&unrelated).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    let owner = session.execution_object_id(source.node_id()).unwrap();
    let unrelated_id = session.execution_object_id(unrelated.node_id()).unwrap();
    let old = source.get_effect("soften").unwrap();
    let before_unrelated = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == unrelated_id)
        .unwrap()
        .clone();
    let revision = scene.revision();

    scene
        .live(&mut session)
        .become_family(&source, saved.root(), Default::default())
        .unwrap();

    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(session.execution_object_id(source.node_id()), Some(owner));
    assert!(old.authored_definition().is_err());
    let new = source.get_effect("soften").unwrap();
    assert_ne!(old.node_id(), new.node_id());
    let row = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap();
    assert_eq!(row.style.opacity, 0.25);
    assert_eq!(
        row.effects.as_ref().unwrap().attachments(),
        &[new.node_id()]
    );
    assert_eq!(
        new.authored_definition().unwrap().radius().unwrap().value(),
        4.0
    );
    assert_eq!(
        session
            .frame()
            .objects
            .iter()
            .find(|row| row.id == unrelated_id)
            .unwrap(),
        &before_unrelated
    );
    let unrelated_index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == unrelated_id)
        .unwrap();
    assert!(!session
        .take_renderer_publication()
        .changes()
        .object_indices()
        .contains(&unrelated_index));

    let target = scene.live(&mut session).copy_family(&source).unwrap();
    scene
        .live(&mut session)
        .set_family_object_opacity(target.root(), 0.75)
        .unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &source,
                target_state: target.root(),
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    let context = session.publication_context();
    let revision = scene.revision();
    let frame = session.frame().clone();
    let bindings = snapshot(&scene, source.node_id());
    assert!(scene
        .live(&mut session)
        .become_family(&source, saved.root(), Default::default())
        .is_err());
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
    assert_eq!(session.frame(), &frame);
    assert_eq!(snapshot(&scene, source.node_id()), bindings);
    assert!(new.authored_definition().is_ok());
}

#[test]
fn live_family_become_captures_effective_opacity_and_effects_for_both_topology_paths() {
    for receiver_count in [1, 2] {
        let mut scene = Scene::new();
        let leaf = scene.square(0.5).unwrap();
        let mut driven = scene.family(&[(&leaf).into()]).unwrap();
        driven
            .add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
                "soften",
            )
            .unwrap();
        driven.set_object_opacity(0.25).unwrap();
        let endpoint = driven.copy_family().unwrap();
        endpoint.root().set_object_opacity(0.75).unwrap();
        scene
            .set_effect(
                endpoint.root(),
                "soften",
                EffectUpdate::default().radius(Pixels(6.0)),
            )
            .unwrap();
        let x = scene.circle(0.1).unwrap();
        let y = scene.circle(0.2).unwrap();
        let receiver = scene
            .family(&[(&x).into(), (&y).into()][..receiver_count])
            .unwrap();
        scene
            .add_many(&[(&driven).into(), (&receiver).into()])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_family_transform_to(
                &driven,
                endpoint.root(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        session.advance_segment_to(segment, 1.0).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        session.seek(0.5).unwrap();
        let target_effect = driven.get_effect("soften").unwrap();

        scene
            .live(&mut session)
            .become_family(&receiver, &driven, Default::default())
            .unwrap();

        assert_eq!(receiver.own_style().unwrap().object_opacity, 0.5);
        assert_eq!(
            receiver
                .get_effect("soften")
                .unwrap()
                .authored_definition()
                .unwrap()
                .radius()
                .unwrap()
                .value(),
            4.0
        );
        let id = session.execution_object_id(receiver.node_id()).unwrap();
        let row = session
            .frame()
            .objects
            .iter()
            .find(|row| row.id == id)
            .unwrap();
        assert_eq!(row.style.opacity, 0.5);
        assert_eq!(
            row.effects
                .as_ref()
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .definition
                .radius()
                .unwrap()
                .value(),
            4.0
        );
        assert_eq!(driven.own_style().unwrap().object_opacity, 0.75);
        assert_eq!(
            target_effect
                .authored_definition()
                .unwrap()
                .radius()
                .unwrap()
                .value(),
            6.0
        );
    }
}

#[test]
fn maximum_ordered_stack_restore_validates_final_eight_bindings_without_old_generations() {
    let mut scene = Scene::new();
    let mut source = scene.square(0.5).unwrap();
    for index in 0..noon_core::MAX_EFFECT_STACK_LEN {
        source
            .add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(0.0))).unwrap(),
                &format!("pass-{index}"),
            )
            .unwrap();
    }
    let saved = source.target_editor().unwrap();
    scene
        .set_effect(
            &saved,
            "pass-0",
            EffectUpdate::default().radius(Pixels(7.0)),
        )
        .unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let old = snapshot(&scene, source.node_id());
    scene
        .live(&mut session)
        .become_mobject(&source, &saved, Default::default())
        .unwrap();
    let new = snapshot(&scene, source.node_id());
    assert_eq!(new.len(), noon_core::MAX_EFFECT_STACK_LEN);
    assert_eq!(
        appearance(&new),
        appearance(&snapshot(&scene, saved.node_id()))
    );
    let id = session.execution_object_id(source.node_id()).unwrap();
    let row = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap();
    assert_eq!(
        row.effects.as_ref().unwrap().attachments(),
        new.iter()
            .map(|effect| effect.attachment)
            .collect::<Vec<_>>()
    );
    for effect in old {
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_effect_state(effect.attachment)
            .is_err());
    }
}

#[test]
fn late_invalid_group_isolation_preserves_old_effects_and_entire_publication() {
    let mut scene = Scene::new();
    let mut a = scene.circle(0.4).unwrap();
    a.set_glow(EffectUpdate::default().intensity(0.2)).unwrap();
    let b = scene.circle(0.4).unwrap();
    let c = scene.circle(0.4).unwrap();
    let source = scene.family(&[(&a).into(), (&c).into()]).unwrap();
    let target = source.copy_family().unwrap();
    scene
        .add_effect(target.root(), Blur::default(), "soften")
        .unwrap();
    scene.add_many(&[(&source).into(), (&b).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut priorities = noon_core::SemanticMutationTransaction::new();
    priorities
        .set_z_index(b.node_id(), 1.0)
        .set_z_index(c.node_id(), 2.0);
    scene.live(&mut session).apply(priorities).unwrap();
    let old = a.get_effect("glow").unwrap();
    let frame = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let bindings = snapshot(&scene, a.node_id());
    assert!(scene
        .live(&mut session)
        .become_family(&source, target.root(), Default::default())
        .is_err());
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(snapshot(&scene, a.node_id()), bindings);
    assert!(old.authored_definition().is_ok());
    assert!(source.get_effect("soften").is_err());
}

#[test]
fn paired_restore_programme_recovers_the_original_effective_values() {
    use noon::{LiveProgramStatus, RustHostCallbackTable};
    let mut program = noon::example_scenes::effect_restore::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    program.resume().unwrap();
    let through =
        |program: &mut noon::LiveProgram<noon::example_scenes::effect_restore::EffectRestore>,
         callbacks: &mut RustHostCallbackTable,
         time| {
            for _ in 0..32 {
                match program.drive_to(callbacks, time).unwrap() {
                    LiveProgramStatus::PublicationPending(_) => {
                        let context = program.take_renderer_publication().context();
                        program.admit_publication(context).unwrap();
                        program.resume().unwrap();
                    }
                    LiveProgramStatus::Awaiting(_) | LiveProgramStatus::Finished => return,
                    status => panic!("unexpected programme state {status:?}"),
                }
            }
            panic!("programme did not settle");
        };
    through(&mut program, &mut callbacks, 0.25);
    let before = program.session().frame().objects.clone();
    through(&mut program, &mut callbacks, 1.75);
    for old in &before {
        let new = program
            .session()
            .frame()
            .objects
            .iter()
            .find(|row| row.id == old.id)
            .unwrap();
        assert_eq!(old.transform, new.transform, "transform {:?}", old.id);
        assert_eq!(old.style, new.style, "style {:?}", old.id);
        assert_eq!(old.fill_paint, new.fill_paint, "paint {:?}", old.id);
        assert_eq!(old.appearance, new.appearance, "appearance {:?}", old.id);
        let definitions = |row: &noon_runtime::FrameObjectState| {
            row.effects.as_ref().map(|stack| {
                stack
                    .iter()
                    .map(|effect| effect.definition)
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(definitions(old), definitions(new), "effects {:?}", old.id);
    }
}

#[test]
fn repeated_live_restore_reuses_semantic_slots_and_keeps_only_current_generations() {
    let mut scene = Scene::new();
    let mut source = scene.square(0.5).unwrap();
    source.add_effect(Blur::default(), "soften").unwrap();
    let saved = source.target_editor().unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let live_nodes = scene.integration_store().borrow().len();
    let mut warmed_capacity = None;
    for cycle in 0..32 {
        let old = source.get_effect("soften").unwrap();
        scene
            .live(&mut session)
            .become_mobject(&source, &saved, Default::default())
            .unwrap();
        let current = source.get_effect("soften").unwrap();
        assert_ne!(old.node_id(), current.node_id());
        assert!(old.authored_definition().is_err());
        let store = scene.integration_store().borrow();
        assert_eq!(
            store.len(),
            live_nodes,
            "replacement must retire its old generation"
        );
        if cycle == 1 {
            warmed_capacity = Some(store.slot_capacity());
        }
        if cycle > 1 {
            assert_eq!(
                Some(store.slot_capacity()),
                warmed_capacity,
                "semantic slot capacity must reach a plateau"
            );
        }
    }
}
