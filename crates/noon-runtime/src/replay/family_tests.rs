//! Retained family channels use their original scheduler, not a second replay model.
use super::*;
use noon_compile::{CompiledFamilyAnimation, CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, FamilyAnimationMode, FamilyAnimationSpec,
    GeometryRef, ObjectId, RateFunction, RetainedAnimationMembers, RetainedFamilyAnimationPlan,
    SemanticStore, Style, TextResourceArena, Transform2D,
};

fn object() -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(40),
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn animation(start: f64, duration: f64, reverse: bool) -> CompiledFamilyAnimation {
    let object = object();
    let mut semantics = SemanticStore::new();
    let leaf = semantics.insert_authoring_object();
    let members =
        RetainedAnimationMembers::resolve(&object.content, &TextResourceArena::new()).unwrap();
    CompiledFamilyAnimation {
        target: object.id,
        plan: RetainedFamilyAnimationPlan::single_leaf(leaf, object.id, members).unwrap(),
        spec: FamilyAnimationSpec::new(
            FamilyAnimationMode::DrawBorderThenFill,
            start,
            duration,
            0.0,
            RateFunction::Linear,
            false,
            reverse,
        )
        .unwrap(),
        time_map: CompositionTimeMap::identity(),
    }
}

fn runtime() -> SceneInstance {
    SceneInstance::new(CompiledScene::compile_objects(vec![object()], &[]).unwrap())
}

#[test]
fn family_replay_matches_first_pass_across_mapped_reverse_and_membership_changes() {
    let mut runtime = runtime();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let mut frames = vec![runtime.advance_to(0.5).unwrap().clone()];
    runtime.advance_to(1.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation(
            1.0, 2.0, false,
        )))
        .unwrap();
    for time in [1.0, 1.5, 2.0, 3.0, 3.25] {
        frames.push(runtime.advance_to(time).unwrap().clone());
    }
    let mut reverse = animation(0.0, 8.0, true);
    reverse.time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
        0.5,
        0.25,
        RateFunction::Linear,
    )]);
    runtime.advance_to(4.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(reverse))
        .unwrap();
    for time in [4.0, 4.5, 5.0, 6.0, 6.25] {
        frames.push(runtime.advance_to(time).unwrap().clone());
    }
    runtime.advance_to(6.5).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveObject(object().id))
        .unwrap();
    frames.push(runtime.advance_to(6.75).unwrap().clone());
    runtime.advance_to(7.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(object()))
        .unwrap();
    frames.push(runtime.advance_to(7.25).unwrap().clone());
    frames.push(runtime.advance_to(8.0).unwrap().clone());
    runtime.seal_replay().unwrap();

    let plans = runtime.family_animation_plans().as_ptr();
    assert_eq!(runtime.family_animation_plans().len(), 2);
    assert_eq!(
        frames[4].family_animations[0].unwrap().overall_progress,
        1.0
    );
    assert!(frames[5].family_animations[0].is_none());
    assert!(frames[9].family_animations[0].unwrap().reverse_member_order);
    assert!(frames[10].family_animations[0].is_none());
    for expected in frames.iter().rev().chain(frames.iter()) {
        assert_eq!(runtime.seek(expected.time).unwrap(), expected);
        assert_eq!(runtime.family_animation_plans().as_ptr(), plans);
        assert_eq!(runtime.family_animation_plans().len(), 2);
    }
    runtime.seek(frames[0].time).unwrap();
    for expected in &frames[1..] {
        assert_eq!(runtime.advance_to(expected.time).unwrap(), expected);
    }
    runtime.seek(7.25).unwrap();
    runtime.take_frame_changes();
    runtime.advance_to(7.5).unwrap();
    assert_eq!(runtime.replay_stats().revisions_crossed, 0);
    assert_eq!(runtime.last_timeline_scheduler_stats().events_crossed, 0);
    assert!(runtime.take_frame_changes().is_empty());
}

#[test]
fn preexisting_family_channel_is_part_of_the_initial_replay_projection() {
    let mut runtime = runtime();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation(
            0.0, 2.0, false,
        )))
        .unwrap();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let middle = runtime.advance_to(1.0).unwrap().clone();
    runtime.advance_to(3.0).unwrap();
    runtime.seal_replay().unwrap();
    assert_eq!(runtime.seek(1.0).unwrap(), &middle);
    assert_eq!(runtime.replay_stats().revisions_retained, 0);
}

#[test]
fn retroactive_family_introduction_invalidates_replay_without_rejecting_live_execution() {
    let mut runtime = runtime();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(2.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation(
            1.0, 2.0, false,
        )))
        .unwrap();
    assert!(!runtime.replay_retention_valid());
    assert_eq!(runtime.seal_replay(), Err(ReplayError::UnsupportedDomain));
    assert!(runtime.frame().family_animations[0].is_some());
}

#[test]
fn admission_uses_mapped_start_not_the_unmapped_specification() {
    for (publication, accepted) in [(3.0, true), (4.0, true), (4.5, false)] {
        let mut runtime = runtime();
        runtime
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        runtime.advance_to(publication).unwrap();
        let mut family = animation(0.0, 8.0, false);
        family.time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.5,
            0.25,
            RateFunction::Linear,
        )]);
        runtime
            .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(family))
            .unwrap();
        assert_eq!(runtime.replay_retention_valid(), accepted);
        runtime.advance_to(7.0).unwrap();
        assert_eq!(runtime.seal_replay().is_ok(), accepted);
        if accepted {
            assert!(runtime.seek(3.5).unwrap().family_animations[0].is_none());
            assert_eq!(
                runtime.seek(5.0).unwrap().family_animations[0]
                    .unwrap()
                    .overall_progress,
                0.5
            );
        }
    }
}

#[test]
fn immutable_family_registration_still_consumes_a_revision_budget() {
    let mut runtime = runtime();
    runtime
        .begin_replay_retention(ReplayLimits {
            revisions: 0,
            payloads: 0,
        })
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation(
            0.0, 2.0, false,
        )))
        .unwrap();
    assert_eq!(runtime.seal_replay(), Err(ReplayError::RetentionLimit));
}

#[test]
fn invalid_family_patch_does_not_poison_an_existing_replay_scope() {
    let mut runtime = runtime();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    let mut family = animation(0.0, 2.0, false);
    family.target = ObjectId::new(999);
    assert!(runtime
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(family))
        .is_err());
    assert!(runtime.replay_retention_valid());
    runtime.seal_replay().unwrap();
}
