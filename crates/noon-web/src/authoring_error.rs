//! Error *representation* at the optional language boundary, not semantic policy.
//!
//! Shared Rust producers remain authoritative. Match their variants here, never
//! diagnostic text. Only failure paths allocate this projection; native/direct
//! Rust engine calls do not depend on it. Unmigrated String producers explicitly
//! remain unclassified until their R2 domain supplies a typed cause.

use std::error::Error;

use noon::{
    AuthoringError, ExecutionSegmentCompletionError, ExecutionSegmentError,
    ExecutionSessionCallbackError, ExecutionSessionCallbackReadError,
    ExecutionSessionPublicationError, LiveSessionError,
};
use noon_core::{
    SemanticMutationTransactionError, SemanticSceneOperationError, SemanticStoreError,
};

/// Language-boundary diagnostics projected from shared Rust errors.
///
/// This is an optional host representation, not the Rust engine's error API.
#[derive(Debug)]
pub struct AuthoringFailure {
    pub category: &'static str,
    pub code: &'static str,
    pub message: String,
    pub cause: Option<Box<Self>>,
}

impl AuthoringFailure {
    pub(crate) fn new(category: &'static str, code: &'static str, message: impl ToString) -> Self {
        Self {
            category,
            code,
            message: message.to_string(),
            cause: None,
        }
    }

    pub(crate) fn with_message(mut self, message: impl ToString) -> Self {
        self.message = message.to_string();
        self
    }

    fn caused_by(code: &'static str, message: impl ToString, cause: Self) -> Self {
        Self {
            category: cause.category,
            code,
            message: message.to_string(),
            cause: Some(Box::new(cause)),
        }
    }

    /// Preserve diagnostics/sources for a producer outside this slice, without
    /// pretending that wording proves an input, capability or lifecycle category.
    fn unclassified(code: &'static str, error: &(dyn Error + 'static)) -> Self {
        Self {
            category: "unclassified",
            code,
            message: error.to_string(),
            cause: error
                .source()
                .map(|cause| Box::new(Self::unclassified("source", cause))),
        }
    }
}

