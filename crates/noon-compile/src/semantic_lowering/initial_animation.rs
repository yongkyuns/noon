use noon_core::{
    AnimationOptions, Property, SemanticAnimationCompositionKind, SemanticAnimationError,
    SemanticAnimationIntent, SemanticNodeId, SemanticObjectContent, SemanticObjectTrackProperty,
    SemanticObjectTrackValues, SemanticSceneOperationError, SemanticStore, TrackDefinition,
    TrackId, TrackValues, TransformTrackEndpoint,
};

use crate::{
    CompilePatchError, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
    SemanticExecutionIndex,
};

use super::{
    compiled_scene::{lower_semantic_geometry_value, SemanticGeometryValueError},
    lower_semantic_animation_schedule, lower_semantic_text_glyph_animations,
    projection::{lower_semantic_style, lower_semantic_transform, SemanticLoweringError},
    reject_family_driver_conflicts, SemanticAnimationScheduleError,
    SemanticScheduledAnimationPayload, TextGlyphLoweringError,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticInitialAnimationError {
    InvalidOrigin(f64),
    Schedule(SemanticAnimationScheduleError),
    Family(TextGlyphLoweringError),
    Animation(SemanticAnimationError),
    InvalidRoot {
        animation: SemanticNodeId,
    },
    InvalidLeaf {
        animation: SemanticNodeId,
    },
    TargetOutsideProjection {
        animation: SemanticNodeId,
        target: SemanticNodeId,
    },
    ValueOutOfRange {
        animation: SemanticNodeId,
    },
    Endpoint {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    EndpointValue {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticLoweringError,
    },
    EndpointGeometry {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticGeometryValueError,
    },
    TooManyTracks(usize),
    Compiled(CompilePatchError),
}

impl std::fmt::Display for SemanticInitialAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrigin(origin) => write!(formatter, "initial animation origin must be finite: {origin}"),
            Self::Schedule(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
            Self::Animation(error) => error.fmt(formatter),
            Self::InvalidRoot { animation } => write!(
                formatter,
                "initial animation root {}:{} must be an option-free parallel composition",
                animation.slot(),
                animation.generation()
            ),
            Self::InvalidLeaf { animation } => write!(
                formatter,
                "initial animation child {}:{} must be an exact object track or neutral family composition",
                animation.slot(),
                animation.generation()
            ),
            Self::TargetOutsideProjection { animation, target } => write!(
                formatter,
                "initial animation {}:{} targets object {}:{} outside the selected scene projection",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::ValueOutOfRange { animation } => write!(
                formatter,
                "initial animation {}:{} has an endpoint outside the execution value range",
                animation.slot(),
                animation.generation()
            ),
            Self::Endpoint {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "initial animation {}:{} cannot read endpoint {}:{}: {error}",
                animation.slot(),
                animation.generation(),
                node.slot(),
                node.generation()
            ),
            Self::EndpointValue {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "initial animation {}:{} cannot lower endpoint {}:{}: {error}",
                animation.slot(),
                animation.generation(),
                node.slot(),
                node.generation()
            ),
            Self::EndpointGeometry {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "initial animation {}:{} cannot lower endpoint {}:{} geometry: {error}",
                animation.slot(),
                animation.generation(),
                node.slot(),
                node.generation()
            ),
            Self::TooManyTracks(count) => {
                write!(formatter, "initial animation contains too many tracks: {count}")
            }
            Self::Compiled(error) => write!(formatter, "initial track installation failed: {error}"),
        }
    }
}

impl std::error::Error for SemanticInitialAnimationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Schedule(error) => Some(error),
            Self::Family(error) => Some(error),
            Self::Animation(error) => Some(error),
            Self::Endpoint { error, .. } => Some(error),
            Self::EndpointValue { error, .. } => Some(error),
            Self::EndpointGeometry { error, .. } => Some(error),
            Self::Compiled(error) => Some(error),
            _ => None,
        }
    }
}

