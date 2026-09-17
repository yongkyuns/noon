use noon_core::{
    ObjectContentRef, ObjectId, Property, SemanticNodeId, SemanticStore, Style, TrackValues,
    Transform2D, Vec2,
};

use super::matching_family_transform_activation::PreparedMatchingFamilyTransformActivation;
use super::matching_shape_leftovers::{
    prepare_matching_shape_leftover_layout, PreparedMatchingShapeLeftoverLayoutError,
};

/// One execution-timed channel for a matching-shape leftover presentation.
///
/// The track owns no stable or transient identity. Source leftovers are later attached
/// to their existing execution row; target leftovers are later attached to an
/// identity-free authored transient base after painter placement has been resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeLeftoverFadeTrack {
    pub property: Property,
    pub values: TrackValues,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
}

/// FadeOut staging for one unmatched source member that already owns stable execution
/// identity.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeSourceLeftoverFade {
    pub source_index: usize,
    pub node: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub tracks: Vec<PreparedMatchingShapeLeftoverFadeTrack>,
}

/// Authored visual base for one unmatched detached target member.
///
/// This value deliberately has no execution object, painter anchor, or semantic
/// membership mutation. It is the compiler-owned source for a later identity-free
/// transient presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeTargetTransientBase {
    pub node: SemanticNodeId,
    pub z_index: f64,
    pub content: ObjectContentRef,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub reveal: f32,
    pub morph: f32,
}

/// FadeIn staging for one unmatched detached target member.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeTargetLeftoverFade {
    pub target_index: usize,
    pub base: PreparedMatchingShapeTargetTransientBase,
    pub tracks: Vec<PreparedMatchingShapeLeftoverFadeTrack>,
}

/// Directional default-leftover projection for one matching-shape activation.
///
/// Matched groups are absent by construction. Every source-rest member uses the same
/// source->target group displacement. Every target-rest member stays at its authored
/// target transform and fades in there. No source/target member is paired positionally.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedMatchingShapeLeftoverFadeProjection {
    source_fades: Vec<PreparedMatchingShapeSourceLeftoverFade>,
    target_fades: Vec<PreparedMatchingShapeTargetLeftoverFade>,
}

impl PreparedMatchingShapeLeftoverFadeProjection {
    pub fn source_fades(&self) -> &[PreparedMatchingShapeSourceLeftoverFade] {
        &self.source_fades
    }

    pub fn target_fades(&self) -> &[PreparedMatchingShapeTargetLeftoverFade] {
        &self.target_fades
    }