impl std::fmt::Display for AuthoringFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for AuthoringFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|cause| cause as &dyn Error)
    }
}
impl From<String> for AuthoringFailure {
    fn from(message: String) -> Self {
        Self::new("unclassified", "unclassified", message)
    }
}
impl From<&str> for AuthoringFailure {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

impl From<AuthoringError> for AuthoringFailure {
    fn from(error: AuthoringError) -> Self {
        let message = error.to_string();
        match error {
            AuthoringError::ForeignStore => {
                Self::new("foreign_handle", "authoring.foreign_store", message)
            }
            AuthoringError::Semantic(cause) => {
                Self::caused_by("authoring.semantic", message, cause.into())
            }
            AuthoringError::Transaction(cause) => {
                Self::caused_by("authoring.transaction", message, cause.into())
            }
            AuthoringError::MissingGeometryResource(_) => Self::new(
                "missing_resource",
                "authoring.missing_geometry_resource",
                message,
            ),
            AuthoringError::MissingTextResource(_) => Self::new(
                "missing_resource",
                "authoring.missing_text_resource",
                message,
            ),
            // The public producer is non-exhaustive; future domains must opt in
            // with a reviewed projection, not silently acquire a guessed class.
            other => Self::unclassified("authoring.unclassified", &other),
        }
    }
}

impl From<SemanticSceneOperationError> for AuthoringFailure {
    fn from(error: SemanticSceneOperationError) -> Self {
        use SemanticSceneOperationError as E;
        let message = error.to_string();
        let (category, code) = match error {
            E::UnknownNode(_) => ("stale_handle", "semantic.unknown_node"),
            E::NotSemanticObject(_) => ("invalid_input", "semantic.not_object"),
            E::NotSemanticFamily(_) => ("invalid_input", "semantic.not_family"),
            E::NotSemanticAuthoringNode(_) => ("invalid_input", "semantic.not_authoring_node"),
            E::DuplicateMembershipTarget(_) => ("invalid_input", "membership.duplicate_target"),
            E::MissingMembershipTarget(_) => ("invalid_input", "membership.missing_target"),
            E::AmbiguousMembershipTarget(_) => ("invalid_input", "membership.ambiguous_target"),
            E::AmbiguousCrossRootAlias(_) => {
                ("unsupported_operation", "membership.cross_root_alias")
            }
            E::Store(cause) => return Self::caused_by("semantic.store", message, cause.into()),
        };
        Self::new(category, code, message)
    }
}

impl From<SemanticStoreError> for AuthoringFailure {
    fn from(error: SemanticStoreError) -> Self {
        use SemanticStoreError as E;
        let (category, code) = match &error {
            E::UnknownNode(_) => ("stale_handle", "store.unknown_node"),
            E::NotFamily(_) => ("invalid_input", "store.not_family"),
            E::NotFamilyMember { .. } => ("invalid_input", "store.not_family_member"),
            E::FamilyCycle { .. } => ("invalid_input", "store.family_cycle"),
            E::DuplicateSourceIdentity(_) => ("invalid_input", "store.duplicate_source_identity"),
        };
        Self::new(category, code, error)
    }
}

impl From<SemanticMutationTransactionError> for AuthoringFailure {
    fn from(error: SemanticMutationTransactionError) -> Self {
        use SemanticMutationTransactionError as E;
        let message = error.to_string();
        match error {
            E::Object { error, .. } => Self::caused_by("transaction.object", message, error.into()),
            E::Family { error, .. } => Self::caused_by("transaction.family", message, error.into()),
            E::AnimationTarget { error, .. } => {
                Self::caused_by("transaction.animation_target", message, error.into())
            }
            E::Node { error, .. } => Self::caused_by("transaction.node", message, error.into()),
            E::NonFinitePropertyValue { .. } => Self::new(
                "invalid_input",
                "transaction.non_finite_property_value",
                message,
            ),
            E::UnsupportedPropertyWrite { .. } => Self::new(
                "unsupported_operation",
                "transaction.unsupported_property_write",
                message,
            ),
            E::DuplicateFamilyEdge { .. } => Self::new(
                "invalid_input",
                "transaction.duplicate_family_edge",
                message,
            ),
            E::DuplicateFamilyOrder { .. } => Self::new(
                "invalid_input",
                "transaction.duplicate_family_order",
                message,
            ),
            E::FamilyEdgeUsesRemovedNode { .. }
            | E::FamilyOrderUsesRemovedNode { .. }
            | E::TargetRemoved { .. } => {
                Self::new("stale_handle", "transaction.removed_node", message)
            }
            other => Self::unclassified("transaction.unclassified", &other),
        }
    }
}

// Callback transactions already have typed shared errors. Preserve their domain
// causes here; decoder/player-local String failures remain explicitly unclassified.
impl From<ExecutionSessionCallbackReadError> for AuthoringFailure {
    fn from(error: ExecutionSessionCallbackReadError) -> Self {
        use ExecutionSessionCallbackReadError as E;
        let message = error.to_string();
        match error {
            E::NoPendingPhase => Self::new(
                "stale_publication",
                "callback_read.no_pending_phase",
                message,
            ),
            E::StaleToken { expected, actual } => Self::new(
                "stale_publication",
                "callback_read.stale_token",
                format!("{message}; expected {expected:?}, actual {actual:?}"),
            ),
            E::UnknownSignal(_) => {
                Self::new("stale_handle", "callback_read.unknown_signal", message)
            }
            E::NonScalarSignal(_) => {
                Self::new("invalid_input", "callback_read.non_scalar_signal", message)
            }
            E::UnknownObject(_) => {
                Self::new("stale_handle", "callback_read.unknown_object", message)
            }
        }
    }
}

impl From<ExecutionSessionCallbackError> for AuthoringFailure {
    fn from(error: ExecutionSessionCallbackError) -> Self {
        use ExecutionSessionCallbackError as E;
        let message = error.to_string();
        match error {
            E::NoPendingPhase => {
                Self::new("stale_publication", "callback.no_pending_phase", message)
            }
            E::StaleToken { expected, actual } => Self::new(
                "stale_publication",
                "callback.stale_token",
                format!("{message}; expected {expected:?}, actual {actual:?}"),
            ),
            E::UnknownObject(_) => Self::new("stale_handle", "callback.unknown_object", message),
            E::Read(cause) => Self::caused_by("callback.read", message, cause.into()),
            E::Evaluation(cause) => Self::caused_by(
                "callback.evaluation",
                message,
                Self::unclassified("runtime.evaluation", &cause),
            ),
            E::InvalidEffectiveWrite(cause) => Self::caused_by(
                "callback.invalid_effective_write",
                message,
                Self::unclassified("runtime.effective_write", &cause),
            ),
            E::Commit(cause) => Self::caused_by(
                "callback.commit",
                message,
                Self::unclassified("runtime.frame_commit", &cause),
            ),
            // Advance/driver policy is outside this transaction-boundary slice.
            other => Self::unclassified("callback.unclassified", &other),
        }
    }
}

impl From<ExecutionSessionPublicationError> for AuthoringFailure {
    fn from(error: ExecutionSessionPublicationError) -> Self {
        use ExecutionSessionPublicationError as E;
        let message = error.to_string();
        match error {
            E::RequiredCallbackPending => {
                Self::new("pending_work", "publication.callback_pending", message)
            }
            E::SegmentCompletionPending => {
                Self::new("pending_work", "publication.segment_pending", message)
            }
            E::ForeignSemanticStore => {
                Self::new("foreign_handle", "publication.foreign_store", message)
            }
            E::StaleSceneRevision { .. } => Self::new(
                "stale_publication",
                "publication.stale_scene_revision",
                message,
            ),
            E::UnknownObject(_) => Self::new("stale_handle", "publication.unknown_object", message),
            E::Semantic(cause) => Self::caused_by("publication.semantic", message, cause.into()),
            E::Lowering(cause) => Self::caused_by("publication.lowering", message, cause.into()),
            E::Runtime(cause) => Self::caused_by(
                "publication.runtime",
                message,
                Self::unclassified("runtime.publication", &cause),
            ),
            E::ExecutionSlot(cause) => Self::caused_by(
                "publication.execution_slot",
                message,
                Self::unclassified("runtime.execution_slot", &cause),
            ),
        }
    }
}

impl From<noon_compile::SemanticPublicationLoweringError> for AuthoringFailure {
    fn from(error: noon_compile::SemanticPublicationLoweringError) -> Self {
        use noon_compile::SemanticPublicationLoweringError as E;
        let code = match &error {
            E::UnsupportedMutation { .. } => "lowering.unsupported_mutation",
            E::UnsupportedReactiveMembership { .. } => "lowering.unsupported_reactive_membership",
            E::UnsupportedTextMembership { .. } => "lowering.unsupported_text_membership",
            E::UnsupportedCameraMembership { .. } => "lowering.unsupported_camera_membership",
            E::UnsupportedNodeRemoval { .. } => "lowering.unsupported_node_removal",
            // Other lowering producer domains are consumed by later R2 slices.
            _ => return Self::unclassified("lowering.unclassified", &error),
        };
        Self::new("unsupported_operation", code, error)
    }
}

impl From<ExecutionSegmentError> for AuthoringFailure {
    fn from(error: ExecutionSegmentError) -> Self {
        let code = match &error {
            ExecutionSegmentError::InvalidDuration(_) => "segment.invalid_duration",
            ExecutionSegmentError::EndTimeOverflow { .. } => "segment.end_time_overflow",
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<ExecutionSegmentCompletionError> for AuthoringFailure {
    fn from(error: ExecutionSegmentCompletionError) -> Self {
        use ExecutionSegmentCompletionError as E;
        let message = error.to_string();
        match error {
            E::ForeignSegment { .. } => {
                Self::new("foreign_handle", "completion.foreign_segment", message)
            }
            E::NoPendingCompletion => Self::new(
                "stale_publication",
                "completion.no_pending_completion",
                message,
            ),
            E::StaleSegment { .. } => {
                Self::new("stale_publication", "completion.stale_segment", message)
            }
            E::NotAtBoundary { .. } => {
                Self::new("pending_work", "completion.not_at_boundary", message)
            }
            E::RequiredCallbackPending => {
                Self::new("pending_work", "completion.callback_pending", message)
            }
            E::CallbackNotCoherent => {
                Self::new("pending_work", "completion.callback_not_coherent", message)
            }
            E::CallbackTerminated(_) => Self::new(
                "callback_failure",
                "completion.callback_terminated",
                message,
            ),
            E::MissingLifecycleRoot(_) => {
                Self::new("stale_handle", "completion.missing_lifecycle_root", message)
            }
            E::UnsupportedHostDriverRelease(_) => Self::new(
                "unsupported_operation",
                "completion.unsupported_host_driver_release",
                message,
            ),
            E::Publication(cause) => {
                Self::caused_by("completion.publication", message, cause.into())
            }
            other => Self::unclassified("completion.unclassified", &other),
        }
    }
}

impl From<LiveSessionError> for AuthoringFailure {
    fn from(error: LiveSessionError) -> Self {
        use LiveSessionError as E;
        let message = error.to_string();
        match error {
            E::Authoring(cause) => Self::caused_by("live.authoring", message, cause.into()),
            E::ForeignMobjectStore => Self::new("foreign_handle", "live.foreign_store", message),
            E::Segment(cause) => Self::caused_by("live.segment", message, cause.into()),
            E::Completion(cause) => Self::caused_by("live.completion", message, cause.into()),
            E::Publication(cause) => Self::caused_by("live.publication", message, cause.into()),
            other => Self::unclassified("live.unclassified", &other),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl From<crate::canonical_authoring_scene::PlayerReturnError> for AuthoringFailure {
    fn from(error: crate::canonical_authoring_scene::PlayerReturnError) -> Self {
        use crate::canonical_authoring_scene::PlayerReturnError as E;
        let code = match error {
            E::NotLeased => "ownership.not_leased",
            E::ForeignScene => "ownership.foreign_scene",
            E::StaleLease => "ownership.stale_lease",
        };
        Self::new("ownership", code, error)
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn js_error(error: impl Into<AuthoringFailure>) -> wasm_bindgen::JsValue {
    use wasm_bindgen::JsValue;
    let error = error.into();
    let object = js_sys::Error::new(&error.message);
    object.set_name("NoonError");
    for (name, value) in [
        ("noonErrorVersion", JsValue::from_f64(1.0)),
        ("category", JsValue::from_str(error.category)),
        ("code", JsValue::from_str(error.code)),
    ] {
        js_sys::Reflect::set(&object, &JsValue::from_str(name), &value)
            .expect("new Error object accepts own diagnostic properties");
    }
    if let Some(cause) = error.cause {
        js_sys::Reflect::set(&object, &JsValue::from_str("cause"), &js_error(*cause))
            .expect("new Error object accepts its cause");
    }
    object.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::SemanticNodeId;

    #[test]
    fn typed_membership_chain_preserves_categories_and_sources() {
        let node = SemanticNodeId::new(7, 3);
        let failure = AuthoringFailure::from(LiveSessionError::Authoring(
            AuthoringError::Semantic(SemanticSceneOperationError::MissingMembershipTarget(node)),
        ));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "live.authoring");
        let authoring = failure.cause.as_ref().unwrap();
        assert_eq!(authoring.code, "authoring.semantic");
        assert_eq!(
            authoring.cause.as_ref().unwrap().code,
            "membership.missing_target"
        );
        assert!(failure.source().is_some());
        assert!(!failure.message.is_empty());
    }

    #[test]
    fn diagnostics_cannot_select_categories() {
        for text in [
            "unsupported",
            "foreign store",
            "stale handle",
            "pending callback",
        ] {
            assert_eq!(AuthoringFailure::from(text).category, "unclassified");
        }
        let node = SemanticNodeId::new(7, 3);
        assert_eq!(
            AuthoringFailure::from(AuthoringError::ForeignStore).category,
            "foreign_handle"
        );
        assert_eq!(
            AuthoringFailure::from(SemanticSceneOperationError::UnknownNode(node)).category,
            "stale_handle"
        );
        assert_eq!(
            AuthoringFailure::from(SemanticSceneOperationError::AmbiguousCrossRootAlias(node))
                .category,
            "unsupported_operation"
        );
        assert_eq!(
            AuthoringFailure::from(ExecutionSessionPublicationError::RequiredCallbackPending)
                .category,
            "pending_work"
        );
    }
}
