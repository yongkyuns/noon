use std::collections::{hash_map::Entry, HashMap};

use noon_core::{
    validate_track_definition, ObjectId, Property, SemanticAnimationError, SemanticAnimationIntent,
    SemanticFadeDirection, SemanticLoweringError, SemanticNodeId, SemanticObjectContent,
    SemanticObjectProperty, SemanticSceneOperationError, SemanticSignalValue, SemanticStore,
    StoredGeometry, Style, TimelineError, TrackDefinition, TrackId, TrackValues, Transform2D,
};

use super::super::{
    projection::{
        lower_semantic_style_value, lower_semantic_transform_value, SemanticExecutionValueError,
        SemanticLoweringError as StyleLoweringError,
    },
    SemanticAnimationScheduleProjection, SemanticScheduledAnimationLeaf,
    SemanticScheduledAnimationPayload,
};
use super::transform_payload::{
    is_supported_analytic_content_morph, validate_transform_payload_shape,
    SemanticAffineAnimationField, TransformPayloadValidationIssue,
};

/// The activation-time effective domains consumed by the shared animation lowerer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectiveAnimationProperties {
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
}

/// Exact authored reconciliation performed when one execution channel is released.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAnimationCompletion {
    Property {
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    },
    Fill {
        paint: Option<noon_core::SemanticPaint>,
        opacity: f64,
    },
    Stroke {
        paint: Option<noon_core::SemanticPaint>,
        opacity: f64,
    },
    /// Execution-only lifecycle completion. Appearance is not authored object state.
    Fade { direction: SemanticFadeDirection },
    /// Execution-only Create completion. Reveal is not authored object state.
    Create,
    /// Exact authored endpoint for the bounded shared analytic content morph.
    ContentMorph { content: SemanticObjectContent },
}

/// One existing execution-timeline channel lowered from an activated semantic animation.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAffineAnimationTrack {
    pub animation: SemanticNodeId,
    pub target: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub property: Property,
    pub completion: SemanticAnimationCompletion,
    pub values: TrackValues,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
}

impl SemanticAffineAnimationTrack {
    /// Attach execution-local activation identity at the existing timeline boundary.
    pub fn with_track_id(
        &self,
        id: TrackId,
    ) -> Result<TrackDefinition, SemanticAffineAnimationTrackError> {
        let track = TrackDefinition {
            id,
            object: self.execution_object_id,
            property: self.property,
            values: self.values.clone(),
            timing: self.timing,
            time_map: self.time_map.clone(),
        };
        validate_track_definition(&track).map_err(|error| {
            SemanticAffineAnimationTrackError::InvalidTrack {
                animation: self.animation,
                error,
            }
        })?;
        Ok(track)
    }
}

/// Activation-level supported-channel continuation of the semantic animation payload seam.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticAffineAnimationTrackProjection {
    tracks: Vec<SemanticAffineAnimationTrack>,
}

