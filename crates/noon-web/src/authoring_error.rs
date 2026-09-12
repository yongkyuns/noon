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
    pub(crate) fn unclassified(code: &'static str, error: &(dyn Error + 'static)) -> Self {
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
impl From<noon::FamilyCallbackPaintError> for AuthoringFailure {
    fn from(error: noon::FamilyCallbackPaintError) -> Self {
        use noon::FamilyCallbackPaintError::*;
        match error {
            Authoring(e) => Self::from(e),
            Store(e) => Self::from(e),
            Callback(e) => Self::caused_by("callback.family.read", e.to_string(), e.into()),
            StaleRevision { .. } => {
                Self::new("stale_handle", "callback.family.stale_revision", error)
            }
            InvalidPaint(cause) => Self::caused_by(
                "callback.family.invalid_paint",
                cause.to_string(),
                cause.into(),
            ),
        }
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
            AuthoringError::Unsupported(cause) => Self::caused_by(
                "authoring.unsupported",
                message,
                Self::new(
                    "unsupported_operation",
                    "authoring.unsupported_operation",
                    cause,
                ),
            ),
            AuthoringError::InvalidRenderNumber { .. } => {
                Self::new("invalid_input", "authoring.invalid_render_number", message)
            }
            AuthoringError::NonPositiveNumber { .. } => {
                Self::new("invalid_input", "authoring.non_positive_number", message)
            }
            AuthoringError::InvalidOpacity { .. } => {
                Self::new("invalid_input", "authoring.invalid_opacity", message)
            }
            AuthoringError::InvalidEllipseDimensions { .. } => Self::new(
                "invalid_input",
                "authoring.invalid_ellipse_dimensions",
                message,
            ),
            AuthoringError::NonFiniteGeometry => {
                Self::new("invalid_input", "authoring.non_finite_geometry", message)
            }
            AuthoringError::NonFiniteObjectState => Self::new(
                "invalid_input",
                "authoring.non_finite_object_state",
                message,
            ),
            AuthoringError::NonFiniteTransform => {
                Self::new("invalid_input", "authoring.non_finite_transform", message)
            }
            AuthoringError::NonFiniteStyle => {
                Self::new("invalid_input", "authoring.non_finite_style", message)
            }
            AuthoringError::NegativeStrokeWidth(_) => {
                Self::new("invalid_input", "authoring.negative_stroke_width", message)
            }
            AuthoringError::InvalidStrokeWidthMode(_) => Self::new(
                "invalid_input",
                "authoring.invalid_stroke_width_mode",
                message,
            ),
            AuthoringError::InvalidStrokeJoin(_) => {
                Self::new("invalid_input", "authoring.invalid_stroke_join", message)
            }
            AuthoringError::InvalidStrokeCap(_) => {
                Self::new("invalid_input", "authoring.invalid_stroke_cap", message)
            }
            AuthoringError::NonFiniteDirection => {
                Self::new("invalid_input", "authoring.non_finite_direction", message)
            }
            AuthoringError::EmptyColorGradient => {
                Self::new("invalid_input", "authoring.empty_color_gradient", message)
            }
            AuthoringError::ZeroDirection => {
                Self::new("invalid_input", "authoring.zero_direction", message)
            }
            AuthoringError::NonFiniteLineEndpoints => Self::new(
                "invalid_input",
                "authoring.non_finite_line_endpoints",
                message,
            ),
            AuthoringError::DegenerateLine => {
                Self::new("invalid_input", "authoring.degenerate_line", message)
            }
            AuthoringError::UnorderedBounds(_) => {
                Self::new("invalid_input", "authoring.unordered_bounds", message)
            }
            AuthoringError::InvalidDimension(_) => {
                Self::new("invalid_input", "authoring.invalid_dimension", message)
            }
            AuthoringError::ZeroStretchTarget => {
                Self::new("invalid_input", "authoring.zero_stretch_target", message)
            }
            AuthoringError::ZeroMatchHeight => {
                Self::new("invalid_input", "authoring.zero_match_height", message)
            }
            AuthoringError::ZeroMatchWidth => {
                Self::new("invalid_input", "authoring.zero_match_width", message)
            }
            AuthoringError::MissingLayoutBounds(_) => {
                Self::new("invalid_input", "authoring.missing_layout_bounds", message)
            }
            AuthoringError::InvalidSubmobjectIndex { .. } => Self::new(
                "invalid_input",
                "authoring.invalid_submobject_index",
                message,
            ),
            AuthoringError::InvalidGridDimensions { .. } => Self::new(
                "invalid_input",
                "authoring.invalid_grid_dimensions",
                message,
            ),
            AuthoringError::InvalidFlipAxis => {
                Self::new("invalid_input", "authoring.invalid_flip_axis", message)
            }
            AuthoringError::InvalidGridOption(_) => {
                Self::new("invalid_input", "authoring.invalid_grid_option", message)
            }
            AuthoringError::InsufficientGridCapacity { .. } => Self::new(
                "invalid_input",
                "authoring.insufficient_grid_capacity",
                message,
            ),
            AuthoringError::AlreadyScopedTracker(_) => {
                Self::new("invalid_input", "authoring.already_scoped_tracker", message)
            }
            AuthoringError::EmptyInputName { .. } => {
                Self::new("invalid_input", "authoring.empty_input_name", message)
            }
            AuthoringError::CameraRequiresEmptyScene(_) => Self::new(
                "invalid_input",
                "authoring.camera_requires_empty_scene",
                message,
            ),
            AuthoringError::FamilyPairing(cause) => {
                Self::caused_by("authoring.family_pairing", message, cause.into())
            }
            AuthoringError::VectorLowering(cause) => Self::caused_by(
                "authoring.vector_lowering",
                message,
                Self::new("invalid_input", "vector.invalid_coordinates", cause),
            ),
            AuthoringError::GeometryResource(cause) => {
                Self::caused_by("authoring.geometry_resource", message, cause.into())
            }
            AuthoringError::PathQuery(cause) => {
                Self::new("invalid_input", "path.invalid_query", cause)
            }
            AuthoringError::Boolean(cause) => {
                Self::new("invalid_input", "path.invalid_boolean", cause)
            }
            AuthoringError::Arc(cause) => Self::caused_by(
                "authoring.arc",
                message,
                Self::new("invalid_input", "arc.invalid_input", cause),
            ),
            AuthoringError::Elbow(cause) => Self::caused_by(
                "authoring.elbow",
                message,
                Self::new("invalid_input", "elbow.invalid_input", cause),
            ),
            AuthoringError::RoundedRectangle(cause) => Self::caused_by(
                "authoring.rounded_rectangle",
                message,
                Self::new("invalid_input", "rounded_rectangle.invalid_input", cause),
            ),
            AuthoringError::DashedLine(cause) => Self::caused_by(
                "authoring.dashed_line",
                message,
                Self::new("invalid_input", "dashed_line.invalid_input", cause),
            ),
            AuthoringError::Signal(cause) => {
                Self::caused_by("authoring.signal", message, cause.into())
            }
            AuthoringError::SignalBinding(cause) => {
                Self::caused_by("authoring.signal_binding", message, cause.into())
            }
            AuthoringError::ScalarQuery(cause) => {
                Self::caused_by("authoring.scalar_query", message, cause.into())
            }
            // The public producer is non-exhaustive; future domains must opt in
            // with a reviewed projection, not silently acquire a guessed class.
            other => Self::unclassified("authoring.unclassified", &other),
        }
    }
}

