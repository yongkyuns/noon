use noon_core::{
    resolve_animation_options, AnimationDefaults, AnimationOptions, AnimationOptionsError,
    ResolvedAnimationOptions, SemanticNodeId, SemanticSceneOperationError, SemanticStore,
};

/// One affine transform component that cannot enter the current 2D payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticAffineAnimationField {
    Translation,
    Scale,
    RotationZ,
}

impl std::fmt::Display for SemanticAffineAnimationField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Translation => "translation",
            Self::Scale => "scale",
            Self::RotationZ => "rotation_z",
        })
    }
}

/// Structural payload cases outside the supported ordinary 2D transform subset.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum TransformPayloadValidationIssue {
    ContentChange,
    StyleChange,
    PainterOrderChange,
    BindingChange,
    DepthChange(SemanticAffineAnimationField),
    Lifecycle { remover: bool, introducer: bool },
}

/// Read-only validation failure for a TransformTo payload.
///
/// This deliberately covers only store-owned input validation. Execution capture,
/// scheduling, track allocation, and runtime publication belong to generic prepared
/// animation composition lowering.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticTransformToPayloadError {
    Options(AnimationOptionsError),
    Target {
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    UnsupportedContentChange,
    UnsupportedStyleChange,
    UnsupportedPainterOrderChange,
    UnsupportedBindingChange,
    UnsupportedDepthChange(SemanticAffineAnimationField),
    UnsupportedLifecycle {
        remover: bool,
        introducer: bool,
    },
}

impl std::fmt::Display for SemanticTransformToPayloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "semantic TransformTo payload validation failed: {self:?}"
        )
    }
}

impl std::error::Error for SemanticTransformToPayloadError {}

impl SemanticTransformToPayloadError {
    /// Whether the source and target are valid semantic objects but request a
    /// payload that the current ordinary 2D transform vocabulary does not support.
    pub const fn is_unsupported_payload(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedContentChange
                | Self::UnsupportedStyleChange
                | Self::UnsupportedPainterOrderChange
                | Self::UnsupportedBindingChange
                | Self::UnsupportedDepthChange(_)
                | Self::UnsupportedLifecycle { .. }
        )
    }
}

impl From<TransformPayloadValidationIssue> for SemanticTransformToPayloadError {
    fn from(value: TransformPayloadValidationIssue) -> Self {
        match value {
            TransformPayloadValidationIssue::ContentChange => Self::UnsupportedContentChange,
            TransformPayloadValidationIssue::StyleChange => Self::UnsupportedStyleChange,
            TransformPayloadValidationIssue::PainterOrderChange => {
                Self::UnsupportedPainterOrderChange
            }
            TransformPayloadValidationIssue::BindingChange => Self::UnsupportedBindingChange,
            TransformPayloadValidationIssue::DepthChange(field) => {
                Self::UnsupportedDepthChange(field)
            }
            TransformPayloadValidationIssue::Lifecycle {
                remover,
                introducer,
            } => Self::UnsupportedLifecycle {
                remover,
                introducer,
            },
        }
    }
}

/// Validate an inert TransformTo payload before declaration, runtime, or execution
/// identity is created. Language facades use this shared read-only check to select
/// the canonical ordinary transform subset.
pub fn validate_semantic_transform_to_payload(
    store: &SemanticStore,
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    options: AnimationOptions,
) -> Result<(), SemanticTransformToPayloadError> {
    let source = store
        .semantic_object_state_checked(target)
        .map_err(|error| SemanticTransformToPayloadError::Target {
            node: target,
            error,
        })?;
    let target_object = store
        .semantic_object_state_checked(target_state)
        .map_err(|error| SemanticTransformToPayloadError::Target {
            node: target_state,
            error,
        })?;
    let options = resolve_animation_options(AnimationDefaults::MANIM, options, options)
        .map_err(SemanticTransformToPayloadError::Options)?;
    validate_transform_payload_shape(source, target_object, options).map_err(Into::into)
}

pub(super) fn validate_transform_payload_shape(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    options: ResolvedAnimationOptions,
) -> Result<(), TransformPayloadValidationIssue> {
    if options.remover || options.introducer {
        return Err(TransformPayloadValidationIssue::Lifecycle {
            remover: options.remover,
            introducer: options.introducer,
        });
    }
    if source.content != target.content && !is_supported_analytic_content_morph(source, target) {
        return Err(TransformPayloadValidationIssue::ContentChange);
    }
    if source.style.stroke_width != target.style.stroke_width
        || source.style.stroke_width_mode != target.style.stroke_width_mode
        || source.style.stroke_join != target.style.stroke_join
        || source.style.stroke_cap != target.style.stroke_cap
    {
        return Err(TransformPayloadValidationIssue::StyleChange);
    }
    if source.z_index() != target.z_index() {
        return Err(TransformPayloadValidationIssue::PainterOrderChange);
    }
    if source.signal_bindings() != target.signal_bindings() {
        return Err(TransformPayloadValidationIssue::BindingChange);
    }
    if source.transform.translation.z != target.transform.translation.z {
        return Err(TransformPayloadValidationIssue::DepthChange(
            SemanticAffineAnimationField::Translation,
        ));
    }
    if source.transform.scale.z != target.transform.scale.z {
        return Err(TransformPayloadValidationIssue::DepthChange(
            SemanticAffineAnimationField::Scale,
        ));
    }
    Ok(())
}

pub(super) fn is_supported_analytic_content_morph(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
) -> bool {
    use noon_core::{SemanticObjectContent::Geometry, StoredGeometry};

    matches!(
        (source.content, target.content),
        (
            Geometry(StoredGeometry::Circle { .. }),
            Geometry(StoredGeometry::Rectangle { .. })
        ) | (
            Geometry(StoredGeometry::Rectangle { .. }),
            Geometry(StoredGeometry::Circle { .. })
        )
    )
}
