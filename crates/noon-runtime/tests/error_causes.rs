//! Runtime wrappers retain existing typed failures for facade/boundary mapping.
use noon_compile::CompilePatchError;
use noon_runtime::{
    AuthoredPublicationError, EvaluationError, PreparedFrameCommitError,
    RetainedFamilyFramePlanError, RetainedPlannedFamilyFrameError,
};
use std::error::Error;

fn assert_cause<C: Error + Clone + PartialEq + 'static, E: Error>(
    cause: C,
    wrap: impl FnOnce(C) -> E,
) {
    let expected = cause.clone();
    let error = wrap(cause);
    assert_eq!(
        error.source().and_then(|source| source.downcast_ref::<C>()),
        Some(&expected)
    );
}

#[test]
fn publication_and_evaluation_wrappers_expose_immediate_causes() {
    assert_cause(
        noon_core::ReactiveError::DependencyCycle,
        EvaluationError::Reactive,
    );
    assert_cause(
        EvaluationError::RequiredCallbackPending,
        AuthoredPublicationError::Evaluation,
    );
    assert_cause(
        PreparedFrameCommitError::StaleTime {
            expected: 1.0,
            actual: 2.0,
        },
        AuthoredPublicationError::PreparedFrame,
    );
    assert_cause(
        CompilePatchError::InvalidTrack(noon_core::TimelineError::InvalidDuration(-1.0)),
        AuthoredPublicationError::Compile,
    );
    assert!(EvaluationError::RequiredCallbackPending.source().is_none());
    assert!(AuthoredPublicationError::StaleEffectiveCarryForward
        .source()
        .is_none());
}

#[test]
fn renderer_independent_family_wrappers_preserve_plan_failures() {
    let id = noon_core::SemanticStore::new().insert_family();
    assert_cause(
        noon_core::RetainedFamilyAnimationEvaluationError::MissingLeafDescriptor(id),
        RetainedFamilyFramePlanError::Evaluation,
    );
    assert_cause(
        RetainedFamilyFramePlanError::ObjectIndexOutOfBounds {
            index: 2,
            object_count: 1,
        },
        RetainedPlannedFamilyFrameError::Plan,
    );
    assert!(RetainedPlannedFamilyFrameError::FrameShapeMismatch
        .source()
        .is_none());
}

#[test]
fn publication_chain_reaches_original_compiler_timeline_failure_by_reference() {
    let error = AuthoredPublicationError::Compile(CompilePatchError::InvalidTrack(
        noon_core::TimelineError::InvalidDuration(-1.0),
    ));
    let AuthoredPublicationError::Compile(inner) = &error else {
        unreachable!()
    };
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<CompilePatchError>()
        .unwrap();
    assert!(std::ptr::eq(source, inner));
    let timeline = source
        .source()
        .unwrap()
        .downcast_ref::<noon_core::TimelineError>()
        .unwrap();
    assert_eq!(timeline, &noon_core::TimelineError::InvalidDuration(-1.0));
    assert!(timeline.source().is_none());
}