pub(super) fn install_initial_animation_root(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    compiled: &mut CompiledScene,
    root: SemanticNodeId,
    origin: f64,
) -> Result<(), SemanticInitialAnimationError> {
    if !origin.is_finite() {
        return Err(SemanticInitialAnimationError::InvalidOrigin(origin));
    }
    let root_state = store
        .semantic_animation_state(root)
        .map_err(SemanticInitialAnimationError::Animation)?;
    let SemanticAnimationIntent::Composition { kind, children } = root_state.intent() else {
        return Err(SemanticInitialAnimationError::InvalidRoot { animation: root });
    };
    if *kind != SemanticAnimationCompositionKind::Parallel
        || root_state.options() != AnimationOptions::new()
    {
        return Err(SemanticInitialAnimationError::InvalidRoot { animation: root });
    }

    let mut definitions = Vec::new();
    let mut family_animations = Vec::new();
    let mut family_drivers = Vec::new();
    for &animation in children {
        let state = store
            .semantic_animation_state(animation)
            .map_err(SemanticInitialAnimationError::Animation)?;
        let SemanticAnimationIntent::ObjectPropertyTrack {
            target,
            property,
            values,
            timing,
            time_map,
        } = state.intent()
        else {
            let schedule = lower_semantic_animation_schedule(
                store,
                index,
                animation,
                origin,
                AnimationOptions::new(),
            )
            .map_err(SemanticInitialAnimationError::Schedule)?;
            if !schedule.scalar_leaves().is_empty() || schedule.leaves().is_empty() {
                return Err(SemanticInitialAnimationError::InvalidLeaf { animation });
            }
            for leaf in schedule.leaves() {
                if !matches!(
                    leaf.payload,
                    SemanticScheduledAnimationPayload::TextGlyph { .. }
                ) || leaf.options.introducer
                    || leaf.options.remover
                {
                    return Err(SemanticInitialAnimationError::InvalidLeaf {
                        animation: leaf.animation,
                    });
                }
                if compiled.object_index(leaf.execution_object_id).is_none() {
                    return Err(SemanticInitialAnimationError::TargetOutsideProjection {
                        animation: leaf.animation,
                        target: leaf.target,
                    });
                }
                let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                    .map_err(|error| {
                        SemanticInitialAnimationError::Family(
                            TextGlyphLoweringError::InvalidTimeMap(error),
                        )
                    })?;
                family_drivers.push((
                    leaf.animation.into(),
                    leaf.execution_object_id,
                    interval,
                    true,
                ));
            }
            family_animations.extend(
                lower_semantic_text_glyph_animations(store, &schedule)
                    .map_err(SemanticInitialAnimationError::Family)?,
            );
            continue;
        };
        if state.options() != AnimationOptions::new() {
            return Err(SemanticInitialAnimationError::InvalidLeaf { animation });
        }
        let object = index
            .execution_object_id(*target)
            .filter(|object| compiled.object_index(*object).is_some())
            .ok_or(SemanticInitialAnimationError::TargetOutsideProjection {
                animation,
                target: *target,
            })?;
        let (property, values) = lower_object_track_values(store, animation, *property, values)?;
        let id = TrackId::new(
            u64::try_from(definitions.len())
                .map_err(|_| SemanticInitialAnimationError::TooManyTracks(definitions.len()))?,
        );
        definitions.push(TrackDefinition {
            id,
            object,
            property,
            values,
            timing: *timing,
            time_map: time_map.clone(),
        });
    }

    reject_family_driver_conflicts(&family_drivers)
        .map_err(SemanticInitialAnimationError::Family)?;
    let transaction = ExecutionMutationTransaction::from_mutations(
        definitions.into_iter().map(ExecutionPatch::AddTrack).chain(
            family_animations
                .into_iter()
                .map(ExecutionPatch::AddFamilyAnimation),
        ),
    );
    compiled
        .preflight_execution_transaction(&transaction)
        .map_err(SemanticInitialAnimationError::Compiled)?;
    for mutation in transaction.mutations() {
        compiled
            .apply_execution_patch(mutation)
            .map_err(SemanticInitialAnimationError::Compiled)?;
    }
    Ok(())
}

