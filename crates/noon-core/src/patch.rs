use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::timeline::validate_track_timing;
use crate::{
    ObjectDefinition, ObjectId, SceneDefinition, Style, TimelineError, TrackDefinition, TrackId,
    Transform2D,
};

/// Coarse invalidation class for a semantic scene mutation.
///
/// The ordering is intentional: a transaction's impact is the maximum impact
/// of any mutation it contains. Runtime layers may refine these classes, but a
/// lower-impact mutation must never require more work merely because it arrived
/// through an interactive callback rather than ordinary authoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationImpact {
    Property,
    Timeline,
    Structure,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenePatch {
    CreateObject(ObjectDefinition),
    RemoveObject(ObjectId),
    SetTransform {
        object: ObjectId,
        transform: Transform2D,
    },
    SetStyle {
        object: ObjectId,
        style: Style,
    },
    AddTrack(TrackDefinition),
    ReplaceTrack(TrackDefinition),
    RemoveTrack(TrackId),
}

impl ScenePatch {
    pub const fn impact(&self) -> MutationImpact {
        match self {
            Self::SetTransform { .. } | Self::SetStyle { .. } => MutationImpact::Property,
            Self::AddTrack(_) | Self::ReplaceTrack(_) | Self::RemoveTrack(_) => {
                MutationImpact::Timeline
            }
            Self::CreateObject(_) | Self::RemoveObject(_) => MutationImpact::Structure,
        }
    }
}

/// Atomic group of semantic mutations.
///
/// Host callbacks, live editor actions, and frontend-driven updates should
/// converge on this transaction boundary. A transaction is validated against a
/// staged scene and becomes visible only if every contained mutation succeeds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MutationTransaction {
    mutations: Vec<ScenePatch>,
}

impl MutationTransaction {
    pub const fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    pub fn from_mutations(mutations: impl IntoIterator<Item = ScenePatch>) -> Self {
        Self {
            mutations: mutations.into_iter().collect(),
        }
    }

    pub fn push(&mut self, mutation: ScenePatch) {
        self.mutations.push(mutation);
    }

    pub fn mutations(&self) -> &[ScenePatch] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn impact(&self) -> Option<MutationImpact> {
        self.mutations.iter().map(ScenePatch::impact).max()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatchError {
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    DuplicateTrack(TrackId),
    UnknownTrack(TrackId),
    InvalidTrack(TimelineError),
    ObjectIdExhausted,
    TrackIdExhausted,
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateObject(id) => write!(formatter, "duplicate object id {}", id.get()),
            Self::UnknownObject(id) => write!(formatter, "unknown object id {}", id.get()),
            Self::DuplicateTrack(id) => write!(formatter, "duplicate track id {}", id.get()),
            Self::UnknownTrack(id) => write!(formatter, "unknown track id {}", id.get()),
            Self::InvalidTrack(error) => write!(formatter, "invalid track: {error}"),
            Self::ObjectIdExhausted => formatter.write_str("Noon object ID space exhausted"),
            Self::TrackIdExhausted => formatter.write_str("Noon track ID space exhausted"),
        }
    }
}

impl std::error::Error for PatchError {}

impl SceneDefinition {
    /// Builds a scene from transported definitions in linear expected time while
    /// preserving document order and validating all stable identities.
    pub fn from_parts(
        objects: Vec<ObjectDefinition>,
        tracks: Vec<TrackDefinition>,
    ) -> Result<Self, PatchError> {
        let mut object_ids = HashSet::with_capacity(objects.len());
        let mut next_object_id = 0;
        for object in &objects {
            if !object_ids.insert(object.id) {
                return Err(PatchError::DuplicateObject(object.id));
            }
            let next = object
                .id
                .get()
                .checked_add(1)
                .ok_or(PatchError::ObjectIdExhausted)?;
            next_object_id = next_object_id.max(next);
        }

        let mut track_ids = HashSet::with_capacity(tracks.len());
        let mut next_track_id = 0;
        for track in &tracks {
            if !track_ids.insert(track.id) {
                return Err(PatchError::DuplicateTrack(track.id));
            }
            if !object_ids.contains(&track.object) {
                return Err(PatchError::UnknownObject(track.object));
            }
            Self::validate_track_fields(track)?;
            let next = track
                .id
                .get()
                .checked_add(1)
                .ok_or(PatchError::TrackIdExhausted)?;
            next_track_id = next_track_id.max(next);
        }

        Ok(Self {
            objects,
            next_object_id,
            tracks,
            next_track_id,
        })
    }

