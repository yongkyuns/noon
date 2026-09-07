use std::collections::{BTreeMap, BTreeSet};

use noon_core::{
    resolve_track_timing, validate_style, validate_track_definition, validate_transform,
    ObjectContentRef, ObjectId, Property, TrackDefinition, TrackId, TrackValues,
};

use crate::{
    compile_patch_error, compile_transform_geometry_plan, map_object_state_error,
    validate_compiled_object, validate_execution_content, validate_execution_content_resource,
    CompilePatchError, CompiledChannelKey, CompiledScene, CompiledTrack,
    CompiledTransactionPreflightStats, ExecutionMutationTransaction, ExecutionPatch,
};

#[derive(Clone, Copy, Debug)]
struct TrackShadow {
    id: TrackId,
    object_index: u32,
    property: Property,
    start_time: f64,
    duration: f64,
    reconciled: bool,
    presence: Option<(bool, bool)>,
}

impl TrackShadow {
    fn from_compiled(track: &CompiledTrack) -> Self {
        Self {
            id: track.id,
            object_index: track.object_index,
            property: track.property,
            start_time: track.timing.start_time,
            duration: track.timing.duration,
            reconciled: track.reconciled,
            presence: presence_endpoints(track.property, &track.values),
        }
    }

    fn from_definition(track: &TrackDefinition, object_index: u32) -> Self {
        let timing = resolve_track_timing(track).expect("track was validated before preflight");
        Self {
            id: track.id,
            object_index,
            property: track.property,
            start_time: timing.start_time,
            duration: timing.duration,
            reconciled: false,
            presence: presence_endpoints(track.property, &track.values),
        }
    }
}

#[derive(Clone, Copy)]
enum ObjectOverlay {
    Present { index: u32, is_text: bool },
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
            Some(ObjectOverlay::Present { index, .. }) => Some(index),
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