impl From<noon_core::AnimationOptionsError> for AuthoringFailure {
    fn from(error: noon_core::AnimationOptionsError) -> Self {
        use noon_core::AnimationOptionsError as E;
        let (category, code) = match &error {
            E::UnsupportedPathArc(_) => ("unsupported_operation", "animation.unsupported_path_arc"),
            E::UnsupportedReverseRateFunction => {
                ("unsupported_operation", "animation.unsupported_reverse")
            }
            E::InvalidRunTime(_) => ("invalid_input", "animation.invalid_run_time"),
            E::InvalidLagRatio(_) => ("invalid_input", "animation.invalid_lag_ratio"),
            E::InvalidPathArc(_) => ("invalid_input", "animation.invalid_path_arc"),
        };
        Self::new(category, code, error)
    }
}

impl From<noon_core::GeometryResourceError> for AuthoringFailure {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        use noon_core::GeometryResourceError as E;
        match error {
            E::NonFinitePath => Self::new("invalid_input", "geometry.non_finite_path", error),
            E::UnknownResource(_) => {
                Self::new("missing_resource", "geometry.unknown_resource", error)
            }
            _ => Self::unclassified("geometry.unclassified", &error),
        }
    }
}

impl From<noon_core::SemanticFamilyPairingError> for AuthoringFailure {
    fn from(error: noon_core::SemanticFamilyPairingError) -> Self {
        use noon_core::SemanticFamilyPairingError as E;
        let (category, code) = match &error {
            E::UnknownNode(_) => ("stale_handle", "family_pairing.unknown_node"),
            E::RootIsNotFamily(_) => ("invalid_input", "family_pairing.not_family"),
            E::Empty => ("invalid_input", "family_pairing.empty"),
            E::TopologyMismatch { .. } => {
                ("unsupported_operation", "family_pairing.topology_mismatch")
            }
            E::UnsupportedLeaf(_) => ("unsupported_operation", "family_pairing.unsupported_leaf"),
            E::AliasMismatch { .. } => ("unsupported_operation", "family_pairing.alias_mismatch"),
        };
        Self::new(category, code, error)
    }
}