    pub fn apply_patch(&mut self, patch: ScenePatch) -> Result<(), PatchError> {
        match patch {
            ScenePatch::CreateObject(object) => self.insert_object(object),
            ScenePatch::RemoveObject(id) => self.remove_object(id),
            ScenePatch::SetTransform { object, transform } => {
                self.object_mut(object)
                    .ok_or(PatchError::UnknownObject(object))?
                    .transform = transform;
                Ok(())
            }
            ScenePatch::SetStyle { object, style } => {
                self.object_mut(object)
                    .ok_or(PatchError::UnknownObject(object))?
                    .style = style;
                Ok(())
            }
            ScenePatch::AddTrack(track) => self.insert_track(track),
            ScenePatch::ReplaceTrack(track) => self.replace_track(track),
            ScenePatch::RemoveTrack(id) => self.remove_track(id),
        }
    }

    /// Applies all mutations atomically.
    ///
    /// This is deliberately independent of transport sequencing: transactions
    /// are a semantic/runtime concept, whereas sequencing is a protocol concern.
    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<(), PatchError> {
        if transaction.is_empty() {
            return Ok(());
        }

        let mut staged = self.clone();
        for mutation in transaction.mutations() {
            staged.apply_patch(mutation.clone())?;
        }
        *self = staged;
        Ok(())
    }

    fn insert_object(&mut self, object: ObjectDefinition) -> Result<(), PatchError> {
        if self.object(object.id).is_some() {
            return Err(PatchError::DuplicateObject(object.id));
        }
        let next = object
            .id
            .get()
            .checked_add(1)
            .ok_or(PatchError::ObjectIdExhausted)?;
        self.next_object_id = self.next_object_id.max(next);
        self.objects.push(object);
        Ok(())
    }

    fn remove_object(&mut self, id: ObjectId) -> Result<(), PatchError> {
        let original_len = self.objects.len();
        self.objects.retain(|object| object.id != id);
        if self.objects.len() == original_len {
            return Err(PatchError::UnknownObject(id));
        }
        self.tracks.retain(|track| track.object != id);
        Ok(())
    }

    fn insert_track(&mut self, track: TrackDefinition) -> Result<(), PatchError> {
        if self.tracks.iter().any(|existing| existing.id == track.id) {
            return Err(PatchError::DuplicateTrack(track.id));
        }
        self.validate_patch_track(&track)?;
        let next = track
            .id
            .get()
            .checked_add(1)
            .ok_or(PatchError::TrackIdExhausted)?;
        self.next_track_id = self.next_track_id.max(next);
        self.tracks.push(track);
        Ok(())
    }

    fn replace_track(&mut self, track: TrackDefinition) -> Result<(), PatchError> {
        self.validate_patch_track(&track)?;
        let existing = self
            .tracks
            .iter_mut()
            .find(|existing| existing.id == track.id)
            .ok_or(PatchError::UnknownTrack(track.id))?;
        *existing = track;
        Ok(())
    }

    fn remove_track(&mut self, id: TrackId) -> Result<(), PatchError> {
        let original_len = self.tracks.len();
        self.tracks.retain(|track| track.id != id);
        if self.tracks.len() == original_len {
            return Err(PatchError::UnknownTrack(id));
        }
        Ok(())
    }

    fn validate_patch_track(&self, track: &TrackDefinition) -> Result<(), PatchError> {
        if self.object(track.object).is_none() {
            return Err(PatchError::UnknownObject(track.object));
        }
        Self::validate_track_fields(track)
    }

    fn validate_track_fields(track: &TrackDefinition) -> Result<(), PatchError> {
        validate_track_timing(track.property, track.timing).map_err(PatchError::InvalidTrack)?;
        let expected = track.property.value_kind();
        let actual = track.values.value_kind();
        if expected != actual {
            return Err(PatchError::InvalidTrack(TimelineError::ValueTypeMismatch {
                property: track.property,
                expected,
                actual,
            }));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{GeometryRef, Property, RateFunction, TrackTiming, TrackValues, Vec2};

    use super::*;

    #[test]
    fn object_patches_preserve_unrelated_identity() {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 2.0));
        let second_before = scene.object(second).expect("object exists").clone();

        scene
            .apply_patch(ScenePatch::SetTransform {
                object: first,
                transform: Transform2D {
                    translation: Vec2::new(4.0, 3.0),
                    ..Transform2D::IDENTITY
                },
            })
            .expect("valid patch");

        assert_eq!(scene.object(second), Some(&second_before));
    }

