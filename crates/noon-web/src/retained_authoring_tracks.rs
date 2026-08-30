use std::collections::BTreeMap;

use noon_core::{
    CompositionTimeMap, ObjectId, Property, TrackDefinition, TrackId, TrackTiming, TrackValues,
    Vec2,
};
use serde::{Deserialize, Serialize};

/// Source-level retained animation track.
///
/// Frontends own semantic object identity and animation values, but not runtime track
/// identity. Rust assigns [`TrackId`] values only after the retained and legacy object
/// domains have been merged, keeping the authoring wire independent of compiler-local
/// sequencing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTrackAuthoringSpec {
    pub object: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    #[serde(default, skip_serializing_if = "CompositionTimeMap::is_identity")]
    pub time_map: CompositionTimeMap,
}

impl RetainedTrackAuthoringSpec {
    pub fn new(
        object: ObjectId,
        property: Property,
        values: TrackValues,
        timing: TrackTiming,
    ) -> Self {
        Self {
            object,
            property,
            values,
            timing,
            time_map: CompositionTimeMap::identity(),
        }
    }

    fn into_definition(
        mut self,
        id: TrackId,
        retained_scale_factors: &BTreeMap<ObjectId, Vec2>,
    ) -> TrackDefinition {
        if self.property == Property::Scale {
            if let (TrackValues::Vec2 { from, to }, Some(factor)) =
                (&mut self.values, retained_scale_factors.get(&self.object))
            {
                *from = from.component_mul(*factor);
                *to = to.component_mul(*factor);
            }
        }
        TrackDefinition {
            id,
            object: self.object,
            property: self.property,
            values: self.values,
            timing: self.timing,
            time_map: self.time_map,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedTrackMaterializationError {
    TrackIdSpaceExhausted,
}

impl std::fmt::Display for RetainedTrackMaterializationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrackIdSpaceExhausted => {
                formatter.write_str("retained authoring track ID space is exhausted")
            }
        }
    }
}

impl std::error::Error for RetainedTrackMaterializationError {}

/// Append source-level retained tracks after the legacy track-ID range.
///
/// IDs are assigned deterministically from the maximum existing ID rather than from
/// the legacy track count, so sparse imported scenes remain collision-free. Scale
/// values stay in frontend authoring space and are normalized here with Rust-owned
/// backend factors before runtime compilation. All semantic validity is intentionally
/// left to `RetainedCompiledScene::compile`, the canonical validator for the unified
/// retained object domain.
pub fn materialize_retained_tracks(
    legacy_tracks: &[TrackDefinition],
    retained_tracks: Vec<RetainedTrackAuthoringSpec>,
    retained_scale_factors: &BTreeMap<ObjectId, Vec2>,
) -> Result<Vec<TrackDefinition>, RetainedTrackMaterializationError> {
    if retained_tracks.is_empty() {
        return Ok(legacy_tracks.to_vec());
    }

    let first_retained_id = legacy_tracks
        .iter()
        .map(|track| track.id.get())
        .max()
        .map_or(Some(0), |id| id.checked_add(1))
        .ok_or(RetainedTrackMaterializationError::TrackIdSpaceExhausted)?;
    let retained_count = u64::try_from(retained_tracks.len())
        .map_err(|_| RetainedTrackMaterializationError::TrackIdSpaceExhausted)?;
    first_retained_id
        .checked_add(retained_count)
        .ok_or(RetainedTrackMaterializationError::TrackIdSpaceExhausted)?;

    let mut tracks = Vec::with_capacity(legacy_tracks.len() + retained_tracks.len());
    tracks.extend_from_slice(legacy_tracks);
    tracks.extend(
        retained_tracks
            .into_iter()
            .enumerate()
            .map(|(index, track)| {
                let offset =
                    u64::try_from(index).expect("retained track count was validated as u64");
                track.into_definition(
                    TrackId::new(first_retained_id + offset),
                    retained_scale_factors,
                )
            }),
    );
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use noon_core::{RateFunction, TrackTiming};

    use super::*;

    fn position(object: ObjectId) -> RetainedTrackAuthoringSpec {
        RetainedTrackAuthoringSpec::new(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        )
    }

    #[test]
    fn retained_track_ids_follow_sparse_legacy_ids_deterministically() {
        let object = ObjectId::new(4);
        let legacy = TrackDefinition {
            id: TrackId::new(7),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.5 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let tracks = materialize_retained_tracks(
            std::slice::from_ref(&legacy),
            vec![position(object)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(tracks[0], legacy);
        assert_eq!(tracks[1].id, TrackId::new(8));
        assert_eq!(tracks[1].object, object);
    }

    #[test]
    fn empty_legacy_track_set_starts_retained_ids_at_zero() {
        let object = ObjectId::new(9);
        let tracks = materialize_retained_tracks(
            &[],
            vec![position(object), position(object)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(tracks[0].id, TrackId::new(0));
        assert_eq!(tracks[1].id, TrackId::new(1));
    }

    #[test]
    fn retained_scale_values_are_normalized_by_rust_owned_backend_factor() {
        let object = ObjectId::new(9);
        let track = RetainedTrackAuthoringSpec::new(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );
        let factors = BTreeMap::from([(object, Vec2::new(0.01, 0.02))]);
        let tracks = materialize_retained_tracks(&[], vec![track], &factors).unwrap();
        assert_eq!(
            tracks[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(0.01, 0.02),
                to: Vec2::ZERO,
            }
        );
    }

    #[test]
    fn empty_retained_track_set_preserves_legacy_tracks_even_at_max_id() {
        let object = ObjectId::new(1);
        let legacy = TrackDefinition {
            id: TrackId::new(u64::MAX),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(
            materialize_retained_tracks(
                std::slice::from_ref(&legacy),
                Vec::new(),
                &BTreeMap::new(),
            )
            .unwrap(),
            vec![legacy]
        );
    }

    #[test]
    fn exhausted_legacy_track_id_rejects_without_partial_output() {
        let object = ObjectId::new(1);
        let legacy = TrackDefinition {
            id: TrackId::new(u64::MAX),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(
            materialize_retained_tracks(&[legacy], vec![position(object)], &BTreeMap::new()),
            Err(RetainedTrackMaterializationError::TrackIdSpaceExhausted)
        );
    }
}
