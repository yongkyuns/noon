//! Explicit phase knots lower to the existing scalar retained-reveal channel.

use std::{collections::HashMap, sync::Arc};

use noon_core::{
    GeometryResource, ObjectId, PathCommand, PreparedSemanticMutationTransaction, Property,
    ResolvedAnimationOptions, RevealPhaseMap, SemanticAnimationIntent, SemanticObjectProperty,
    SemanticObjectState, SemanticStore, SemanticTransactionAnimationIntent,
    SemanticTransactionNodeRef, TrackValues, VectorPath,
};

use super::affine::{LoweredAffineChannel, SemanticAnimationCompletion};

#[derive(Clone, Debug, PartialEq)]
pub enum PhaseRevealLoweringError {
    InvalidDeclaration,
    UnsupportedTarget,
    UnsupportedOptions,
    InvalidPath(noon_geometry::GeometryError),
    CurveCountMismatch { expected: usize, actual: usize },
    ContentDriverConflict,
}

impl std::fmt::Display for PhaseRevealLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDeclaration => f.write_str("phase reveal schedule does not match its declaration"),
            Self::UnsupportedTarget => f.write_str("phase reveal requires one continuous static retained planar vector path without signal or host drivers"),
            Self::UnsupportedOptions => f.write_str("phase reveal requires bounded reversible timing and introduction, without lag or path-arc options"),
            Self::InvalidPath(error) => error.fmt(f),
            Self::CurveCountMismatch { expected, actual } => write!(f, "phase reveal map has {expected} curve intervals, but the retained path has {actual} drawable curves"),
            Self::ContentDriverConflict => f.write_str("phase reveal cannot share a target with a content-changing animation"),
        }
    }
}

impl std::error::Error for PhaseRevealLoweringError {}

pub(super) fn prepared_data(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    animation: SemanticTransactionNodeRef,
) -> Result<Arc<RevealPhaseMap>, PhaseRevealLoweringError> {
    match animation {
        SemanticTransactionNodeRef::Existing(node) => {
            let state = prepared
                .store()
                .semantic_animation_state(node)
                .map_err(|_| PhaseRevealLoweringError::InvalidDeclaration)?;
            match state.intent() {
                SemanticAnimationIntent::PhaseReveal { phase_map, .. } => Ok(phase_map.clone()),
                _ => Err(PhaseRevealLoweringError::InvalidDeclaration),
            }
        }
        SemanticTransactionNodeRef::Pending(token) => {
            let state = prepared
                .pending_animation(token)
                .map_err(|_| PhaseRevealLoweringError::InvalidDeclaration)?;
            match state.intent() {
                SemanticTransactionAnimationIntent::PhaseReveal { phase_map, .. } => {
                    Ok(phase_map.clone())
                }
                _ => Err(PhaseRevealLoweringError::InvalidDeclaration),
            }
        }
    }
}

pub(super) fn retained_path<'a>(
    source: &SemanticObjectState,
    store: &'a SemanticStore,
) -> Option<&'a VectorPath> {
    let handle = source.content.geometry()?.resource_handle()?;
    match store.geometry_resources().get(handle)? {
        GeometryResource::VectorPath(path) => Some(path),
        GeometryResource::Mesh(_) => None,
    }
}

pub(super) fn channel(
    store: &SemanticStore,
    source: &SemanticObjectState,
    path: &VectorPath,
    phase_map: Arc<RevealPhaseMap>,
    options: ResolvedAnimationOptions,
) -> Result<LoweredAffineChannel, PhaseRevealLoweringError> {
    if !options.introducer
        || options.lag_ratio != 0.0
        || options.path_arc != 0.0
        || !options.rate_func.preserves_unit_interval()
    {
        return Err(PhaseRevealLoweringError::UnsupportedOptions);
    }
    if !source.signal_bindings().is_empty()
        || super::super::projection::object_requires_spatial_lowering(source, store)
        || path.morph_target().is_some()
        || path
            .manim_points()
            .is_some_and(|points| points.iter().any(|point| point.z != 0.0))
        || path
            .commands()
            .iter()
            .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
            .count()
            != 1
    {
        return Err(PhaseRevealLoweringError::UnsupportedTarget);
    }
    let actual = noon_geometry::retained_reveal_curve_count(path)
        .map_err(PhaseRevealLoweringError::InvalidPath)?;
    if actual != phase_map.curve_count() {
        return Err(PhaseRevealLoweringError::CurveCountMismatch {
            expected: phase_map.curve_count(),
            actual,
        });
    }
    Ok(LoweredAffineChannel {
        property: Property::Reveal,
        conflict_property: SemanticObjectProperty::Presence,
        completion: SemanticAnimationCompletion::RevealLifecycle {
            remove: options.remover,
        },
        values: TrackValues::PhaseReveal { phase_map },
    })
}

/// Candidate-sized admission; ordinary Create remains compatible with morphs.
/// A map is tied to one retained topology, so content-changing tracks cannot be
/// admitted alongside it, even when their composition intervals do not overlap.
pub(super) fn content_conflict<'a, T: Copy + 'a>(
    tracks: impl Clone + Iterator<Item = (ObjectId, T, Property, &'a TrackValues)>,
) -> Option<T> {
    let mapped: HashMap<_, _> = tracks
        .clone()
        .filter(|(_, _, _, values)| matches!(values, TrackValues::PhaseReveal { .. }))
        .map(|(object, animation, _, _)| (object, animation))
        .collect();
    if mapped.is_empty() {
        return None;
    }
    tracks
        .filter(|(_, _, property, _)| matches!(property, Property::Morph | Property::Transform))
        .find_map(|(object, _, _, _)| mapped.get(&object).copied())
}

#[cfg(test)]
mod tests;
