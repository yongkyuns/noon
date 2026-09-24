//! Runtime-owned finite, restoring property drivers. Animation meaning and track
//! construction remain in the semantic/compiler layers; this module reuses the
//! ordinary timing, interpolation and effective-publication path.

use std::collections::BTreeMap;

use noon_compile::{CompilePatchError, CompiledChannelKey, ExecutionPatch};
use noon_core::{
    mapped_continuous_progress, resolve_track_timing, validate_track_definition, ObjectId,
    Property, TrackDefinition,
};

use crate::{
    frame_row_mut, interpolate_track_values, EffectivePropertyWrite, EvaluatedValue,
    EvaluationError, FrameState, PreparedFrameCommitError, RuntimeIdentity, SceneInstance,
};

/// An operation identity, not semantic identity. A cloned/replaced runtime cannot
/// use a token issued by its predecessor, even when the source scene is identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyAnimationToken {
    runtime: RuntimeIdentity,
    sequence: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyAnimationError {
    ReplaySealed,
    Empty,
    InvalidTrack(noon_core::TimelineError),
    UnsupportedProperty(Property),
    MultipleObjects,
    UnsupportedContent(ObjectId),
    DuplicateChannel(Property),
    InvalidDuration,
    UnknownObject(ObjectId),
    TargetNotPresent(ObjectId),
    TargetHasRenderOverride(ObjectId),
    ChannelBusy {
        object: ObjectId,
        property: Property,
    },
    InitialValueMismatch(Property),
    InvalidDelta(f64),
    IdentityExhausted,
    ForeignToken,
    UnknownToken,
    Evaluation(EvaluationError),
    Commit(PreparedFrameCommitError),
    Validation(CompilePatchError),
}

impl std::fmt::Display for PropertyAnimationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "restoring property animation: {self:?}")
    }
}
impl std::error::Error for PropertyAnimationError {}

#[derive(Clone, Debug)]
struct Channel {
    track: TrackDefinition,
    restore: EffectivePropertyWrite,
}

#[derive(Clone, Debug)]
struct Active {
    object: ObjectId,
    object_index: usize,
    elapsed: f64,
    duration: f64,
    channels: Vec<Channel>,
}

#[derive(Clone, Debug)]
pub(crate) struct PropertyAnimations {
    next_sequence: Option<u64>,
    active: BTreeMap<u64, Active>,
    owners: BTreeMap<CompiledChannelKey, u64>,
}

impl Default for PropertyAnimations {
    fn default() -> Self {
        Self {
            next_sequence: Some(0),
            active: BTreeMap::new(),
            owners: BTreeMap::new(),
        }
    }
}

/// A sparse sample prepared against the same publication as its ordinary frame.
/// Elapsed time and retirement change only when that frame commits successfully.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreparedPropertyAnimations {
    pub writes: Vec<(usize, EffectivePropertyWrite)>,
    pub elapsed: Vec<(u64, f64)>,
    pub finished: Vec<u64>,
    pub clock_changed: bool,
}

impl PropertyAnimations {
    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    fn remove(&mut self, sequence: u64) -> Option<Active> {
        let active = self.active.remove(&sequence)?;
        for channel in &active.channels {
            self.owners.remove(&CompiledChannelKey::new(
                active.object_index as u32,
                channel.track.property,
            ));
        }
        Some(active)
    }

    pub(crate) fn clear(&mut self) {
        self.active.clear();
        self.owners.clear();
        // Never recycle an operation identity after seek/cancellation.
    }

    pub(crate) fn commit(&mut self, prepared: &PreparedPropertyAnimations) {
        for &(sequence, elapsed) in &prepared.elapsed {
            self.active
                .get_mut(&sequence)
                .expect("publication pins active driver")
                .elapsed = elapsed;
        }
        for &sequence in &prepared.finished {
            self.remove(sequence);
        }
    }
}

