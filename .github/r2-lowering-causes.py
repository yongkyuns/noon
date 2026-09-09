"""One-shot forwarding edit; never shipped as product machinery."""
from pathlib import Path
FORWARD = {
 'noon-compile/src/lib.rs': {
  'CompileError': ['InvalidTrack(error)'],
  'CompilePatchError': ['InvalidTrack(error)', 'Resource(error)'],
 },
 'noon-compile/src/semantic_lowering/animation_payload/affine.rs': {
  'SemanticAffineAnimationTrackError': ['Animation(error)', 'Target { error, .. }', 'InvalidSubsetDisplayTimeMap { error, .. }', 'InvalidTargetValue { error, .. }', 'InvalidTargetStyle { error, .. }', 'InvalidTrack { error, .. }'],
 },
 'noon-compile/src/semantic_lowering/animation_payload/prepared_composition.rs': {
  'PreparedSemanticAnimationLoweringError': ['Schedule(error)', 'TextGlyph(error)', 'Target { error, .. }', 'InvalidSubsetDisplayTimeMap { error, .. }', 'InvalidTargetValue { error, .. }', 'InvalidTargetStyle { error, .. }'],
 },
 'noon-compile/src/semantic_lowering/animation_payload/text_write.rs': {
  'TextGlyphLoweringError': ['InvalidMembers(error)', 'InvalidPlan(error)', 'InvalidSpec(error)', 'InvalidTimeMap(error)'],
 },
 'noon-compile/src/semantic_lowering/animation_payload/transform_payload.rs': {
  'SemanticTransformToPayloadError': ['Options(error)', 'Target { error, .. }'],
 },
 'noon-compile/src/semantic_lowering/animation_schedule.rs': {
  'PreparedScalarAnimationTrackError': ['Signal { error, .. }', 'InvalidTimeMap { error, .. }'],
  'PreparedSemanticAnimationScheduleError': ['Lookup { error, .. }', 'Options { error, .. }', 'Composition { error, .. }'],
  'SemanticAnimationScheduleError': ['Animation(error)', 'Options { error, .. }', 'Composition { error, .. }'],
 },
 'noon-compile/src/semantic_lowering/compiled_scene.rs': {'SemanticCompiledSceneError': ['Resource { error, .. }']},
 'noon-compile/src/semantic_lowering/entrypoint.rs': {'SemanticExecutionLoweringError': ['Object(error)', 'Reactive(error)', 'Compiled(error)', 'InitialAnimation(error)']},
 'noon-compile/src/semantic_lowering/initial_animation.rs': {'SemanticInitialAnimationError': ['Schedule(error)', 'Family(error)', 'Animation(error)', 'Endpoint { error, .. }', 'EndpointValue { error, .. }', 'EndpointGeometry { error, .. }', 'Compiled(error)']},
 'noon-compile/src/semantic_lowering/projection.rs': {'SemanticLoweringError': ['Store(error)']},
 'noon-compile/src/semantic_lowering/publication.rs': {'SemanticPublicationLoweringError': ['PreparedValue { error, .. }', 'PreparedGeometry { error, .. }', 'PreparedContent { error, .. }', 'Read(error)', 'Value(error)']},
 'noon-compile/src/semantic_lowering/reactive.rs': {'PreparedScalarSignalTimelineError': ['Lowering(error)'], 'SemanticReactiveLoweringError': ['Signal(error)', 'Reactive(error)']},
 'noon-runtime/src/execution_slots/runtime_transaction.rs': {'AuthoredPublicationError': ['Evaluation(error)', 'PreparedFrame(error)', 'Compile(error)']},
 'noon-runtime/src/lib.rs': {'EvaluationError': ['Reactive(error)']},
 'noon-runtime/src/reactive/family_plan_frame.rs': {'RetainedFamilyFramePlanError': ['Evaluation(error)']},
 'noon-runtime/src/reactive/family_plan_set_frame.rs': {'RetainedPlannedFamilyFrameError': ['Plan(error)']},
}
for relative, types in FORWARD.items():
 path = Path('crates') / relative
 text = path.read_text()
 for name, arms in types.items():
  before = f'impl std::error::Error for {name} {{}}'
  assert text.count(before) == 1, (relative, name)
  after = f"impl std::error::Error for {name} {{\n    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {{\n        match self {{\n"
  after += ''.join(f'            Self::{arm} => Some(error),\n' for arm in arms)
  after += '            _ => None,\n        }\n    }\n}'
  text = text.replace(before, after)
 path.write_text(text)
