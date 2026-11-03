//! Experimental sparse lifecycle/content prefix transfer. Uses the existing compiler
//! revision representation; no second scene, evaluator, or resource registry.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use crate::{CompiledEffectStack, CompiledObject};
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
            | Property::EffectProgress
            | Property::EffectSoftness
            | Property::EffectSeed
    )
}

/// The bounded built-in history profile: optional Blur, then optional Glow,
/// then optional stack-owned visibility NoiseDissolve. Arbitrary stacks and
/// shader programs remain outside this profile.
pub(super) fn local_effect_stack(stack: &CompiledEffectStack) -> bool {
    use noon_core::EffectDefinition::{Blur, Glow, NoiseDissolve};

    match (stack.definitions(), stack.visibility()) {
        ([], None) => true,
        ([Blur(_)], None) => true,
        ([Glow(_)], None) => true,
        ([Blur(_), Glow(_)], None) => true,
        ([NoiseDissolve(_)], Some(visibility))
        | ([Blur(_), NoiseDissolve(_)], Some(visibility))
        | ([Glow(_), NoiseDissolve(_)], Some(visibility))
        | ([Blur(_), Glow(_), NoiseDissolve(_)], Some(visibility)) => {
            stack.attachments().last() == Some(&visibility.attachment)
        }
        _ => false,
    }
}

fn local_track(track: Option<&CompiledTrack>) -> bool {
    track.is_none_or(|track| {
        local_property(track.property) && super::path_motion_command_count(&track.values) == 0
    })
}

fn local_effect_track(track: &CompiledTrack, object: Option<&CompiledObject>) -> bool {
    use noon_core::{EffectTrackValue, TrackValues};

    let property = track.property;
    if !matches!(
        property,
        Property::EffectColor
            | Property::EffectRadius
            | Property::EffectIntensity
            | Property::EffectProgress
            | Property::EffectSoftness
            | Property::EffectSeed
    ) {
        return true;
    }
    let Some(object) = object else {
        return false;
    };
    let Some(stack) = object.effects.as_deref() else {
        return false;
    };
    if !local_effect_stack(stack) {
        return false;
    }
    let TrackValues::Effect {
        attachment,
        from,
        to,
        ..
    } = &track.values
    else {
        return false;
    };
    if from.property() != property || to.property() != property {
        return false;
    }
    let Some(effect) = stack.get(*attachment) else {
        return false;
    };
    match (property, from, to) {
        (Property::EffectColor, EffectTrackValue::Color(_), EffectTrackValue::Color(_)) => {
            effect.definition.as_glow().is_some()
        }
        (Property::EffectRadius, EffectTrackValue::Radius(_), EffectTrackValue::Radius(_)) => {
            effect.definition.as_blur().is_some() || effect.definition.as_glow().is_some()
        }
        (
            Property::EffectIntensity,
            EffectTrackValue::Intensity(_),
            EffectTrackValue::Intensity(_),
        ) => effect.definition.as_glow().is_some(),
        (
            Property::EffectProgress | Property::EffectSoftness,
            EffectTrackValue::Progress(_) | EffectTrackValue::Softness(_),
            EffectTrackValue::Progress(_) | EffectTrackValue::Softness(_),
        ) => {
            stack
                .visibility()
                .is_some_and(|visibility| visibility.attachment == *attachment)
                && effect.definition.as_noise_dissolve().is_some()
        }
        (Property::EffectSeed, EffectTrackValue::Seed(_), EffectTrackValue::Seed(_)) => {
            stack
                .visibility()
                .is_some_and(|visibility| visibility.attachment == *attachment)
                && effect.definition.as_noise_dissolve().is_some()
        }
        _ => false,
    }
}

