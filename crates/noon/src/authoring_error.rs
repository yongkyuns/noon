//! Structured failures produced by shared Rust authoring operations.
//!
//! Existing semantic, resource and transaction causes remain intact. These errors
//! do not replace live publication, segment or activation errors, and never infer
//! categories from diagnostics.

/// A concrete unsupported payload or operation in ordinary shared authoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedAuthoringOperation {
    /// Point matching requires vector geometry on both operands.
    PointMatchContent,
    /// Effective family layout requires authored content without overrides.
    EffectiveFamilyLayoutRenderOverride,
    /// move_to cannot compose with an active effective affine driver.
    PlacementEffectiveAffineDriver,
    /// move_to cannot use an effective layout with render-content overrides.
    PlacementRenderOverride,
    /// effective Line endpoint queries currently support affine and style drivers only.
    EffectiveLineRenderOverride,
    /// Effective path observations require a queryable retained content version.
    EffectivePathRenderOverride,
    /// Text and external content do not expose a vector path.
    PathQueryContent,
    /// effective layout queries currently support affine and style drivers only.
    EffectiveLayoutRenderOverride,
    /// object state capture cannot represent a non-unit effective appearance.
    CaptureNonUnitAppearance,
    /// object state capture requires effective authored content without reveal or morph overrides.
    CaptureRenderOverride,
    /// cannot capture a reactive binding into object state.
    CaptureReactiveBinding,
    /// target editor cannot capture a runtime style backed by a paint resource.
    CaptureResourcePaint,
    /// Manim color queries do not support resource paints.
    ResourcePaintColorQuery,
    /// Manim opacity queries do not support resource paints.
    ResourcePaintOpacityQuery,
    /// external geometry must resolve to an immutable semantic resource.
    ExternalGeometry,
    /// Line.match_points requires an analytic Line source.
    LineMatchSourceContent,
    /// dimension stretching of rotated objects is unsupported.
    RotatedDimensionStretch,
}

impl std::fmt::Display for UnsupportedAuthoringOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EffectiveFamilyLayoutRenderOverride => "effective family layout cannot use render-content overrides",
            Self::PlacementEffectiveAffineDriver => "move_to cannot compose with an active effective affine driver",
            Self::PlacementRenderOverride => "move_to cannot use an effective layout with render-content overrides",
            Self::EffectiveLineRenderOverride => "effective Line endpoint queries currently support affine and style drivers only",
            Self::EffectiveLayoutRenderOverride => "effective layout queries currently support affine and style drivers only",
            Self::CaptureNonUnitAppearance => "object state capture cannot represent a non-unit effective appearance",
            Self::CaptureRenderOverride => "object state capture requires effective authored content without reveal or morph overrides",
            Self::CaptureReactiveBinding => "cannot capture a reactive binding into object state",
            Self::CaptureResourcePaint => "target editor cannot capture a runtime style backed by a paint resource",
            Self::ResourcePaintColorQuery => "Manim color queries do not support resource paints",
            Self::ResourcePaintOpacityQuery => "Manim opacity queries do not support resource paints",
            Self::EffectivePathRenderOverride => "path queries require current retained content without active render overrides",
            Self::PathQueryContent => "path queries require retained geometry",
            Self::ExternalGeometry => "external geometry must resolve to an immutable semantic resource",
            Self::PointMatchContent => "match_points requires vector geometry on both operands",
            Self::LineMatchSourceContent => "Line.match_points requires an analytic Line source",
            Self::RotatedDimensionStretch => "dimension stretching of rotated objects is unsupported",
        })
    }
}

impl std::error::Error for UnsupportedAuthoringOperation {}