impl SceneInstance {
    /// Activate already-lowered continuous property tracks without extending or
    /// advancing authored time. Each operation targets one live object. Disjoint
    /// channels/objects may coexist; a competing claim is rejected, never stacked.
    ///
    /// This first policy reserves claimed channels against ordinary timeline and
    /// reactive drivers, including future tracks. Editing the target retires the
    /// operation before the new authored value is applied. Removal/seek retire it
    /// as well. Sealed replay remains read-only; no history is silently discarded.
    pub fn start_restoring_property_animation(
        &mut self,
        tracks: &[TrackDefinition],
    ) -> Result<PropertyAnimationToken, PropertyAnimationError> {
        use PropertyAnimationError as E;
        if self.replay_is_sealed() {
            return Err(E::ReplaySealed);
        }
        let object = tracks.first().ok_or(E::Empty)?.object;
        let index = self
            .frame_index_for_object(object)
            .ok_or(E::UnknownObject(object))?;
        if self.frame.objects[index].content.geometry().is_none() {
            return Err(E::UnsupportedContent(object));
        }
        if !self.frame.presences[index] {
            return Err(E::TargetNotPresent(object));
        }
        if self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some()
        {
            return Err(E::TargetHasRenderOverride(object));
        }
        let mut channels = BTreeMap::new();
        let mut duration = 0.0_f64;
        for source in tracks {
            validate_track_definition(source).map_err(E::InvalidTrack)?;
            if source.object != object {
                return Err(E::MultipleObjects);
            }
            let mut track = source.clone();
            track.timing = resolve_track_timing(source).map_err(E::InvalidTrack)?;
            let property = track.property;
            let restore = self
                .property_write_at(index, property)
                .ok_or(E::UnsupportedProperty(property))?;
            if track.timing.start_time < 0.0 || track.timing.duration <= 0.0 {
                return Err(E::InvalidDuration);
            }
            let end = track.timing.start_time + track.timing.duration;
            if !end.is_finite() || end <= track.timing.start_time {
                return Err(E::InvalidDuration);
            }
            duration = duration.max(end);
            if channels.contains_key(&CompiledChannelKey::new(index as u32, property)) {
                return Err(E::DuplicateChannel(property));
            }
            // Transform/morph/reveal can own the affine render frame, and a
            // presence track can retire the target, even without a component track.
            for dependency in [
                property,
                Property::Transform,
                Property::Morph,
                Property::Reveal,
                Property::Presence,
            ] {
                let key = CompiledChannelKey::new(index as u32, dependency);
                let timeline_busy = self.compiled.channel_tracks(key).iter().any(|track| {
                    !track.reconciled
                        || self.frame.time < track.timing.start_time + track.timing.duration
                });
                if self.property_animations.owners.contains_key(&key)
                    || timeline_busy
                    || self
                        .reactive
                        .as_ref()
                        .is_some_and(|runtime| runtime.owns_property(object, dependency))
                {
                    return Err(E::ChannelBusy { object, property });
                }
            }
            if self.object_has_effective_driver(object) {
                return Err(E::ChannelBusy { object, property });
            }
            if mapped_continuous_progress(track.timing, &track.time_map, 0.0)
                .and_then(|alpha| interpolate_track_values(&track.values, alpha))
                .and_then(|value| property_write(object, property, value))
                .is_some_and(|initial| initial != restore)
            {
                return Err(E::InitialValueMismatch(property));
            }
            channels.insert(
                CompiledChannelKey::new(index as u32, property),
                Channel { track, restore },
            );
        }
        let sequence = self
            .property_animations
            .next_sequence
            .ok_or(E::IdentityExhausted)?;
        let next_epoch = self
            .publication
            .frame_epoch()
            .checked_next()
            .ok_or(E::Evaluation(EvaluationError::FrameEpochExhausted(
                self.publication.frame_epoch(),
            )))?;
        // No semantic mutation, track append, object clone or scene-wide traversal.
        self.property_animations.next_sequence = sequence.checked_add(1);
        for &key in channels.keys() {
            self.property_animations.owners.insert(key, sequence);
        }
        self.property_animations.active.insert(
            sequence,
            Active {
                object,
                object_index: index,
                elapsed: 0.0,
                duration,
                channels: channels.into_values().collect(),
            },
        );
        self.invalidate_replay_input();
        // Activation invalidates outstanding prepared work even if alpha=0 has no
        // pixel difference. Render dirtiness and clock demand remain independent.
        self.publication = self.publication.with_frame_epoch(next_epoch);
        Ok(PropertyAnimationToken {
            runtime: self.identity,
            sequence,
        })
    }

    pub fn property_animation_elapsed(&self, token: PropertyAnimationToken) -> Option<f64> {
        (token.runtime == self.identity)
            .then(|| self.property_animations.active.get(&token.sequence))
            .flatten()
            .map(|active| active.elapsed)
    }

    pub fn has_property_animations(&self) -> bool {
        !self.property_animations.is_empty()
    }

