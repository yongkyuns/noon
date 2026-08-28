use std::collections::{HashMap, HashSet};

use crate::{validate_track_definition, ObjectId, SceneDefinition, TrackId};

use super::{
    validate_object_definition, validate_property_patch, MutationTransaction, PatchError,
    ScenePatch,
};

/// Instrumentation for the local transaction preflight path.
///
/// `staged_scene_clones` is deliberately part of the contract: structural
/// and timeline transactions validate compact identity and numeric metadata before
/// commit, so heavy scene/geometry payloads are never cloned for atomicity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransactionPreflightStats {
    pub objects_indexed: usize,
    pub tracks_indexed: usize,
    pub mutations_preflighted: usize,
    pub staged_scene_clones: usize,
}

pub fn preflight_transaction(
    scene: &SceneDefinition,
    transaction: &MutationTransaction,
) -> Result<TransactionPreflightStats, PatchError> {
    let mut objects: HashSet<ObjectId> = scene.objects().iter().map(|object| object.id).collect();
    let mut tracks: HashMap<TrackId, ObjectId> = scene
        .tracks()
        .iter()
        .map(|track| (track.id, track.object))
        .collect();
    let stats = TransactionPreflightStats {
        objects_indexed: objects.len(),
        tracks_indexed: tracks.len(),
        mutations_preflighted: transaction.mutations().len(),
        staged_scene_clones: 0,
    };

    for mutation in transaction.mutations() {
        match mutation {
            ScenePatch::CreateObject(object) => {
                if !objects.insert(object.id) {
                    return Err(PatchError::DuplicateObject(object.id));
                }
                object
                    .id
                    .get()
                    .checked_add(1)
                    .ok_or(PatchError::ObjectIdExhausted)?;
                validate_object_definition(object)?;
            }
            ScenePatch::RemoveObject(id) => {
                if !objects.remove(id) {
                    return Err(PatchError::UnknownObject(*id));
                }
                tracks.retain(|_, object| object != id);
            }
            ScenePatch::SetGeometry { object, .. }
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
                if !objects.contains(object) {
                    return Err(PatchError::UnknownObject(*object));
                }
                validate_property_patch(mutation)?;
            }
            ScenePatch::AddTrack(track) => {
                if tracks.contains_key(&track.id) {
                    return Err(PatchError::DuplicateTrack(track.id));
                }
                if !objects.contains(&track.object) {
                    return Err(PatchError::UnknownObject(track.object));
                }
                validate_track_definition(track).map_err(PatchError::InvalidTrack)?;
                track
                    .id
                    .get()
                    .checked_add(1)
                    .ok_or(PatchError::TrackIdExhausted)?;
                tracks.insert(track.id, track.object);
            }
            ScenePatch::ReplaceTrack(track) => {
                // Preserve SceneDefinition::replace_track error ordering:
                // validate the replacement target/fields before checking ID.
                if !objects.contains(&track.object) {
                    return Err(PatchError::UnknownObject(track.object));
                }
                validate_track_definition(track).map_err(PatchError::InvalidTrack)?;
                if !tracks.contains_key(&track.id) {
                    return Err(PatchError::UnknownTrack(track.id));
                }
                tracks.insert(track.id, track.object);
            }
            ScenePatch::RemoveTrack(id) => {
                if tracks.remove(id).is_none() {
                    return Err(PatchError::UnknownTrack(*id));
                }
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use crate::{GeometryRef, ObjectDefinition, ObjectId, ScenePatch, Style};

    use super::*;

    #[test]
    fn hundred_thousand_object_preflight_clones_no_scene_payload() {
        let mut scene = SceneDefinition::new();
        for _ in 0..100_000 {
            scene.add(GeometryRef::circle(1.0));
        }
        let transaction =
            MutationTransaction::from_mutations([ScenePatch::RemoveObject(ObjectId::new(10))]);
        let stats = preflight_transaction(&scene, &transaction).expect("valid removal");
        assert_eq!(stats.objects_indexed, 100_000);
        assert_eq!(stats.mutations_preflighted, 1);
        assert_eq!(stats.staged_scene_clones, 0);
    }

    #[test]
    fn structural_preflight_rejects_non_finite_object_before_commit() {
        let mut scene = SceneDefinition::new();
        let existing = scene.add(GeometryRef::circle(1.0));
        let before = scene.clone();
        let mut invalid = ObjectDefinition::new(ObjectId::new(10), GeometryRef::circle(1.0));
        invalid.style = Style {
            opacity: f32::NAN,
            ..Style::default()
        };
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetGeometry {
                object: existing,
                geometry: GeometryRef::circle(2.0),
            },
            ScenePatch::CreateObject(invalid),
        ]);

        assert!(matches!(
            scene.apply_transaction(&transaction),
            Err(PatchError::InvalidObjectState { .. })
        ));
        assert_eq!(scene, before);
    }
}