fn lower_object_track_values(
    store: &SemanticStore,
    animation: SemanticNodeId,
    property: SemanticObjectTrackProperty,
    values: &SemanticObjectTrackValues,
) -> Result<(Property, TrackValues), SemanticInitialAnimationError> {
    let direct = match (property, values) {
        (SemanticObjectTrackProperty::Presence, SemanticObjectTrackValues::Bool { from, to }) => (
            Property::Presence,
            TrackValues::Bool {
                from: *from,
                to: *to,
            },
        ),
        (SemanticObjectTrackProperty::Position, SemanticObjectTrackValues::Vec3 { from, to }) => (
            Property::Position,
            TrackValues::Vec2 {
                from: from
                    .lower_xy_f32()
                    .map_err(|_| SemanticInitialAnimationError::ValueOutOfRange { animation })?,
                to: to
                    .lower_xy_f32()
                    .map_err(|_| SemanticInitialAnimationError::ValueOutOfRange { animation })?,
            },
        ),
        (SemanticObjectTrackProperty::Scale, SemanticObjectTrackValues::Vec3 { from, to }) => (
            Property::Scale,
            TrackValues::Vec2 {
                from: from
                    .lower_xy_f32()
                    .map_err(|_| SemanticInitialAnimationError::ValueOutOfRange { animation })?,
                to: to
                    .lower_xy_f32()
                    .map_err(|_| SemanticInitialAnimationError::ValueOutOfRange { animation })?,
            },
        ),
        (SemanticObjectTrackProperty::Fill, SemanticObjectTrackValues::Color { from, to }) => (
            Property::Fill,
            TrackValues::Color {
                from: *from,
                to: *to,
            },
        ),
        (SemanticObjectTrackProperty::Stroke, SemanticObjectTrackValues::Color { from, to }) => (
            Property::Stroke,
            TrackValues::Color {
                from: *from,
                to: *to,
            },
        ),
        (semantic_property, SemanticObjectTrackValues::Scalar { from, to }) => {
            let property = match semantic_property {
                SemanticObjectTrackProperty::Rotation => Property::Rotation,
                SemanticObjectTrackProperty::StrokeWidth => Property::StrokeWidth,
                SemanticObjectTrackProperty::Opacity => Property::Opacity,
                SemanticObjectTrackProperty::Appearance => Property::Appearance,
                SemanticObjectTrackProperty::Reveal => Property::Reveal,
                SemanticObjectTrackProperty::Morph => Property::Morph,
                _ => return Err(SemanticInitialAnimationError::InvalidLeaf { animation }),
            };
            let lower = |value: f64| {
                if value.abs() <= f32::MAX as f64 {
                    Ok(value as f32)
                } else {
                    Err(SemanticInitialAnimationError::ValueOutOfRange { animation })
                }
            };
            (
                property,
                TrackValues::Scalar {
                    from: lower(*from)?,
                    to: lower(*to)?,
                },
            )
        }
        (
            SemanticObjectTrackProperty::Transform,
            SemanticObjectTrackValues::Object { from, to },
        ) => (
            Property::Transform,
            TrackValues::Object {
                from: lower_transform_endpoint(store, animation, *from)?,
                to: lower_transform_endpoint(store, animation, *to)?,
            },
        ),
        _ => return Err(SemanticInitialAnimationError::InvalidLeaf { animation }),
    };
    Ok(direct)
}

fn lower_transform_endpoint(
    store: &SemanticStore,
    animation: SemanticNodeId,
    node: SemanticNodeId,
) -> Result<TransformTrackEndpoint, SemanticInitialAnimationError> {
    let state = store.semantic_object_state_checked(node).map_err(|error| {
        SemanticInitialAnimationError::Endpoint {
            animation,
            node,
            error,
        }
    })?;
    let SemanticObjectContent::Geometry(content) = state.content else {
        return Err(SemanticInitialAnimationError::InvalidLeaf { animation });
    };
    let geometry =
        lower_semantic_geometry_value(content.geometry(), Some(store)).map_err(|error| {
            SemanticInitialAnimationError::EndpointGeometry {
                animation,
                node,
                error,
            }
        })?;
    let transform = lower_semantic_transform(node, state).map_err(|error| {
        SemanticInitialAnimationError::EndpointValue {
            animation,
            node,
            error,
        }
    })?;
    let style = lower_semantic_style(node, state).map_err(|error| {
        SemanticInitialAnimationError::EndpointValue {
            animation,
            node,
            error,
        }
    })?;
    Ok(TransformTrackEndpoint {
        geometry,
        transform,
        style,
    })
}

#[cfg(test)]
mod tests;
