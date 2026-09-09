//! Error *representation* at the optional language boundary, not semantic policy.
//!
//! Shared Rust producers remain authoritative. Match their variants here, never
//! diagnostic text. Only failure paths allocate this projection; native/direct
//! Rust engine calls do not depend on it. Unmigrated String producers explicitly
//! remain unclassified until their R2 domain supplies a typed cause.

use std::error::Error;

use noon::{
    ArcAuthoringError, AuthoringError, DashedLineAuthoringError, ElbowAuthoringError,
    ExecutionSegmentCompletionError, ExecutionSegmentError, ExecutionSessionCallbackError,
    ExecutionSessionCallbackReadError, ExecutionSessionPublicationError, LiveSessionError,
    RoundedRectangleAuthoringError, UnsupportedAuthoringOperation,
};
use noon_core::{
    GeometryResourceError, SemanticFamilyPairingError, SemanticGeometryLayoutError,
    SemanticLoweringError, SemanticMutationTransactionError, SemanticScalarSignalQueryError,
    SemanticSceneOperationError, SemanticSignalBindingError, SemanticSignalError,
    SemanticStoreError,
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
            AuthoringError::CameraRequiresEmptyScene(_) => Self::new(
                "invalid_input",
                "authoring.camera_requires_empty_scene",
                message,
            ),
            AuthoringError::FamilyPairing(cause) => {
                Self::caused_by("authoring.family_pairing", message, cause.into())
            }
            AuthoringError::ForeignStore => {
                Self::new("foreign_handle", "authoring.foreign_store", message)
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
            AuthoringError::ZeroReplaceExtent => {
                Self::new("invalid_input", "authoring.zero_replace_extent", message)
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
            AuthoringError::InsufficientGridCapacity { .. } => Self::new(
                "invalid_input",
                "authoring.insufficient_grid_capacity",
                message,
            ),
            AuthoringError::MissingCopySource(_) => {
                Self::new("invalid_input", "authoring.missing_copy_source", message)
            }
            AuthoringError::AlreadyScopedTracker(_) => {
                Self::new("invalid_input", "authoring.already_scoped_tracker", message)
            }
            AuthoringError::EmptyInputName { .. } => {
                Self::new("invalid_input", "authoring.empty_input_name", message)
            }
            AuthoringError::Unsupported(cause) => {
                Self::caused_by("authoring.unsupported", message, cause.into())
            }
            AuthoringError::Semantic(cause) => {
                Self::caused_by("authoring.semantic", message, cause.into())
            }
            AuthoringError::Transaction(cause) => {
                Self::caused_by("authoring.transaction", message, cause.into())
            }
            AuthoringError::VectorLowering(cause) => {
                Self::caused_by("authoring.vector_lowering", message, cause.into())
            }
            AuthoringError::GeometryResource(cause) => {
                Self::caused_by("authoring.geometry_resource", message, cause.into())
            }
            AuthoringError::GeometryLayout(cause) => {
                Self::caused_by("authoring.geometry_layout", message, cause.into())
            }
            AuthoringError::Arc(cause) => Self::caused_by("authoring.arc", message, cause.into()),
            AuthoringError::Elbow(cause) => {
                Self::caused_by("authoring.elbow", message, cause.into())
            }
            AuthoringError::RoundedRectangle(cause) => {
                Self::caused_by("authoring.rounded_rectangle", message, cause.into())
            }
            AuthoringError::DashedLine(cause) => {
                Self::caused_by("authoring.dashed_line", message, cause.into())
            }
            AuthoringError::Signal(cause) => {
                Self::caused_by("authoring.signal", message, cause.into())
            }
            AuthoringError::SignalBinding(cause) => {
                Self::caused_by("authoring.signal_binding", message, cause.into())
            }
            AuthoringError::ScalarQuery(cause) => {
                Self::caused_by("authoring.scalar_query", message, cause.into())
            }
            // These are internal preparation invariants rather than supported
            // public-operation categories. Keep them explicit until their owner
            // decides whether they should ever escape the Rust facade.
            AuthoringError::UnresolvedCreatedNode(_) | AuthoringError::IncompleteArrangement => {
                Self::new("unclassified", "authoring.internal", message)
            }
            // The producer is non-exhaustive: future domains must opt in rather
            // than silently inheriting a category from wording.
            other => Self::unclassified("authoring.unclassified", &other),
        }
    }
}