    pub(crate) fn advance_property_animation_frame(
        &mut self,
        time: f64,
    ) -> Result<&FrameState, EvaluationError> {
        let prepared = self.prepare_advance_to(time)?;
        let effective = self
            .prepare_effective_property_batch(&[])
            .map_err(EvaluationError::InvalidEffectiveWrite)?;
        self.commit_prepared_frame(prepared, effective)
            .map_err(EvaluationError::PreparedCommit)
    }

    /// The host supplies elapsed time, but the runtime owns accumulation, sample
    /// timing, retirement and publication. Authored time does not move here.
    pub fn advance_property_animations_by(
        &mut self,
        delta: f64,
    ) -> Result<&FrameState, PropertyAnimationError> {
        use PropertyAnimationError as E;
        if !delta.is_finite() || delta < 0.0 {
            return Err(E::InvalidDelta(delta));
        }
        if self.property_animations.is_empty() || delta == 0.0 {
            return Ok(&self.frame);
        }
        let sample = self
            .prepare_property_animations(delta)
            .map_err(E::Evaluation)?;
        let mut prepared = self
            .prepare_current_effective_frame()
            .map_err(E::Evaluation)?;
        prepared.property_animations = sample;
        let effective = self
            .prepare_effective_property_batch(&[])
            .map_err(E::Validation)?;
        self.commit_prepared_frame(prepared, effective)
            .map_err(E::Commit)
    }

    pub fn cancel_property_animation(
        &mut self,
        token: PropertyAnimationToken,
    ) -> Result<&FrameState, PropertyAnimationError> {
        use PropertyAnimationError as E;
        if token.runtime != self.identity {
            return Err(E::ForeignToken);
        }
        let active = self
            .property_animations
            .active
            .get(&token.sequence)
            .ok_or(E::UnknownToken)?;
        let index = active.object_index;
        let restores: Vec<_> = active
            .channels
            .iter()
            .map(|channel| (channel.track.property, channel.restore))
            .collect();
        let mut prepared = self
            .prepare_current_effective_frame()
            .map_err(E::Evaluation)?;
        // Replace only this operation's sample with its exact scoped release.
        for (property, restore) in restores {
            let write = prepared
                .property_animations
                .writes
                .iter_mut()
                .find(|(row, write)| *row == index && write_properties(*write).contains(&property))
                .expect("active operation sampled during preparation");
            write.1 = restore;
        }
        prepared.property_animations.finished.push(token.sequence);
        prepared.property_animations.clock_changed = true;
        let effective = self
            .prepare_effective_property_batch(&[])
            .map_err(E::Validation)?;
        self.commit_prepared_frame(prepared, effective)
            .map_err(E::Commit)
    }

    pub(crate) fn prepare_property_animations(
        &self,
        delta: f64,
    ) -> Result<PreparedPropertyAnimations, EvaluationError> {
        let mut prepared = PreparedPropertyAnimations::default();
        let mut writes = Vec::new();
        for (&sequence, active) in &self.property_animations.active {
            // Clamp each bounded operation before adding; enormous valid deltas
            // finish it rather than overflowing a global accumulated clock.
            let elapsed = active.elapsed + delta.min(active.duration - active.elapsed);
            prepared.clock_changed |= elapsed != active.elapsed;
            prepared.elapsed.push((sequence, elapsed));
            let finished = elapsed >= active.duration;
            if finished {
                prepared.finished.push(sequence);
            }
            for channel in &active.channels {
                let value = if finished {
                    channel.restore
                } else {
                    mapped_continuous_progress(
                        channel.track.timing,
                        &channel.track.time_map,
                        elapsed,
                    )
                    .and_then(|alpha| interpolate_track_values(&channel.track.values, alpha))
                    .and_then(|value| property_write(active.object, channel.track.property, value))
                    .unwrap_or(channel.restore)
                };
                writes.push(value);
            }
        }
        // Numeric validation remains the normal compiler-backed effective path.
        prepared.writes = self
            .prepare_effective_property_batch(&writes)
            .map_err(EvaluationError::InvalidEffectiveWrite)?
            .writes;
        Ok(prepared)
    }

    pub(crate) fn check_property_animation_writes(
        &self,
        writes: &[(usize, EffectivePropertyWrite)],
    ) -> Result<(), PreparedFrameCommitError> {
        for &(index, write) in writes {
            for &property in write_properties(write) {
                if self
                    .property_animations
                    .owners
                    .contains_key(&CompiledChannelKey::new(index as u32, property))
                {
                    return Err(PreparedFrameCommitError::PropertyAnimationConflict {
                        object: write.object(),
                        property,
                    });
                }
            }
        }
        Ok(())
    }

