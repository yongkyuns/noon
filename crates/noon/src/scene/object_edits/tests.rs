use super::*;
use crate::{AnimationOptions, ExecutionSessionPublicationError, RateFunction};
use noon_core::{SemanticPaint, RIGHT, UP};

type Edit = fn(&mut Scene, &Mobject) -> Result<(), AuthoringError>;

const VALID_EDITS: [Edit; 4] = [
    |scene, object| scene.set_color(object, Color::YELLOW),
    |scene, object| scene.set_fill(object, Color::BLUE, 0.5),
    |scene, object| scene.set_fill_opacity(object, 0.3),
    |scene, object| scene.shift(object, RIGHT),
];

fn bootstrap(scene: &mut Scene) {
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    scene.owned_execution_mut().take_frame_changes();
}

#[test]
fn persistent_edits_share_cold_and_running_semantics_and_remain_local() {
    for running in [false, true] {
        let mut scene = Scene::new();
        let object = scene.square(0.5).unwrap();
        scene.add(&object).unwrap();
        let unrelated: Vec<_> = (0..64)
            .map(|_| {
                let sibling = scene.circle(0.2).unwrap();
                scene.add(&sibling).unwrap();
                sibling
            })
            .collect();
        let unrelated_states: Vec<_> = unrelated.iter().map(|row| row.state().unwrap()).collect();
        if running {
            bootstrap(&mut scene);
        }
        let before_revision = scene.revision();
        let before_content = object.state().unwrap().content;
        let before_nodes = scene.integration_store().borrow().len();
        let before_runtime = scene.running_execution().map(|execution| {
            (
                execution.runtime_identity(),
                execution.frame().time,
                execution.execution_object_id(object.node_id()),
                execution.frame().objects.clone(),
            )
        });

        scene.set_fill(&object, Color::BLUE, 0.6).unwrap();
        scene.set_fill_opacity(&object, 0.25).unwrap();
        scene.set_color(&object, Color::YELLOW).unwrap();
        scene.shift(&object, RIGHT * 2.0 + UP).unwrap();

        assert_eq!(scene.revision().get(), before_revision.get() + 4);
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .last_mutation_stats()
                .slots_written,
            1
        );
        let authored = scene.authored(&object).unwrap();
        assert_eq!(authored.content, before_content);
        assert_eq!(
            authored.transform.translation,
            SemanticVec3::new(2.0, 1.0, 0.0)
        );
        assert_eq!(
            authored.style.fill,
            Some(SemanticPaint::Solid(Color::YELLOW))
        );
        assert_eq!(authored.style.fill_opacity, 0.25);
        for (sibling, before) in unrelated.iter().zip(&unrelated_states) {
            assert_eq!(&sibling.state().unwrap(), before);
        }

        if !running {
            bootstrap(&mut scene);
        }
        let execution = scene.owned_execution();
        if let Some((identity, time, slot, rows)) = before_runtime {
            assert_eq!(execution.runtime_identity(), identity);
            assert_eq!(execution.frame().time, time);
            assert_eq!(execution.execution_object_id(object.node_id()), slot);
            assert_eq!(
                execution
                    .frame()
                    .objects
                    .iter()
                    .filter(|row| Some(row.id) != slot)
                    .collect::<Vec<_>>(),
                rows.iter()
                    .filter(|row| Some(row.id) != slot)
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            execution.publication_context().scene_revision(),
            scene.revision()
        );
        assert_eq!(execution.frame().objects.len(), unrelated.len() + 1);
        let effective = scene.effective(&object).unwrap();
        assert_eq!(effective.transform.translation, RIGHT * 2.0 + UP);
        assert_eq!(effective.fill_opacity(), 0.25);
        assert_eq!(
            effective.style.fill,
            Some(Color::rgba(
                Color::YELLOW.red,
                Color::YELLOW.green,
                Color::YELLOW.blue,
                0.25
            ))
        );
    }
}

#[test]
fn invalid_foreign_and_stale_edits_leave_both_authorities_unchanged() {
    let invalid: [Edit; 4] = [
        |scene, object| scene.set_color(object, Color::rgba(f32::NAN, 0.5, 0.5, 1.0)),
        |scene, object| scene.set_fill(object, Color::BLUE, f64::NAN),
        |scene, object| scene.set_fill_opacity(object, f64::INFINITY),
        |scene, object| scene.shift(object, Vec2::new(f32::INFINITY, 1.0)),
    ];
    for running in [false, true] {
        let mut scene = Scene::new();
        let object = scene.circle(0.5).unwrap();
        let stale = scene.square(0.5).unwrap();
        scene.add(&object).unwrap();
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(stale.node_id());
        scene.apply_semantic_transaction(remove).unwrap();
        let foreign = Scene::new().square(0.5).unwrap();
        let foreign_before = foreign.state().unwrap();
        if running {
            bootstrap(&mut scene);
        }
        let before_state = object.state().unwrap();
        let before_revision = scene.revision();
        let before_nodes = scene.integration_store().borrow().len();
        let before_runtime = scene
            .running_execution()
            .map(|execution| (execution.publication_context(), execution.frame().clone()));

        for index in 0..VALID_EDITS.len() {
            assert!(invalid[index](&mut scene, &object).is_err());
            assert!(matches!(
                VALID_EDITS[index](&mut scene, &foreign),
                Err(AuthoringError::ForeignStore)
            ));
            assert!(VALID_EDITS[index](&mut scene, &stale).is_err());
            assert_eq!(object.state().unwrap(), before_state);
            assert_eq!(foreign.state().unwrap(), foreign_before);
            assert_eq!(scene.revision(), before_revision);
            assert_eq!(scene.integration_store().borrow().len(), before_nodes);
            assert_eq!(
                scene
                    .running_execution()
                    .map(|execution| (execution.publication_context(), execution.frame().clone())),
                before_runtime
            );
            if let Some(execution) = scene.running_execution_mut() {
                assert!(execution.take_frame_changes().is_empty());
            }
        }
    }
}

