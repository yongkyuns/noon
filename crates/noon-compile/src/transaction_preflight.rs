use std::collections::{BTreeMap, BTreeSet};

use noon_core::{
    validate_object_definition, validate_property_patch, validate_track_definition,
    MutationTransaction, ObjectId, Property, ScenePatch, TrackDefinition, TrackId, TrackValues,
};

use crate::{
    compile_patch_error, compile_transform_geometry_plan, map_object_state_error,
    CompilePatchError, CompiledChannelKey, CompiledScene, CompiledTrack,
    CompiledTransactionPreflightStats,
};

#[derive(Clone, Copy, Debug)]
struct TrackShadow {
    id: TrackId,
    object_index: u32,
    property: Property,
    start_time: f64,
    presence: Option<(bool, bool)>,
}

impl TrackShadow {
    fn from_compiled(track: &CompiledTrack) -> Self {
        Self {
            id: track.id,
            object_index: track.object_index,
            property: track.property,
            start_time: track.timing.start_time,
            presence: presence_endpoints(track.property, &track.values),
        }
    }

    fn from_definition(track: &TrackDefinition, object_index: u32) -> Self {
        Self {
            id: track.id,
            object_index,
            property: track.property,
            start_time: track.timing.start_time,
            presence: presence_endpoints(track.property, &track.values),
        }
    }
}

#[derive(Clone, Copy)]
enum ObjectOverlay {
    Present(u32),
    Removed,
}

/// A transaction-local sparse overlay. It reads untouched identity and channel
/// metadata from the compiled scene and stores only identities changed by this
/// transaction; payload vectors are never cloned for validation.
struct PreflightOverlay {
    objects: BTreeMap<ObjectId, ObjectOverlay>,
    tracks: BTreeMap<TrackId, Option<TrackShadow>>,
    channels: BTreeMap<CompiledChannelKey, BTreeSet<TrackId>>,
    seen_objects: BTreeSet<ObjectId>,
    seen_tracks: BTreeSet<TrackId>,
    next_object_index: usize,
    track_metadata_visits: usize,
}

impl PreflightOverlay {
    fn new(scene: &CompiledScene) -> Self {
        Self {
            objects: BTreeMap::new(),
            tracks: BTreeMap::new(),
            channels: BTreeMap::new(),
            seen_objects: BTreeSet::new(),
            seen_tracks: BTreeSet::new(),
            next_object_index: scene.objects.len(),
            track_metadata_visits: 0,
        }
    }

    fn object_index(&mut self, scene: &CompiledScene, id: ObjectId) -> Option<u32> {
        match self.objects.get(&id).copied() {
            Some(ObjectOverlay::Present(index)) => Some(index),
            Some(ObjectOverlay::Removed) => None,
            None => {
                let index = scene.object_indices.get(&id).copied();
                if index.is_some() {
                    self.seen_objects.insert(id);
                }
                index
            }
        }
    }

    fn track(&mut self, scene: &CompiledScene, id: TrackId) -> Option<TrackShadow> {
        self.track_metadata_visits += 1;
        match self.tracks.get(&id).copied() {
            Some(track) => track,
            None => {
                let track = scene.track(id).map(TrackShadow::from_compiled);
                if track.is_some() {
                    self.seen_tracks.insert(id);
                }
                track
            }
        }
    }

