//! Explicit reclamation of retired compiled-object rows.
//!
//! This is a maintenance barrier, never part of an ordinary execution patch. It
//! deliberately relocates live execution rows, so its caller must rebuild every
//! row-indexed derived consumer under a new execution revision.

use std::collections::BTreeMap;

use crate::{CompiledChannelKey, CompiledScene, CompiledTrack, CompiledTrackLocator};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledSceneCompactionError {
    /// These projections contain row indices that this bounded maintenance slice
    /// does not yet remap. Rejecting keeps their current execution projection
    /// authoritative instead of quietly dropping it.
    DerivedStatePresent,
}

impl std::fmt::Display for CompiledSceneCompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DerivedStatePresent => formatter.write_str(
                "retired-row compaction does not support retained graph, family, or numeric-text state",
            ),
        }
    }
}

impl std::error::Error for CompiledSceneCompactionError {}

/// Instrumentation for the explicitly scheduled retired-row maintenance barrier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledSceneCompactionStats {
    pub object_slots_before: usize,
    pub object_slots_after: usize,
    pub object_slots_reclaimed: usize,
    pub track_rows_reindexed: usize,
}

impl CompiledScene {
    /// Pack live execution rows and release all retired object-row history.
    ///
    /// This intentionally visits retained slots and live tracks. Runtime owners
    /// must use their explicit maintenance barrier to renew execution/frame
    /// revisions before exposing the relocated rows.
    pub fn compact_retired_object_slots(
        &mut self,
    ) -> Result<CompiledSceneCompactionStats, CompiledSceneCompactionError> {
        let slots_before = self.objects.len();
        if self.retired_object_indices.is_empty() {
            return Ok(CompiledSceneCompactionStats {
                object_slots_before: slots_before,
                object_slots_after: slots_before,
                ..CompiledSceneCompactionStats::default()
            });
        }
        if !self.family_animation_plans.is_empty()
            || !self.family_animations.is_empty()
            || !self.graph_edge_dependencies.is_empty()
            || !self.numeric_text_drivers.is_empty()
        {
            return Err(CompiledSceneCompactionError::DerivedStatePresent);
        }

        let mut new_indices = vec![None; self.objects.len()];
        let mut objects = Vec::with_capacity(self.live_object_count);
        for (old_index, object) in self.objects.iter().enumerate() {
            if object.live {
                let new_index = u32::try_from(objects.len())
                    .expect("existing compiled object capacity fits u32");
                new_indices[old_index] = Some(new_index);
                objects.push(object.clone());
            }
        }

        let remap = |old_index: u32| {
            new_indices[old_index as usize]
                .expect("live compiled references never target retired rows")
        };
        let mut tracks = BTreeMap::<CompiledChannelKey, Vec<CompiledTrack>>::new();
        let mut track_locators = BTreeMap::new();
        let mut track_rows_reindexed = 0;
        for track in self.tracks.values().flatten() {
            let mut track = track.clone();
            track.object_index = remap(track.object_index);
            let channel = CompiledChannelKey::new(track.object_index, track.property);
            track_locators.insert(track.id, CompiledTrackLocator::from_track(&track));
            tracks.entry(channel).or_default().push(track);
            track_rows_reindexed += 1;
        }

        let family_order: Vec<u32> = self
            .family_order
            .iter()
            .map(|&index| remap(index))
            .collect();
        let painter_order: Vec<u32> = self
            .painter_order
            .iter()
            .map(|&index| remap(index))
            .collect();
        let mut family_ranks = vec![None; objects.len()];
        for (rank, &index) in family_order.iter().enumerate() {
            family_ranks[index as usize] = Some(rank as u32);
        }
        let mut painter_ranks = vec![None; objects.len()];
        for (rank, &index) in painter_order.iter().enumerate() {
            painter_ranks[index as usize] = Some(rank as u32);
        }
        let object_indices = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id, index as u32))
            .collect();

        self.objects = objects;
        self.live_object_count = self.objects.len();
        self.object_indices = object_indices;
        self.retired_object_indices.clear();
        self.tracks = tracks;
        self.track_locators = track_locators;
        self.family_order = family_order;
        self.family_ranks = family_ranks;
        self.painter_order = painter_order;
        self.painter_ranks = painter_ranks;

        Ok(CompiledSceneCompactionStats {
            object_slots_before: slots_before,
            object_slots_after: self.objects.len(),
            object_slots_reclaimed: slots_before - self.objects.len(),
            track_rows_reindexed,
        })
    }
}