fn local_object(object: &CompiledObject, physical: &CompiledObject) -> bool {
    object.live
        && object.id == physical.id
        && object.spatial.is_none()
        && object
            .effects
            .as_ref()
            .is_none_or(|stack| local_effect_stack(stack))
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
        old.tracks.iter().all(|(id, before)| {
            let after = self.track(*id);
            if !local_track(before.as_ref()) || !local_track(after) {
                return false;
            }
            let Some(index) = before
                .as_ref()
                .or(after)
                .map(|track| track.object_index as usize)
            else {
                return true;
            };
            let physical = self.objects.get(index).filter(|object| object.live);
            let before_object = old
                .rows
                .iter()
                .find(|row| row.index as usize == index)
                .map_or(physical, |row| row.object.as_ref());
            before
                .as_ref()
                .is_none_or(|track| local_effect_track(track, before_object))
                && after.is_none_or(|track| local_effect_track(track, physical))
        })
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
    /// Rows may carry the bounded ordered Blur/Glow profile with optional
    /// trailing visibility NoiseDissolve and its typed tracks. Other effect
    /// definitions remain outside this suffix profile.
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
            let mut after_objects = BTreeMap::new();
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
                after_objects.insert(saved.index, current.clone());
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
                if let Some(index) = before
                    .as_ref()
                    .or(after.as_ref())
                    .map(|track| track.object_index)
                {
                    let physical = self
                        .objects
                        .get(index as usize)
                        .filter(|object| object.live);
                    let before_object = row_overlay.get(&index).map_or(physical, Option::as_ref);
                    let after_object = after_objects
                        .get(&index)
                        .map_or(before_object, Option::as_ref);
                    if before
                        .as_ref()
                        .is_some_and(|track| !local_effect_track(track, before_object))
                        || after
                            .as_ref()
                            .is_some_and(|track| !local_effect_track(track, after_object))
                    {
                        return None;
                    }
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
                effect_payload_bytes: old.effect_payload_bytes,
                composed_payload_bytes: old.composed_payload_bytes,
            });
        }
        output.reverse();
        Some(output)
    }
}

#[cfg(test)]
mod effect_profile_tests {
    use super::*;
    use crate::{CompiledEffect, CompiledObject};
    use noon_core::{
        Blur, CompositionTimeMap, EffectDefinition, EffectTrackEndBehavior, EffectTrackValue,
        GeometryRef, Glow, NoiseDissolve, ObjectId, RateFunction, SemanticNodeId, Style,
        TrackTiming, TrackValues, Transform2D,
    };
    use std::sync::Arc;

    fn effect_track(
        property: Property,
        attachment: SemanticNodeId,
        from: EffectTrackValue,
        to: EffectTrackValue,
    ) -> CompiledTrack {
        CompiledTrack {
            id: TrackId::new(0),
            object_index: 0,
            property,
            values: TrackValues::Effect {
                attachment,
                from,
                to,
                end_behavior: EffectTrackEndBehavior::Sample,
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
            transform_geometry_plan: None,
            reconciled: false,
        }
    }

    #[test]
    fn effect_tracks_are_limited_to_the_matching_profile_attachment() {
        let blur = SemanticNodeId::new(7, 1);
        let glow = SemanticNodeId::new(8, 1);
        let visibility = SemanticNodeId::new(9, 1);
        let mut object = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        object.effects = Some(Arc::new(
            CompiledEffectStack::new_with_visibility(
                [
                    CompiledEffect {
                        attachment: blur,
                        definition: EffectDefinition::Blur(Blur::default()),
                    },
                    CompiledEffect {
                        attachment: glow,
                        definition: EffectDefinition::Glow(Glow::default()),
                    },
                    CompiledEffect {
                        attachment: visibility,
                        definition: EffectDefinition::NoiseDissolve(NoiseDissolve::default()),
                    },
                ],
                Some(visibility),
            )
            .unwrap(),
        ));

        assert!(local_effect_track(
            &effect_track(
                Property::EffectRadius,
                blur,
                EffectTrackValue::Radius(Blur::default().radius()),
                EffectTrackValue::Radius(Blur::default().radius()),
            ),
            Some(&object),
        ));
        assert!(local_effect_track(
            &effect_track(
                Property::EffectIntensity,
                glow,
                EffectTrackValue::Intensity(0.0),
                EffectTrackValue::Intensity(1.0),
            ),
            Some(&object),
        ));
        assert!(local_effect_track(
            &effect_track(
                Property::EffectProgress,
                visibility,
                EffectTrackValue::Progress(0.0),
                EffectTrackValue::Progress(1.0),
            ),
            Some(&object),
        ));
        assert!(!local_effect_track(
            &effect_track(
                Property::EffectIntensity,
                blur,
                EffectTrackValue::Intensity(0.0),
                EffectTrackValue::Intensity(1.0),
            ),
            Some(&object),
        ));
        assert!(!local_effect_track(
            &effect_track(
                Property::EffectProgress,
                glow,
                EffectTrackValue::Progress(0.0),
                EffectTrackValue::Progress(1.0),
            ),
            Some(&object),
        ));
    }
}
