use crate::{
    ObjectDefinition, ObjectId, SceneDefinition, Style, TimelineError, TrackDefinition, TrackId,
    Transform2D,
};

#[derive(Clone, Debug, PartialEq)]
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
        if !track.timing.start_time.is_finite() {
            return Err(PatchError::InvalidTrack(TimelineError::InvalidStartTime(
                track.timing.start_time,
            )));
        }
        if !track.timing.duration.is_finite() || track.timing.duration <= 0.0 {
            return Err(PatchError::InvalidTrack(TimelineError::InvalidDuration(
                track.timing.duration,
            )));
        }
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
    use crate::{Easing, GeometryRef, Property, TrackTiming, TrackValues, Vec2};

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
                TrackTiming::new(0.0, 1.0, Easing::Linear),
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
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
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
}