impl SemanticAffineAnimationTrackProjection {
    pub fn tracks(&self) -> &[SemanticAffineAnimationTrack] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAffineAnimationTrackError {
    Animation(SemanticAnimationError),
    Target {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    ScheduleMismatch {
        animation: SemanticNodeId,
    },
    MissingEffectiveTransform {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    InvalidEffectiveTransform {
        animation: SemanticNodeId,
        target: SemanticNodeId,
    },
    InvalidEffectiveStyle {
        animation: SemanticNodeId,
        target: SemanticNodeId,
    },
    UnsupportedContentChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedStyleChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedPainterOrderChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedBindingChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedDepthChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
    },
    UnsupportedLifecycle {
        animation: SemanticNodeId,
        remover: bool,
        introducer: bool,
    },
    ReactiveDriverConflict {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    MultipleDrivers {
        first_animation: SemanticNodeId,
        next_animation: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    InvalidTargetValue {
        animation: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
        error: SemanticLoweringError,
    },
    TargetValueOutOfRange {
        animation: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
    },
    InvalidTargetStyle {
        animation: SemanticNodeId,
        target_state: SemanticNodeId,
        error: StyleLoweringError,
    },
    InvalidTrack {
        animation: SemanticNodeId,
        error: TimelineError,
    },
}

impl std::fmt::Display for SemanticAffineAnimationTrackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Target {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot read object {}:{}: {error}",
                animation.slot(),
                animation.generation(),
                node.slot(),
                node.generation()
            ),
            Self::ScheduleMismatch { animation } => write!(
                formatter,
                "scheduled semantic animation {}:{} no longer matches its authored TransformTo declaration",
                animation.slot(),
                animation.generation()
            ),
            Self::MissingEffectiveTransform {
                animation,
                target,
                execution_object_id,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot capture effective transform for target {}:{} / execution object {}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                execution_object_id.get()
            ),
            Self::InvalidEffectiveTransform { animation, target } => write!(
                formatter,
                "semantic animation {}:{} received a non-finite effective transform for target {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::InvalidEffectiveStyle { animation, target } => write!(
                formatter,
                "semantic animation {}:{} received a non-finite effective style for target {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::UnsupportedContentChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes content from target {}:{} to target-state {}:{} before non-affine TransformTo payload lowering is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedStyleChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes style from target {}:{} to target-state {}:{} before style animation payload lowering is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedPainterOrderChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes z/painter order from target {}:{} to target-state {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedBindingChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes native-reactive binding topology from target {}:{} to target-state {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedDepthChange {
                animation,
                target,
                target_state,
                field,
            } => write!(
                formatter,
                "semantic animation {}:{} changes {field} depth between target {}:{} and target-state {}:{} before the current 2D execution domain can represent it",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedLifecycle {
                animation,
                remover,
                introducer,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot lower remover={remover} introducer={introducer} through affine-only execution tracks",
                animation.slot(),
                animation.generation()
            ),
            Self::ReactiveDriverConflict {
                animation,
                target,
                property,
            } => write!(
                formatter,
                "semantic animation {}:{} and a native-reactive binding both drive {property:?} on target {}:{} before driver arbitration is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::MultipleDrivers {
                first_animation,
                next_animation,
                target,
                property,
            } => write!(
                formatter,
                "semantic animations {}:{} and {}:{} both drive {property:?} on target {}:{} in one activation before composition driver chaining/arbitration is defined",
                first_animation.slot(),
                first_animation.generation(),
                next_animation.slot(),
                next_animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::InvalidTargetValue {
                animation,
                target_state,
                field,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} target-state {}:{} has invalid {field}: {error}",
                animation.slot(),
                animation.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::TargetValueOutOfRange {
                animation,
                target_state,
                field,
            } => write!(
                formatter,
                "semantic animation {}:{} target-state {}:{} {field} cannot lower to the current f32 execution domain",
                animation.slot(),
                animation.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::InvalidTargetStyle {
                animation,
                target_state,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} target-state {}:{} has invalid style: {error}",
                animation.slot(),
                animation.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::InvalidTrack { animation, error } => write!(
                formatter,
                "semantic animation {}:{} produced an invalid execution track: {error}",
                animation.slot(),
                animation.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticAffineAnimationTrackError {}

/// Lower the supported transform/style payload of one resolved semantic activation.
///
/// `effective_properties` is supplied by the execution/session owner and sampled at
/// most once per execution object. The compiler therefore never substitutes authored
/// base state for activation-time effective state and does not own the session barrier.
///
/// This path fails closed for unsupported content/stroke state, lifecycle semantics,
/// reactive/timeline driver conflicts, stale scheduled leaves, and multiple drivers
/// of one target/property until the corresponding shared policies exist.
pub fn lower_semantic_affine_animation_tracks<F>(
    store: &SemanticStore,
    schedule: &SemanticAnimationScheduleProjection,
    mut effective_properties: F,
) -> Result<SemanticAffineAnimationTrackProjection, SemanticAffineAnimationTrackError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    let mut captures = HashMap::<ObjectId, EffectiveAnimationProperties>::new();
    let mut driven = HashMap::<(u64, u8), SemanticNodeId>::new();
    let mut tracks = Vec::new();

    for leaf in schedule.leaves() {
        validate_leaf_matches_declaration(store, leaf)?;
        let target_state = match leaf.payload {
            SemanticScheduledAnimationPayload::TransformTo { target_state } => target_state,
            SemanticScheduledAnimationPayload::Fade { .. }
            | SemanticScheduledAnimationPayload::Create => {
                return Err(SemanticAffineAnimationTrackError::UnsupportedLifecycle {
                    animation: leaf.animation,
                    remover: leaf.options.remover,
                    introducer: leaf.options.introducer,
                });
            }
        };
        let source = object_state(store, leaf, leaf.target)?;
        let target = object_state(store, leaf, target_state)?;
        validate_affine_payload(source, target, leaf.options)
            .map_err(|issue| existing_payload_error(leaf, issue))?;
        let from = if let Some(captured) = captures.get(&leaf.execution_object_id).copied() {
            captured
        } else {
            let captured = effective_properties(leaf.execution_object_id).ok_or(
                SemanticAffineAnimationTrackError::MissingEffectiveTransform {
                    animation: leaf.animation,
                    target: leaf.target,
                    execution_object_id: leaf.execution_object_id,
                },
            )?;
            captures.insert(leaf.execution_object_id, captured);
            captured
        };
        let channels = lower_transform_channels(source, target, from)
            .map_err(|issue| existing_payload_error(leaf, issue))?;
        for channel in channels {
            let umbrella_conflict = transform_driver_conflict(
                &driven,
                leaf.execution_object_id,
                channel.property,
                leaf.animation,
            );
            if let Some(first_animation) = umbrella_conflict {
                return Err(SemanticAffineAnimationTrackError::MultipleDrivers {
                    first_animation,
                    next_animation: leaf.animation,
                    target: leaf.target,
                    property: channel.conflict_property,
                });
            }
            match driven.entry(driver_key(leaf.execution_object_id, channel.property)) {
                Entry::Occupied(entry) => {
                    return Err(SemanticAffineAnimationTrackError::MultipleDrivers {
                        first_animation: *entry.get(),
                        next_animation: leaf.animation,
                        target: leaf.target,
                        property: channel.conflict_property,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(leaf.animation);
                }
            }
            tracks.push(SemanticAffineAnimationTrack {
                animation: leaf.animation,
                target: leaf.target,
                execution_object_id: leaf.execution_object_id,
                property: channel.property,
                completion: channel.completion,
                values: channel.values,
                timing: leaf.timing,
                time_map: leaf.time_map.clone(),
            });
        }
    }

    Ok(SemanticAffineAnimationTrackProjection { tracks })
}

fn validate_leaf_matches_declaration(
    store: &SemanticStore,
    leaf: &SemanticScheduledAnimationLeaf,
) -> Result<(), SemanticAffineAnimationTrackError> {
    let animation = store
        .semantic_animation_state(leaf.animation)
        .map_err(SemanticAffineAnimationTrackError::Animation)?;
    match animation.intent() {
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        } if *target == leaf.target
            && leaf.payload
                == SemanticScheduledAnimationPayload::TransformTo {
                    target_state: *target_state,
                } =>
        {
            Ok(())
        }
        SemanticAnimationIntent::Fade { target, direction }
            if *target == leaf.target
                && leaf.payload
                    == SemanticScheduledAnimationPayload::Fade {
                        direction: *direction,
                    } =>
        {
            Ok(())
        }
        SemanticAnimationIntent::Create { target }
            if *target == leaf.target
                && leaf.payload == SemanticScheduledAnimationPayload::Create =>
        {
            Ok(())
        }
        _ => Err(SemanticAffineAnimationTrackError::ScheduleMismatch {
            animation: leaf.animation,
        }),
    }
}

fn scheduled_target_state(leaf: &SemanticScheduledAnimationLeaf) -> SemanticNodeId {
    match leaf.payload {
        SemanticScheduledAnimationPayload::TransformTo { target_state } => target_state,
        SemanticScheduledAnimationPayload::Fade { .. }
        | SemanticScheduledAnimationPayload::Create => {
            unreachable!("affine payload errors are only produced for TransformTo leaves")
        }
    }
}

fn object_state<'a>(
    store: &'a SemanticStore,
    leaf: &SemanticScheduledAnimationLeaf,
    node: SemanticNodeId,
) -> Result<&'a noon_core::SemanticObjectState, SemanticAffineAnimationTrackError> {
    store.semantic_object_state_checked(node).map_err(|error| {
        SemanticAffineAnimationTrackError::Target {
            animation: leaf.animation,
            node,
            error,
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LoweredAffineChannel {
    pub property: Property,
    pub conflict_property: SemanticObjectProperty,
    pub completion: SemanticAnimationCompletion,
    pub values: TrackValues,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum AffinePayloadIssue {
    InvalidEffectiveTransform,
    InvalidEffectiveStyle,
    UnsupportedContentChange,
    UnsupportedStyleChange,
    UnsupportedPainterOrderChange,
    UnsupportedBindingChange,
    UnsupportedDepthChange(SemanticAffineAnimationField),
    UnsupportedLifecycle {
        remover: bool,
        introducer: bool,
    },
    ReactiveDriverConflict(SemanticObjectProperty),
    InvalidTargetValue {
        field: SemanticAffineAnimationField,
        error: SemanticLoweringError,
    },
    TargetValueOutOfRange(SemanticAffineAnimationField),
    InvalidTargetStyle(SemanticExecutionValueError),
}

pub(super) fn validate_affine_payload(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    options: noon_core::ResolvedAnimationOptions,
) -> Result<(), AffinePayloadIssue> {
    validate_transform_payload_shape(source, target, options).map_err(Into::into)
}

impl From<TransformPayloadValidationIssue> for AffinePayloadIssue {
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

pub(super) fn lower_affine_channels(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    from: EffectiveAnimationProperties,
) -> Result<Vec<LoweredAffineChannel>, AffinePayloadIssue> {
    if !transform_is_finite(from.transform) {
        return Err(AffinePayloadIssue::InvalidEffectiveTransform);
    }
    if !style_is_finite(from.style) {
        return Err(AffinePayloadIssue::InvalidEffectiveStyle);
    }
    let translation = target
        .transform
        .translation
        .lower_xy_f32()
        .map_err(|error| AffinePayloadIssue::InvalidTargetValue {
            field: SemanticAffineAnimationField::Translation,
            error,
        })?;
    let scale = target.transform.scale.lower_xy_f32().map_err(|error| {
        AffinePayloadIssue::InvalidTargetValue {
            field: SemanticAffineAnimationField::Scale,
            error,
        }
    })?;
    let rotation = target.transform.rotation_z;
    if !rotation.is_finite() || rotation.abs() > f32::MAX as f64 {
        return Err(AffinePayloadIssue::TargetValueOutOfRange(
            SemanticAffineAnimationField::RotationZ,
        ));
    }
    let to = Transform2D {
        translation,
        rotation: rotation as f32,
        scale,
    };
    let target_style =
        lower_semantic_style_value(target).map_err(AffinePayloadIssue::InvalidTargetStyle)?;
    let mut channels = Vec::with_capacity(6);
    push_affine_channel(
        source,
        SemanticObjectProperty::Translation,
        Property::Position,
        TrackValues::Vec2 {
            from: from.transform.translation,
            to: to.translation,
        },
        SemanticAnimationCompletion::Property {
            property: SemanticObjectProperty::Translation,
            value: SemanticSignalValue::Vec3(target.transform.translation),
        },
        from.transform.translation != to.translation,
        &mut channels,
    )?;
    push_affine_channel(
        source,
        SemanticObjectProperty::RotationZ,
        Property::Rotation,
        TrackValues::Scalar {
            from: from.transform.rotation,
            to: to.rotation,
        },
        SemanticAnimationCompletion::Property {
            property: SemanticObjectProperty::RotationZ,
            value: SemanticSignalValue::Scalar(target.transform.rotation_z),
        },
        from.transform.rotation != to.rotation,
        &mut channels,
    )?;
    push_affine_channel(
        source,
        SemanticObjectProperty::Scale,
        Property::Scale,
        TrackValues::Vec2 {
            from: from.transform.scale,
            to: to.scale,
        },
        SemanticAnimationCompletion::Property {
            property: SemanticObjectProperty::Scale,
            value: SemanticSignalValue::Vec3(target.transform.scale),
        },
        from.transform.scale != to.scale,
        &mut channels,
    )?;
    let fill_changed = source.style.fill != target.style.fill
        || source.style.fill_opacity != target.style.fill_opacity
        || (from.style.fill != target_style.fill
            && !has_binding(source, SemanticObjectProperty::FillOpacity));
    push_affine_channel(
        source,
        SemanticObjectProperty::FillOpacity,
        Property::Fill,
        TrackValues::Color {
            from: from.style.fill,
            to: target_style.fill,
        },
        SemanticAnimationCompletion::Fill {
            paint: target.style.fill.clone(),
            opacity: target.style.fill_opacity,
        },
        fill_changed,
        &mut channels,
    )?;
    let stroke_changed = source.style.stroke != target.style.stroke
        || source.style.stroke_opacity != target.style.stroke_opacity
        || (from.style.stroke != target_style.stroke
            && !has_binding(source, SemanticObjectProperty::StrokeOpacity));
    push_affine_channel(
        source,
        SemanticObjectProperty::StrokeOpacity,
        Property::Stroke,
        TrackValues::Color {
            from: from.style.stroke,
            to: target_style.stroke,
        },
        SemanticAnimationCompletion::Stroke {
            paint: target.style.stroke.clone(),
            opacity: target.style.stroke_opacity,
        },
        stroke_changed,
        &mut channels,
    )?;
    let opacity_changed = source.style.object_opacity != target.style.object_opacity
        || (from.style.opacity != target_style.opacity
            && !has_binding(source, SemanticObjectProperty::ObjectOpacity));
    push_affine_channel(
        source,
        SemanticObjectProperty::ObjectOpacity,
        Property::Opacity,
        TrackValues::Scalar {
            from: from.style.opacity,
            to: target_style.opacity,
        },
        SemanticAnimationCompletion::Property {
            property: SemanticObjectProperty::ObjectOpacity,
            value: SemanticSignalValue::Scalar(target.style.object_opacity),
        },
        opacity_changed,
        &mut channels,
    )?;
    Ok(channels)
}

pub(super) fn lower_transform_channels(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    from: EffectiveAnimationProperties,
) -> Result<Vec<LoweredAffineChannel>, AffinePayloadIssue> {
    if source.content == target.content {
        return lower_affine_channels(source, target, from);
    }
    if !is_supported_analytic_content_morph(source, target) {
        return Err(AffinePayloadIssue::UnsupportedContentChange);
    }
    if let Some(binding) = source.signal_bindings().first() {
        return Err(AffinePayloadIssue::ReactiveDriverConflict(
            binding.property(),
        ));
    }
    if !transform_is_finite(from.transform) {
        return Err(AffinePayloadIssue::InvalidEffectiveTransform);
    }
    if !style_is_finite(from.style) {
        return Err(AffinePayloadIssue::InvalidEffectiveStyle);
    }

    let geometry = |content| match content {
        SemanticObjectContent::Geometry(StoredGeometry::Circle { radius }) => {
            Ok(noon_core::GeometryRef::circle(radius))
        }
        SemanticObjectContent::Geometry(StoredGeometry::Rectangle { size }) => {
            Ok(noon_core::GeometryRef::Rectangle { size })
        }
        _ => Err(AffinePayloadIssue::UnsupportedContentChange),
    };
    let target_transform = lower_semantic_transform_value(target)
        .map_err(|_| AffinePayloadIssue::InvalidEffectiveTransform)?;
    let target_style =
        lower_semantic_style_value(target).map_err(AffinePayloadIssue::InvalidTargetStyle)?;
    let (prepared, render_transform) = crate::transform::compile_analytic_content_morph(
        &geometry(source.content)?,
        &geometry(target.content)?,
        from.style,
        target_style,
        from.transform,
        target_transform,
    )
    .map_err(|_| AffinePayloadIssue::UnsupportedContentChange)?;
    let mut channels = lower_affine_channels(source, target, from)?;
    channels.push(LoweredAffineChannel {
        property: Property::Morph,
        conflict_property: SemanticObjectProperty::Translation,
        completion: SemanticAnimationCompletion::ContentMorph {
            content: target.content,
        },
        values: TrackValues::PreparedMorph {
            from: 0.0,
            to: 1.0,
            geometry: prepared,
            render_transform,
        },
    });
    Ok(channels)
}

#[allow(clippy::too_many_arguments)]
fn push_affine_channel(
    source: &noon_core::SemanticObjectState,
    semantic_property: SemanticObjectProperty,
    property: Property,
    values: TrackValues,
    completion: SemanticAnimationCompletion,
    changed: bool,
    channels: &mut Vec<LoweredAffineChannel>,
) -> Result<(), AffinePayloadIssue> {
    if !changed {
        return Ok(());
    }
    if has_binding(source, semantic_property) {
        return Err(AffinePayloadIssue::ReactiveDriverConflict(
            semantic_property,
        ));
    }
    channels.push(LoweredAffineChannel {
        property,
        conflict_property: semantic_property,
        completion,
        values,
    });
    Ok(())
}

// An unchanged authored domain with a native binding remains owned by that
// binding. Capturing its current effective value does not request a new driver.
fn has_binding(source: &noon_core::SemanticObjectState, property: SemanticObjectProperty) -> bool {
    source
        .signal_bindings()
        .iter()
        .any(|binding| binding.property() == property)
}

fn existing_payload_error(
    leaf: &SemanticScheduledAnimationLeaf,
    issue: AffinePayloadIssue,
) -> SemanticAffineAnimationTrackError {
    match issue {
        AffinePayloadIssue::InvalidEffectiveTransform => {
            SemanticAffineAnimationTrackError::InvalidEffectiveTransform {
                animation: leaf.animation,
                target: leaf.target,
            }
        }
        AffinePayloadIssue::InvalidEffectiveStyle => {
            SemanticAffineAnimationTrackError::InvalidEffectiveStyle {
                animation: leaf.animation,
                target: leaf.target,
            }
        }
        AffinePayloadIssue::UnsupportedContentChange => {
            SemanticAffineAnimationTrackError::UnsupportedContentChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: scheduled_target_state(leaf),
            }
        }
        AffinePayloadIssue::UnsupportedStyleChange => {
            SemanticAffineAnimationTrackError::UnsupportedStyleChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: scheduled_target_state(leaf),
            }
        }
        AffinePayloadIssue::UnsupportedPainterOrderChange => {
            SemanticAffineAnimationTrackError::UnsupportedPainterOrderChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: scheduled_target_state(leaf),
            }
        }
        AffinePayloadIssue::UnsupportedBindingChange => {
            SemanticAffineAnimationTrackError::UnsupportedBindingChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: scheduled_target_state(leaf),
            }
        }
        AffinePayloadIssue::UnsupportedDepthChange(field) => {
            SemanticAffineAnimationTrackError::UnsupportedDepthChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: scheduled_target_state(leaf),
                field,
            }
        }
        AffinePayloadIssue::UnsupportedLifecycle {
            remover,
            introducer,
        } => SemanticAffineAnimationTrackError::UnsupportedLifecycle {
            animation: leaf.animation,
            remover,
            introducer,
        },
        AffinePayloadIssue::ReactiveDriverConflict(property) => {
            SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation: leaf.animation,
                target: leaf.target,
                property,
            }
        }
        AffinePayloadIssue::InvalidTargetValue { field, error } => {
            SemanticAffineAnimationTrackError::InvalidTargetValue {
                animation: leaf.animation,
                target_state: scheduled_target_state(leaf),
                field,
                error,
            }
        }
        AffinePayloadIssue::TargetValueOutOfRange(field) => {
            SemanticAffineAnimationTrackError::TargetValueOutOfRange {
                animation: leaf.animation,
                target_state: scheduled_target_state(leaf),
                field,
            }
        }
        AffinePayloadIssue::InvalidTargetStyle(error) => {
            SemanticAffineAnimationTrackError::InvalidTargetStyle {
                animation: leaf.animation,
                target_state: scheduled_target_state(leaf),
                error: error.with_node(scheduled_target_state(leaf)),
            }
        }
    }
}

/// Check only this object's bounded channel set, never the entire composition.
pub(super) fn transform_driver_conflict<T: Copy + PartialEq>(
    driven: &HashMap<(u64, u8), T>,
    object: ObjectId,
    property: Property,
    animation: T,
) -> Option<T> {
    let (object, transform_slot) = driver_key(object, Property::Transform);
    let (_, morph_slot) = driver_key(object, Property::Morph);
    if property == Property::Morph {
        (0..morph_slot).find_map(|slot| {
            driven
                .get(&(object, slot))
                .copied()
                .filter(|owner| *owner != animation)
        })
    } else if property == Property::Transform {
        (0..=transform_slot).find_map(|slot| driven.get(&(object, slot)).copied())
    } else {
        driven
            .get(&(object, transform_slot))
            .or_else(|| driven.get(&(object, morph_slot)))
            .copied()
            .filter(|owner| *owner != animation)
    }
}

pub(super) fn driver_key(object: ObjectId, property: Property) -> (u64, u8) {
    let slot = match property {
        Property::Position => 0,
        Property::Rotation => 1,
        Property::Scale => 2,
        Property::Fill => 3,
        Property::Stroke => 4,
        Property::Opacity => 5,
        Property::Appearance => 6,
        Property::Reveal => 7,
        Property::Transform => 8,
        Property::Morph => 9,
        _ => unreachable!("shared animation payload lowering only registers supported drivers"),
    };
    (object.get(), slot)
}

fn style_is_finite(style: Style) -> bool {
    let color_is_finite = |color: Option<noon_core::Color>| {
        color.is_none_or(|color| {
            color.red.is_finite()
                && color.green.is_finite()
                && color.blue.is_finite()
                && color.alpha.is_finite()
        })
    };
    color_is_finite(style.fill)
        && color_is_finite(style.stroke)
        && style.stroke_width.is_finite()
        && style.opacity.is_finite()
}

pub(super) fn transform_is_finite(transform: Transform2D) -> bool {
    transform.translation.x.is_finite()
        && transform.translation.y.is_finite()
        && transform.rotation.is_finite()
        && transform.scale.x.is_finite()
        && transform.scale.y.is_finite()
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, Color, SemanticObjectState, SemanticPaint, SemanticVec3, StoredGeometry,
        Vec2,
    };

    use super::*;
    use crate::{lower_semantic_animation_schedule, SemanticExecutionIndex};

    fn effective(transform: Transform2D) -> EffectiveAnimationProperties {
        EffectiveAnimationProperties {
            transform,
            style: Style::default(),
            appearance: 1.0,
        }
    }

    fn visible_object(store: &mut SemanticStore) -> SemanticNodeId {
        let id = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }));
        store.attach_to_scene(id).unwrap();
        id
    }

    fn index(store: &SemanticStore) -> SemanticExecutionIndex {
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(store).unwrap();
        index
    }

    fn schedule(
        store: &SemanticStore,
        index: &SemanticExecutionIndex,
        animation: SemanticNodeId,
    ) -> SemanticAnimationScheduleProjection {
        lower_semantic_animation_schedule(store, index, animation, 4.0, AnimationOptions::new())
            .unwrap()
    }

    #[test]
    fn lowers_affine_target_from_effective_activation_state() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(10.0, -3.0, 0.0);
        target_state.transform.rotation_z = 1.25;
        target_state.transform.scale = SemanticVec3::new(2.0, 0.5, 1.0);
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let projection = lower_semantic_affine_animation_tracks(
            &store,
            &schedule(&store, &index, animation),
            |_| {
                Some(effective(Transform2D {
                    translation: Vec2::new(5.0, 7.0),
                    rotation: 0.25,
                    scale: Vec2::new(1.5, 1.5),
                }))
            },
        )
        .unwrap();

        assert_eq!(projection.len(), 3);
        assert_eq!(projection.tracks()[0].property, Property::Position);
        assert_eq!(
            projection.tracks()[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(5.0, 7.0),
                to: Vec2::new(10.0, -3.0),
            }
        );
        assert_eq!(
            projection.tracks()[1].values,
            TrackValues::Scalar {
                from: 0.25,
                to: 1.25,
            }
        );
        assert_eq!(
            projection.tracks()[2].values,
            TrackValues::Vec2 {
                from: Vec2::new(1.5, 1.5),
                to: Vec2::new(2.0, 0.5),
            }
        );
        assert_eq!(
            projection.tracks()[0]
                .with_track_id(TrackId::new(42))
                .unwrap()
                .id,
            TrackId::new(42)
        );
    }

    #[test]
    fn paint_channels_capture_effective_values_and_reconcile_exact_fields() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.style.fill = Some(SemanticPaint::Solid(Color::RED));
        target_state.style.fill_opacity = 0.5;
        target_state.style.stroke = Some(SemanticPaint::Solid(Color::GREEN));
        target_state.style.stroke_opacity = 0.4;
        target_state.style.object_opacity = 0.25;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let current = Style {
            fill: Some(Color::BLUE),
            stroke: Some(Color::WHITE),
            opacity: 0.75,
            ..Style::default()
        };

        let projection = lower_semantic_affine_animation_tracks(
            &store,
            &schedule(&store, &index, animation),
            |_| {
                Some(EffectiveAnimationProperties {
                    transform: Transform2D::default(),
                    style: current,
                    appearance: 1.0,
                })
            },
        )
        .unwrap();

        assert_eq!(projection.len(), 3);
        assert_eq!(projection.tracks()[0].property, Property::Fill);
        assert_eq!(
            projection.tracks()[0].values,
            TrackValues::Color {
                from: Some(Color::BLUE),
                to: Some(Color {
                    alpha: 0.5,
                    ..Color::RED
                }),
            }
        );
        assert_eq!(
            projection.tracks()[0].completion,
            SemanticAnimationCompletion::Fill {
                paint: Some(SemanticPaint::Solid(Color::RED)),
                opacity: 0.5,
            }
        );
        assert_eq!(projection.tracks()[1].property, Property::Stroke);
        assert_eq!(
            projection.tracks()[1].completion,
            SemanticAnimationCompletion::Stroke {
                paint: Some(SemanticPaint::Solid(Color::GREEN)),
                opacity: 0.4,
            }
        );
        assert_eq!(projection.tracks()[2].property, Property::Opacity);
        assert_eq!(
            projection.tracks()[2].completion,
            SemanticAnimationCompletion::Property {
                property: SemanticObjectProperty::ObjectOpacity,
                value: SemanticSignalValue::Scalar(0.25),
            }
        );
    }