    fn set_track(&mut self, id: TrackId, replacement: Option<TrackShadow>) {
        if let Some(Some(previous)) = self.tracks.get(&id) {
            let channel = CompiledChannelKey::new(previous.object_index, previous.property);
            if let Some(ids) = self.channels.get_mut(&channel) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.channels.remove(&channel);
                }
            }
        }
        if let Some(track) = replacement {
            self.channels
                .entry(CompiledChannelKey::new(track.object_index, track.property))
                .or_default()
                .insert(id);
        }
        self.tracks.insert(id, replacement);
    }

    fn remove_object_tracks(&mut self, scene: &CompiledScene, object_index: u32) {
        for channel in scene.channels_for_object_index(object_index) {
            for track in scene.channel_tracks(channel) {
                self.track_metadata_visits += 1;
                self.seen_tracks.insert(track.id);
                // An earlier replacement may have moved this base track to a
                // different live object. Remove its current projection, never
                // blindly its historical owner.
                match self.tracks.get(&track.id) {
                    Some(Some(current)) if current.object_index != object_index => {}
                    _ => {
                        self.set_track(track.id, None);
                    }
                }
            }
        }
        let first = CompiledChannelKey::new(object_index, Property::Presence);
        let last = CompiledChannelKey::new(object_index, Property::Morph);
        let staged = self
            .channels
            .range(first..=last)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect::<Vec<_>>();
        for id in staged {
            self.track_metadata_visits += 1;
            self.set_track(id, None);
        }
    }

    fn presence_channel(&mut self, scene: &CompiledScene, object_index: u32) -> Vec<TrackShadow> {
        let channel = CompiledChannelKey::new(object_index, Property::Presence);
        let mut tracks = Vec::new();
        let mut base_ids = BTreeSet::new();
        for track in scene.channel_tracks(channel) {
            self.track_metadata_visits += 1;
            self.seen_tracks.insert(track.id);
            base_ids.insert(track.id);
            match self.tracks.get(&track.id).copied() {
                Some(Some(shadow))
                    if shadow.object_index == object_index
                        && shadow.property == Property::Presence =>
                {
                    tracks.push(shadow)
                }
                Some(Some(_)) => {}
                Some(None) => {}
                None => tracks.push(TrackShadow::from_compiled(track)),
            }
        }
        for id in self.channels.get(&channel).into_iter().flatten() {
            self.track_metadata_visits += 1;
            if base_ids.contains(id) {
                continue;
            }
            tracks.push(self.tracks[id].expect("indexed staged channel contains a live track"));
        }
        tracks
    }
}

pub(super) fn preflight_transaction(
    scene: &CompiledScene,
    transaction: &MutationTransaction,
) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
    let mut overlay = PreflightOverlay::new(scene);

    for patch in transaction.mutations() {
        match patch {
            ScenePatch::CreateObject(object) => {
                if overlay.object_index(scene, object.id).is_some() {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let index = u32::try_from(overlay.next_object_index)
                    .map_err(|_| CompilePatchError::TooManyObjects(overlay.next_object_index))?;
                validate_object_definition(object).map_err(map_object_state_error)?;
                overlay.next_object_index += 1;
                overlay
                    .objects
                    .insert(object.id, ObjectOverlay::Present(index));
            }
            ScenePatch::RemoveObject(id) => {
                let index = overlay
                    .object_index(scene, *id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                overlay.objects.insert(*id, ObjectOverlay::Removed);
                overlay.remove_object_tracks(scene, index);
            }
            ScenePatch::SetGeometry { object, .. }
            | ScenePatch::SetTransform { object, .. }
            | ScenePatch::SetStyle { object, .. } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                validate_property_patch(patch).map_err(map_object_state_error)?;
            }
            ScenePatch::AddTrack(track) => {
                if overlay.track(scene, track.id).is_some() {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                let shadow = TrackShadow::from_definition(track, object_index);
                overlay.set_track(track.id, Some(shadow));
                if shadow.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, object_index)?;
                }
            }
            ScenePatch::ReplaceTrack(track) => {
                let old = overlay
                    .track(scene, track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                let replacement = TrackShadow::from_definition(track, object_index);
                overlay.set_track(track.id, Some(replacement));
                if old.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, old.object_index)?;
                }
                if replacement.property == Property::Presence
                    && (old.property != Property::Presence
                        || old.object_index != replacement.object_index)
                {
                    validate_presence_channel(scene, &mut overlay, replacement.object_index)?;
                }
            }
            ScenePatch::RemoveTrack(id) => {
                let removed = overlay
                    .track(scene, *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                overlay.set_track(*id, None);
                if removed.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, removed.object_index)?;
                }
            }
        }
    }

    Ok(CompiledTransactionPreflightStats {
        objects_indexed: overlay.seen_objects.len(),
        tracks_indexed: overlay.seen_tracks.len(),
        track_metadata_visits: overlay.track_metadata_visits,
        mutations_preflighted: transaction.mutations().len(),
        staged_compiled_scene_clones: 0,
    })
}

