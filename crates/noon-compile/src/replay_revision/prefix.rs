//! Experimental sparse lifecycle/content prefix transfer. Uses the existing compiler
//! revision representation; no second scene, evaluator, or resource registry.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use crate::CompiledObject;
use noon_core::{Property, TrackId};
use std::collections::BTreeMap;

mod family;
mod order;
mod resources;
mod storage;
use order::SparseOrder;

fn local_property(property: Property) -> bool {
    matches!(
        property,
        Property::Presence
            | Property::ZIndex
            | Property::Transform
            | Property::Position
            | Property::Rotation
            | Property::Scale
            | Property::Fill
            | Property::Stroke
            | Property::StrokeWidth
            | Property::Opacity
            | Property::Appearance
            | Property::Reveal
            | Property::Morph
            | Property::EffectColor
            | Property::EffectRadius
            | Property::EffectIntensity
    )
}

fn local_track(track: Option<&CompiledTrack>) -> bool {
    track.is_none_or(|track| {
        local_property(track.property) && super::path_motion_command_count(&track.values) == 0
    })
}

fn local_object(object: &CompiledObject, physical: &CompiledObject) -> bool {
    object.live
        && object.id == physical.id
        && object.spatial.is_none()
        && object
            .effects
            .as_ref()
            .is_none_or(|stack| stack.len() == 1 && stack.definitions()[0].as_glow().is_some())
}

impl CompiledReplayRevision {
    fn local_prefix_metadata(&self) -> bool {
        self.inset_2d_view.is_none()
            && self.camera_motion_payloads == 0
            && self.path_motion_commands == 0
            && self.channels.iter().all(|key| local_property(key.property))
            && self.rows.iter().filter(|row| row.order.is_some()).count() <= 1
    }

    /// Inset relations are retained by ordinary replay, but not yet presented
    /// by prepared readers. Bootstrap must inspect saved history as well as the
    /// current projection so a removed relation cannot disappear from admission.
    #[doc(hidden)]
    pub fn changes_inset_2d_view(&self) -> bool {
        self.inset_2d_view.is_some()
    }
}

impl CompiledScene {
    /// Necessary suffix-profile checks before advancing the canonical producer.
    /// Visits only saved payloads and their current counterparts, without cloning
    /// rows/tracks or reconstructing ordering. Complete extraction remains fallible.
    pub fn supports_prefix_revision(&self, old: &CompiledReplayRevision) -> bool {
        if !old.local_prefix_metadata() {
            return false;
        }
        for saved in &old.rows {
            let Some(physical) = self.objects.get(saved.index as usize) else {
                return false;
            };
            for object in saved.object.iter().chain(physical.live.then_some(physical)) {
                if !local_object(object, physical) {
                    return false;
                }
            }
        }
        old.tracks
            .iter()
            .all(|(id, before)| local_track(before.as_ref()) && local_track(self.track(*id)))
    }

    /// Recover the forward sides of a NEW suffix of inverse revisions without
    /// rewinding the producer or cloning its compiled scene/history.
    ///
    /// Reverse traversal keeps only touched rows/tracks in a scratch overlay.
    /// Capturing each revision from the latest scene directly would leak a later
    /// write into earlier timestamps when a row is changed more than once.
    ///
    /// Ordinary 2D lifecycle, content and property tracks are admitted. Family
    /// transforms, path-motion and spatial plans require separate qualification.
    /// Append-only glyph plans travel through the existing immutable plan table.
    /// Validated 2D transform/morph plans retain their existing immutable geometry.
    /// Rows may carry one resource-free glow attachment and its ordinary
    /// generation-scoped color/radius/intensity tracks; ordered stacks and
    /// other effect definitions remain outside this suffix profile.
    /// Ordering reconstruction visits only affected rows and their neighbours;
    /// it never clones/scans the full painter/family order.
    /// The caller must pass the complete, ordered suffix ending at this scene's
    /// current frontier and validate originating runtime/scope identity first.
    /// None leaves this scene unchanged and publishes nothing.
    pub fn forward_prefix_revisions(
        &self,
        inverse: &[&CompiledReplayRevision],
    ) -> Option<Vec<CompiledReplayRevision>> {
        let mut row_overlay = BTreeMap::new();
        let mut order = SparseOrder::new(self);
        let mut track_overlay: BTreeMap<TrackId, Option<CompiledTrack>> = BTreeMap::new();
        let mut output = Vec::with_capacity(inverse.len());
        for old in inverse.iter().rev() {
            // Empty revisions denote append-only glyph plans. Their immutable
            // table suffix is delivered separately; mapped starts gate activity.
            if !old.local_prefix_metadata() {
                return None;
            }
            let mut rows = Vec::with_capacity(old.rows.len());
            for saved in &old.rows {
                let physical = self.objects.get(saved.index as usize)?;
                let current = row_overlay
                    .entry(saved.index)
                    .or_insert_with(|| physical.live.then(|| physical.clone()));
                for object in saved.object.iter().chain(current.iter()) {
                    if !local_object(object, physical) {
                        return None;
                    }
                }
                // Capture the actual opposite membership/order, then undo this
                // one local order change in the sparse scratch view.
                let forward_order = if let Some(before) = saved.order {
                    let after = order.successor(saved.index);
                    order.restore(saved.index, saved.object.is_some(), before)?;
                    Some(after)
                } else {
                    if saved.object.is_none() || current.is_none() {
                        return None;
                    }
                    None
                };
                rows.push(SavedRow {
                    index: saved.index,
                    object: current.clone(),
                    order: forward_order,
                });
                *current = saved.object.clone();
            }
            let mut tracks = Vec::with_capacity(old.tracks.len());
            for (id, before) in &old.tracks {
                let after = track_overlay
                    .entry(*id)
                    .or_insert_with(|| self.track(*id).cloned());
                if !local_track(before.as_ref()) || !local_track(after.as_ref()) {
                    return None;
                }
                tracks.push((*id, after.clone()));
                *after = before.clone();
            }
            output.push(CompiledReplayRevision {
                rows,
                inset_2d_view: None,
                tracks,
                channels: old.channels.clone(),
                camera_motion_payloads: 0,
                path_motion_commands: 0,
                geometry_payload_bytes: old.geometry_payload_bytes,
            });
        }
        output.reverse();
        Some(output)
    }
}