impl From<noon_core::SemanticSignalError> for AuthoringFailure {
    fn from(error: noon_core::SemanticSignalError) -> Self {
        use noon_core::SemanticSignalError as E;
        let (category, code) = match &error {
            E::UnknownSignal(_) => ("stale_handle", "signal.unknown_signal"),
            E::NotSignal(_) => ("invalid_input", "signal.not_signal"),
            E::NonFiniteValue => ("invalid_input", "signal.non_finite_value"),
            E::DependencyCycle(_) => ("invalid_input", "signal.dependency_cycle"),
            E::InvalidNativeInputInitialValue { .. } => {
                ("invalid_input", "signal.invalid_initial_value")
            }
            E::InvalidUnaryExpression { .. } => {
                ("invalid_input", "signal.invalid_unary_expression")
            }
            E::InvalidBinaryExpression { .. } => {
                ("invalid_input", "signal.invalid_binary_expression")
            }
            E::SourceTypeMismatch { .. } => ("invalid_input", "signal.source_type_mismatch"),
            E::NativeInputRequiresInputSignal { .. } => {
                ("invalid_input", "signal.native_input_requires_input")
            }
            E::NativeInputTypeMismatch { .. } => {
                ("invalid_input", "signal.native_input_type_mismatch")
            }
            E::TimelineOwnedSignal { .. } => ("unsupported_operation", "signal.timeline_owned"),
            E::NativeOwnedSignal { .. } => ("unsupported_operation", "signal.native_owned"),
        };
        Self::new(category, code, error)
    }
}

impl From<noon_core::SemanticSignalBindingError> for AuthoringFailure {
    fn from(error: noon_core::SemanticSignalBindingError) -> Self {
        use noon_core::SemanticSignalBindingError as E;
        let message = error.to_string();
        match error {
            E::Signal(cause) => Self::caused_by("binding.signal", message, cause.into()),
            E::Target(cause) => Self::caused_by("binding.target", message, cause.into()),
            E::TypeMismatch { .. } => Self::new("invalid_input", "binding.type_mismatch", message),
            E::PropertyAlreadyBound { .. } => {
                Self::new("invalid_input", "binding.property_already_bound", message)
            }
        }
    }
}

