//! Sparse, reversible changes to a validated execution projection.
//!
//! These values are deliberately not semantic mutations and have no independent
//! identity allocator. They may only be exchanged, in order, by their originating
//! compiled scene's finite replay scope. Immutable content stays shared.

use std::collections::BTreeSet;

use crate::{
    CompiledChannelKey, CompiledObject, CompiledScene, CompiledTrack, CompiledTrackLocator,
    ExecutionPatch,
};
use noon_core::{ObjectId, TrackId};

#[derive(Clone, Debug)]
struct SavedRow {
    index: u32,
    object: Option<CompiledObject>,
    // None: value/channel change only. Some(None): member at the family tail.
    order: Option<Option<u32>>,
}

/// One compiler-owned opposite revision, proportional to the affected payloads.
#[derive(Clone, Debug)]
pub struct CompiledReplayRevision {
    rows: Vec<SavedRow>,
    channels: Vec<CompiledChannelKey>,
    tracks: Vec<(TrackId, Option<CompiledTrack>)>,
}

impl CompiledReplayRevision {
    pub fn object_indices(&self) -> impl Iterator<Item = u32> + '_ {
        self.rows.iter().map(|row| row.index)
    }
    pub fn channels(&self) -> impl Iterator<Item = CompiledChannelKey> + '_ {
        self.channels.iter().copied()
    }
    /// Reserve one payload for each affected track even when the saved side is
    /// absent. Exchanges therefore cannot grow beyond the admitted scope budget.
    pub fn retention_cost(&self) -> usize {
        self.rows.len() + self.channels.len() + self.tracks.len()
    }
}

impl CompiledScene {
    /// Capture only the state this patch can replace. Unsupported execution domains
    /// return None so the enclosing replay capability fails closed, not silently.
    pub fn prepare_replay_revision(
        &self,
        patch: &ExecutionPatch,
    ) -> Option<CompiledReplayRevision> {
        let mut rows = BTreeSet::new();
        let mut channels = BTreeSet::new();
        let mut tracks = BTreeSet::new();
        let mut order = false;
        let index = |object: ObjectId| self.object_index(object);
        match patch {
            ExecutionPatch::CreateObject(object) => {
                let slot = self
                    .retired_object_indices
                    .get(&object.id)
                    .copied()
                    .unwrap_or(self.objects.len().try_into().ok()?);
                rows.insert(slot);
                order = true;
            }
            ExecutionPatch::RemoveObject(object) => {
                let slot = index(*object)?;
                rows.insert(slot);
                for channel in self.channels_for_object_index(slot) {
                    channels.insert(channel);
                    tracks.extend(self.channel_tracks(channel).iter().map(|track| track.id));
                }
                order = true;
            }
            ExecutionPatch::ReorderObject { object, .. }
            | ExecutionPatch::SetZIndex { object, .. } => {
                rows.insert(index(*object)?);
                order = true;
            }
            ExecutionPatch::SetContent { object, .. }
            | ExecutionPatch::SetTransform { object, .. }
            | ExecutionPatch::SetStyle { object, .. } => {
                rows.insert(index(*object)?);
            }
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                tracks.insert(track.id);
                let slot = index(track.object)?;
                rows.insert(slot);
                channels.insert(CompiledChannelKey::new(slot, track.property));
                if let Some(channel) = self.channel_for_track(track.id) {
                    rows.insert(channel.object_index);
                    channels.insert(channel);
                }
            }
            ExecutionPatch::RemoveTrack(track) | ExecutionPatch::ReconcileTrack { track, .. } => {
                tracks.insert(*track);
                let channel = self.channel_for_track(*track)?;
                rows.insert(channel.object_index);
                channels.insert(channel);
            }
            ExecutionPatch::AddFamilyAnimation(_) => return None,
        }
        Some(CompiledReplayRevision {
            rows: rows
                .into_iter()
                .map(|index| SavedRow {
                    index,
                    object: self
                        .objects
                        .get(index as usize)
                        .filter(|object| object.live)
                        .cloned(),
                    order: order.then(|| self.family_successor(index)),
                })
                .collect(),
            channels: channels.into_iter().collect(),
            tracks: tracks
                .into_iter()
                .map(|id| (id, self.track(id).cloned()))
                .collect(),
        })
    }

    fn family_successor(&self, index: u32) -> Option<u32> {
        let rank = self.family_rank(index)? as usize;
        self.family_order.get(rank + 1).copied()
    }

    /// Exchange a previously validated local revision with the current projection.
    /// The caller owns ordering and lifetime; no semantic code is re-executed.
    pub fn exchange_replay_revision(&mut self, revision: &mut CompiledReplayRevision) {
        for saved in &mut revision.rows {
            let index = saved.index as usize;
            // Every captured create has completed before the scope can be sealed.
            let current = self.objects[index]
                .live
                .then(|| self.objects[index].clone());
            let current_order = saved.order.map(|_| self.family_successor(saved.index));
            if saved.order.is_some() {
                if let Some(rank) = self.family_ranks[index].take() {
                    self.family_order.remove(rank as usize);
                    for rank in rank as usize..self.family_order.len() {
                        self.family_ranks[self.family_order[rank] as usize] = Some(rank as u32);
                    }
                }
                if let Some(rank) = self.painter_ranks[index].take() {
                    self.painter_order.remove(rank as usize);
                    for rank in rank as usize..self.painter_order.len() {
                        self.painter_ranks[self.painter_order[rank] as usize] = Some(rank as u32);
                    }
                }
            }
            let id = self.objects[index].id;
            self.object_indices.remove(&id);
            self.retired_object_indices.remove(&id);
            self.live_object_count -= usize::from(self.objects[index].live);
            if let Some(object) = saved.object.take() {
                self.objects[index] = object;
                self.object_indices
                    .insert(self.objects[index].id, saved.index);
                self.live_object_count += 1;
            } else {
                self.objects[index].live = false;
                self.objects[index].dynamic = Default::default();
                self.retired_object_indices.insert(id, saved.index);
            }
            if let Some(before) = saved.order {
                if self.objects[index].live {
                    let rank = before
                        .and_then(|anchor| self.family_rank(anchor))
                        .map_or(self.family_order.len(), |rank| rank as usize);
                    self.family_order.insert(rank, saved.index);
                    for rank in rank..self.family_order.len() {
                        self.family_ranks[self.family_order[rank] as usize] = Some(rank as u32);
                    }
                    self.painter_ranks[index] = Some(self.painter_order.len() as u32);
                    self.painter_order.push(saved.index);
                    self.reposition_painter_row(saved.index);
                }
            }
            saved.object = current;
            saved.order = current_order;
        }
        for (id, saved) in &mut revision.tracks {
            let current = self.track_locators.remove(id).map(|locator| {
                let channel = CompiledChannelKey::new(locator.object_index, locator.property);
                let position = self.track_position(locator);
                let tracks = self
                    .tracks
                    .get_mut(&channel)
                    .expect("retained track channel");
                let track = tracks.remove(position);
                if tracks.is_empty() {
                    self.tracks.remove(&channel);
                }
                self.track_count -= 1;
                track
            });
            if let Some(track) = saved.take() {
                let channel = CompiledChannelKey::new(track.object_index, track.property);
                let tracks = self.tracks.entry(channel).or_default();
                let position = crate::track_insertion_position(tracks, &track);
                self.track_locators
                    .insert(*id, CompiledTrackLocator::from_track(&track));
                tracks.insert(position, track);
                self.track_count += 1;
            }
            *saved = current;
        }
    }
}