fn validate_track(track: &TrackDefinition) -> Result<(), CompilePatchError> {
    validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
    compile_transform_geometry_plan(track).map_err(|error| compile_patch_error(track.id, error))?;
    Ok(())
}

fn presence_endpoints(property: Property, values: &TrackValues) -> Option<(bool, bool)> {
    if property != Property::Presence {
        return None;
    }
    let TrackValues::Bool { from, to } = values else {
        unreachable!("validated Presence track must contain bool values");
    };
    Some((*from, *to))
}

fn validate_presence_channel(
    scene: &CompiledScene,
    overlay: &mut PreflightOverlay,
    object_index: u32,
) -> Result<(), CompilePatchError> {
    let mut tracks = overlay.presence_channel(scene, object_index);
    tracks.sort_by(|left, right| {
        left.start_time
            .total_cmp(&right.start_time)
            .then_with(|| left.id.cmp(&right.id))
    });
    for pair in tracks.windows(2) {
        if pair[0].presence.expect("presence channel").1
            != pair[1].presence.expect("presence channel").0
        {
            return Err(CompilePatchError::DiscontinuousPresence {
                previous: pair[0].id,
                next: pair[1].id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Easing, GeometryRef, MutationTransaction, ObjectDefinition, SceneDefinition, ScenePatch,
        Style, TrackDefinition, TrackTiming, TrackValues,
    };

    use crate::{CompilePatchError, CompiledScene};

    #[test]
    fn property_batch_ignores_one_hundred_thousand_unrelated_objects_and_tracks() {
        let mut scene = SceneDefinition::new();
        let mut objects = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            objects.push(scene.add(GeometryRef::circle(1.0)));
        }
        for object in objects.iter().take(1_000) {
            scene
                .animate_position(
                    *object,
                    noon_core::Vec2::ZERO,
                    noon_core::Vec2::ONE,
                    TrackTiming::new(0.0, 1.0, Easing::Linear),
                )
                .unwrap();
        }
        let target = objects[99_999];
        let track = scene
            .animate_position(
                target,
                noon_core::Vec2::ZERO,
                noon_core::Vec2::ONE,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();

        let stats = compiled
            .preflight_transaction(&MutationTransaction::from_mutations([
                ScenePatch::SetStyle {
                    object: target,
                    style: Style::default(),
                },
            ]))
            .unwrap();

        assert_eq!(stats.objects_indexed, 1);
        assert_eq!(stats.tracks_indexed, 0);
        assert_eq!(stats.track_metadata_visits, 0);
        assert_eq!(stats.mutations_preflighted, 1);
        assert_eq!(stats.staged_compiled_scene_clones, 0);
        assert_eq!(compiled.track(track).unwrap().object_index, 99_999);
    }

    #[test]
    fn late_presence_replacement_rejects_without_touching_unrelated_tracks() {
        let mut scene = SceneDefinition::new();
        let target = scene.add(GeometryRef::circle(1.0));
        let unrelated = scene.add(GeometryRef::circle(1.0));
        let first = scene.set_presence_at(target, false, true, 1.0).unwrap();
        let second = scene.set_presence_at(target, true, false, 2.0).unwrap();
        let unrelated_track = scene
            .animate_position(
                unrelated,
                noon_core::Vec2::ZERO,
                noon_core::Vec2::ONE,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();
        let before = compiled.track(second).unwrap().clone();

        let error = compiled
            .preflight_transaction(&MutationTransaction::from_mutations([
                ScenePatch::SetStyle {
                    object: unrelated,
                    style: Style::default(),
                },
                ScenePatch::ReplaceTrack(TrackDefinition {
                    id: second,
                    object: target,
                    property: noon_core::Property::Presence,
                    values: TrackValues::Bool {
                        from: false,
                        to: false,
                    },
                    timing: TrackTiming::instant(2.0),
                    time_map: noon_core::CompositionTimeMap::identity(),
                }),
            ]))
            .unwrap_err();

        assert_eq!(
            error,
            CompilePatchError::DiscontinuousPresence {
                previous: first,
                next: second,
            }
        );
        assert_eq!(compiled.track(second), Some(&before));
        assert!(compiled.track(unrelated_track).is_some());
    }

    #[test]
    fn remove_recreate_and_track_replacement_use_only_transaction_overlay() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let track = scene
            .animate_position(
                object,
                noon_core::Vec2::ZERO,
                noon_core::Vec2::ONE,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();

        let replacement = TrackDefinition {
            id: track,
            object,
            property: noon_core::Property::Position,
            values: TrackValues::Vec2 {
                from: noon_core::Vec2::ONE,
                to: noon_core::Vec2::new(2.0, 2.0),
            },
            timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };
        let stats = compiled
            .preflight_transaction(&MutationTransaction::from_mutations([
                ScenePatch::RemoveObject(object),
                ScenePatch::CreateObject(ObjectDefinition::new(object, GeometryRef::circle(2.0))),
                ScenePatch::AddTrack(replacement.clone()),
                ScenePatch::ReplaceTrack(replacement),
            ]))
            .unwrap();

        assert_eq!(stats.objects_indexed, 1);
        assert_eq!(stats.tracks_indexed, 1);
        assert_eq!(stats.staged_compiled_scene_clones, 0);
    }

    #[test]
    fn removing_original_owner_preserves_a_track_moved_earlier_in_the_batch() {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::circle(2.0));
        let track = scene
            .animate_position(
                first,
                noon_core::Vec2::ZERO,
                noon_core::Vec2::ONE,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();
        let moved = TrackDefinition {
            id: track,
            object: second,
            property: noon_core::Property::Position,
            values: TrackValues::Vec2 {
                from: noon_core::Vec2::ZERO,
                to: noon_core::Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };
        let prefix = [
            ScenePatch::ReplaceTrack(moved.clone()),
            ScenePatch::RemoveObject(first),
        ];
        for last in [
            ScenePatch::RemoveTrack(track),
            ScenePatch::ReplaceTrack(moved.clone()),
        ] {
            let mutations = prefix.clone().into_iter().chain([last]).collect::<Vec<_>>();
            compiled
                .preflight_transaction(&MutationTransaction::from_mutations(mutations.clone()))
                .unwrap();
            let mut applied = compiled.clone();
            for patch in mutations {
                applied.apply_patch(&patch).unwrap();
            }
        }
        assert_eq!(
            compiled.preflight_transaction(&MutationTransaction::from_mutations(
                prefix.into_iter().chain([ScenePatch::AddTrack(moved)])
            )),
            Err(CompilePatchError::DuplicateTrack(track))
        );
    }

    #[test]
    fn presence_validation_counts_only_the_affected_base_channel() {
        let mut scene = SceneDefinition::new();
        let target = scene.add(GeometryRef::circle(1.0));
        let unrelated = scene.add(GeometryRef::circle(2.0));
        scene.set_presence_at(target, false, true, 1.0).unwrap();
        scene.set_presence_at(target, true, false, 2.0).unwrap();
        for index in 0..1000 {
            scene
                .set_presence_at(unrelated, index % 2 != 0, index % 2 == 0, index as f64)
                .unwrap();
        }
        let compiled = CompiledScene::compile(&scene).unwrap();
        let transaction =
            MutationTransaction::from_mutations([ScenePatch::AddTrack(TrackDefinition {
                id: noon_core::TrackId::new(10_000),
                object: target,
                property: noon_core::Property::Presence,
                values: TrackValues::Bool {
                    from: false,
                    to: true,
                },
                timing: TrackTiming::instant(3.0),
                time_map: noon_core::CompositionTimeMap::identity(),
            })]);
        let stats = compiled.preflight_transaction(&transaction).unwrap();
        assert_eq!(stats.tracks_indexed, 2);
        assert_eq!(stats.objects_indexed, 1);
    }

    #[test]
    fn sparse_preflight_agrees_with_sequential_compiler_validation() {
        let mut scene = SceneDefinition::new();
        let objects = [
            scene.add(GeometryRef::circle(1.0)),
            scene.add(GeometryRef::circle(2.0)),
        ];
        scene
            .animate_position(
                objects[0],
                noon_core::Vec2::ZERO,
                noon_core::Vec2::ONE,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();
        // A deterministic model comparison explores interactions between moved
        // tracks, deleted/recreated objects, and presence-channel replacements.
        let mut random = 37_u64;
        for _ in 0..2_000 {
            let mut patches = Vec::new();
            for _ in 0..6 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let object = objects[((random >> 32) & 1) as usize];
                let id = noon_core::TrackId::new((random >> 40) % 4);
                let presence = random & 8 != 0;
                let track = TrackDefinition {
                    id,
                    object,
                    property: if presence {
                        noon_core::Property::Presence
                    } else {
                        noon_core::Property::Position
                    },
                    values: if presence {
                        TrackValues::Bool {
                            from: random & 16 != 0,
                            to: random & 32 != 0,
                        }
                    } else {
                        TrackValues::Vec2 {
                            from: noon_core::Vec2::ZERO,
                            to: noon_core::Vec2::ONE,
                        }
                    },
                    timing: if presence {
                        TrackTiming::instant((random >> 48) as f64 % 4.0)
                    } else {
                        TrackTiming::new(0.0, 1.0, Easing::Linear)
                    },
                    time_map: noon_core::CompositionTimeMap::identity(),
                };
                patches.push(match (random >> 24) % 6 {
                    0 => ScenePatch::CreateObject(ObjectDefinition::new(
                        object,
                        GeometryRef::circle(3.0),
                    )),
                    1 => ScenePatch::RemoveObject(object),
                    2 => ScenePatch::AddTrack(track),
                    3 => ScenePatch::ReplaceTrack(track),
                    4 => ScenePatch::RemoveTrack(id),
                    _ => ScenePatch::SetStyle {
                        object,
                        style: Style::default(),
                    },
                });
            }
            let mut reference = compiled.clone();
            let expected = patches
                .iter()
                .try_for_each(|patch| reference.apply_patch(patch));
            let transaction = MutationTransaction::from_mutations(patches);
            let actual = compiled.preflight_transaction(&transaction).map(|_| ());
            assert_eq!(actual, expected, "transaction: {transaction:?}");
        }
    }

    #[test]
    fn independent_presence_edits_visit_linear_metadata_in_a_large_batch() {
        let mut scene = SceneDefinition::new();
        let mut objects = Vec::new();
        for _ in 0..1_000 {
            let object = scene.add(GeometryRef::circle(1.0));
            scene.set_presence_at(object, false, true, 1.0).unwrap();
            objects.push(object);
        }
        let compiled = CompiledScene::compile(&scene).unwrap();
        for size in [16, 1_000] {
            let transaction =
                MutationTransaction::from_mutations(objects.iter().take(size).enumerate().map(
                    |(index, object)| {
                        ScenePatch::AddTrack(TrackDefinition {
                            id: noon_core::TrackId::new(10_000 + index as u64),
                            object: *object,
                            property: noon_core::Property::Presence,
                            values: TrackValues::Bool {
                                from: true,
                                to: false,
                            },
                            timing: TrackTiming::instant(2.0),
                            time_map: noon_core::CompositionTimeMap::identity(),
                        })
                    },
                ));
            let stats = compiled.preflight_transaction(&transaction).unwrap();
            assert_eq!(stats.objects_indexed, size);
            assert_eq!(stats.tracks_indexed, size);
            assert_eq!(stats.track_metadata_visits, size * 3);
        }
    }
}