impl From<noon_core::SemanticScalarSignalQueryError> for AuthoringFailure {
    fn from(error: noon_core::SemanticScalarSignalQueryError) -> Self {
        use noon_core::SemanticScalarSignalQueryError as E;
        let message = error.to_string();
        match error {
            E::Signal(cause) => Self::caused_by("scalar_query.signal", message, cause.into()),
            E::NotInputSignal(_) => Self::new("invalid_input", "scalar_query.not_input", message),
            E::NonScalarSignal(_) => Self::new("invalid_input", "scalar_query.non_scalar", message),
            E::InvalidTime => Self::new("invalid_input", "scalar_query.invalid_time", message),
        }
    }
}

impl From<noon::TextAuthoringError> for AuthoringFailure {
    fn from(error: noon::TextAuthoringError) -> Self {
        use noon::TextAuthoringError as E;
        let message = error.to_string();
        match error {
            E::InvalidFontSize(_) => Self::new("invalid_input", "text.invalid_font_size", message),
            E::InvalidOpacity(_) => Self::new("invalid_input", "text.invalid_opacity", message),
            E::FontUnavailable(_) => {
                Self::new("missing_resource", "text.font_unavailable", message)
            }
            E::MissingGeometryResource => {
                Self::new("missing_resource", "text.missing_geometry", message)
            }
            E::MissingFontResource => Self::new("missing_resource", "text.missing_font", message),
            E::Semantic(cause) => Self::caused_by("text.authoring", message, cause.into()),
            E::Import(cause) => Self::caused_by("text.import", message, cause.into()),
            // Provider diagnostics remain attached, without inventing provider policy.
            other => Self::unclassified("text.provider", &other),
        }
    }
}