impl From<UnsupportedAuthoringOperation> for AuthoringFailure {
    fn from(error: UnsupportedAuthoringOperation) -> Self {
        use UnsupportedAuthoringOperation as E;
        let code = match error {
            E::EffectiveFamilyLayoutRenderOverride => {
                "unsupported.effective_family_layout_render_override"
            }
            E::PlacementEffectiveAffineDriver => "unsupported.placement_effective_affine_driver",
            E::PlacementRenderOverride => "unsupported.placement_render_override",
            E::EffectiveLineRenderOverride => "unsupported.effective_line_render_override",
            E::EffectiveLayoutRenderOverride => "unsupported.effective_layout_render_override",
            E::CaptureNonUnitAppearance => "unsupported.capture_non_unit_appearance",
            E::CaptureRenderOverride => "unsupported.capture_render_override",
            E::CaptureReactiveBinding => "unsupported.capture_reactive_binding",
            E::CaptureResourcePaint => "unsupported.capture_resource_paint",
            E::ResourcePaintColorQuery => "unsupported.resource_paint_color_query",
            E::ResourcePaintOpacityQuery => "unsupported.resource_paint_opacity_query",
            E::ExternalGeometry => "unsupported.external_geometry",
            E::LineMatchNonuniformScale => "unsupported.line_match_nonuniform_scale",
            E::LineMatchTargetContent => "unsupported.line_match_target_content",
            E::LineMatchSourceContent => "unsupported.line_match_source_content",
            E::LineEndpointContent => "unsupported.line_endpoint_content",
            E::RotatedDimensionStretch => "unsupported.rotated_dimension_stretch",
            other => return Self::unclassified("unsupported.unclassified", &other),
        };
        Self::new("unsupported_operation", code, error)
    }
}

impl From<SemanticFamilyPairingError> for AuthoringFailure {
    fn from(error: SemanticFamilyPairingError) -> Self {
        use SemanticFamilyPairingError as E;
        let (category, code) = match &error {
            E::UnknownNode(_) => ("stale_handle", "family_pairing.unknown_node"),
            E::RootIsNotFamily(_) => ("invalid_input", "family_pairing.root_not_family"),
            E::TopologyMismatch { .. } => ("invalid_input", "family_pairing.topology_mismatch"),
            E::UnsupportedLeaf(_) => ("unsupported_operation", "family_pairing.unsupported_leaf"),
            E::AliasMismatch { .. } => ("invalid_input", "family_pairing.alias_mismatch"),
            E::Empty => ("invalid_input", "family_pairing.empty"),
        };
        Self::new(category, code, error)
    }
}