#[test]
fn style_edit_does_not_bake_an_effective_timeline_transform_into_authored_state() {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.set_fill(&object, Color::RED, 1.0).unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let root = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![animation.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut execution = scene.execution_session_with_animation_root(&root).unwrap();
    execution.seek(1.0).unwrap();
    scene.install_execution(execution);
    let identity = scene.owned_execution().runtime_identity();
    let authored_transform = scene.authored(&object).unwrap().transform;
    let effective_transform = scene.effective(&object).unwrap().transform;
    assert_eq!(effective_transform.translation, RIGHT);
    assert_eq!(
        authored_transform.translation,
        SemanticVec3::new(0.0, 0.0, 0.0)
    );

    scene.set_fill(&object, Color::BLUE, 0.3).unwrap();
    scene.set_color(&object, Color::YELLOW).unwrap();
    assert_eq!(
        scene.authored(&object).unwrap().transform,
        authored_transform
    );
    assert_eq!(
        scene.effective(&object).unwrap().transform,
        effective_transform
    );
    assert_eq!(scene.owned_execution().frame().time, 1.0);
    assert_eq!(scene.owned_execution().runtime_identity(), identity);
    assert!((scene.effective(&object).unwrap().fill_opacity() - 0.3).abs() < 1e-6);
}

#[test]
fn active_segment_rejection_is_atomic_across_aliased_descendants() {
    let mut scene = Scene::new();
    let root = scene.square(0.5).unwrap();
    let child = scene.circle(0.2).unwrap();
    let nested = scene.family(&[(&child).into()]).unwrap();
    root.add_many(&[(&child).into(), (&nested).into()]).unwrap();
    scene.add(&root).unwrap();
    bootstrap(&mut scene);
    let segment = crate::animation_request::activate(
        &mut scene.owned_live(),
        child.animate().shift(RIGHT * 2.0).into(),
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )
    .unwrap();
    scene.owned_live().advance_segment_to(segment, 1.0).unwrap();
    scene.owned_execution_mut().take_frame_changes();
    let before_root = root.state().unwrap();
    let before_child = child.state().unwrap();
    let before = scene.owned_execution().publication_context();
    let before_frame = scene.owned_execution().frame().clone();
    assert_eq!(
        scene.effective(&child).unwrap().transform.translation,
        RIGHT
    );
    assert_eq!(
        before_child.transform.translation,
        SemanticVec3::new(0.0, 0.0, 0.0)
    );

    for edit in VALID_EDITS {
        assert!(matches!(
            edit(&mut scene, &root),
            Err(AuthoringError::ExecutionPublication(
                ExecutionSessionPublicationError::SegmentCompletionPending
            ))
        ));
        assert_eq!(root.state().unwrap(), before_root);
        assert_eq!(child.state().unwrap(), before_child);
        assert_eq!(scene.owned_execution().publication_context(), before);
        assert_eq!(scene.owned_execution().frame(), &before_frame);
        assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
    }

    scene
        .owned_live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.owned_live().complete_segment(segment).unwrap();
    let revision = scene.revision();
    scene.shift(&root, UP).unwrap();
    assert_eq!(scene.revision().get(), revision.get() + 1);
    assert_eq!(scene.effective(&root).unwrap().transform.translation, UP);
    assert_eq!(
        scene.effective(&child).unwrap().transform.translation,
        RIGHT * 2.0 + UP
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .last_mutation_stats()
            .slots_written,
        2
    );
}

#[test]
fn stale_running_revision_rejects_all_persistent_edits_without_repairing_or_partial_writes() {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    bootstrap(&mut scene);
    // An integration-side authored edit intentionally leaves the runtime stale.
    object.clone().set_fill(1.0, 0.0, 0.0, 0.4).unwrap();
    let state = object.state().unwrap();
    let revision = scene.revision();
    let published = scene.owned_execution().publication_context();
    let frame = scene.owned_execution().frame().clone();
    for edit in VALID_EDITS {
        assert!(matches!(
            edit(&mut scene, &object),
            Err(AuthoringError::ExecutionPublication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            ))
        ));
        assert_eq!(object.state().unwrap(), state);
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.owned_execution().publication_context(), published);
        assert_eq!(scene.owned_execution().frame(), &frame);
        assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
    }
}
