//! Experimental fixed-membership prefix transfer. Uses the existing compiler
//! revision representation; no second scene, evaluator, or resource registry.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use noon_core::{Property, TrackId};
use std::collections::BTreeMap;

fn local_property(property: Property) -> bool {
    matches!(
        property,
        Property::Transform
            | Property::Position
            | Property::Rotation
            | Property::Scale
            | Property::Fill
            | Property::Stroke
            | Property::StrokeWidth
            | Property::Opacity
            | Property::Appearance
            | Property::Reveal
    )
}

fn local_track(track: &Option<CompiledTrack>) -> bool {
    track.as_ref().is_none_or(|track| {
        local_property(track.property)
            && track.transform_geometry_plan.is_none()
            && super::path_motion_command_count(&track.values) == 0
    })
}

impl CompiledScene {
    /// Recover the forward sides of a NEW suffix of inverse revisions without
    /// rewinding the producer or cloning its compiled scene/history.
    ///
    /// Reverse traversal keeps only touched rows/tracks in a scratch overlay.
    /// Capturing each revision from the latest scene directly would leak a later
    /// write into earlier timestamps when a row is changed more than once.
    ///
    /// Admission is deliberately narrow: existing live rows, unchanged content,
    /// order, glow and spatial domains, and ordinary property tracks. Structural,
    /// family, path-motion and external-resource extension need separate support.
    /// The caller must pass the complete, ordered suffix ending at this scene's
    /// current frontier and validate originating runtime/scope identity first.
    /// None leaves this scene unchanged and publishes nothing.
    pub fn forward_prefix_revisions(
        &self,
        inverse: &[&CompiledReplayRevision],
    ) -> Option<Vec<CompiledReplayRevision>> {
        let mut row_overlay = BTreeMap::new();
        let mut track_overlay: BTreeMap<TrackId, Option<CompiledTrack>> = BTreeMap::new();
        let mut output = Vec::with_capacity(inverse.len());
        for old in inverse.iter().rev() {
            // Empty revisions denote append-only family plans, not a quiet wait.
            if old.rows.is_empty()
                || old.camera_motion_payloads != 0
                || old.path_motion_commands != 0
                || old.channels.iter().any(|key| !local_property(key.property))
            {
                return None;
            }
            let mut rows = Vec::with_capacity(old.rows.len());
            for saved in &old.rows {
                if saved.order.is_some() {
                    return None;
                }
                let before = saved.object.as_ref()?;
                let current = row_overlay
                    .entry(saved.index)
                    .or_insert_with(|| self.objects.get(saved.index as usize).cloned());
                let after = current.as_ref()?;
                if !before.live
                    || !after.live
                    || before.id != after.id
                    || before.content != after.content
                    || before.text_bounds != after.text_bounds
                    || before.base_z_index != after.base_z_index
                    || before.spatial.is_some()
                    || after.spatial.is_some()
                    || before.glow.is_some()
                    || after.glow.is_some()
                {
                    return None;
                }
                rows.push(SavedRow {
                    index: saved.index,
                    object: Some(after.clone()),
                    order: None,
                });
                *current = Some(before.clone());
            }
            let mut tracks = Vec::with_capacity(old.tracks.len());
            for (id, before) in &old.tracks {
                let after = track_overlay
                    .entry(*id)
                    .or_insert_with(|| self.track(*id).cloned());
                if !local_track(before) || !local_track(after) {
                    return None;
                }
                tracks.push((*id, after.clone()));
                *after = before.clone();
            }
            output.push(CompiledReplayRevision {
                rows,
                tracks,
                channels: old.channels.clone(),
                camera_motion_payloads: 0,
                path_motion_commands: 0,
            });
        }
        output.reverse();
        Some(output)
    }
}
