use super::*;
use crate::{AuthoringError, Color, LiveProgramStatus, Scene, UnsupportedAuthoringOperation, Vec2};
use noon_core::{SemanticMutationTransaction, SemanticObjectProperty, SemanticVec3};

fn members(family: &MobjectFamily) -> Vec<crate::SemanticNodeId> {
    family
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap()
        .to_vec()
}

fn assert_rejected_without_change<T>(
    script: &ScriptScene,
    objects: &[&Mobject],
    families: &[&MobjectFamily],
    action: impl FnOnce() -> Result<T, ScriptError>,
) -> ScriptError {
    let snapshot = || {
        script
            .with_scene(|scene| {
                Ok((
                    scene.revision(),
                    scene.integration_store().borrow().len(),
                    objects
                        .iter()
                        .map(|object| object.state().unwrap())
                        .collect::<Vec<_>>(),
                    families
                        .iter()
                        .map(|family| members(family))
                        .collect::<Vec<_>>(),
                    scene.running_execution().map(|execution| {
                        (execution.publication_context(), execution.frame().clone())
                    }),
                ))
            })
            .unwrap()
    };
    let before = snapshot();
    let error = action().err().expect("the invalid operation must fail");
    assert_eq!(snapshot(), before);
    error
}

#[test]
fn family_source_preserves_order_identity_and_locality_across_linear_and_grid_layout() {
    let mut program = Scene::script(|scene| async move {
        let first = scene.square(1.0)?;
        let second = scene.square(1.0)?;
        let family = scene.family([&first, &second, &first])?;
        assert_eq!(members(&family), [second.node_id(), first.node_id()]);

        let nested = scene.family(vec![&first, &second])?;
        let mixed = [
            MobjectTarget::Object(&first),
            MobjectTarget::Family(&nested),
        ];
        let aliased = scene.family(&mixed[..])?;
        assert_eq!(members(&aliased), [first.node_id(), nested.node_id()]);
        scene.arrange_family_with_options(
            &family,
            &FamilyArrangeOptions::new(1.0, 0.0, 0.5, true),
        )?;
        scene.set_stroke_width(&first, 12.0)?;
        assert!(matches!(
            scene.effective_family_layout(&family),
            Err(ScriptError::Authoring(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::EffectiveStateUnavailable
            )))
        ));
        assert!(scene.effective_layout(&first).is_err());
        scene.add(&family)?;
        let mut unrelated = Vec::new();
        for index in 0..32 {
            let object = scene.circle(0.1)?;
            scene.shift(&object, Vec2::new(index as f32, 5.0))?;
            scene.add(&object)?;
            unrelated.push(object);
        }
        scene.wait(0.0).await?;

        let layout = scene.effective_family_layout(&family)?;
        assert_eq!(
            (layout.center, layout.width, layout.height),
            ((0.0, 0.0), 2.5, 1.0)
        );
        assert_eq!(scene.effective_layout(&first)?.width, 1.0);
        let before = scene.with_scene(|scene| {
            let execution = scene.owned_execution();
            Ok((
                execution.runtime_identity(),
                execution.execution_object_id(first.node_id()),
                execution.execution_object_id(second.node_id()),
                execution
                    .frame()
                    .objects
                    .iter()
                    .skip(2)
                    .cloned()
                    .collect::<Vec<_>>(),
                scene.integration_store().borrow().len(),
            ))
        })?;

        let running = scene.family([&first, &second])?;
        assert_eq!(
            scene.remove_family_members(&running, [&first, &first])?,
            [false, true]
        );
        assert_eq!(
            scene.add_family_members(&running, [&first, &second, &first])?,
            [false, false, true]
        );
        assert_eq!(members(&running), [second.node_id(), first.node_id()]);
        scene.arrange_family_in_grid_with_options(
            &running,
            &FamilyGridOptions {
                rows: Some(2),
                columns: Some(1),
                gap: (0.5, 0.5),
                ..Default::default()
            },
        )?;
        let layout = scene.effective_family_layout(&running)?;
        assert_eq!(
            (layout.center, layout.width, layout.height),
            ((0.0, 0.0), 1.0, 2.5)
        );
        assert_eq!(scene.effective_layout(&second)?.center, (0.0, 0.75));
        assert_eq!(scene.effective_layout(&first)?.center, (0.0, -0.75));
        scene.with_scene(|scene| {
            let execution = scene.owned_execution();
            assert_eq!(execution.runtime_identity(), before.0);
            assert_eq!(execution.execution_object_id(first.node_id()), before.1);
            assert_eq!(execution.execution_object_id(second.node_id()), before.2);
            assert_eq!(
                execution
                    .frame()
                    .objects
                    .iter()
                    .skip(2)
                    .cloned()
                    .collect::<Vec<_>>(),
                before.3
            );
            assert_eq!(scene.integration_store().borrow().len(), before.4 + 1);
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .last_mutation_stats()
                    .slots_written,
                2
            );
            assert_eq!(layout.publication, execution.publication_context());
            Ok(())
        })?;

        scene.remove(&family)?;
        scene.add(&running)?;
        assert_eq!(members(&running), [second.node_id(), first.node_id()]);
        assert_eq!(scene.effective_family_layout(&running)?.height, 2.5);
        assert_eq!(unrelated.len(), 32);
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(program.session().frame().objects.len(), 34);
}

