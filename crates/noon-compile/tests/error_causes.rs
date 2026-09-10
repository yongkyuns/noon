//! Typed causes survive compiler wrappers without changing rejection policy.
use noon_compile::*;
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore, StoredGeometry};
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
    assert!(!error.source().unwrap().to_string().is_empty());
}
fn node() -> SemanticNodeId {
    SemanticStore::new().insert_family()
}
fn resource() -> CompiledResourceError {
    CompiledResourceError::MissingFont(noon_core::FontResourceKey {
        face_key: "missing-font".into(),
        face_index: 0,
    })
}
fn scene_error(id: SemanticNodeId) -> noon_core::SemanticSceneOperationError {
    noon_core::SemanticSceneOperationError::NotSemanticObject(id)
}
fn value_error(id: SemanticNodeId) -> SemanticLoweringError {
    SemanticLoweringError::ValueOutOfRange {
        node: id,
        field: SemanticExecutionField::Translation,
    }
}
fn compact_error() -> SemanticExecutionValueError {
    SemanticExecutionValueError::ValueOutOfRange {
        field: SemanticExecutionField::Translation,
    }
}
fn time_map() -> noon_core::CompositionTimeMapError {
    noon_core::CompositionTimeMapError::IntervalOutsideParent { index: 0 }
}

#[test]
fn compiled_errors() {
    let id = node();
    assert_cause(
        noon_core::TimelineError::InvalidDuration(-1.0),
        CompileError::InvalidTrack,
    );
    assert_cause(
        noon_core::TimelineError::InvalidDuration(-1.0),
        CompilePatchError::InvalidTrack,
    );
    assert_cause(resource(), CompilePatchError::Resource);
    assert_cause(resource(), |error| SemanticCompiledSceneError::Resource {
        node: id,
        error,
    });
}

#[test]
fn scheduled_errors() {
    let id = node();
    assert_cause(
        noon_core::SemanticAnimationError::UnknownAnimation(id),
        SemanticAnimationScheduleError::Animation,
    );
    assert_cause(
        noon_core::AnimationOptionsError::InvalidRunTime(-1.0),
        |error| SemanticAnimationScheduleError::Options {
            animation: id,
            error,
        },
    );
    assert_cause(noon_core::CompositionError::Empty, |error| {
        SemanticAnimationScheduleError::Composition {
            animation: id,
            error,
        }
    });
    assert_cause(
        noon_core::SemanticTransactionReadError::UnknownExistingNode(id),
        PreparedSemanticAnimationLookupError::Transaction,
    );
    assert_cause(
        noon_core::SemanticAnimationError::UnknownAnimation(id),
        PreparedSemanticAnimationLookupError::Existing,
    );
    assert_cause(
        PreparedSemanticAnimationLookupError::InitialObjectPropertyTrack,
        |error| PreparedSemanticAnimationScheduleError::Lookup {
            animation: id.into(),
            error,
        },
    );
    assert_cause(
        noon_core::AnimationOptionsError::InvalidRunTime(-1.0),
        |error| PreparedSemanticAnimationScheduleError::Options {
            animation: id.into(),
            error,
        },
    );
    assert_cause(noon_core::CompositionError::Empty, |error| {
        PreparedSemanticAnimationScheduleError::Composition {
            animation: id.into(),
            error,
        }
    });
    assert_cause(
        noon_core::SemanticScalarSignalQueryError::InvalidTime,
        |error| PreparedScalarAnimationTrackError::Signal { signal: id, error },
    );
    assert_cause(time_map(), |error| {
        PreparedScalarAnimationTrackError::InvalidTimeMap {
            animation: id.into(),
            error,
        }
    });
}