    fn object_is_text(&mut self, scene: &CompiledScene, id: ObjectId) -> Option<bool> {
        match self.objects.get(&id).copied() {
            Some(ObjectOverlay::Present { is_text, .. }) => Some(is_text),
            Some(ObjectOverlay::Removed) => None,
            None => {
                let index = scene.object_indices.get(&id).copied()?;
                self.seen_objects.insert(id);
                Some(scene.objects[index as usize].text().is_some())
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

    fn channel(
        &mut self,
        scene: &CompiledScene,
        object_index: u32,
        property: Property,
    ) -> Vec<TrackShadow> {
        let channel = CompiledChannelKey::new(object_index, property);
        let mut tracks = Vec::new();
        let mut base_ids = BTreeSet::new();
        for track in scene.channel_tracks(channel) {
            self.track_metadata_visits += 1;
            self.seen_tracks.insert(track.id);
            base_ids.insert(track.id);
            match self.tracks.get(&track.id).copied() {
                Some(Some(shadow))
                    if shadow.object_index == object_index && shadow.property == property =>
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
    transaction: &ExecutionMutationTransaction,
) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
    preflight_transaction_with_resources(scene, transaction, &crate::CompiledResources::default())
}

pub(super) fn preflight_transaction_with_resources(
    scene: &CompiledScene,
    transaction: &ExecutionMutationTransaction,
    additions: &crate::CompiledResources,
) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
    let mut overlay = PreflightOverlay::new(scene);

    for patch in transaction.mutations() {
        match patch {
            ExecutionPatch::CreateObject(object) => {
                if overlay.object_index(scene, object.id).is_some() {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let index = u32::try_from(overlay.next_object_index)
                    .map_err(|_| CompilePatchError::TooManyObjects(overlay.next_object_index))?;
                validate_compiled_object(object)?;
                validate_execution_content_resource(
                    &scene.resources,
                    Some(additions),
                    object.id,
                    &object.content,
                    object.text_bounds,
                )?;
                overlay.next_object_index += 1;
                overlay.objects.insert(
                    object.id,
                    ObjectOverlay::Present {
                        index,
                        is_text: object.text().is_some(),
                    },
                );
            }
            ExecutionPatch::RemoveObject(id) => {
                let index = overlay
                    .object_index(scene, *id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                overlay.objects.insert(*id, ObjectOverlay::Removed);
                overlay.remove_object_tracks(scene, index);
            }
            ExecutionPatch::SetContent {
                object,
                content,
                text_bounds,
            } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_execution_content(*object, content, *text_bounds)?;
                validate_execution_content_resource(
                    &scene.resources,
                    Some(additions),
                    *object,
                    content,
                    *text_bounds,
                )?;
                for property in [Property::Transform, Property::Morph] {
                    if let Some(track) = overlay
                        .channel(scene, index, property)
                        .iter()
                        .find(|track| !track.reconciled)
                    {
                        return Err(CompilePatchError::ContentReplacementHasGeometryDriver {
                            object: *object,
                            track: track.id,
                            property,
                        });
                    }
                }
                overlay.objects.insert(
                    *object,
                    ObjectOverlay::Present {
                        index,
                        is_text: matches!(content, ObjectContentRef::Text(_)),
                    },
                );
            }
            ExecutionPatch::SetTransform { object, transform } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                validate_transform(*object, *transform).map_err(map_object_state_error)?;
            }
            ExecutionPatch::SetStyle { object, style } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                validate_style(*object, *style).map_err(map_object_state_error)?;
            }
            ExecutionPatch::AddTrack(track) => {
                if overlay.track(scene, track.id).is_some() {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                reject_geometry_track_on_text(scene, &mut overlay, track)?;
                let shadow = TrackShadow::from_definition(track, object_index);
                overlay.set_track(track.id, Some(shadow));
                if shadow.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, object_index)?;
                }
            }
            ExecutionPatch::ReplaceTrack(track) => {
                let old = overlay
                    .track(scene, track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                reject_geometry_track_on_text(scene, &mut overlay, track)?;
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
            ExecutionPatch::RemoveTrack(id) => {
                let removed = overlay
                    .track(scene, *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                overlay.set_track(*id, None);
                if removed.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, removed.object_index)?;
                }
            }
            ExecutionPatch::ReconcileTrack {
                track,
                object,
                property,
                end_time,
            } => {
                let mut reconciled = overlay
                    .track(scene, *track)
                    .ok_or(CompilePatchError::UnknownTrack(*track))?;
                if reconciled.reconciled {
                    return Err(CompilePatchError::TrackAlreadyReconciled(*track));
                }
                let object_index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let actual_end = reconciled.start_time + reconciled.duration;
                if reconciled.object_index != object_index
                    || reconciled.property != *property
                    || actual_end.total_cmp(end_time) != std::cmp::Ordering::Equal
                {
                    return Err(CompilePatchError::TrackReconciliationMismatch(*track));
                }
                if reconciled.duration <= 0.0
                    || !matches!(
                        reconciled.property,
                        Property::Position
                            | Property::Rotation
                            | Property::Scale
                            | Property::Fill
                            | Property::Stroke
                            | Property::StrokeWidth
                            | Property::Opacity
                            | Property::Appearance
                            | Property::Reveal
                            | Property::Morph
                    )
                {
                    return Err(CompilePatchError::UnsupportedTrackReconciliation(*track));
                }
                for other in overlay.channel(scene, object_index, *property) {
                    if other.id == *track {
                        continue;
                    }
                    let other_end = other.start_time + other.duration;
                    if reconciled.start_time < other_end && other.start_time < actual_end {
                        return Err(CompilePatchError::OverlappingTrackReconciliation {
                            track: *track,
                            other: other.id,
                        });
                    }
                }
                reconciled.reconciled = true;
                overlay.set_track(*track, Some(reconciled));
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

fn reject_geometry_track_on_text(
    scene: &CompiledScene,
    overlay: &mut PreflightOverlay,
    track: &TrackDefinition,
) -> Result<(), CompilePatchError> {
    if matches!(track.property, Property::Transform | Property::Morph)
        && overlay.object_is_text(scene, track.object) == Some(true)
    {
        return Err(CompilePatchError::GeometryTrackTargetsText {
            track: track.id,
            property: track.property,
        });
    }
    Ok(())
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
    let mut tracks = overlay.channel(scene, object_index, Property::Presence);
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
mod tests;