#[test]
fn invalid_family_batches_are_atomic_during_setup_and_after_a_boundary() {
    let mut program = Scene::script(|scene| async move {
        let first = scene.square(1.0)?;
        let second = scene.square(1.0)?;
        let extra = scene.circle(0.5)?;
        let family = scene.family([&first, &second])?;
        scene.add(&family)?;
        let stale = scene.circle(0.25)?;
        let stale_family = scene.family([&stale])?;
        scene.with_scene(|scene| {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.remove_node(stale_family.node_id());
            transaction.remove_node(stale.node_id());
            scene.apply_semantic_transaction(transaction)?;
            Ok(())
        })?;
        let mut foreign_scene = Scene::new();
        let foreign = foreign_scene.circle(0.5)?;
        let foreign_family = foreign_scene.family(&[(&foreign).into()])?;

        for running in [false, true] {
            if running {
                scene.wait(0.0).await?;
            }
            let objects = [&first, &second, &extra];
            let families = [&family];
            for invalid in [&foreign, &stale] {
                assert_rejected_without_change(&scene, &objects, &families, || {
                    scene.family([&extra, invalid])
                });
                assert_rejected_without_change(&scene, &objects, &families, || {
                    scene.add_family_members(&family, [&extra, invalid])
                });
                assert_rejected_without_change(&scene, &objects, &families, || {
                    scene.remove_family_members(&family, [&first, invalid])
                });
                assert_rejected_without_change(&scene, &objects, &families, || scene.add(invalid));
            }
            // The valid first child must not leak when the second creates a cycle
            // during shared transaction preparation rather than handle preflight.
            assert_rejected_without_change(&scene, &objects, &families, || {
                scene.add_family_members(
                    &family,
                    [
                        MobjectTarget::Object(&extra),
                        MobjectTarget::Family(&family),
                    ],
                )
            });
            for invalid in [&foreign_family, &stale_family] {
                assert_rejected_without_change(&scene, &objects, &families, || scene.add(invalid));
                assert_rejected_without_change(&scene, &objects, &families, || {
                    scene.add_family_members(invalid, [&extra])
                });
                assert_rejected_without_change(&scene, &objects, &families, || {
                    scene.arrange_family_with_options(
                        invalid,
                        &FamilyArrangeOptions::new(1.0, 0.0, 0.25, true),
                    )
                });
            }
            assert_rejected_without_change(&scene, &objects, &families, || {
                scene.arrange_family_in_grid_with_options(
                    &family,
                    &FamilyGridOptions {
                        rows: Some(0),
                        ..Default::default()
                    },
                )
            });
            assert_rejected_without_change(&scene, &objects, &families, || {
                scene.arrange_family_with_options(
                    &family,
                    &FamilyArrangeOptions::new(1.0, 0.0, f64::NAN, true),
                )
            });
        }
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[test]
fn effective_queries_observe_drivers_while_layout_rejects_them_atomically() {
    let mut program = Scene::script(|scene| async move {
        let first = scene.square(1.0)?;
        let second = scene.square(1.0)?;
        let family = scene.family([&first, &second])?;
        scene.add(&family)?;
        scene.with_scene(|scene| {
            let mut store = scene.integration_store().borrow_mut();
            let signal = store
                .insert_semantic_input_signal(SemanticVec3::new(2.0, 0.0, 0.0))
                .unwrap();
            store
                .bind_semantic_signal(
                    signal,
                    second.node_id(),
                    SemanticObjectProperty::Translation,
                )
                .unwrap();
            Ok(())
        })?;
        scene.wait(0.0).await?;
        assert_eq!(
            scene.authored(&second)?.transform.translation,
            SemanticVec3::ZERO
        );
        assert_eq!(scene.effective_layout(&second)?.center, (2.0, 0.0));
        assert_eq!(scene.effective_family_layout(&family)?.width, 3.0);
        let error = assert_rejected_without_change(&scene, &[&first, &second], &[&family], || {
            scene.arrange_family_with_options(
                &family,
                &FamilyArrangeOptions::new(1.0, 0.0, 0.25, true),
            )
        });
        assert!(matches!(
            error,
            ScriptError::Authoring(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
            ))
        ));
        assert_rejected_without_change(&scene, &[&first, &second], &[&family], || {
            scene.arrange_family_in_grid_with_options(&family, &FamilyGridOptions::default())
        });
        scene.set_color(&second, Color::BLUE)?;
        scene.set_stroke_width(&second, 4.0)?;
        assert_eq!(
            scene.authored(&second)?.transform.translation,
            SemanticVec3::ZERO
        );
        assert_eq!(scene.effective_layout(&second)?.center, (2.0, 0.0));
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[test]
fn render_override_layout_failure_preserves_every_member_and_publication() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.circle(0.5).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    scene.add(&first).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let segment = scene
        .owned_live()
        .declare_and_activate_create(&second, crate::AnimationOptions::new())
        .unwrap();
    scene.owned_live().advance_segment_to(segment, 0.5).unwrap();
    let before = scene.owned_execution().publication_context();
    let frame = scene.owned_execution().frame().clone();
    let states = [first.state().unwrap(), second.state().unwrap()];
    assert!(matches!(
        scene.effective_layout(&second),
        Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::EffectiveLayoutRenderOverride
        ))
    ));
    assert!(matches!(
        scene.effective_family_layout(&family),
        Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride
        ))
    ));
    assert!(scene
        .arrange_family_with_options(&family, &FamilyArrangeOptions::new(1.0, 0.0, 0.5, true))
        .is_err());
    assert!(scene
        .arrange_family_in_grid_with_options(&family, &FamilyGridOptions::default())
        .is_err());
    assert_eq!([first.state().unwrap(), second.state().unwrap()], states);
    assert_eq!(scene.owned_execution().publication_context(), before);
    assert_eq!(scene.owned_execution().frame(), &frame);
}