#[test]
fn affine_errors() {
    let id = node();
    assert_cause(
        noon_core::SemanticAnimationError::UnknownAnimation(id),
        SemanticAffineAnimationTrackError::Animation,
    );
    assert_cause(scene_error(id), |error| {
        SemanticAffineAnimationTrackError::Target {
            animation: id,
            node: id,
            error,
        }
    });
    assert_cause(time_map(), |error| {
        SemanticAffineAnimationTrackError::InvalidSubsetDisplayTimeMap {
            animation: id,
            error,
        }
    });
    assert_cause(
        noon_core::SemanticLoweringError::CoordinateOutOfRange(noon_core::SemanticVec3::new(
            f64::MAX,
            0.0,
            0.0,
        )),
        |error| SemanticAffineAnimationTrackError::InvalidTargetValue {
            animation: id,
            target_state: id,
            field: SemanticAffineAnimationField::Translation,
            error,
        },
    );
    assert_cause(value_error(id), |error| {
        SemanticAffineAnimationTrackError::InvalidTargetStyle {
            animation: id,
            target_state: id,
            error,
        }
    });
    assert_cause(noon_core::TimelineError::InvalidDuration(-1.0), |error| {
        SemanticAffineAnimationTrackError::InvalidTrack {
            animation: id,
            error,
        }
    });
    assert_cause(
        noon_core::AnimationOptionsError::UnsupportedPathArc(1.0),
        SemanticTransformToPayloadError::Options,
    );
    assert_cause(scene_error(id), |error| {
        SemanticTransformToPayloadError::Target { node: id, error }
    });
}

#[test]
fn prepared_animation_errors() {
    let id = node();
    assert_cause(
        PreparedSemanticAnimationScheduleError::InvalidStartTime(-1.0),
        PreparedSemanticAnimationLoweringError::Schedule,
    );
    assert_cause(
        TextGlyphLoweringError::MissingSemanticTarget(id.into()),
        PreparedSemanticAnimationLoweringError::TextGlyph,
    );
    assert_cause(
        noon_core::SemanticTransactionReadError::UnknownExistingNode(id),
        |error| PreparedSemanticAnimationLoweringError::Target {
            animation: id.into(),
            node: id.into(),
            error,
        },
    );
    assert_cause(time_map(), |error| {
        PreparedSemanticAnimationLoweringError::InvalidSubsetDisplayTimeMap {
            animation: id.into(),
            error,
        }
    });
    assert_cause(
        noon_core::SemanticLoweringError::CoordinateOutOfRange(noon_core::SemanticVec3::new(
            f64::MAX,
            0.0,
            0.0,
        )),
        |error| PreparedSemanticAnimationLoweringError::InvalidTargetValue {
            animation: id.into(),
            target_state: id.into(),
            field: SemanticAffineAnimationField::Translation,
            error,
        },
    );
    assert_cause(compact_error(), |error| {
        PreparedSemanticAnimationLoweringError::InvalidTargetStyle {
            animation: id.into(),
            target_state: id.into(),
            error,
        }
    });
    assert_cause(
        noon_core::RetainedAnimationMemberError::TooManyMembers(usize::MAX),
        TextGlyphLoweringError::InvalidMembers,
    );
    assert_cause(
        noon_core::RetainedFamilyAnimationMemberPlanError::Members(
            noon_core::RetainedAnimationMemberError::TooManyMembers(usize::MAX),
        ),
        TextGlyphLoweringError::InvalidPlan,
    );
    assert_cause(
        noon_core::FamilyAnimationError::InvalidDuration(-1.0),
        TextGlyphLoweringError::InvalidSpec,
    );
    assert_cause(time_map(), TextGlyphLoweringError::InvalidTimeMap);
}

#[test]
fn initial_animation_errors() {
    let id = node();
    assert_cause(
        SemanticAnimationScheduleError::InvalidStartTime(-1.0),
        SemanticInitialAnimationError::Schedule,
    );
    assert_cause(
        TextGlyphLoweringError::MissingSemanticTarget(id.into()),
        SemanticInitialAnimationError::Family,
    );
    assert_cause(
        noon_core::SemanticAnimationError::UnknownAnimation(id),
        SemanticInitialAnimationError::Animation,
    );
    assert_cause(scene_error(id), |error| {
        SemanticInitialAnimationError::Endpoint {
            animation: id,
            node: id,
            error,
        }
    });
    assert_cause(value_error(id), |error| {
        SemanticInitialAnimationError::EndpointValue {
            animation: id,
            node: id,
            error,
        }
    });
    assert_cause(
        SemanticGeometryValueError::InvalidAnalyticGeometry,
        |error| SemanticInitialAnimationError::EndpointGeometry {
            animation: id,
            node: id,
            error,
        },
    );
    assert_cause(
        CompilePatchError::InvalidTrack(noon_core::TimelineError::InvalidDuration(-1.0)),
        SemanticInitialAnimationError::Compiled,
    );
}

