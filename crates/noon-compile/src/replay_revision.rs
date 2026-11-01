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

#[derive(Clone, Debug)]
struct SavedInset2DView {
    object_index: u32,
    view: Option<crate::CompiledInset2DView>,
}

/// One compiler-owned opposite revision, proportional to the affected payloads.
#[derive(Clone, Debug)]
pub struct CompiledReplayRevision {
    rows: Vec<SavedRow>,
    // At most one relation per addressed patch. Keep rare metadata boxed rather
    // than widening every retained ordinary row or saving a view-list snapshot.
    inset_2d_view: Option<Box<SavedInset2DView>>,
    channels: Vec<CompiledChannelKey>,
    tracks: Vec<(TrackId, Option<CompiledTrack>)>,
    // Reserve both sides of changed camera metadata. Exchanging revisions must
    // never retain a larger motion history than the scope budget admitted.
    camera_motion_payloads: usize,
    // Variable path snapshots are immutable Arc payloads, but are retained by
    // inverse tracks. Charge each side by its command count in the finite replay
    // payload budget just as camera intervals are charged by payload count.
    path_motion_commands: usize,
    // Both exchange sides retain immutable sample tables. The entry budget is
    // conservative per track; prepared resource bytes deduplicate shared Arcs.
    sampled_motion_keys: usize,
    reveal_phase_keys: usize,
}