    #[test]
    fn fill_binding_conflict_fails_before_style_track_publication() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let opacity = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store
            .bind_semantic_signal(opacity, target, SemanticObjectProperty::FillOpacity)
            .unwrap();
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.style.fill_opacity = 0.5;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(effective(Transform2D::default())),
            ),
            Err(SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation,
                target,
                property: SemanticObjectProperty::FillOpacity,
            })
        );
    }

    #[test]
    fn stroke_binding_conflict_fails_before_paint_track_publication() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let opacity = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store
            .bind_semantic_signal(opacity, target, SemanticObjectProperty::StrokeOpacity)
            .unwrap();
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.style.stroke = Some(SemanticPaint::Solid(Color::GREEN));
        target_state.style.stroke_opacity = 0.5;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(effective(Transform2D::default())),
            ),
            Err(SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation,
                target,
                property: SemanticObjectProperty::StrokeOpacity,
            })
        );
    }

    #[test]
    fn stroke_width_change_remains_explicitly_unsupported() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.style.stroke_width = 2.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(effective(Transform2D::default())),
            ),
            Err(SemanticAffineAnimationTrackError::UnsupportedStyleChange {
                animation,
                target,
                target_state,
            })
        );
    }

    #[test]
    fn unchanged_bound_style_domains_remain_reactive_during_transform() {
        for property in [
            SemanticObjectProperty::FillOpacity,
            SemanticObjectProperty::ObjectOpacity,
        ] {
            let mut store = SemanticStore::new();
            let object = visible_object(&mut store);
            let signal = store.insert_semantic_input_signal(0.65_f64).unwrap();
            store
                .bind_semantic_signal(signal, object, property)
                .unwrap();
            let mut target = store.semantic_object_state_checked(object).unwrap().clone();
            target.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
            let target = store.insert_semantic_object(target);
            let animation = store
                .insert_semantic_transform_animation(object, target, AnimationOptions::new())
                .unwrap();
            let index = index(&store);
            let mut current = effective(Transform2D::default());
            match property {
                SemanticObjectProperty::FillOpacity => {
                    current.style.fill.as_mut().unwrap().alpha = 0.65
                }
                SemanticObjectProperty::ObjectOpacity => current.style.opacity = 0.65,
                _ => unreachable!(),
            }
            let existing = lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(current),
            )
            .unwrap();
            assert_eq!(existing.len(), 1);
            assert_eq!(existing.tracks()[0].property, Property::Position);
        }
    }

    #[test]
    fn unchanged_bound_stroke_remains_reactive_for_predeclared_transform() {
        let mut store = SemanticStore::new();
        let object = visible_object(&mut store);
        let signal = store.insert_semantic_input_signal(0.65_f64).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::StrokeOpacity)
            .unwrap();
        let mut target = store.semantic_object_state_checked(object).unwrap().clone();
        target.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let current = EffectiveAnimationProperties {
            transform: Transform2D::default(),
            style: Style {
                stroke: Some(Color {
                    alpha: 0.65,
                    ..Color::WHITE
                }),
                ..Style::default()
            },
            appearance: 1.0,
        };

        let predeclared = lower_semantic_affine_animation_tracks(
            &store,
            &schedule(&store, &index, animation),
            |_| Some(current),
        )
        .unwrap();
        assert_eq!(predeclared.len(), 1);
        assert_eq!(predeclared.tracks()[0].property, Property::Position);
    }

    #[test]
    fn captures_each_execution_object_once_per_activation() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);

        let mut rotation_state = store.semantic_object_state_checked(target).unwrap().clone();
        rotation_state.transform.rotation_z = 0.5;
        let rotation_state = store.insert_semantic_object(rotation_state);
        let rotation = store
            .insert_semantic_transform_animation(target, rotation_state, AnimationOptions::new())
            .unwrap();

        let mut scale_state = store.semantic_object_state_checked(target).unwrap().clone();
        scale_state.transform.scale = SemanticVec3::new(2.0, 2.0, 1.0);
        let scale_state = store.insert_semantic_object(scale_state);
        let scale = store
            .insert_semantic_transform_animation(target, scale_state, AnimationOptions::new())
            .unwrap();
        let root = store
            .insert_semantic_sequence_animation(&[rotation, scale], AnimationOptions::new())
            .unwrap();

        let index = index(&store);
        let calls = std::cell::Cell::new(0);
        let projection =
            lower_semantic_affine_animation_tracks(&store, &schedule(&store, &index, root), |_| {
                calls.set(calls.get() + 1);
                Some(effective(Transform2D::default()))
            })
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert_eq!(projection.len(), 2);
    }

    #[test]
    fn stale_scheduled_leaf_is_rejected_against_authored_declaration() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 2.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let mut leaf = schedule(&store, &index, animation).leaves()[0].clone();
        leaf.payload = SemanticScheduledAnimationPayload::TransformTo {
            target_state: target,
        };

        assert_eq!(
            validate_leaf_matches_declaration(&store, &leaf),
            Err(SemanticAffineAnimationTrackError::ScheduleMismatch { animation })
        );
    }

    #[test]
    fn predeclared_fade_requires_the_prepared_lifecycle_activation_path() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let animation = store
            .insert_semantic_fade_animation(
                target,
                SemanticFadeDirection::Out,
                AnimationOptions::new(),
            )
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(effective(Transform2D::default())),
            ),
            Err(SemanticAffineAnimationTrackError::UnsupportedLifecycle {
                animation,
                remover: true,
                introducer: false,
            })
        );
    }

    #[test]
    fn duplicate_property_drivers_fail_closed_with_keyed_detection() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut first_state = store.semantic_object_state_checked(target).unwrap().clone();
        first_state.transform.translation.x = 2.0;
        let first_state = store.insert_semantic_object(first_state);
        let first = store
            .insert_semantic_transform_animation(target, first_state, AnimationOptions::new())
            .unwrap();
        let mut second_state = store.semantic_object_state_checked(target).unwrap().clone();
        second_state.transform.translation.x = 3.0;
        let second_state = store.insert_semantic_object(second_state);
        let second = store
            .insert_semantic_transform_animation(target, second_state, AnimationOptions::new())
            .unwrap();
        let root = store
            .insert_semantic_sequence_animation(&[first, second], AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(&store, &schedule(&store, &index, root), |_| {
                Some(effective(Transform2D::default()))
            },),
            Err(SemanticAffineAnimationTrackError::MultipleDrivers {
                first_animation: first,
                next_animation: second,
                target,
                property: SemanticObjectProperty::Translation,
            })
        );
    }

    #[test]
    fn reactive_affine_driver_conflict_fails_closed() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::ZERO)
            .unwrap();
        store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::Translation)
            .unwrap();
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 4.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(effective(Transform2D::default())),
            ),
            Err(SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation,
                target,
                property: SemanticObjectProperty::Translation,
            })
        );
    }

    #[test]
    fn missing_effective_capture_never_falls_back_to_authored_state() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 2.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let execution_object_id = index.execution_object_id(target).unwrap();

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| None,
            ),
            Err(
                SemanticAffineAnimationTrackError::MissingEffectiveTransform {
                    animation,
                    target,
                    execution_object_id,
                }
            )
        );
    }
}