/// Failure of a shared authored object, value, layout or membership operation.
///
/// Inspect variants or [`std::error::Error::source`], never diagnostics, for
/// control flow. String fields below retain actual input names/values, not an
/// unclassified error message. Failed validation does not publish a mutation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AuthoringError {
    /// The camera must be initialized before ordinary root content is added.
    CameraRequiresEmptyScene(noon_core::SemanticNodeId),
    /// A committed creation did not resolve its prepared local token.
    UnresolvedCreatedNode(noon_core::SemanticLocalNodeToken),
    /// Existing shared domain cause.
    FamilyPairing(noon_core::SemanticFamilyPairingError),
    /// A handle belongs to another semantic store.
    ForeignStore,
    /// An object references a missing or stale geometry resource.
    MissingGeometryResource(noon_core::GeometryResourceHandle),
    /// An object references a missing or stale text resource.
    MissingTextResource(noon_core::TextResourceHandle),
    /// The named input is non-finite or cannot be represented as f32.
    InvalidRenderNumber { name: String, value: f64 },
    /// The named input must be strictly positive.
    NonPositiveNumber { name: String, value: f64 },
    /// The named opacity is outside the inclusive unit interval.
    InvalidOpacity { name: String, value: f64 },
    /// At least one validated ellipse dimension is nonpositive.
    InvalidEllipseDimensions { width: f64, height: f64 },
    /// Geometry contains non-finite values.
    NonFiniteGeometry,
    /// The requested object state contains non-finite values.
    NonFiniteObjectState,
    /// An imported compact transform contains non-finite values.
    NonFiniteTransform,
    /// An imported compact style contains non-finite values.
    NonFiniteStyle,
    /// The validated stroke width is negative.
    NegativeStrokeWidth(f64),
    /// The supplied stroke-width-mode name is unsupported.
    InvalidStrokeWidthMode(String),
    /// The supplied stroke-join name is unsupported.
    InvalidStrokeJoin(String),
    /// The supplied stroke-cap name is unsupported.
    InvalidStrokeCap(String),
    /// A layout direction is non-finite.
    NonFiniteDirection,
    /// A color gradient requires at least one reference color.
    EmptyColorGradient,
    /// A layout direction has zero length.
    ZeroDirection,
    /// Line matching received non-finite endpoints.
    NonFiniteLineEndpoints,
    /// At least one line involved in matching has zero length.
    DegenerateLine,
    /// A requested bounds rectangle has reversed limits.
    UnorderedBounds(noon_core::Bounds2D64),
    /// An operation requested a dimension other than width or height.
    InvalidDimension(u32),
    /// Stretch matching would divide by a zero target extent.
    ZeroStretchTarget,
    /// Height matching would divide by a zero target height.
    ZeroMatchHeight,
    /// Width matching would divide by a zero target width.
    ZeroMatchWidth,
    /// An operation requires finite bounds for this node.
    MissingLayoutBounds(noon_core::SemanticNodeId),
    /// The requested direct family member index does not exist.
    InvalidSubmobjectIndex {
        family: noon_core::SemanticNodeId,
        index: isize,
    },
    /// Explicit grid dimensions must be positive.
    InvalidGridDimensions {
        rows: Option<usize>,
        columns: Option<usize>,
    },
    /// The explicit grid cannot contain all direct members.
    InsufficientGridCapacity {
        rows: Option<usize>,
        columns: usize,
        members: usize,
    },
    /// Grid alignment, sizing or flow options are inconsistent.
    InvalidGridOption(&'static str),
    /// A planar flip needs a nonzero XY axis or a pure Z axis.
    InvalidFlipAxis,
    /// An internal arrangement plan has not observed all required bounds.
    IncompleteArrangement,
    /// The node is not represented in this family-local copy mapping.
    MissingCopySource(noon_core::SemanticNodeId),
    /// A detached-only tracker operation was requested after association.
    AlreadyScopedTracker(noon_core::SemanticNodeId),
    /// A native input declaration received an empty name.
    EmptyInputName { kind: String },
    /// This operation requires an unsupported payload or capability.
    Unsupported(UnsupportedAuthoringOperation),
    /// The semantic operation rejected a node, family, or membership request.
    Semantic(noon_core::SemanticSceneOperationError),
    /// Transaction preflight failed before commit.
    Transaction(noon_core::SemanticMutationTransactionError),
    /// A high-precision value cannot lower to renderer coordinates.
    VectorLowering(noon_core::SemanticLoweringError),
    /// Immutable geometry resource validation failed.
    GeometryResource(noon_core::GeometryResourceError),
    /// A path observation received invalid geometry or sampling parameters.
    PathQuery(noon_geometry::PathProportionError),
    /// The shared arc constructor rejected its inputs.
    Arc(crate::arc_authoring::ArcAuthoringError),
    /// The shared elbow constructor rejected its inputs.
    Elbow(crate::elbow_authoring::ElbowAuthoringError),
    /// The shared rounded-rectangle constructor rejected its inputs.
    RoundedRectangle(crate::rounded_rectangle_authoring::RoundedRectangleAuthoringError),
    /// The shared dashed-line constructor rejected its inputs.
    DashedLine(crate::dashed_line_authoring::DashedLineAuthoringError),
    /// The shared signal operation rejected its inputs.
    Signal(noon_core::SemanticSignalError),
    /// The shared signal binding operation was rejected.
    SignalBinding(noon_core::SemanticSignalBindingError),
    /// The shared scalar query was rejected.
    ScalarQuery(noon_core::SemanticScalarSignalQueryError),
}