impl CompiledReplayRevision {
    pub fn object_indices(&self) -> impl Iterator<Item = u32> + '_ {
        self.rows
            .iter()
            .map(|row| row.index)
            .chain(self.inset_2d_view.iter().map(|saved| saved.object_index))
    }
    pub fn channels(&self) -> impl Iterator<Item = CompiledChannelKey> + '_ {
        self.channels.iter().copied()
    }
    /// Reserve each affected track or inset relation even when its saved side is
    /// absent. Exchanges therefore cannot grow beyond the admitted scope budget.
    pub fn retention_cost(&self) -> usize {
        usize::from(self.inset_2d_view.is_some())
            + self.rows.len()
            + self.channels.len()
            + self.tracks.len()
            + self.camera_motion_payloads
            + self.path_motion_commands
            + self.sampled_motion_keys
            + self.reveal_phase_keys
    }

    /// New-suffix resource accounting inspects only saved track payloads.
    pub fn sampled_motion_plans(
        &self,
    ) -> impl Iterator<Item = &std::sync::Arc<noon_core::SampledMotionPlan>> {
        self.tracks
            .iter()
            .filter_map(|(_, track)| track.as_ref()?.values.sampled_motion_plan())
    }

    pub fn reveal_phase_maps(
        &self,
    ) -> impl Iterator<Item = &std::sync::Arc<noon_core::RevealPhaseMap>> {
        self.tracks
            .iter()
            .filter_map(|(_, track)| track.as_ref()?.values.reveal_phase_map())
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
        let mut inset_2d_view = None;
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
            ExecutionPatch::SetInset2DView { object, .. } => {
                inset_2d_view = Some(Box::new(SavedInset2DView {
                    object_index: index(*object)?,
                    view: self.inset_2d_views.get(object).copied(),
                }));
            }
            ExecutionPatch::RemoveObject(object) => {
                if let Some(view) = self.inset_2d_views.get(object) {
                    inset_2d_view = Some(Box::new(SavedInset2DView {
                        object_index: index(*object)?,
                        view: Some(*view),
                    }));
                }
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
            | ExecutionPatch::SetSemanticTransform { object, .. }
            | ExecutionPatch::SetSpatialState { object, .. }
            | ExecutionPatch::SetDrawEnabled { object, .. }
            | ExecutionPatch::SetClickIndicate { object, .. }
            | ExecutionPatch::SetStyle { object, .. }
            | ExecutionPatch::SetViewColorProfile { object, .. }
            | ExecutionPatch::SetEffects { object, .. } => {
                rows.insert(index(*object)?);
            }
            ExecutionPatch::SetEffectStack {
                object,
                expected,
                effects,
            } => {
                let slot = index(*object)?;
                rows.insert(slot);
                for &attachment in expected {
                    if effects
                        .as_ref()
                        .is_some_and(|stack| stack.get(attachment).is_some())
                    {
                        continue;
                    }
                    let retired = self.objects[slot as usize]
                        .effects
                        .as_ref()
                        .and_then(|stack| stack.get(attachment))?;
                    for property in retired.parameter_properties() {
                        let channel = CompiledChannelKey::effect(slot, property, attachment);
                        if !self.channel_tracks(channel).is_empty() {
                            channels.insert(channel);
                            tracks
                                .extend(self.channel_tracks(channel).iter().map(|track| track.id));
                        }
                    }
                }
            }
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                tracks.insert(track.id);
                let slot = index(track.object)?;
                rows.insert(slot);
                channels.insert(CompiledChannelKey::for_values(
                    slot,
                    track.property,
                    &track.values,
                ));
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
            ExecutionPatch::AddFamilyAnimation(_) => {
                // Family plans/channels are immutable and append-only, like the
                // resource arena. Keep their indices resident across replay;
                // their existing mapped interval gates their visible effect.
                // The runtime admits this only when the interval does not start
                // before publication. No row or channel payload is replaced.
            }
            ExecutionPatch::SetGraphDependencies { .. }
            | ExecutionPatch::SetSpatialAnchorGroupBoundsMembers { .. } => return None,
        }
        let saved_motion_payloads: usize = rows
            .iter()
            .filter_map(|&index| {
                self.objects
                    .get(index as usize)?
                    .spatial
                    .as_deref()?
                    .camera_motions
                    .as_deref()
            })
            .map(<[_]>::len)
            .sum();
        let incoming_motion_payloads = match patch {
            ExecutionPatch::CreateObject(object) => object.spatial.as_deref(),
            ExecutionPatch::SetSpatialState { spatial, .. } => spatial.as_ref(),
            _ => None,
        }
        .and_then(|spatial| spatial.camera_motions.as_deref())
        .map_or(0, <[_]>::len);
        let saved_path_motion_commands: usize = tracks
            .iter()
            .filter_map(|id| self.track(*id))
            .map(|track| path_motion_command_count(&track.values))
            .sum();
        let incoming_path_motion_commands = match patch {
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                path_motion_command_count(&track.values)
            }
            _ => 0,
        };
        let saved_sampled_motion_keys = tracks
            .iter()
            .filter_map(|id| self.track(*id))
            .filter_map(|track| track.values.sampled_motion_plan())
            .try_fold(0usize, |total, plan| total.checked_add(plan.motion().len()))?;
        let incoming_sampled_motion_keys = match patch {
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => track
                .values
                .sampled_motion_plan()
                .map_or(0, |plan| plan.motion().len()),
            _ => 0,
        };
        let saved_reveal_phase_keys = tracks
            .iter()
            .filter_map(|id| self.track(*id))
            .filter_map(|track| track.values.reveal_phase_map())
            .try_fold(0usize, |total, map| total.checked_add(map.len()))?;
        let incoming_reveal_phase_keys = match patch {
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                track.values.reveal_phase_map().map_or(0, |map| map.len())
            }
            _ => 0,
        };
        Some(CompiledReplayRevision {
            inset_2d_view,
            camera_motion_payloads: saved_motion_payloads + incoming_motion_payloads,
            path_motion_commands: saved_path_motion_commands + incoming_path_motion_commands,
            sampled_motion_keys: saved_sampled_motion_keys
                .checked_add(incoming_sampled_motion_keys)?,
            reveal_phase_keys: saved_reveal_phase_keys.checked_add(incoming_reveal_phase_keys)?,
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
            let previous_composed = self.composed_plan(self.objects[index].id);
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
                        self.refresh_prefix_rank(
                            self.objects[self.painter_order[rank] as usize].id,
                            Some(rank),
                        );
                    }
                }
            }
            let id = self.objects[index].id;
            self.click_indicate_count -= usize::from(
                self.objects[index].live && self.objects[index].click_indicate.is_some(),
            );
            self.object_indices.remove(&id);
            self.retired_object_indices.remove(&id);
            self.live_object_count -= usize::from(self.objects[index].live);
            if let Some(object) = saved.object.take() {
                self.click_indicate_count += usize::from(object.click_indicate.is_some());
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
            self.update_composed_dependency(id, previous_composed);
            if self.objects[index].id != id {
                self.refresh_prefix_admission(self.objects[index].id);
            }
            saved.object = current;
            saved.order = current_order;
        }
        if let Some(saved) = &mut revision.inset_2d_view {
            let display = self.objects[saved.object_index as usize].id;
            let current = self.inset_2d_views.remove(&display);
            if let Some(view) = saved.view.take() {
                self.inset_2d_views.insert(display, view);
            }
            saved.view = current;
        }
        for (id, saved) in &mut revision.tracks {
            let current = self.track_locators.remove(id).map(|locator| {
                let channel = locator.channel();
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
                let channel = track.channel();
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

fn path_motion_command_count(values: &noon_core::TrackValues) -> usize {
    match values {
        noon_core::TrackValues::PathVec2 { path, .. } => path.retained_command_count(),
        _ => 0,
    }
}

#[cfg(feature = "prepared-prefix")]
mod prefix;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompiledInset2DView;
    use noon_core::{GeometryRef, Style, Transform2D, Vec2};

    #[test]
    fn phase_reveal_revision_charges_both_exchange_sides_and_shares_maps() {
        use noon_core::{
            CompositionTimeMap, Property, RateFunction, RevealPhaseMap, TrackDefinition,
            TrackTiming, TrackValues, VectorPath,
        };
        use std::sync::Arc;
        let old_map = Arc::new(RevealPhaseMap::new(vec![0.0, 0.1, 1.0]).unwrap());
        let new_map = Arc::new(RevealPhaseMap::new(vec![0.0, 0.9, 1.0]).unwrap());
        let path = Arc::new(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE)
                .line_to(Vec2::new(2.0, 0.0)),
        );
        let old = TrackDefinition {
            id: TrackId::new(1),
            object: ObjectId::new(1),
            property: Property::Reveal,
            values: TrackValues::PhaseReveal {
                phase_map: old_map.clone(),
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let mut scene = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                ObjectId::new(1),
                GeometryRef::VectorPath(path),
                Transform2D::IDENTITY,
                Style::default(),
            )],
            &[old.clone()],
        )
        .unwrap();
        let replacement = ExecutionPatch::ReplaceTrack(TrackDefinition {
            values: TrackValues::PhaseReveal {
                phase_map: new_map.clone(),
            },
            ..old
        });
        let mut revision = scene.prepare_replay_revision(&replacement).unwrap();
        assert_eq!(revision.retention_cost(), 9); // row + channel + track + both three-knot maps.
        assert!(Arc::ptr_eq(
            revision.reveal_phase_maps().next().unwrap(),
            &old_map
        ));
        scene.apply_execution_patch(&replacement).unwrap();
        #[cfg(feature = "prepared-prefix")]
        {
            let forward = scene.forward_prefix_revisions(&[&revision]).unwrap();
            assert_eq!(forward[0].retention_cost(), revision.retention_cost());
            assert!(Arc::ptr_eq(
                forward[0].reveal_phase_maps().next().unwrap(),
                &new_map
            ));
        }
        scene.exchange_replay_revision(&mut revision);
        assert!(Arc::ptr_eq(
            revision.reveal_phase_maps().next().unwrap(),
            &new_map
        ));
        assert!(Arc::ptr_eq(
            scene
                .track(TrackId::new(1))
                .unwrap()
                .values
                .reveal_phase_map()
                .unwrap(),
            &old_map
        ));
        assert_eq!(revision.retention_cost(), 9);
    }

    #[test]
    fn inset_revision_cost_is_sparse_and_identity_links_survive_compaction() {
        let ids = [ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)];
        let mut scene = CompiledScene::compile_objects(
            ids.map(|id| {
                CompiledObject::new(
                    id,
                    GeometryRef::Rectangle {
                        size: Vec2::new(2.0, 1.0),
                    },
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .to_vec(),
            &[],
        )
        .unwrap();
        let set = ExecutionPatch::SetInset2DView {
            object: ids[2],
            view: Some(CompiledInset2DView {
                camera_frame: ids[1],
                capture_own_display: false,
            }),
        };
        let mut revision = scene.prepare_replay_revision(&set).unwrap();
        assert_eq!(revision.retention_cost(), 1);
        assert_eq!(revision.object_indices().collect::<Vec<_>>(), vec![2]);
        scene.apply_execution_patch(&set).unwrap();
        let expected = scene.inset_2d_views().collect::<Vec<_>>();
        scene.exchange_replay_revision(&mut revision);
        assert!(scene.inset_2d_views().next().is_none());
        scene.exchange_replay_revision(&mut revision);
        assert_eq!(scene.inset_2d_views().collect::<Vec<_>>(), expected);
        let remove = ExecutionPatch::RemoveObject(ids[0]);
        assert_eq!(
            scene
                .prepare_replay_revision(&remove)
                .unwrap()
                .retention_cost(),
            1,
            "an ordinary row must not retain an absent inset payload"
        );
        scene.apply_execution_patch(&remove).unwrap();
        drop(revision); // A maintenance barrier ends the old row-index lifetime.
        scene.compact_retired_object_slots().unwrap();
        assert_eq!(scene.inset_2d_views().collect::<Vec<_>>(), expected);
        let invalid = ExecutionPatch::SetInset2DView {
            object: ids[2],
            view: Some(CompiledInset2DView {
                camera_frame: ids[2],
                capture_own_display: false,
            }),
        };
        assert!(scene
            .preflight_execution_transaction(&crate::ExecutionMutationTransaction::from_mutations(
                [invalid.clone()]
            ))
            .is_err());
        assert!(scene.apply_execution_patch(&invalid).is_err());
        assert_eq!(scene.inset_2d_views().collect::<Vec<_>>(), expected);
    }
}