p = Path('crates/noon-compile/src/semantic_lowering/animation_schedule.rs')
s = p.read_text()
needle = '''pub enum PreparedSemanticAnimationLookupError {
    Transaction(SemanticTransactionReadError),
    Existing(SemanticAnimationError),
    InitialObjectPropertyTrack,
}'''
assert s.count(needle) == 1
s = s.replace(needle, needle + '''

impl std::fmt::Display for PreparedSemanticAnimationLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transaction(error) => error.fmt(formatter),
            Self::Existing(error) => error.fmt(formatter),
            Self::InitialObjectPropertyTrack => formatter.write_str(
                "initial object property tracks are not prepared animation declarations",
            ),
        }
    }
}

impl std::error::Error for PreparedSemanticAnimationLookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transaction(error) => Some(error),
            Self::Existing(error) => Some(error),
            Self::InitialObjectPropertyTrack => None,
        }
    }
}''')
p.write_text(s)
# Generate explicit Rust regressions, not a product error framework.
CASES = {
 'compiled_errors': [
  ('noon_core::TimelineError::InvalidDuration(-1.0)', 'CompileError::InvalidTrack'),
  ('noon_core::TimelineError::InvalidDuration(-1.0)', 'CompilePatchError::InvalidTrack'),
  ('resource()', 'CompilePatchError::Resource'),
  ('resource()', '|error| SemanticCompiledSceneError::Resource { node: id, error }'),
 ],
 'scheduled_errors': [
  ('noon_core::SemanticAnimationError::UnknownAnimation(id)', 'SemanticAnimationScheduleError::Animation'),
  ('noon_core::AnimationOptionsError::InvalidRunTime(-1.0)', '|error| SemanticAnimationScheduleError::Options { animation: id, error }'),
  ('noon_core::CompositionError::Empty', '|error| SemanticAnimationScheduleError::Composition { animation: id, error }'),
  ('noon_core::SemanticTransactionReadError::UnknownExistingNode(id)', 'PreparedSemanticAnimationLookupError::Transaction'),
  ('noon_core::SemanticAnimationError::UnknownAnimation(id)', 'PreparedSemanticAnimationLookupError::Existing'),
  ('PreparedSemanticAnimationLookupError::InitialObjectPropertyTrack', '|error| PreparedSemanticAnimationScheduleError::Lookup { animation: id.into(), error }'),
  ('noon_core::AnimationOptionsError::InvalidRunTime(-1.0)', '|error| PreparedSemanticAnimationScheduleError::Options { animation: id.into(), error }'),
  ('noon_core::CompositionError::Empty', '|error| PreparedSemanticAnimationScheduleError::Composition { animation: id.into(), error }'),
  ('noon_core::SemanticScalarSignalQueryError::InvalidTime', '|error| PreparedScalarAnimationTrackError::Signal { signal: id, error }'),
  ('time_map()', '|error| PreparedScalarAnimationTrackError::InvalidTimeMap { animation: id.into(), error }'),
 ],
 'affine_errors': [
  ('noon_core::SemanticAnimationError::UnknownAnimation(id)', 'SemanticAffineAnimationTrackError::Animation'),
  ('scene_error(id)', '|error| SemanticAffineAnimationTrackError::Target { animation: id, node: id, error }'),
  ('time_map()', '|error| SemanticAffineAnimationTrackError::InvalidSubsetDisplayTimeMap { animation: id, error }'),
  ('noon_core::SemanticLoweringError::CoordinateOutOfRange(noon_core::SemanticVec3::new(f64::MAX, 0.0, 0.0))', '|error| SemanticAffineAnimationTrackError::InvalidTargetValue { animation: id, target_state: id, field: SemanticAffineAnimationField::Translation, error }'),
  ('value_error(id)', '|error| SemanticAffineAnimationTrackError::InvalidTargetStyle { animation: id, target_state: id, error }'),
  ('noon_core::TimelineError::InvalidDuration(-1.0)', '|error| SemanticAffineAnimationTrackError::InvalidTrack { animation: id, error }'),
  ('noon_core::AnimationOptionsError::UnsupportedPathArc(1.0)', 'SemanticTransformToPayloadError::Options'),
  ('scene_error(id)', '|error| SemanticTransformToPayloadError::Target { node: id, error }'),
 ],
 'prepared_animation_errors': [
  ('PreparedSemanticAnimationScheduleError::InvalidStartTime(-1.0)', 'PreparedSemanticAnimationLoweringError::Schedule'),
  ('TextGlyphLoweringError::MissingSemanticTarget(id.into())', 'PreparedSemanticAnimationLoweringError::TextGlyph'),
  ('noon_core::SemanticTransactionReadError::UnknownExistingNode(id)', '|error| PreparedSemanticAnimationLoweringError::Target { animation: id.into(), node: id.into(), error }'),
  ('time_map()', '|error| PreparedSemanticAnimationLoweringError::InvalidSubsetDisplayTimeMap { animation: id.into(), error }'),
  ('noon_core::SemanticLoweringError::CoordinateOutOfRange(noon_core::SemanticVec3::new(f64::MAX, 0.0, 0.0))', '|error| PreparedSemanticAnimationLoweringError::InvalidTargetValue { animation: id.into(), target_state: id.into(), field: SemanticAffineAnimationField::Translation, error }'),
  ('compact_error()', '|error| PreparedSemanticAnimationLoweringError::InvalidTargetStyle { animation: id.into(), target_state: id.into(), error }'),
  ('noon_core::RetainedAnimationMemberError::TooManyMembers(usize::MAX)', 'TextGlyphLoweringError::InvalidMembers'),
  ('noon_core::RetainedFamilyAnimationMemberPlanError::Members(noon_core::RetainedAnimationMemberError::TooManyMembers(usize::MAX))', 'TextGlyphLoweringError::InvalidPlan'),
  ('noon_core::FamilyAnimationError::InvalidDuration(-1.0)', 'TextGlyphLoweringError::InvalidSpec'),
  ('time_map()', 'TextGlyphLoweringError::InvalidTimeMap'),
 ],
 'initial_animation_errors': [
  ('SemanticAnimationScheduleError::InvalidStartTime(-1.0)', 'SemanticInitialAnimationError::Schedule'),
  ('TextGlyphLoweringError::MissingSemanticTarget(id.into())', 'SemanticInitialAnimationError::Family'),
  ('noon_core::SemanticAnimationError::UnknownAnimation(id)', 'SemanticInitialAnimationError::Animation'),
  ('scene_error(id)', '|error| SemanticInitialAnimationError::Endpoint { animation: id, node: id, error }'),
  ('value_error(id)', '|error| SemanticInitialAnimationError::EndpointValue { animation: id, node: id, error }'),
  ('SemanticGeometryValueError::InvalidAnalyticGeometry', '|error| SemanticInitialAnimationError::EndpointGeometry { animation: id, node: id, error }'),
  ('CompilePatchError::InvalidTrack(noon_core::TimelineError::InvalidDuration(-1.0))', 'SemanticInitialAnimationError::Compiled'),
 ],
 'publication_and_reactive_errors': [
  ('noon_core::SemanticStoreError::UnknownNode(id)', 'SemanticLoweringError::Store'),
  ('compact_error()', '|error| SemanticPublicationLoweringError::PreparedValue { object: id.into(), error }'),
  ('SemanticGeometryValueError::InvalidAnalyticGeometry', '|error| SemanticPublicationLoweringError::PreparedGeometry { object: id.into(), error }'),
  ('SemanticCompiledSceneError::InvalidAnalyticGeometry { node: id }', '|error| SemanticPublicationLoweringError::PreparedContent { object: id.into(), error }'),
  ('noon_core::SemanticTransactionReadError::UnknownExistingNode(id)', 'SemanticPublicationLoweringError::Read'),
  ('value_error(id)', 'SemanticPublicationLoweringError::Value'),
  ('noon_core::SemanticSignalError::UnknownSignal(id)', 'SemanticReactiveLoweringError::Signal'),
  ('noon_core::ReactiveError::DependencyCycle', 'SemanticReactiveLoweringError::Reactive'),
  ('SemanticReactiveLoweringError::DependencyCycle(id)', 'PreparedScalarSignalTimelineError::Lowering'),
  ('value_error(id)', 'SemanticExecutionLoweringError::Object'),
  ('SemanticReactiveLoweringError::DependencyCycle(id)', 'SemanticExecutionLoweringError::Reactive'),
  ('SemanticCompiledSceneError::InvalidAnalyticGeometry { node: id }', 'SemanticExecutionLoweringError::Compiled'),
  ('SemanticInitialAnimationError::InvalidOrigin(-1.0)', 'SemanticExecutionLoweringError::InitialAnimation'),
 ],
}
compiler_tests = '''//! Typed causes survive compiler wrappers without changing rejection policy.
use std::error::Error;
use noon_compile::*;
use noon_core::{SemanticNodeId, SemanticStore, SemanticObjectState, StoredGeometry};

fn assert_cause<C: Error + Clone + PartialEq + 'static, E: Error>(cause: C, wrap: impl FnOnce(C) -> E) {
    let expected = cause.clone();
    let error = wrap(cause);
    assert_eq!(error.source().and_then(|source| source.downcast_ref::<C>()), Some(&expected));
    assert!(!error.source().unwrap().to_string().is_empty());
}
fn node() -> SemanticNodeId { SemanticStore::new().insert_family() }
fn resource() -> CompiledResourceError {
    CompiledResourceError::MissingFont(noon_core::FontResourceKey { face_key: "missing-font".into(), face_index: 0 })
}
fn scene_error(id: SemanticNodeId) -> noon_core::SemanticSceneOperationError {
    noon_core::SemanticSceneOperationError::NotSemanticObject(id)
}
fn value_error(id: SemanticNodeId) -> SemanticLoweringError {
    SemanticLoweringError::ValueOutOfRange { node: id, field: SemanticExecutionField::Translation }
}
fn compact_error() -> SemanticExecutionValueError {
    SemanticExecutionValueError::ValueOutOfRange { field: SemanticExecutionField::Translation }
}
fn time_map() -> noon_core::CompositionTimeMapError {
    noon_core::CompositionTimeMapError::IntervalOutsideParent { index: 0 }
}
'''
for name, cases in CASES.items():
 compiler_tests += '\n#[test]\nfn '+name+'() {\n    let id = node();\n'
 compiler_tests += ''.join(f'    assert_cause({cause}, {wrap});\n' for cause, wrap in cases)
 compiler_tests += '}\n'