    #[test]
    fn mutation_impact_matches_required_execution_work() {
        let object = ObjectId::new(1);
        assert_eq!(
            ScenePatch::SetTransform {
                object,
                transform: Transform2D::IDENTITY,
            }
            .impact(),
            MutationImpact::Property
        );
        assert_eq!(
            ScenePatch::RemoveTrack(TrackId::new(2)).impact(),
            MutationImpact::Timeline
        );
        assert_eq!(
            ScenePatch::RemoveObject(object).impact(),
            MutationImpact::Structure
        );

        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetStyle {
                object,
                style: Style::default(),
            },
            ScenePatch::RemoveTrack(TrackId::new(2)),
        ]);
        assert_eq!(transaction.impact(), Some(MutationImpact::Timeline));
        assert_eq!(MutationTransaction::new().impact(), None);
    }

    #[test]
    fn transaction_is_atomic_when_a_later_mutation_fails() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let before = scene.clone();

        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(5.0, 2.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::RemoveObject(ObjectId::new(999)),
        ]);

        assert!(matches!(
            scene.apply_transaction(&transaction),
            Err(PatchError::UnknownObject(ObjectId(999)))
        ));
        assert_eq!(scene, before);
    }

    #[test]
    fn transaction_commits_all_mutations_together() {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 2.0));

        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object: first,
                transform: Transform2D {
                    translation: Vec2::new(2.0, 1.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::RemoveObject(second),
        ]);
        scene
            .apply_transaction(&transaction)
            .expect("transaction must commit");

        assert_eq!(
            scene.object(first).expect("first object remains").transform.translation,
            Vec2::new(2.0, 1.0)
        );
        assert!(scene.object(second).is_none());
    }

    #[test]
    fn explicit_object_creation_advances_future_ids() {
        let mut scene = SceneDefinition::new();
        scene
            .apply_patch(ScenePatch::CreateObject(ObjectDefinition::new(
                ObjectId::new(10),
                GeometryRef::circle(1.0),
            )))
            .expect("valid patch");

        assert_eq!(scene.add(GeometryRef::circle(2.0)), ObjectId::new(11));
    }

    #[test]
    fn removing_an_object_removes_its_dependent_tracks() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .expect("valid track");

        scene
            .apply_patch(ScenePatch::RemoveObject(object))
            .expect("valid patch");

        assert!(scene.object(object).is_none());
        assert!(scene.tracks().is_empty());
    }

    #[test]
    fn tracks_can_be_added_replaced_and_removed_by_stable_id() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let id = TrackId::new(8);
        let track = TrackDefinition {
            id,
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        };

        scene
            .apply_patch(ScenePatch::AddTrack(track.clone()))
            .expect("valid patch");
        assert_eq!(scene.tracks(), std::slice::from_ref(&track));

        let replacement = TrackDefinition {
            values: TrackValues::Scalar {
                from: 0.5,
                to: 0.25,
            },
            ..track
        };
        scene
            .apply_patch(ScenePatch::ReplaceTrack(replacement.clone()))
            .expect("valid patch");
        assert_eq!(scene.tracks(), &[replacement]);

        scene
            .apply_patch(ScenePatch::RemoveTrack(id))
            .expect("valid patch");
        assert!(scene.tracks().is_empty());
    }

    #[test]
    fn bulk_construction_preserves_order_and_advances_ids() {
        let objects = vec![
            ObjectDefinition::new(ObjectId::new(7), GeometryRef::circle(1.0)),
            ObjectDefinition::new(ObjectId::new(2), GeometryRef::rectangle(2.0, 3.0)),
        ];
        let tracks = vec![TrackDefinition {
            id: TrackId::new(4),
            object: ObjectId::new(2),
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        }];

        let mut scene = SceneDefinition::from_parts(objects.clone(), tracks.clone())
            .expect("bulk scene must be valid");

        assert_eq!(scene.objects(), objects);
        assert_eq!(scene.tracks(), tracks);
        assert_eq!(scene.add(GeometryRef::circle(0.5)), ObjectId::new(8));
        assert_eq!(
            scene
                .animate_scalar(
                    ObjectId::new(7),
                    Property::Opacity,
                    1.0,
                    0.0,
                    TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                )
                .expect("track must be valid"),
            TrackId::new(5)
        );
    }

    #[test]
    fn bulk_construction_accepts_zero_duration_presence_events() {
        let object = ObjectDefinition::new(ObjectId::new(1), GeometryRef::circle(1.0));
        let track = TrackDefinition {
            id: TrackId::new(2),
            object: object.id,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(3.0),
        };

        let scene = SceneDefinition::from_parts(vec![object], vec![track.clone()])
            .expect("presence event must survive IR reconstruction");
        assert_eq!(scene.tracks(), &[track]);
    }

    #[test]
    fn bulk_construction_rejects_duplicate_and_dangling_ids() {
        let duplicate = ObjectDefinition::new(ObjectId::new(3), GeometryRef::circle(1.0));
        assert!(matches!(
            SceneDefinition::from_parts(vec![duplicate.clone(), duplicate], Vec::new()),
            Err(PatchError::DuplicateObject(ObjectId(3)))
        ));

        let dangling_track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(99),
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        };
        assert!(matches!(
            SceneDefinition::from_parts(Vec::new(), vec![dangling_track]),
            Err(PatchError::UnknownObject(ObjectId(99)))
        ));
    }
}