    /// Called only after complete validation and before an authored target edit.
    /// The entire affected operation retires atomically; a later tick cannot write
    /// its old snapshot over new content, or resurrect a removed/reused identity.
    pub(crate) fn retire_property_animations_for_patch(&mut self, patch: &ExecutionPatch) {
        let object = match patch {
            ExecutionPatch::SetContent { object, .. }
            | ExecutionPatch::SetTransform { object, .. }
            | ExecutionPatch::SetStyle { object, .. }
            | ExecutionPatch::RemoveObject(object)
            | ExecutionPatch::ReconcileTrack { object, .. } => Some(*object),
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                Some(track.object)
            }
            ExecutionPatch::RemoveTrack(id) => self.compiled.track_object(*id),
            ExecutionPatch::AddFamilyAnimation(animation) => Some(animation.target),
            _ => None,
        };
        let Some(index) = object.and_then(|id| self.frame_index_for_object(id)) else {
            return;
        };
        let keys: Vec<_> = [
            Property::Position,
            Property::Rotation,
            Property::Scale,
            Property::Fill,
            Property::Stroke,
            Property::StrokeWidth,
            Property::Opacity,
        ]
        .into_iter()
        .filter_map(|property| {
            self.property_animations
                .owners
                .get(&CompiledChannelKey::new(index as u32, property))
                .copied()
        })
        .collect();
        for sequence in keys {
            let Some(active) = self.property_animations.remove(sequence) else {
                continue;
            };
            for channel in active.channels {
                crate::apply_effective_property_to_row(
                    frame_row_mut(&mut self.frame, index),
                    channel.restore,
                );
            }
            self.mark_changed(index);
        }
    }

    fn property_write_at(
        &self,
        index: usize,
        property: Property,
    ) -> Option<EffectivePropertyWrite> {
        let object = &self.frame.objects[index];
        let value = match property {
            Property::Position => EvaluatedValue::Vec2(object.transform.translation),
            Property::Rotation => EvaluatedValue::Scalar(object.transform.rotation),
            Property::Scale => EvaluatedValue::Vec2(object.transform.scale),
            Property::Fill => EvaluatedValue::Color(object.style.fill),
            Property::Stroke => EvaluatedValue::Color(object.style.stroke),
            Property::StrokeWidth => EvaluatedValue::Scalar(object.style.stroke_width),
            Property::Opacity => EvaluatedValue::Scalar(object.style.opacity),
            _ => return None,
        };
        property_write(object.id, property, value)
    }
}

fn property_write(
    object: ObjectId,
    property: Property,
    value: EvaluatedValue,
) -> Option<EffectivePropertyWrite> {
    use EffectivePropertyWrite as W;
    Some(match (property, value) {
        (Property::Position, EvaluatedValue::Vec2(translation)) => W::Translation {
            object,
            translation,
        },
        (Property::Rotation, EvaluatedValue::Scalar(rotation)) => W::Rotation { object, rotation },
        (Property::Scale, EvaluatedValue::Vec2(scale)) => W::Scale { object, scale },
        (Property::Fill, EvaluatedValue::Color(fill)) => W::Fill { object, fill },
        (Property::Stroke, EvaluatedValue::Color(stroke)) => W::Stroke { object, stroke },
        (Property::StrokeWidth, EvaluatedValue::Scalar(stroke_width)) => W::StrokeWidth {
            object,
            stroke_width,
        },
        (Property::Opacity, EvaluatedValue::Scalar(opacity)) => W::Opacity { object, opacity },
        _ => return None,
    })
}

fn write_properties(write: EffectivePropertyWrite) -> &'static [Property] {
    use EffectivePropertyWrite as W;
    use Property as P;
    match write {
        W::Transform { .. } => &[P::Position, P::Rotation, P::Scale],
        W::Style { .. } => &[P::Fill, P::Stroke, P::StrokeWidth, P::Opacity],
        W::Translation { .. } => &[P::Position],
        W::Rotation { .. } => &[P::Rotation],
        W::Scale { .. } => &[P::Scale],
        W::Fill { .. } => &[P::Fill],
        W::Stroke { .. } => &[P::Stroke],
        W::StrokeWidth { .. } => &[P::StrokeWidth],
        W::Opacity { .. } => &[P::Opacity],
    }
}

#[cfg(test)]
mod tests;