impl From<noon_core::SemanticTextImportError> for AuthoringFailure {
    fn from(error: noon_core::SemanticTextImportError) -> Self {
        use noon_core::SemanticTextImportError as E;
        match error {
            E::MissingFont(_) => Self::new("missing_resource", "text_import.missing_font", error),
            E::MissingGeometry(_) => {
                Self::new("missing_resource", "text_import.missing_geometry", error)
            }
            E::NonFiniteGeometry(_) => {
                Self::new("invalid_input", "text_import.non_finite_geometry", error)
            }
            E::Validation(ref cause) => Self::caused_by(
                "text_import.validation",
                error.to_string(),
                Self::new("invalid_input", "text.invalid_resource", cause),
            ),
            E::Font(ref cause) => Self::caused_by(
                "text_import.font",
                error.to_string(),
                Self::new("invalid_input", "text.invalid_font", cause),
            ),
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
            E::Signal { error, .. } => Self::caused_by("transaction.signal", message, error.into()),
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
            E::Evaluation(cause) => Self::caused_by("callback.evaluation", message, cause.into()),
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

// Runtime advancement is already typed. Keep the category and immediate source
// at the optional host boundary; do not reinterpret or preflight the time here.
impl From<noon_runtime::EvaluationError> for AuthoringFailure {
    fn from(error: noon_runtime::EvaluationError) -> Self {
        use noon_runtime::EvaluationError as E;
        let message = error.to_string();
        match error {
            E::InvalidTime(_) => Self::new("invalid_input", "evaluation.invalid_time", message),
            E::NonMonotonicPreparedAdvance { .. } => Self::new(
                "invalid_input",
                "evaluation.non_monotonic_prepared_advance",
                message,
            ),
            E::RequiredCallbackPending => {
                Self::new("pending_work", "evaluation.callback_pending", message)
            }
            E::RequiredCallbackBarrier => Self::new(
                "unsupported_operation",
                "evaluation.callback_barrier",
                message,
            ),
            // Exhaustion and reactive producer domains have no inferred class.
            other => Self::unclassified("evaluation.unclassified", &other),
        }
    }
}

impl From<noon::ExecutionSegmentAdvanceError> for AuthoringFailure {
    fn from(error: noon::ExecutionSegmentAdvanceError) -> Self {
        use noon::ExecutionSegmentAdvanceError as E;
        let message = error.to_string();
        match error {
            E::ForeignSegment { .. } => {
                Self::new("foreign_handle", "advance.foreign_segment", message)
            }
            E::NoPendingCompletion { .. } => Self::new(
                "stale_publication",
                "advance.no_pending_completion",
                message,
            ),
            E::StaleSegment { .. } => {
                Self::new("stale_publication", "advance.stale_segment", message)
            }
            E::Evaluation(cause) => Self::caused_by("advance.evaluation", message, cause.into()),
            E::Callback(cause) => Self::caused_by("advance.callback", message, cause.into()),
        }
    }
}

impl From<crate::ClockError> for AuthoringFailure {
    fn from(error: crate::ClockError) -> Self {
        // These are the existing presentation-clock guards used by liveEvaluate.
        // Other browser-control entrypoints are outside this slice.
        match error {
            crate::ClockError::InvalidSceneTime(_) => {
                Self::new("invalid_input", "clock.invalid_scene_time", error)
            }
            crate::ClockError::SceneTimeOutsideLoop { .. } => {
                Self::new("invalid_input", "clock.time_outside_loop", error)
            }
            other => Self::unclassified("clock.unclassified", &other),
        }
    }
}

impl From<LiveSessionError> for AuthoringFailure {
    fn from(error: LiveSessionError) -> Self {
        use LiveSessionError as E;
        let message = error.to_string();
        match error {
            E::Authoring(cause) => Self::caused_by("live.authoring", message, cause.into()),
            E::Callback(cause) => Self::caused_by("live.callback", message, cause.into()),
            E::Text(cause) => Self::caused_by("live.text", message, cause.into()),
            E::ForeignMobjectStore => Self::new("foreign_handle", "live.foreign_store", message),
            E::Segment(cause) => Self::caused_by("live.segment", message, cause.into()),
            E::Advance(cause) => Self::caused_by("live.advance", message, cause.into()),
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
    fn typed_family_paint_rejections_keep_their_concrete_cause() {
        for (cause, code) in [
            (
                AuthoringError::InvalidOpacity {
                    name: "opacity".into(),
                    value: 1.5,
                },
                "authoring.invalid_opacity",
            ),
            (
                AuthoringError::InvalidRenderNumber {
                    name: "stroke width".into(),
                    value: f64::INFINITY,
                },
                "authoring.invalid_render_number",
            ),
            (
                AuthoringError::NegativeStrokeWidth(-1.0),
                "authoring.negative_stroke_width",
            ),
        ] {
            let diagnostic = cause.to_string();
            let failure =
                AuthoringFailure::from(noon::FamilyCallbackPaintError::InvalidPaint(cause));
            assert_eq!(failure.category, "invalid_input");
            assert_eq!(failure.code, "callback.family.invalid_paint");
            assert_eq!(failure.message, diagnostic);
            let nested = failure.cause.as_deref().unwrap();
            assert_eq!(nested.category, "invalid_input");
            assert_eq!(nested.code, code);
            assert_eq!(nested.message, diagnostic);
            assert!(nested.cause.is_none());
            assert!(std::ptr::eq(
                nested,
                failure
                    .source()
                    .unwrap()
                    .downcast_ref::<AuthoringFailure>()
                    .unwrap()
            ));
        }
        // A newly accepted typed input keeps its own category/code even when
        // the user-supplied value names unrelated error categories.
        let failure = AuthoringFailure::from(AuthoringError::InvalidStrokeCap(
            "invalid opacity; negative stroke width".into(),
        ));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "authoring.invalid_stroke_cap");
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
    fn assert_authoring_projection(error: AuthoringError, category: &str, codes: &[&str]) {
        let diagnostic = error.to_string();
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, category);
        assert_eq!(failure.message, diagnostic);
        assert!(!diagnostic.is_empty());
        let mut projected = Vec::new();
        let mut current = Some(&failure);
        while let Some(cause) = current {
            projected.push(cause.code);
            assert_eq!(cause.category, category);
            assert!(!cause.message.is_empty());
            current = cause.cause.as_deref();
        }
        assert_eq!(projected, codes);
    }

    #[test]
    fn accepted_public_operations_project_real_rust_causes_and_retry() {
        use noon::{LayoutAnchor, MobjectTarget, Scene};
        let scene = Scene::new();
        let mut first = scene.circle(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = (first.state().unwrap(), second.state().unwrap());
        let revision = scene.integration_store().borrow().scene_revision();
        let nodes = scene.integration_store().borrow().len();
        assert_authoring_projection(
            scene.circle(0.0).unwrap_err(),
            "invalid_input",
            &["authoring.non_positive_number"],
        );
        assert_authoring_projection(
            first.shift(f64::NAN, 0.0).unwrap_err(),
            "invalid_input",
            &["authoring.vector_lowering", "vector.invalid_coordinates"],
        );
        assert_authoring_projection(
            first.set_fill_opacity(1.5).unwrap_err(),
            "invalid_input",
            &["authoring.invalid_opacity"],
        );
        assert_authoring_projection(
            first
                .path_query()
                .unwrap()
                .point_from_proportion(-1.0)
                .unwrap_err(),
            "invalid_input",
            &["path.invalid_query"],
        );
        assert_authoring_projection(
            LayoutAnchor::from(&family).member(-3).layout().unwrap_err(),
            "invalid_input",
            &["authoring.invalid_submobject_index"],
        );
        assert_authoring_projection(
            family
                .arrange_in_grid(Some(1), Some(1), 0.1, 0.1)
                .unwrap_err(),
            "invalid_input",
            &["authoring.insufficient_grid_capacity"],
        );
        let foreign = Scene::new().circle(1.0).unwrap();
        assert_authoring_projection(
            family
                .copy_with_references(&[MobjectTarget::Object(&foreign)])
                .unwrap_err(),
            "foreign_handle",
            &["authoring.foreign_store"],
        );
        assert_authoring_projection(
            scene.value_tracker(f64::NAN).unwrap_err(),
            "invalid_input",
            &["authoring.signal", "signal.non_finite_value"],
        );
        assert_eq!((first.state().unwrap(), second.state().unwrap()), before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        first.shift(0.5, -0.5).unwrap();
        first.set_fill_opacity(0.5).unwrap();
        family.arrange_in_grid(Some(1), Some(2), 0.1, 0.1).unwrap();
        assert_eq!(first.fill_opacity().unwrap(), 0.5);
        assert!(family.copy_family().is_ok());
        assert_eq!(
            scene
                .value_tracker_value(&scene.value_tracker(2.0).unwrap())
                .unwrap(),
            2.0
        );
    }

    #[test]
    fn accepted_live_projection_preserves_atomic_publication_and_local_retry() {
        let mut scene = noon::Scene::new();
        let target = scene.circle(1.0).unwrap();
        let unrelated = scene.square(1.0).unwrap();
        scene.add(&target).unwrap();
        scene.add(&unrelated).unwrap();
        let mut execution = scene.execution_session().unwrap();
        execution.take_frame_changes();
        let before = execution.frame().clone();
        let publication = execution.publication_context();
        let authored = target.state().unwrap();
        let error = scene
            .live(&mut execution)
            .set_fill_opacity(&target, 1.5)
            .unwrap_err();
        let message = error.to_string();
        let projected = AuthoringFailure::from(error);
        assert_eq!(projected.category, "invalid_input");
        assert_eq!(projected.code, "live.authoring");
        assert_eq!(
            projected.cause.as_ref().unwrap().code,
            "authoring.invalid_opacity"
        );
        assert_eq!(projected.message, message);
        assert_eq!(execution.frame(), &before);
        assert_eq!(execution.publication_context(), publication);
        assert_eq!(target.state().unwrap(), authored);
        assert!(execution.take_frame_changes().is_empty());
        scene
            .live(&mut execution)
            .set_fill_opacity(&target, 0.5)
            .unwrap();
        assert_eq!(execution.take_frame_changes().object_indices(), &[0]);
        assert_eq!(execution.frame().objects[1], before.objects[1]);
    }
}
