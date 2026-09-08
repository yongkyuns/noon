//! Candidate-local activation captures for non-overlapping prepared channels.
use std::collections::HashMap;

use noon_core::{
    continuous_time_map_interval, ObjectId, Property, SemanticTransactionNodeRef, TrackValues,
};

use super::super::PreparedSemanticScheduledAnimationLeaf;
use super::affine::{driver_key, EffectiveAnimationProperties};
use super::prepared_composition::PreparedSemanticAnimationTrack;

#[derive(Default)]
pub(super) struct ScheduledCaptures {
    base: HashMap<ObjectId, EffectiveAnimationProperties>,
    completed: HashMap<(u64, u8), (Property, TrackValues, bool)>,
    latest_tracks: HashMap<(u64, u8), usize>,
    recorded_tracks: usize,
}

impl ScheduledCaptures {
    pub fn get(&self, object: ObjectId) -> Option<EffectiveAnimationProperties> {
        let mut value = *self.base.get(&object)?;
        for slot in 0..=driver_key(object, Property::Presence).1 {
            let Some((property, values, at_end)) = self.completed.get(&(object.get(), slot)) else {
                continue;
            };
            match (property, values) {
                (Property::Position, TrackValues::Vec2 { from, to }) => {
                    value.transform.translation = if *at_end { *to } else { *from }
                }
                (Property::Scale, TrackValues::Vec2 { from, to }) => {
                    value.transform.scale = if *at_end { *to } else { *from }
                }
                (Property::Rotation, TrackValues::Scalar { from, to }) => {
                    value.transform.rotation = if *at_end { *to } else { *from }
                }
                (Property::Fill, TrackValues::Color { from, to }) => {
                    value.style.fill = if *at_end { *to } else { *from }
                }
                (Property::Stroke, TrackValues::Color { from, to }) => {
                    value.style.stroke = if *at_end { *to } else { *from }
                }
                (Property::StrokeWidth, TrackValues::Scalar { from, to }) => {
                    value.style.stroke_width = if *at_end { *to } else { *from }
                }
                (Property::Opacity, TrackValues::Scalar { from, to }) => {
                    value.style.opacity = if *at_end { *to } else { *from }
                }
                (Property::Appearance, TrackValues::Scalar { from, to }) => {
                    value.appearance = if *at_end { *to } else { *from }
                }
                (Property::Reveal, TrackValues::Scalar { from, to }) => {
                    value.reveal = if *at_end { *to } else { *from }
                }
                _ => {}
            }
        }
        Some(value)
    }

    pub fn insert(&mut self, object: ObjectId, value: EffectiveAnimationProperties) {
        self.base.insert(object, value);
    }

    pub fn begin_leaf(
        &mut self,
        leaf: &PreparedSemanticScheduledAnimationLeaf,
        driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
        tracks: &[PreparedSemanticAnimationTrack],
        intervals: &HashMap<SemanticTransactionNodeRef, (f64, f64)>,
        allow_sequential_capture: bool,
    ) {
        for (index, track) in tracks.iter().enumerate().skip(self.recorded_tracks) {
            self.latest_tracks
                .insert(driver_key(track.execution_object_id, track.property), index);
        }
        self.recorded_tracks = tracks.len();
        if !allow_sequential_capture {
            return;
        }
        let Some((start, _)) = intervals.get(&leaf.animation) else {
            return;
        };
        let object = leaf.execution_object_id;
        // A content morph changes the source geometry used by later lowering.
        // Keep that dependency reserved until shared content activation supports
        // the sequence; affine/style channels can capture their exact endpoints.
        if self
            .latest_tracks
            .contains_key(&driver_key(object, Property::Morph))
            || self
                .latest_tracks
                .contains_key(&driver_key(object, Property::Transform))
        {
            return;
        }
        for slot in 0..=driver_key(object, Property::Presence).1 {
            let key = (object.get(), slot);
            let Some(owner) = driven.get(&key).copied() else {
                continue;
            };
            let Some((_, end)) = intervals.get(&owner) else {
                continue;
            };
            if *end > *start {
                continue;
            }
            if let Some(index) = self.latest_tracks.get(&key) {
                let track = &tracks[*index];
                if track.animation == owner {
                    let alpha = track.timing.easing.evaluate(1.0);
                    if alpha != 0.0 && alpha != 1.0 {
                        continue;
                    }
                    self.completed
                        .insert(key, (track.property, track.values.clone(), alpha == 1.0));
                }
            }
            driven.remove(&key);
        }
    }
}

pub(super) fn known_intervals<'a>(
    leaves: impl Iterator<Item = &'a PreparedSemanticScheduledAnimationLeaf>,
) -> HashMap<SemanticTransactionNodeRef, (f64, f64)> {
    leaves
        .filter_map(|leaf| {
            continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .ok()
                .map(|interval| (leaf.animation, interval))
        })
        .collect()
}
