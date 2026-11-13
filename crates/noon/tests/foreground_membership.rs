//! Typed scene membership and completion consume shared foreground declarations.
use noon::{
    AnimationOptions, ExecutionSession, Mobject, RateFunction, Scene, SceneMembershipRequest,
};
use noon_compile::semantic_execution_object_id;
use noon_core::{ObjectId, SemanticFadeDirection, SemanticNodeId};

fn foreground(scene: &Scene) -> Vec<SemanticNodeId> {
    scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .foreground_members()
        .to_vec()
}

fn display(scene: &Scene) -> Vec<SemanticNodeId> {
    scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
}

fn painter_order(session: &ExecutionSession) -> Vec<ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect()
}

fn assert_order(session: &ExecutionSession, objects: &[&Mobject]) {
    let expected = objects
        .iter()
        .map(|object| semantic_execution_object_id(object.node_id()))
        .collect::<Vec<_>>();
    assert_eq!(painter_order(session), expected);
}

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[test]
fn typed_foreground_edits_have_identical_cold_and_live_semantics() {
    for live in [false, true] {
        let mut scene = Scene::new();
        let a = scene.square(1.0).unwrap();
        let b = scene.circle(1.0).unwrap();
        let c = scene.square(2.0).unwrap();
        scene.add_many(&[(&a).into(), (&b).into()]).unwrap();
        let first = [(&a).into()];
        let second = [(&c).into()];
        let third = [(&b).into()];
        let requests = [
            SceneMembershipRequest::AddForeground(&first),
            SceneMembershipRequest::Add(&second),
            SceneMembershipRequest::AddForeground(&third),
        ];
        let session = if live {
            let mut running = scene.into_execution().unwrap();
            for request in requests {
                running.scene_mut().edit_membership(request).unwrap();
            }
            running
        } else {
            for request in requests {
                scene.edit_membership(request).unwrap();
            }
            scene.into_execution().unwrap()
        };
        assert_eq!(foreground(session.scene()), [a.node_id(), b.node_id()]);
        assert_eq!(
            display(session.scene()),
            [c.node_id(), a.node_id(), b.node_id()]
        );
        assert_order(session.session(), &[&c, &a, &b]);
    }
}

#[test]
fn foreground_reorder_preserves_unrelated_live_rows_and_lowered_values() {
    let mut scene = Scene::new();
    let front = scene.square(1.0).unwrap();
    scene.add(&front).unwrap();
    for _ in 0..1_000 {
        let object = scene.square(0.25).unwrap();
        scene.add(&object).unwrap();
    }
    let mut session = scene.into_execution().unwrap();
    session.session_mut().take_frame_changes();
    let rows = session.session().frame().objects.clone();
    let before = session.session().publication_context();
    session
        .scene_mut()
        .add_foreground_many(&[(&front).into()])
        .unwrap();
    assert_eq!(foreground(session.scene()), [front.node_id()]);
    assert_eq!(session.session().frame().objects, rows);
    assert_eq!(
        painter_order(session.session()).last(),
        Some(&semantic_execution_object_id(front.node_id()))
    );
    assert_eq!(
        session.scene().revision(),
        before.scene_revision().checked_next().unwrap()
    );
    let stats = session.session().last_structural_publication_stats();
    assert_eq!(stats.preparation.object_states_lowered, 0);
    assert_eq!(stats.entered_objects, 0);
    assert_eq!(stats.exited_objects, 0);
    assert_eq!(session.session().last_patch_stats().full_seeks, 0);
    assert_eq!(session.session().last_patch_stats().full_group_rebuilds, 0);

    // Demotion is metadata-only: no display reordering or dirty effective rows.
    session.session_mut().take_frame_changes();
    let order = painter_order(session.session());
    session
        .scene_mut()
        .remove_foreground_many(&[(&front).into()])
        .unwrap();
    assert!(foreground(session.scene()).is_empty());
    assert_eq!(painter_order(session.session()), order);
    assert_eq!(session.session().frame().objects, rows);
    assert!(session.session_mut().take_frame_changes().is_empty());
}

#[test]
fn live_foreground_add_validation_is_atomic_for_foreign_and_duplicate_targets() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let mut foreign = Scene::new();
    let other = foreign.square(1.0).unwrap();
    let mut session = scene.into_execution().unwrap();
    session.session_mut().take_frame_changes();
    let before = session.session().publication_context();
    assert!(session
        .scene_mut()
        .add_foreground_many(&[(&a).into(), (&other).into()])
        .is_err());
    assert!(session
        .scene_mut()
        .add_foreground_many(&[(&a).into(), (&a).into()])
        .is_err());
    assert_eq!(session.session().publication_context(), before);
    assert!(foreground(session.scene()).is_empty());
    assert!(display(session.scene()).is_empty());
    assert!(session.session_mut().take_frame_changes().is_empty());
}