impl From<SemanticLoweringError> for AuthoringFailure {
    fn from(error: SemanticLoweringError) -> Self {
        let code = match error {
            SemanticLoweringError::NonFiniteVector(_) => "vector_lowering.non_finite",
            SemanticLoweringError::CoordinateOutOfRange(_) => "vector_lowering.out_of_range",
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<GeometryResourceError> for AuthoringFailure {
    fn from(error: GeometryResourceError) -> Self {
        match error {
            GeometryResourceError::NonFinitePath => {
                Self::new("invalid_input", "geometry_resource.non_finite_path", error)
            }
            GeometryResourceError::UnknownResource(_) => {
                Self::new("missing_resource", "geometry_resource.unknown", error)
            }
            GeometryResourceError::VersionExhausted(_) => Self::new(
                "unsupported_operation",
                "geometry_resource.version_exhausted",
                error,
            ),
        }
    }
}

impl From<SemanticGeometryLayoutError> for AuthoringFailure {
    fn from(error: SemanticGeometryLayoutError) -> Self {
        match error {
            SemanticGeometryLayoutError::EllipseRequiresCircle(_) => Self::new(
                "invalid_input",
                "geometry_layout.ellipse_requires_circle",
                error,
            ),
        }
    }
}

impl From<ArcAuthoringError> for AuthoringFailure {
    fn from(error: ArcAuthoringError) -> Self {
        use ArcAuthoringError as E;
        let code = match &error {
            E::TooFewComponents(_) => "arc.too_few_components",
            E::NonFiniteRadius(_) => "arc.non_finite_radius",
            E::NonFiniteAngle(_) => "arc.non_finite_angle",
            E::NonFiniteStartAngle(_) => "arc.non_finite_start_angle",
            E::NonFinitePoint(_) => "arc.non_finite_point",
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<ElbowAuthoringError> for AuthoringFailure {
    fn from(error: ElbowAuthoringError) -> Self {
        let code = match error {
            ElbowAuthoringError::NonFiniteWidth(_) => "elbow.non_finite_width",
            ElbowAuthoringError::NonFiniteAngle(_) => "elbow.non_finite_angle",
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<RoundedRectangleAuthoringError> for AuthoringFailure {
    fn from(error: RoundedRectangleAuthoringError) -> Self {
        let code = match &error {
            RoundedRectangleAuthoringError::InvalidWidth(_) => "rounded_rectangle.invalid_width",
            RoundedRectangleAuthoringError::InvalidHeight(_) => "rounded_rectangle.invalid_height",
            RoundedRectangleAuthoringError::NonFiniteCornerRadius(_) => {
                "rounded_rectangle.non_finite_corner_radius"
            }
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<DashedLineAuthoringError> for AuthoringFailure {
    fn from(error: DashedLineAuthoringError) -> Self {
        use DashedLineAuthoringError as E;
        let code = match error {
            E::NonFiniteStart(_) => "dashed_line.non_finite_start",
            E::NonFiniteEnd(_) => "dashed_line.non_finite_end",
            E::NonFiniteLineLength => "dashed_line.non_finite_length",
            E::InvalidDashLength(_) => "dashed_line.invalid_dash_length",
            E::InvalidDashedRatio(_) => "dashed_line.invalid_ratio",
            E::DashCountOverflow(_) => "dashed_line.dash_count_overflow",
        };
        Self::new("invalid_input", code, error)
    }
}

impl From<SemanticSignalError> for AuthoringFailure {
    fn from(error: SemanticSignalError) -> Self {
        use SemanticSignalError as E;
        let (category, code) = match &error {
            E::InvalidNativeInputInitialValue { .. } => {
                ("invalid_input", "signal.invalid_native_input_initial_value")
            }
            E::UnknownSignal(_) => ("stale_handle", "signal.unknown"),
            E::NotSignal(_) => ("invalid_input", "signal.not_signal"),
            E::NonFiniteValue => ("invalid_input", "signal.non_finite_value"),
            E::DependencyCycle(_) => ("invalid_input", "signal.dependency_cycle"),
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

impl From<SemanticSignalBindingError> for AuthoringFailure {
    fn from(error: SemanticSignalBindingError) -> Self {
        use SemanticSignalBindingError as E;
        let message = error.to_string();
        match error {
            E::Signal(cause) => Self::caused_by("signal_binding.signal", message, cause.into()),
            E::Target(cause) => Self::caused_by("signal_binding.target", message, cause.into()),
            E::TypeMismatch { .. } => {
                Self::new("invalid_input", "signal_binding.type_mismatch", message)
            }
            E::PropertyAlreadyBound { .. } => Self::new(
                "invalid_input",
                "signal_binding.property_already_bound",
                message,
            ),
        }
    }
}

impl From<SemanticScalarSignalQueryError> for AuthoringFailure {
    fn from(error: SemanticScalarSignalQueryError) -> Self {
        use SemanticScalarSignalQueryError as E;
        let message = error.to_string();
        match error {
            E::Signal(cause) => Self::caused_by("scalar_query.signal", message, cause.into()),
            E::NotInputSignal(_) => Self::new(
                "unsupported_operation",
                "scalar_query.derived_signal",
                message,
            ),
            E::NonScalarSignal(_) => {
                Self::new("invalid_input", "scalar_query.non_scalar_signal", message)
            }
            E::InvalidTime => Self::new("invalid_input", "scalar_query.invalid_time", message),
        }
    }
}

impl From<SemanticSceneOperationError> for AuthoringFailure {
    fn from(error: SemanticSceneOperationError) -> Self {
        use SemanticSceneOperationError as E;
        let message = error.to_string();
        let (category, code) = match &error {
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
            E::Store(cause) => {
                return Self::caused_by("semantic.store", message, cause.clone().into())
            }
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
        let failure = AuthoringFailure::from(AuthoringError::InvalidStrokeCap(
            "invalid opacity; negative stroke width".into(),
        ));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "authoring.invalid_stroke_cap");
    }

    #[test]
    fn settled_public_authoring_domains_have_explicit_categories() {
        use noon_core::{Bounds2D64, SemanticVec3};
        let node = SemanticNodeId::new(7, 3);
        let errors = vec![
            AuthoringError::CameraRequiresEmptyScene(node),
            AuthoringError::NonPositiveNumber {
                name: "radius".into(),
                value: 0.0,
            },
            AuthoringError::InvalidEllipseDimensions {
                width: 0.0,
                height: 1.0,
            },
            AuthoringError::NonFiniteGeometry,
            AuthoringError::NonFiniteObjectState,
            AuthoringError::NonFiniteTransform,
            AuthoringError::NonFiniteStyle,
            AuthoringError::InvalidStrokeWidthMode("bad".into()),
            AuthoringError::InvalidStrokeJoin("bad".into()),
            AuthoringError::InvalidStrokeCap("bad".into()),
            AuthoringError::NonFiniteDirection,
            AuthoringError::ZeroDirection,
            AuthoringError::NonFiniteLineEndpoints,
            AuthoringError::DegenerateLine,
            AuthoringError::UnorderedBounds(Bounds2D64 {
                min_x: 1.0,
                min_y: 1.0,
                max_x: 0.0,
                max_y: 0.0,
            }),
            AuthoringError::InvalidDimension(3),
            AuthoringError::ZeroReplaceExtent,
            AuthoringError::ZeroStretchTarget,
            AuthoringError::ZeroMatchHeight,
            AuthoringError::ZeroMatchWidth,
            AuthoringError::MissingLayoutBounds(node),
            AuthoringError::InvalidSubmobjectIndex {
                family: node,
                index: 9,
            },
            AuthoringError::InvalidGridDimensions {
                rows: Some(0),
                columns: Some(1),
            },
            AuthoringError::InsufficientGridCapacity {
                rows: Some(1),
                columns: 1,
                members: 2,
            },
            AuthoringError::MissingCopySource(node),
            AuthoringError::AlreadyScopedTracker(node),
            AuthoringError::EmptyInputName {
                kind: "key code".into(),
            },
            AuthoringError::Unsupported(UnsupportedAuthoringOperation::LineEndpointContent),
            AuthoringError::FamilyPairing(SemanticFamilyPairingError::Empty),
            AuthoringError::VectorLowering(SemanticLoweringError::CoordinateOutOfRange(
                SemanticVec3::new(f64::MAX, 0.0, 0.0),
            )),
            AuthoringError::GeometryResource(GeometryResourceError::NonFinitePath),
            AuthoringError::Arc(ArcAuthoringError::TooFewComponents(1)),
            AuthoringError::Elbow(ElbowAuthoringError::NonFiniteAngle(f32::NAN)),
            AuthoringError::RoundedRectangle(RoundedRectangleAuthoringError::InvalidWidth(0.0)),
            AuthoringError::DashedLine(DashedLineAuthoringError::InvalidDashLength(0.0)),
            AuthoringError::Signal(SemanticSignalError::NonFiniteValue),
            AuthoringError::SignalBinding(SemanticSignalBindingError::PropertyAlreadyBound {
                target: node,
                property: noon_core::SemanticObjectProperty::Translation,
                existing_signal: node,
            }),
            AuthoringError::ScalarQuery(SemanticScalarSignalQueryError::InvalidTime),
        ];
        for error in errors {
            let failure = AuthoringFailure::from(error);
            assert_ne!(failure.category, "unclassified", "{}", failure.message);
            assert!(
                !failure.code.ends_with("unclassified"),
                "{}",
                failure.message
            );
        }
        let internal = AuthoringFailure::from(AuthoringError::IncompleteArrangement);
        assert_eq!(internal.category, "unclassified");
        assert_eq!(internal.code, "authoring.internal");
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