#[test]
fn publication_and_reactive_errors() {
    let id = node();
    assert_cause(
        noon_core::SemanticStoreError::UnknownNode(id),
        SemanticLoweringError::Store,
    );
    assert_cause(compact_error(), |error| {
        SemanticPublicationLoweringError::PreparedValue {
            object: id.into(),
            error,
        }
    });
    assert_cause(
        SemanticGeometryValueError::InvalidAnalyticGeometry,
        |error| SemanticPublicationLoweringError::PreparedGeometry {
            object: id.into(),
            error,
        },
    );
    assert_cause(
        SemanticCompiledSceneError::InvalidAnalyticGeometry { node: id },
        |error| SemanticPublicationLoweringError::PreparedContent {
            object: id.into(),
            error,
        },
    );
    assert_cause(
        noon_core::SemanticTransactionReadError::UnknownExistingNode(id),
        SemanticPublicationLoweringError::Read,
    );
    assert_cause(value_error(id), SemanticPublicationLoweringError::Value);
    assert_cause(
        noon_core::SemanticSignalError::UnknownSignal(id),
        SemanticReactiveLoweringError::Signal,
    );
    assert_cause(
        noon_core::ReactiveError::DependencyCycle,
        SemanticReactiveLoweringError::Reactive,
    );
    assert_cause(
        SemanticReactiveLoweringError::DependencyCycle(id),
        PreparedScalarSignalTimelineError::Lowering,
    );
    assert_cause(value_error(id), SemanticExecutionLoweringError::Object);
    assert_cause(
        SemanticReactiveLoweringError::DependencyCycle(id),
        SemanticExecutionLoweringError::Reactive,
    );
    assert_cause(
        SemanticCompiledSceneError::InvalidAnalyticGeometry { node: id },
        SemanticExecutionLoweringError::Compiled,
    );
    assert_cause(
        SemanticInitialAnimationError::InvalidOrigin(-1.0),
        SemanticExecutionLoweringError::InitialAnimation,
    );
}

#[test]
fn prepared_lookup_preserves_a_multilevel_chain_and_leaf_termination() {
    let id = node();
    let error = PreparedSemanticAnimationLoweringError::Schedule(
        PreparedSemanticAnimationScheduleError::Lookup {
            animation: id.into(),
            error: PreparedSemanticAnimationLookupError::Transaction(
                noon_core::SemanticTransactionReadError::UnknownExistingNode(id),
            ),
        },
    );
    let schedule = error
        .source()
        .unwrap()
        .downcast_ref::<PreparedSemanticAnimationScheduleError>()
        .unwrap();
    let lookup = schedule
        .source()
        .unwrap()
        .downcast_ref::<PreparedSemanticAnimationLookupError>()
        .unwrap();
    let read = lookup
        .source()
        .unwrap()
        .downcast_ref::<noon_core::SemanticTransactionReadError>()
        .unwrap();
    assert_eq!(
        read,
        &noon_core::SemanticTransactionReadError::UnknownExistingNode(id)
    );
    assert!(read.source().is_none());
    assert!(
        PreparedSemanticAnimationLookupError::InitialObjectPropertyTrack
            .source()
            .is_none()
    );
    assert!(
        SemanticExecutionLoweringError::InvalidCameraObject { node: id }
            .source()
            .is_none()
    );
    assert!(value_error(id).source().is_none());
}

#[test]
fn actual_invalid_root_keeps_index_and_semantics_then_recovers() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let root = store.insert_family();
    store.add_member(root, object).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let revision = store.scene_revision();
    let nodes = store.len();
    let state = store.semantic_object_state_checked(object).unwrap().clone();
    let error = lower_semantic_execution_root(&store, object, &mut index).unwrap_err();
    let SemanticExecutionLoweringError::Object(inner) = &error else {
        panic!("unexpected error category");
    };
    let observed = error
        .source()
        .unwrap()
        .downcast_ref::<SemanticLoweringError>()
        .unwrap();
    assert!(std::ptr::eq(inner, observed));
    assert_eq!(
        observed
            .source()
            .unwrap()
            .downcast_ref::<noon_core::SemanticStoreError>(),
        Some(&noon_core::SemanticStoreError::NotFamily(object))
    );
    assert!(index.is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), nodes);
    assert_eq!(store.semantic_object_state_checked(object).unwrap(), &state);
    let output = lower_semantic_execution_root(&store, root, &mut index).unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(output.publication_context().scene_revision(), revision);
    assert_eq!(store.scene_revision(), revision);
}