impl std::fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CameraRequiresEmptyScene(_) => {
                f.write_str("2D camera frame must be created before scene content")
            }
            Self::UnresolvedCreatedNode(token) => {
                write!(f, "committed creation did not resolve {token:?}")
            }
            Self::FamilyPairing(error) => error.fmt(f),
            Self::ForeignStore => f.write_str("membership target belongs to another scene store"),
            Self::MissingGeometryResource(handle) => {
                write!(f, "unknown or stale geometry resource {handle:?}")
            }
            Self::MissingTextResource(handle) => {
                write!(f, "unknown or stale text resource {handle:?}")
            }
            Self::InvalidRenderNumber { name, .. } => {
                write!(f, "{name} must be a finite f32-compatible number")
            }
            Self::NonPositiveNumber { name, .. } => write!(f, "{name} must be positive"),
            Self::InvalidOpacity { name, .. } => write!(f, "{name} must be between 0 and 1"),
            Self::InvalidEllipseDimensions { .. } => {
                f.write_str("Ellipse width and height must be positive")
            }
            Self::NonFiniteGeometry => f.write_str("geometry must be finite"),
            Self::NonFiniteObjectState => {
                f.write_str("geometry, transform, and style must be finite")
            }
            Self::NonFiniteTransform => f.write_str("compact transform must be finite"),
            Self::NonFiniteStyle => f.write_str("compact style must be finite"),
            Self::NegativeStrokeWidth(_) => f.write_str("stroke width must be non-negative"),
            Self::InvalidStrokeWidthMode(_) => {
                f.write_str("stroke_width_mode must be scale_with_object or screen_space")
            }
            Self::InvalidStrokeJoin(_) => f.write_str("stroke_join must be round, miter, or bevel"),
            Self::InvalidStrokeCap(_) => f.write_str("stroke_cap must be round, butt, or square"),
            Self::NonFiniteDirection => f.write_str("direction must be finite"),
            Self::EmptyColorGradient => f.write_str("a color gradient requires at least one color"),
            Self::ZeroDirection => f.write_str("direction must be non-zero"),
            Self::NonFiniteLineEndpoints => {
                f.write_str("Line.match_points endpoints must be finite")
            }
            Self::DegenerateLine => {
                f.write_str("Line.match_points requires nondegenerate source and target Lines")
            }
            Self::UnorderedBounds(_) => f.write_str("layout bounds must be ordered"),
            Self::InvalidDimension(_) => {
                f.write_str("dimension fitting supports width (0) and height (1) only")
            }
            Self::ZeroStretchTarget => {
                f.write_str("cannot stretch a zero-width or zero-height target")
            }
            Self::ZeroMatchHeight => f.write_str("cannot match height from a zero-height target"),
            Self::ZeroMatchWidth => f.write_str("cannot match width from a zero-width target"),
            Self::MissingLayoutBounds(node) => {
                write!(f, "layout target {node:?} has no finite bounds")
            }
            Self::InvalidSubmobjectIndex { family, index } => write!(
                f,
                "alignment submobject index {index} is unavailable in {family:?}"
            ),
            Self::InvalidGridDimensions { .. } => {
                f.write_str("grid rows and columns must be positive")
            }
            Self::InsufficientGridCapacity { .. } => {
                f.write_str("too few grid rows and columns to fit all members")
            }
            Self::InvalidGridOption(name) => write!(f, "invalid grid {name} option"),
            Self::InvalidFlipAxis => {
                f.write_str("flip axis must be nonzero and lie in the XY plane or along Z")
            }
            Self::IncompleteArrangement => f.write_str("family arrangement bounds are incomplete"),
            Self::MissingCopySource(source) => {
                write!(f, "source {source:?} is not part of this family copy")
            }
            Self::AlreadyScopedTracker(_) => {
                f.write_str("ValueTracker is already associated with a Scene")
            }
            Self::EmptyInputName { kind } => write!(f, "native input {kind} must not be empty"),
            Self::Semantic(error) => error.fmt(f),
            Self::Transaction(error) => error.fmt(f),
            Self::VectorLowering(error) => error.fmt(f),
            Self::GeometryResource(error) => error.fmt(f),
            Self::PathQuery(error) => error.fmt(f),
            Self::Arc(error) => error.fmt(f),
            Self::Elbow(error) => error.fmt(f),
            Self::RoundedRectangle(error) => error.fmt(f),
            Self::DashedLine(error) => error.fmt(f),
            Self::Signal(error) => error.fmt(f),
            Self::SignalBinding(error) => error.fmt(f),
            Self::ScalarQuery(error) => error.fmt(f),
            Self::Unsupported(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FamilyPairing(error) => Some(error),
            Self::Semantic(error) => Some(error),
            Self::Transaction(error) => Some(error),
            Self::VectorLowering(error) => Some(error),
            Self::GeometryResource(error) => Some(error),
            Self::PathQuery(error) => Some(error),
            Self::Arc(error) => Some(error),
            Self::Elbow(error) => Some(error),
            Self::RoundedRectangle(error) => Some(error),
            Self::DashedLine(error) => Some(error),
            Self::Signal(error) => Some(error),
            Self::SignalBinding(error) => Some(error),
            Self::ScalarQuery(error) => Some(error),
            Self::Unsupported(error) => Some(error),
            _ => None,
        }
    }
}