compiler_tests += '''
#[test]
fn prepared_lookup_preserves_a_multilevel_chain_and_leaf_termination() {
    let id = node();
    let error = PreparedSemanticAnimationLoweringError::Schedule(
        PreparedSemanticAnimationScheduleError::Lookup {
            animation: id.into(),
            error: PreparedSemanticAnimationLookupError::Transaction(
                noon_core::SemanticTransactionReadError::UnknownExistingNode(id)),
        });
    let schedule = error.source().unwrap().downcast_ref::<PreparedSemanticAnimationScheduleError>().unwrap();
    let lookup = schedule.source().unwrap().downcast_ref::<PreparedSemanticAnimationLookupError>().unwrap();
    let read = lookup.source().unwrap().downcast_ref::<noon_core::SemanticTransactionReadError>().unwrap();
    assert_eq!(read, &noon_core::SemanticTransactionReadError::UnknownExistingNode(id));
    assert!(read.source().is_none());
    assert!(PreparedSemanticAnimationLookupError::InitialObjectPropertyTrack.source().is_none());
    assert!(SemanticExecutionLoweringError::InvalidCameraObject { node: id }.source().is_none());
    assert!(value_error(id).source().is_none());
}

#[test]
fn actual_invalid_root_keeps_index_and_semantics_then_recovers() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    let root = store.insert_family();
    store.add_member(root, object).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let revision = store.scene_revision();
    let nodes = store.len();
    let state = store.semantic_object_state_checked(object).unwrap().clone();
    let error = lower_semantic_execution_root(&store, object, &mut index).unwrap_err();
    let SemanticExecutionLoweringError::Object(inner) = &error else { panic!("unexpected error category"); };
    let observed = error.source().unwrap().downcast_ref::<SemanticLoweringError>().unwrap();
    assert!(std::ptr::eq(inner, observed));
    assert_eq!(observed.source().unwrap().downcast_ref::<noon_core::SemanticStoreError>(), Some(&noon_core::SemanticStoreError::NotFamily(object)));
    assert!(index.is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), nodes);
    assert_eq!(store.semantic_object_state_checked(object).unwrap(), &state);
    let output = lower_semantic_execution_root(&store, root, &mut index).unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(output.publication_context().scene_revision(), revision);
    assert_eq!(store.scene_revision(), revision);
}
'''
p = Path('crates/noon-compile/tests/error_causes.rs')
assert not p.exists()
p.write_text(compiler_tests)
p = Path('crates/noon-runtime/tests/error_causes.rs')
assert not p.exists()
p.write_text('''//! Runtime wrappers retain existing typed failures for facade/boundary mapping.
use std::error::Error;
use noon_runtime::{AuthoredPublicationError, EvaluationError, PreparedFrameCommitError,
    RetainedFamilyFramePlanError, RetainedPlannedFamilyFrameError};
use noon_compile::CompilePatchError;

fn assert_cause<C: Error + Clone + PartialEq + 'static, E: Error>(cause: C, wrap: impl FnOnce(C) -> E) {
    let expected = cause.clone();
    let error = wrap(cause);
    assert_eq!(error.source().and_then(|source| source.downcast_ref::<C>()), Some(&expected));
}

#[test]
fn publication_and_evaluation_wrappers_expose_immediate_causes() {
    assert_cause(noon_core::ReactiveError::DependencyCycle, EvaluationError::Reactive);
    assert_cause(EvaluationError::RequiredCallbackPending, AuthoredPublicationError::Evaluation);
    assert_cause(PreparedFrameCommitError::StaleTime { expected: 1.0, actual: 2.0 }, AuthoredPublicationError::PreparedFrame);
    assert_cause(CompilePatchError::InvalidTrack(noon_core::TimelineError::InvalidDuration(-1.0)), AuthoredPublicationError::Compile);
    assert!(EvaluationError::RequiredCallbackPending.source().is_none());
    assert!(AuthoredPublicationError::StaleEffectiveCarryForward.source().is_none());
}

#[test]
fn renderer_independent_family_wrappers_preserve_plan_failures() {
    let id = noon_core::SemanticStore::new().insert_family();
    assert_cause(noon_core::RetainedFamilyAnimationEvaluationError::MissingLeafDescriptor(id), RetainedFamilyFramePlanError::Evaluation);
    assert_cause(RetainedFamilyFramePlanError::ObjectIndexOutOfBounds { index: 2, object_count: 1 }, RetainedPlannedFamilyFrameError::Plan);
    assert!(RetainedPlannedFamilyFrameError::FrameShapeMismatch.source().is_none());
}

#[test]
fn publication_chain_reaches_original_compiler_timeline_failure_by_reference() {
    let error = AuthoredPublicationError::Compile(CompilePatchError::InvalidTrack(noon_core::TimelineError::InvalidDuration(-1.0)));
    let AuthoredPublicationError::Compile(inner) = &error else { unreachable!() };
    let source = error.source().unwrap().downcast_ref::<CompilePatchError>().unwrap();
    assert!(std::ptr::eq(source, inner));
    let timeline = source.source().unwrap().downcast_ref::<noon_core::TimelineError>().unwrap();
    assert_eq!(timeline, &noon_core::TimelineError::InvalidDuration(-1.0));
    assert!(timeline.source().is_none());
}
''')
print('Forwarded existing compiler/runtime causes with explicit per-domain tests.')