#[test]
fn live_child_removal_and_later_add_do_not_resurrect_the_old_foreground_family() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let c = scene.square(1.0).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .scene_mut()
        .add_foreground_many(&[(&family).into()])
        .unwrap();
    session.scene_mut().remove(&a).unwrap();
    session.scene_mut().add(&c).unwrap();
    assert_eq!(foreground(session.scene()), [b.node_id()]);
    assert_order(session.session(), &[&c, &b]);
    assert!(!session.scene().contains_direct(&a).unwrap());
    assert_eq!(
        session
            .scene()
            .integration_store()
            .borrow()
            .node(family.node_id())
            .unwrap()
            .members(),
        &[a.node_id(), b.node_id()]
    );
}

#[test]
fn fade_out_completion_atomically_retires_foreground_and_cannot_be_resurrected() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let later = scene.square(1.0).unwrap();
    scene
        .add_foreground_many(&[(&a).into(), (&b).into()])
        .unwrap();
    let mut session = scene.into_execution().unwrap();
    let segment = session
        .live()
        .declare_and_activate_fade(&a, SemanticFadeDirection::Out, options())
        .unwrap();
    let before = session.session().publication_context();
    assert!(session.live().complete_segment(segment).is_err());
    assert_eq!(session.session().publication_context(), before);
    assert_eq!(foreground(session.scene()), [a.node_id(), b.node_id()]);
    session
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.live().complete_segment(segment).unwrap();
    assert_eq!(foreground(session.scene()), [b.node_id()]);
    let completed = session.session().publication_context();
    session.live().complete_segment(segment).unwrap();
    assert_eq!(session.session().publication_context(), completed);
    session.scene_mut().add(&later).unwrap();
    assert_order(session.session(), &[&later, &b]);
    assert!(!session.scene().contains_direct(&a).unwrap());
    assert!(a.state().is_ok()); // detached identity survives; only scene membership was removed
}

#[test]
fn family_fade_out_retires_nested_foreground_declarations() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.square(1.0).unwrap();
    let later = scene.square(1.0).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    scene.add_foreground_many(&[(&family).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    let segment = session
        .live()
        .declare_and_activate_family_fade(&family, SemanticFadeDirection::Out, options())
        .unwrap();
    session
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.live().complete_segment(segment).unwrap();
    assert!(foreground(session.scene()).is_empty());
    session.scene_mut().add(&later).unwrap();
    assert_order(session.session(), &[&later]);
    assert!(a.state().is_ok());
    assert!(b.state().is_ok());
}

#[test]
fn replacing_partially_demoted_family_never_resurrects_descendants_cold_or_live() {
    for live in [false, true] {
        let mut scene = Scene::new();
        let a = scene.square(1.0).unwrap();
        let b = scene.circle(1.0).unwrap();
        let new = scene.square(2.0).unwrap();
        let later = scene.square(0.5).unwrap();
        let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        scene.add_foreground_many(&[(&family).into()]).unwrap();
        scene.remove_foreground_many(&[(&b).into()]).unwrap();
        assert_eq!(foreground(&scene), [a.node_id()]);
        let session = if live {
            let mut session = scene.into_execution().unwrap();
            let before = session.session().publication_context();
            session
                .scene_mut()
                .replace((&family).into(), (&new).into())
                .unwrap();
            assert_eq!(
                session.scene().revision(),
                before.scene_revision().checked_next().unwrap()
            );
            assert_order(session.session(), &[&new]);
            session.scene_mut().add(&later).unwrap();
            session
        } else {
            scene.replace((&family).into(), (&new).into()).unwrap();
            scene.add(&later).unwrap();
            scene.into_execution().unwrap()
        };
        assert!(foreground(session.scene()).is_empty());
        assert_eq!(display(session.scene()), [new.node_id(), later.node_id()]);
        assert_order(session.session(), &[&new, &later]);
        assert!(!session.scene().contains_direct(&a).unwrap());
        assert!(!session.scene().contains_direct(&b).unwrap());
        assert!(a.state().is_ok());
        assert!(b.state().is_ok());
    }
}

#[test]
fn live_replacement_keeps_surviving_target_foreground_identity() {
    let mut scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let b = scene.circle(1.0).unwrap();
    let c = scene.square(2.0).unwrap();
    let later = scene.square(0.5).unwrap();
    let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let target = scene.family(&[(&a).into(), (&c).into()]).unwrap();
    scene.add_foreground_many(&[(&source).into()]).unwrap();
    scene.remove_foreground_many(&[(&b).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .scene_mut()
        .replace((&source).into(), (&target).into())
        .unwrap();
    assert_eq!(foreground(session.scene()), [a.node_id()]);
    assert_order(session.session(), &[&a, &c]);
    session.scene_mut().add(&later).unwrap();
    assert_order(session.session(), &[&c, &later, &a]);
    assert_eq!(foreground(session.scene()), [a.node_id()]);
}