impl From<noon_core::SemanticSceneOperationError> for AuthoringError {
    fn from(error: noon_core::SemanticSceneOperationError) -> Self {
        Self::Semantic(error)
    }
}

impl From<noon_core::SemanticMutationTransactionError> for AuthoringError {
    fn from(error: noon_core::SemanticMutationTransactionError) -> Self {
        Self::Transaction(error)
    }
}

impl From<noon_core::SemanticLoweringError> for AuthoringError {
    fn from(error: noon_core::SemanticLoweringError) -> Self {
        Self::VectorLowering(error)
    }
}

impl From<noon_core::GeometryResourceError> for AuthoringError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        Self::GeometryResource(error)
    }
}

impl From<crate::arc_authoring::ArcAuthoringError> for AuthoringError {
    fn from(error: crate::arc_authoring::ArcAuthoringError) -> Self {
        Self::Arc(error)
    }
}

impl From<crate::elbow_authoring::ElbowAuthoringError> for AuthoringError {
    fn from(error: crate::elbow_authoring::ElbowAuthoringError) -> Self {
        Self::Elbow(error)
    }
}

impl From<crate::rounded_rectangle_authoring::RoundedRectangleAuthoringError> for AuthoringError {
    fn from(error: crate::rounded_rectangle_authoring::RoundedRectangleAuthoringError) -> Self {
        Self::RoundedRectangle(error)
    }
}

impl From<crate::dashed_line_authoring::DashedLineAuthoringError> for AuthoringError {
    fn from(error: crate::dashed_line_authoring::DashedLineAuthoringError) -> Self {
        Self::DashedLine(error)
    }
}

impl From<noon_core::SemanticSignalError> for AuthoringError {
    fn from(error: noon_core::SemanticSignalError) -> Self {
        Self::Signal(error)
    }
}

impl From<noon_core::SemanticSignalBindingError> for AuthoringError {
    fn from(error: noon_core::SemanticSignalBindingError) -> Self {
        Self::SignalBinding(error)
    }
}

impl From<noon_core::SemanticScalarSignalQueryError> for AuthoringError {
    fn from(error: noon_core::SemanticScalarSignalQueryError) -> Self {
        Self::ScalarQuery(error)
    }
}

impl From<UnsupportedAuthoringOperation> for AuthoringError {
    fn from(error: UnsupportedAuthoringOperation) -> Self {
        Self::Unsupported(error)
    }
}

impl From<noon_core::SemanticStoreError> for AuthoringError {
    fn from(error: noon_core::SemanticStoreError) -> Self {
        Self::Semantic(error.into())
    }
}

impl From<noon_core::SemanticFamilyPairingError> for AuthoringError {
    fn from(error: noon_core::SemanticFamilyPairingError) -> Self {
        Self::FamilyPairing(error)
    }
}
