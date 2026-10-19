#![cfg(feature = "prepared-prefix")]
// M2e: source-owned, typed immutable family-plan append over existing scheduler.
use noon_compile::{CompiledFamilyAnimation, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, FamilyAnimationMode, FamilyAnimationSpec, GeometryRef, ObjectId,
    RateFunction, RetainedAnimationMembers, RetainedFamilyAnimationPlan, SemanticStore, Style,
    TextResourceArena, Transform2D,
};
use noon_runtime::{PrefixError, ReplayError, ReplayLimits, SceneInstance};

fn fixture() -> (SceneInstance, CompiledFamilyAnimation) {
    let object = CompiledObject::new(
        ObjectId::new(40), GeometryRef::circle(1.0),
        Transform2D::IDENTITY, Style::default(),
    );
    let mut semantics = SemanticStore::new();
    let leaf = semantics.insert_authoring_object();
    let members = RetainedAnimationMembers::resolve(
        &object.content, &TextResourceArena::new(),
    ).unwrap();
    let plan = RetainedFamilyAnimationPlan::single_leaf(
        leaf, object.id, members,
    ).unwrap();
    let compiled = CompiledScene::compile_objects(vec![object.clone()], &[]).unwrap();
    let mut source = SceneInstance::new(compiled);
    source.begin_replay_retention(ReplayLimits {
        revisions: 64, payloads: 256,
    }).unwrap();
    let animation = CompiledFamilyAnimation {
        target: object.id,
        plan,
        spec: FamilyAnimationSpec::new(
            FamilyAnimationMode::DrawBorderThenFill, 1.0, 2.0, 0.0,
            RateFunction::Linear, false, false,
        ).unwrap(),
        time_map: CompositionTimeMap::identity(),
    };
    (source, animation)
}

#[test]
fn typed_family_append_admits_reader_without_rebuilding_existing_frame() {
    let (mut source, animation) = fixture();
    source.advance_to(0.5).unwrap();
    let initial = source.frame().clone();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    reader.seek(0.5).unwrap();
    let old = reader.frame().clone();
    assert_eq!(old, initial);
    assert_eq!(reader.take_renderer_publication().frame().objects.len(), 1);

    source.apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation)).unwrap();
    source.advance_to(1.5).unwrap();
    let expected = source.frame().clone();
    assert!(source.active_family_animation_indices().contains(&0));
    source.advance_to(3.5).unwrap();
    let before_append = reader.frame().clone();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().family_plans, 1);
    assert_eq!(batch.stats().revisions, 1);
    assert!(batch.stats().payloads > 0);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &before_append, "history append cannot advance displayed frame");
    assert_eq!(reader.seek(1.5).unwrap(), &expected);
    assert_eq!(reader.seek(0.5).unwrap(), &initial);
    assert_eq!(reader.seek(1.5).unwrap(), &expected);
}

#[test]
fn two_family_plans_in_one_suffix_replay_the_correct_time_qualified_channels() {
    let (mut source, first) = fixture();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    source.apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(first.clone())).unwrap();
    source.advance_to(1.5).unwrap();
    let first_effective = source.frame().clone();
    source.advance_to(3.1).unwrap();
    let second = CompiledFamilyAnimation {
        spec: FamilyAnimationSpec::new(
            FamilyAnimationMode::DrawBorderThenFill, 3.1, 1.5, 0.0,
            RateFunction::Linear, false, true,
        ).unwrap(),
        ..first
    };
    source.apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(second)).unwrap();
    source.advance_to(3.5).unwrap();
    let second_effective = source.frame().clone();
    source.advance_to(5.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().family_plans, 2);
    reader.append(batch).unwrap();
    assert_eq!(reader.seek(1.5).unwrap(), &first_effective);
    assert_eq!(reader.seek(3.5).unwrap(), &second_effective);
    assert_eq!(reader.seek(1.5).unwrap(), &first_effective);
}

#[test]
fn retroactive_family_plan_fails_closed_and_keeps_delivered_history() {
    let (mut source, mut animation) = fixture();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    let first = reader.seek(0.5).unwrap().clone();
    animation.spec = FamilyAnimationSpec::new(
        FamilyAnimationMode::DrawBorderThenFill, 0.0, 2.0, 0.0,
        RateFunction::Linear, false, false,
    ).unwrap();
    source.apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation)).unwrap();
    source.advance_to(3.0).unwrap();
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
    ));
    assert_eq!(reader.seek(0.5).unwrap(), &first);
    assert_eq!(reader.frontier(), 1.0);
}