    pub fn is_empty(&self) -> bool {
        self.source_fades.is_empty() && self.target_fades.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedMatchingShapeLeftoverFadeError {
    Layout(PreparedMatchingShapeLeftoverLayoutError),
    InvalidSourceIndex(usize),
    InvalidTargetIndex(usize),
    TargetState {
        node: SemanticNodeId,
    },
    UnsupportedTargetGeometry {
        node: SemanticNodeId,
    },
    InvalidTargetStyle {
        node: SemanticNodeId,
        error: super::super::projection::SemanticExecutionValueError,
    },
}

impl std::fmt::Display for PreparedMatchingShapeLeftoverFadeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout(error) => error.fmt(formatter),
            Self::InvalidSourceIndex(index) => write!(
                formatter,
                "matching-shape source leftover fade index {index} is out of bounds"
            ),
            Self::InvalidTargetIndex(index) => write!(
                formatter,
                "matching-shape target leftover fade index {index} is out of bounds"
            ),
            Self::TargetState { node } => write!(
                formatter,
                "matching-shape target leftover {}:{} has no valid semantic object state",
                node.slot(),
                node.generation()
            ),
            Self::UnsupportedTargetGeometry { node } => write!(
                formatter,
                "matching-shape target leftover {}:{} cannot lower to transient vector geometry",
                node.slot(),
                node.generation()
            ),
            Self::InvalidTargetStyle { node, error } => write!(
                formatter,
                "matching-shape target leftover {}:{} has invalid execution style: {error}",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingShapeLeftoverFadeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout(error) => Some(error),
            Self::InvalidTargetStyle { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl From<PreparedMatchingShapeLeftoverLayoutError> for PreparedMatchingShapeLeftoverFadeError {
    fn from(value: PreparedMatchingShapeLeftoverLayoutError) -> Self {
        Self::Layout(value)
    }
}

/// Stage Manim-compatible default matching-shape leftovers without choosing renderer
/// painter placement.
///
/// Source leftovers fade out while shifting by the shared source-rest -> target-rest
/// group-center delta. Detached target leftovers keep their exact authored target
/// transform and fade in there. Runtime publication may consume these values only after
/// it can place detached transients without inventing a stable source anchor.
pub fn prepare_matching_shape_leftover_fades(
    store: &SemanticStore,
    activation: &PreparedMatchingFamilyTransformActivation,
) -> Result<PreparedMatchingShapeLeftoverFadeProjection, PreparedMatchingShapeLeftoverFadeError> {
    let matching = activation.matching();
    let correspondence = matching.correspondence();
    let layout = prepare_matching_shape_leftover_layout(store, matching)?;

    let mut source_fades = Vec::with_capacity(correspondence.unmatched_source_indices.len());
    for &source_index in &correspondence.unmatched_source_indices {
        let member = matching.source_members().get(source_index).ok_or(
            PreparedMatchingShapeLeftoverFadeError::InvalidSourceIndex(source_index),
        )?;
        source_fades.push(PreparedMatchingShapeSourceLeftoverFade {
            source_index,
            node: member.node,
            execution_object_id: member.execution_object_id,
            tracks: source_fade_tracks(
                member.effective.transform,
                member.effective.appearance,
                layout.source_to_target,
                activation.timing,
                &activation.time_map,
            ),
        });
    }

    let mut target_fades = Vec::with_capacity(correspondence.unmatched_target_indices.len());
    for &target_index in &correspondence.unmatched_target_indices {
        let member = matching.target_members().get(target_index).ok_or(
            PreparedMatchingShapeLeftoverFadeError::InvalidTargetIndex(target_index),
        )?;
        let state = store
            .semantic_object_state_checked(member.node)
            .map_err(|_| PreparedMatchingShapeLeftoverFadeError::TargetState {
                node: member.node,
            })?;
        let geometry = member.content.geometry().ok_or(
            PreparedMatchingShapeLeftoverFadeError::UnsupportedTargetGeometry { node: member.node },
        )?;
        let geometry =
            super::super::compiled_scene::lower_semantic_geometry_value(geometry, Some(store))
                .map_err(
                    |_| PreparedMatchingShapeLeftoverFadeError::UnsupportedTargetGeometry {
                        node: member.node,
                    },
                )?;
        if !matches!(geometry, noon_core::GeometryRef::VectorPath(_)) {
            return Err(
                PreparedMatchingShapeLeftoverFadeError::UnsupportedTargetGeometry {
                    node: member.node,
                },
            );
        }
        let style =
            super::super::projection::lower_semantic_style_value(state).map_err(|error| {
                PreparedMatchingShapeLeftoverFadeError::InvalidTargetStyle {
                    node: member.node,
                    error,
                }
            })?;
        target_fades.push(PreparedMatchingShapeTargetLeftoverFade {
            target_index,
            base: PreparedMatchingShapeTargetTransientBase {
                node: member.node,
                z_index: state.z_index(),
                content: ObjectContentRef::Geometry(geometry),
                transform: member.transform,
                style,
                appearance: 1.0,
                reveal: 1.0,
                morph: 0.0,
            },
            tracks: target_fade_tracks(activation.timing, &activation.time_map),
        });
    }

    Ok(PreparedMatchingShapeLeftoverFadeProjection {
        source_fades,
        target_fades,
    })
}

fn source_fade_tracks(
    transform: Transform2D,
    appearance: f32,
    source_to_target: Vec2,
    timing: noon_core::TrackTiming,
    time_map: &noon_core::CompositionTimeMap,
) -> Vec<PreparedMatchingShapeLeftoverFadeTrack> {
    let mut tracks = Vec::with_capacity(2);
    if source_to_target != Vec2::ZERO {
        tracks.push(leftover_track(
            Property::Position,
            TrackValues::Vec2 {
                from: transform.translation,
                to: transform.translation + source_to_target,
            },
            timing,
            time_map,
        ));
    }
    tracks.push(leftover_track(
        Property::Appearance,
        TrackValues::Scalar {
            from: appearance,
            to: 0.0,
        },
        timing,
        time_map,
    ));
    tracks
}

fn target_fade_tracks(
    timing: noon_core::TrackTiming,
    time_map: &noon_core::CompositionTimeMap,
) -> Vec<PreparedMatchingShapeLeftoverFadeTrack> {
    vec![leftover_track(
        Property::Appearance,
        TrackValues::Scalar { from: 0.0, to: 1.0 },
        timing,
        time_map,
    )]
}

fn leftover_track(
    property: Property,
    values: TrackValues,
    timing: noon_core::TrackTiming,
    time_map: &noon_core::CompositionTimeMap,
) -> PreparedMatchingShapeLeftoverFadeTrack {
    PreparedMatchingShapeLeftoverFadeTrack {
        property,
        values,
        timing,
        time_map: time_map.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{CompositionTimeMap, RateFunction, TrackTiming};

    fn timing() -> (TrackTiming, CompositionTimeMap) {
        (
            TrackTiming::new(2.0, 3.0, RateFunction::Linear),
            CompositionTimeMap::identity(),
        )
    }

    #[test]
    fn source_leftover_moves_to_target_group_and_disappears() {
        let (timing, time_map) = timing();
        let transform = Transform2D {
            translation: Vec2::new(-3.0, 2.0),
            ..Transform2D::IDENTITY
        };
        let tracks = source_fade_tracks(transform, 0.75, Vec2::new(8.0, -1.0), timing, &time_map);
        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(-3.0, 2.0),
                to: Vec2::new(5.0, 1.0),
            }
        );
        assert_eq!(
            tracks[1].values,
            TrackValues::Scalar {
                from: 0.75,
                to: 0.0,
            }
        );
        assert_eq!(tracks[0].timing, timing);
        assert_eq!(tracks[0].time_map, time_map);
    }

    #[test]
    fn target_leftover_fades_in_at_authored_position_without_motion() {
        let (timing, time_map) = timing();
        let tracks = target_fade_tracks(timing, &time_map);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].property, Property::Appearance);
        assert_eq!(tracks[0].values, TrackValues::Scalar { from: 0.0, to: 1.0 });
        assert_eq!(tracks[0].timing, timing);
        assert_eq!(tracks[0].time_map, time_map);
    }
}
