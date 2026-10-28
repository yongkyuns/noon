//! Bounded input-driven use of ordinary animation channels on a monotonic time domain.
//! Authored time and retained replay history are untouched. One driver per target;
//! completion releases it, and seeking reconstructs the ordinary authored frame.
use crate::{
    apply_evaluated_value, interpolate_track_values, EffectivePropertyWrite, EvaluationStats,
    FrameRowState, PreparedEffectivePropertyBatch, RuntimeIdentity, SceneInstance,
};
use noon_compile::{
    lower_indicate_channels_with_bindings, CompiledChannelKey, EffectiveAnimationProperties,
    LoweredAffineChannel,
};
use noon_core::{
    ObjectId, Property, PublicationContext, RateFunction, SemanticClickGlowPulse,
    SemanticClickIndicate, SemanticNodeId, SemanticVec3, Style, Transform2D,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct PreparedTransientAnimation {
    object: ObjectId,
    runtime: RuntimeIdentity,
    channels: Vec<LoweredAffineChannel>,
    transform: Transform2D,
    style: Style,
    duration: f64,
    publication: PublicationContext,
    authored_time: f64,
    origin: Option<f64>,
    glow: Option<TransientGlowPulse>,
    transient_revision: u64,
}

#[derive(Clone, Copy, Debug)]
struct TransientGlowPulse {
    attachment: SemanticNodeId,
    source: f64,
    target: f64,
    rate: RateFunction,
    reverse: bool,
    temporary: bool,
}

/// Exact semantic generations whose temporary lifetime ends in one prepared tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransientGlowRetirement {
    pub object: ObjectId,
    pub attachment: SemanticNodeId,
}

/// The ordinary transient clock's unpublished effective writes and lifetime suffix.
/// A session may combine these writes with exact semantic attachment retirement.
#[derive(Debug)]
pub struct PreparedInteractionAdvance {
    effective: PreparedEffectivePropertyBatch,
    metadata: PreparedInteractionAdvanceMetadata,
}

#[derive(Debug)]
pub struct PreparedInteractionAdvanceMetadata {
    runtime: RuntimeIdentity,
    publication: PublicationContext,
    authored_time: f64,
    wall_time: f64,
    completed: Vec<ObjectId>,
    stale: Vec<usize>,
    retirements: Vec<TransientGlowRetirement>,
    transient_rows: Vec<usize>,
    transient_revision: u64,
}

impl PreparedInteractionAdvance {
    pub fn retirements(&self) -> &[TransientGlowRetirement] {
        &self.metadata.retirements
    }

    pub fn into_publication_parts(
        self,
    ) -> (
        PreparedEffectivePropertyBatch,
        PreparedInteractionAdvanceMetadata,
    ) {
        (self.effective, self.metadata)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TransientAnimations {
    active: BTreeMap<ObjectId, PreparedTransientAnimation>,
    last_tick: Option<f64>,
    // Preparation validity for this existing owner, never an animation time or
    // semantic identity. Neutral invocations may change without a pixel epoch.
    revision: u64,
}

impl SceneInstance {
    /// Validate the deterministic seek before its scene owner retires any input
    /// attachment. Reserve both lifecycle and seek publication epochs up front.
    pub fn preflight_interaction_seek(&self, time: f64) -> Result<(), crate::EvaluationError> {
        if !time.is_finite() {
            return Err(crate::EvaluationError::InvalidTime(time));
        }
        self.validate_replay_time(time)?;
        if self.interactions_active() {
            self.preflight_transient_glow_edits(1)
                .map_err(|_| crate::EvaluationError::InteractionRevisionExhausted)?;
        }
        let mut epoch = self.publication.frame_epoch();
        let publications = if self.interactions_active() { 2 } else { 1 };
        for _ in 0..publications {
            epoch = epoch
                .checked_next()
                .ok_or(crate::EvaluationError::FrameEpochExhausted(epoch))?;
        }
        Ok(())
    }

    pub(crate) fn preflight_raw_interaction_seek(
        &self,
        time: f64,
    ) -> Result<(), crate::EvaluationError> {
        if self.has_temporary_glow_interactions() {
            return Err(crate::EvaluationError::InteractionRetirementRequired);
        }
        self.preflight_interaction_seek(time)
    }

    pub fn interactions_active(&self) -> bool {
        !self.transient_animations.active.is_empty()
    }

    /// Prepare before admitting the triggering input. Active targets ignore retriggers.
    pub fn prepare_click_indicate(
        &self,
        object: ObjectId,
        binding: SemanticClickIndicate,
    ) -> Result<Option<PreparedTransientAnimation>, String> {
        if self.transient_animations.active.contains_key(&object) {
            return Ok(None);
        }
        if !binding.is_valid() {
            return Err("invalid click Indicate options".into());
        }
        self.preflight_transient_glow_edits(1)?;
        let index = self
            .frame_index_for_object(object)
            .ok_or("interaction target is not live")?;
        let row = &self.frame.objects[index];
        let channels = lower_indicate_channels_with_bindings(
            &[],
            EffectiveAnimationProperties {
                fill_paint: row.fill_paint.clone(),
                effects: row.effects.clone(),
                z_index: row.z_index,
                transform: row.transform,
                style: row.style,
                appearance: row.appearance,
                reveal: self.frame.reveal(index),
                world_transform: row.world_transform(),
                camera_profile: row.camera_profile(),
            },
            binding.scale_factor(),
            binding.color(),
            SemanticVec3::new(
                row.transform.translation.x.into(),
                row.transform.translation.y.into(),
                0.0,
            ),
        )
        .map_err(|error| format!("invalid click Indicate: {error:?}"))?;
        if channels.is_empty()
            || self.interaction_channel_conflict(object, index, &channels, self.frame.time)
        {
            return Ok(None);
        }
        let mut endpoint = FrameRowState::from_frame(&self.frame, index);
        for channel in &channels {
            if !matches!(
                channel.property,
                noon_core::Property::Position
                    | noon_core::Property::Scale
                    | noon_core::Property::Fill
                    | noon_core::Property::Stroke
            ) {
                return Err("unsupported transient animation channel".into());
            }
            let value = interpolate_track_values(&channel.values, 1.0)
                .ok_or("invalid transient channel")?;
            apply_evaluated_value(
                &mut endpoint.as_mut(&row.content),
                channel.property,
                value,
                false,
            );
        }
        self.prepare_effective_property_batch(&[
            EffectivePropertyWrite::Transform {
                object,
                transform: endpoint.transform,
            },
            EffectivePropertyWrite::Style {
                object,
                style: endpoint.style,
            },
        ])
        .map_err(|error| error.to_string())?;
        Ok(Some(PreparedTransientAnimation {
            object,
            runtime: self.identity,
            channels,
            transform: row.transform,
            style: row.style,
            duration: binding.run_time(),
            publication: self.publication,
            authored_time: self.frame.time,
            origin: None,
            glow: None,
            transient_revision: self.transient_animations.revision,
        }))
    }

    /// Prepare the intensity lane against a current or transaction-local neutral
    /// attachment. The session owns semantic profile/enrollment validation.
    pub fn prepare_click_glow_pulse(
        &self,
        object: ObjectId,
        binding: SemanticClickGlowPulse,
        glow: noon_compile::CompiledEffect,
        temporary: bool,
        publication: PublicationContext,
    ) -> Result<Option<PreparedTransientAnimation>, String> {
        if self.transient_animations.active.contains_key(&object) {
            return Ok(None);
        }
        if !binding.is_valid() {
            return Err("invalid click GlowPulse options".into());
        }
        let definition = glow
            .definition
            .as_glow()
            .ok_or("GlowPulse requires a Glow attachment")?;
        self.preflight_transient_glow_edits(1)?;
        let expected = if temporary {
            PublicationContext::new(
                self.publication
                    .scene_revision()
                    .checked_next()
                    .ok_or("scene revision exhausted")?,
                self.publication
                    .execution_revision()
                    .checked_next()
                    .ok_or("execution revision exhausted")?,
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or("frame epoch exhausted")?,
            )
        } else {
            self.publication
        };
        if publication != expected {
            return Err("click glow preparation requires its exact publication".into());
        }
        let index = self
            .frame_index_for_object(object)
            .filter(|&index| self.object_slot_is_live(index))
            .ok_or("interaction target is not live")?;
        let row = &self.frame.objects[index];
        match row
            .effects
            .as_ref()
            .and_then(|stack| stack.get(glow.attachment))
        {
            Some(current) if current == glow => {}
            None if temporary && definition.intensity() == 0.0 => {}
            _ => return Err("foreign or stale click glow attachment".into()),
        }
        if self.interaction_properties_conflict(
            object,
            index,
            [Property::EffectIntensity],
            self.frame.time,
            Some(glow.attachment),
        ) {
            return Ok(None);
        }
        Ok(Some(PreparedTransientAnimation {
            object,
            runtime: self.identity,
            channels: Vec::new(),
            transform: row.transform,
            style: row.style,
            duration: binding.run_time(),
            publication,
            authored_time: self.frame.time,
            origin: None,
            glow: Some(TransientGlowPulse {
                attachment: glow.attachment,
                source: definition.intensity(),
                target: binding.intensity(),
                rate: binding.rate_func(),
                reverse: binding.reverse_rate_function(),
                temporary,
            }),
            transient_revision: self.transient_animations.revision,
        }))
    }

    /// Input admission prepared all fallible channel work before its native effects.
    pub fn start_transient_animation(
        &mut self,
        prepared: PreparedTransientAnimation,
    ) -> Result<(), String> {
        if prepared.runtime != self.identity
            || prepared.publication.scene_revision() != self.publication.scene_revision()
            || prepared.publication.execution_revision() != self.publication.execution_revision()
            || prepared.transient_revision != self.transient_animations.revision
            || prepared.authored_time != self.frame.time
            || !self
                .frame_index_for_object(prepared.object)
                .is_some_and(|index| {
                    let row = &self.frame.objects[index];
                    self.object_slot_is_live(index)
                        && if let Some(pulse) = prepared.glow {
                            row.effects
                                .as_ref()
                                .and_then(|stack| stack.get(pulse.attachment))
                                .is_some_and(|glow| {
                                    glow.definition.as_glow().is_some_and(|definition| {
                                        definition.intensity() == pulse.source
                                    })
                                })
                                && !self.interaction_properties_conflict(
                                    prepared.object,
                                    index,
                                    [Property::EffectIntensity],
                                    prepared.authored_time,
                                    Some(pulse.attachment),
                                )
                        } else {
                            prepared
                                .channels
                                .iter()
                                .all(|channel| match channel.property {
                                    Property::Position => {
                                        row.transform.translation == prepared.transform.translation
                                    }
                                    Property::Scale => {
                                        row.transform.scale == prepared.transform.scale
                                    }
                                    Property::Fill => row.style.fill == prepared.style.fill,
                                    Property::Stroke => row.style.stroke == prepared.style.stroke,
                                    _ => false,
                                })
                        }
                })
        {
            return Err("foreign or stale prepared click animation".into());
        }
        if !self
            .transient_animations
            .active
            .contains_key(&prepared.object)
        {
            let next = self
                .transient_animations
                .revision
                .checked_add(1)
                .ok_or("transient preparation revision exhausted")?;
            self.transient_animations
                .active
                .insert(prepared.object, prepared);
            self.transient_animations.revision = next;
        }
        Ok(())
    }

    /// Platform monotonic seconds, independent of paused/completed authored time.
    /// A standalone Runtime cannot retire semantic attachments; the session uses
    /// the prepared form to include those removals in the same publication.
    pub fn advance_interactions(&mut self, wall_time: f64) -> Result<(), String> {
        let prepared = self.prepare_interactions(wall_time)?;
        if !prepared.retirements().is_empty() {
            return Err("temporary glow retirement requires the semantic session".into());
        }
        self.commit_prepared_interactions(prepared)
    }

    pub fn prepare_interactions(
        &self,
        wall_time: f64,
    ) -> Result<PreparedInteractionAdvance, String> {
        self.prepare_interaction_advance(wall_time, false)
    }

    /// Prepare cancellation without inventing a second clock or changing the
    /// current invocation until its semantic/runtime publication succeeds.
    pub fn prepare_interaction_retirement(&self) -> Result<PreparedInteractionAdvance, String> {
        self.prepare_interaction_advance(self.transient_animations.last_tick.unwrap_or(0.0), true)
    }

    fn prepare_interaction_advance(
        &self,
        wall_time: f64,
        retire: bool,
    ) -> Result<PreparedInteractionAdvance, String> {
        if !wall_time.is_finite() || wall_time < 0.0 {
            return Err("interaction tick must be finite nonnegative monotonic seconds".into());
        }
        if self
            .transient_animations
            .last_tick
            .is_some_and(|last| wall_time < last)
        {
            return Err("interaction clock moved backwards".into());
        }
        if self.interactions_active() && self.transient_animations.revision.checked_add(1).is_none()
        {
            return Err("transient preparation revision exhausted".into());
        }
        let mut writes = Vec::with_capacity(self.transient_animations.active.len() * 4);
        let mut completed = Vec::new();
        let mut stale = Vec::new();
        let mut retirements = Vec::new();
        for (&object, driver) in &self.transient_animations.active {
            let index = self
                .frame_index_for_object(object)
                .filter(|&index| self.object_slot_is_live(index));
            if let Some(pulse) = driver.glow {
                let exact = index
                    .and_then(|index| self.frame.objects[index].effects.as_ref())
                    .is_some_and(|stack| stack.get(pulse.attachment).is_some());
                let conflict = index.is_some_and(|index| {
                    self.interaction_properties_conflict(
                        object,
                        index,
                        [Property::EffectIntensity],
                        driver.authored_time,
                        Some(pulse.attachment),
                    )
                });
                let elapsed = wall_time - driver.origin.unwrap_or(wall_time);
                let finished = retire || !exact || conflict || elapsed >= driver.duration;
                if finished {
                    completed.push(object);
                    if pulse.temporary {
                        retirements.push(TransientGlowRetirement {
                            object,
                            attachment: pulse.attachment,
                        });
                    }
                }
                // A superseding intensity owner supplies the new effective value.
                // Never restore a stale baseline over a conflicting driver.
                if !exact || conflict || (finished && pulse.temporary) {
                    continue;
                }
                let intensity = if finished {
                    pulse.source
                } else {
                    let mut timing = noon_core::TrackTiming::new(0.0, driver.duration, pulse.rate);
                    timing.reverse_rate_function = pulse.reverse;
                    let alpha = timing
                        .evaluate_progress_f64((elapsed / driver.duration).clamp(0.0, 1.0))
                        .clamp(0.0, 1.0);
                    noon_core::EffectTrackValue::Intensity(pulse.source)
                        .sample(noon_core::EffectTrackValue::Intensity(pulse.target), alpha)
                        .expect("validated pulse endpoints and bounded timing")
                        .intensity
                        .expect("intensity sampling owns one parameter")
                };
                writes.push(EffectivePropertyWrite::EffectIntensity {
                    object,
                    attachment: pulse.attachment,
                    intensity,
                });
                continue;
            }
            let Some(index) = index else {
                completed.push(object);
                continue;
            };
            if driver.publication.scene_revision() != self.publication.scene_revision()
                || driver.publication.execution_revision() != self.publication.execution_revision()
                || self.interaction_channel_conflict(
                    object,
                    index,
                    &driver.channels,
                    driver.authored_time,
                )
            {
                stale.push(index);
                completed.push(object);
                continue;
            }
            let elapsed = wall_time - driver.origin.unwrap_or(wall_time);
            let mut row = FrameRowState::from_frame(&self.frame, index);
            row.transform = driver.transform;
            row.style = driver.style;
            if !retire && elapsed < driver.duration {
                let progress =
                    RateFunction::ThereAndBack.evaluate((elapsed / driver.duration) as f32);
                for channel in &driver.channels {
                    let value = interpolate_track_values(&channel.values, progress)
                        .expect("prepared continuous channel");
                    apply_evaluated_value(
                        &mut row.as_mut(&self.frame.objects[index].content),
                        channel.property,
                        value,
                        false,
                    );
                }
            } else {
                completed.push(object);
            }
            for channel in &driver.channels {
                writes.push(match channel.property {
                    Property::Position => EffectivePropertyWrite::Translation {
                        object,
                        translation: row.transform.translation,
                    },
                    Property::Scale => EffectivePropertyWrite::Scale {
                        object,
                        scale: row.transform.scale,
                    },
                    Property::Fill => EffectivePropertyWrite::Fill {
                        object,
                        fill: row.style.fill,
                    },
                    Property::Stroke => EffectivePropertyWrite::Stroke {
                        object,
                        stroke: row.style.stroke,
                    },
                    _ => unreachable!("prepared supported interaction channel"),
                });
            }
        }
        if (!writes.is_empty() || !stale.is_empty())
            && self.publication.frame_epoch().checked_next().is_none()
        {
            return Err("interaction frame epoch exhausted".into());
        }
        let effective = self
            .prepare_effective_property_batch(&writes)
            .map_err(|error| error.to_string())?;
        let transient_rows = effective
            .writes
            .iter()
            .map(|(index, _)| *index)
            .filter(|index| !self.effective_driver_rows.contains(index))
            .collect();
        Ok(PreparedInteractionAdvance {
            effective,
            metadata: PreparedInteractionAdvanceMetadata {
                runtime: self.identity,
                publication: self.publication,
                authored_time: self.frame.time,
                wall_time,
                completed,
                stale,
                retirements,
                transient_rows,
                transient_revision: self.transient_animations.revision,
            },
        })
    }

    pub fn commit_prepared_interactions(
        &mut self,
        prepared: PreparedInteractionAdvance,
    ) -> Result<(), String> {
        self.preflight_interaction_advance(&prepared)?;
        if !prepared.retirements().is_empty() {
            return Err("temporary glow retirement requires a semantic publication".into());
        }
        let (effective, metadata) = prepared.into_publication_parts();
        self.commit_transient_effective_properties(effective)
            .map_err(|error| error.to_string())?;
        self.finish_interaction_advance(metadata, false);
        Ok(())
    }

    pub fn preflight_interaction_advance(
        &self,
        prepared: &PreparedInteractionAdvance,
    ) -> Result<(), String> {
        let metadata = &prepared.metadata;
        if metadata.runtime != self.identity
            || metadata.publication != self.publication
            || metadata.authored_time != self.frame.time
            || metadata.transient_revision != self.transient_animations.revision
        {
            return Err("foreign or stale prepared interaction advance".into());
        }
        Ok(())
    }

    /// The session has committed the prepared effective batch together with the
    /// exact temporary attachment removals. Only infallible invocation metadata
    /// remains; this does not acknowledge an input or create another publication.
    pub fn finish_interaction_advance_after_publication(
        &mut self,
        metadata: PreparedInteractionAdvanceMetadata,
    ) {
        self.finish_interaction_advance(metadata, true);
    }

    fn finish_interaction_advance(
        &mut self,
        metadata: PreparedInteractionAdvanceMetadata,
        published: bool,
    ) {
        assert_eq!(
            metadata.runtime, self.identity,
            "prepared interaction belongs to this Runtime"
        );
        assert_eq!(
            metadata.authored_time, self.frame.time,
            "interaction publication preserves authored time"
        );
        assert_eq!(
            metadata.transient_revision, self.transient_animations.revision,
            "preflighted invocation state remains current"
        );
        let was_active = self.interactions_active();
        let mut stale_changed = false;
        for index in metadata.stale {
            let before = FrameRowState::from_frame(&self.frame, index);
            self.relower_object(index, self.frame.time, &mut EvaluationStats::default());
            self.reapply_reactive_for_object(index);
            if before.differs_from_frame(&self.frame, index) {
                self.mark_changed(index);
                stale_changed = true;
            }
        }
        if stale_changed
            && (!published || self.publication.frame_epoch() == metadata.publication.frame_epoch())
        {
            self.publication = self.publication.with_frame_epoch(
                metadata
                    .publication
                    .frame_epoch()
                    .checked_next()
                    .expect("preflighted interaction epoch"),
            );
        }
        if published {
            // The ordinary combined publication carries these writes through its
            // host-effective path. Their owner remains the existing invocation,
            // not an unbounded host driver retained after the pulse settles.
            for index in metadata.transient_rows {
                self.effective_driver_rows.remove(&index);
            }
        }
        for object in metadata.completed {
            self.transient_animations.active.remove(&object);
        }
        for driver in self.transient_animations.active.values_mut() {
            driver.origin.get_or_insert(metadata.wall_time);
        }
        self.transient_animations.last_tick =
            self.interactions_active().then_some(metadata.wall_time);
        if was_active {
            self.transient_animations.revision = self
                .transient_animations
                .revision
                .checked_add(1)
                .expect("preflighted transient preparation revision");
        }
    }

    pub fn transient_glow_attachment(&self, object: ObjectId) -> Option<(SemanticNodeId, bool)> {
        self.transient_animations
            .active
            .get(&object)
            .and_then(|driver| driver.glow)
            .map(|pulse| (pulse.attachment, pulse.temporary))
    }

    /// Read-only admission before a session stages an absent attachment.
    pub fn can_prepare_click_glow_pulse(&self, object: ObjectId) -> bool {
        !self.transient_animations.active.contains_key(&object)
            && self.frame_index_for_object(object).is_some_and(|index| {
                self.object_slot_is_live(index)
                    && !self.interaction_properties_conflict(
                        object,
                        index,
                        [Property::EffectIntensity],
                        self.frame.time,
                        None,
                    )
            })
    }

    /// Reserve invocation-state validity before a persistent commit can adopt or
    /// supersede input-owned lanes. This counter is independent of pixel changes.
    pub fn preflight_transient_glow_edits(&self, count: usize) -> Result<(), String> {
        let count = u64::try_from(count).map_err(|_| "too many transient ownership edits")?;
        self.transient_animations
            .revision
            .checked_add(count)
            .ok_or_else(|| "transient preparation revision exhausted".to_string())?;
        Ok(())
    }

    pub fn has_temporary_glow_interactions(&self) -> bool {
        self.transient_animations
            .active
            .values()
            .any(|driver| driver.glow.is_some_and(|pulse| pulse.temporary))
    }

    /// Commit parameter ownership only after the matching persistent publication.
    pub fn commit_transient_glow_edit(
        &mut self,
        object: ObjectId,
        attachment: SemanticNodeId,
        adopt: bool,
        supersede: bool,
    ) {
        let Some(driver) = self.transient_animations.active.get_mut(&object) else {
            return;
        };
        let Some(pulse) = driver
            .glow
            .as_mut()
            .filter(|pulse| pulse.attachment == attachment)
        else {
            return;
        };
        if adopt {
            pulse.temporary = false;
        }
        if supersede {
            self.transient_animations.active.remove(&object);
        }
        self.transient_animations.revision = self
            .transient_animations
            .revision
            .checked_add(1)
            .expect("preflighted transient preparation revision");
        if !self.interactions_active() {
            self.transient_animations.last_tick = None;
        }
    }

    /// Remove only the transient carry introduced by a prepared semantic edit.
    pub fn release_transient_effective_carry(&mut self, objects: &[ObjectId]) {
        for &object in objects {
            if let Some(index) = self.frame_index_for_object(object) {
                self.effective_driver_rows.remove(&index);
            }
        }
    }

    /// Authored/native drivers retain precedence over an input effect. Disjoint
    /// timeline channels keep running. A newly active conflicting channel cancels
    /// the effect on its next interaction tick instead of restoring its old value.
    fn interaction_channel_conflict(
        &self,
        object: ObjectId,
        index: usize,
        channels: &[LoweredAffineChannel],
        since: f64,
    ) -> bool {
        self.interaction_properties_conflict(
            object,
            index,
            channels.iter().map(|channel| channel.property),
            since,
            None,
        )
    }

    fn interaction_properties_conflict(
        &self,
        object: ObjectId,
        index: usize,
        properties: impl IntoIterator<Item = Property>,
        since: f64,
        attachment: Option<SemanticNodeId>,
    ) -> bool {
        if self.effective_driver_rows.contains(&index)
            || !self.active_family_animation_indices.is_empty()
            || (since != self.frame.time && !self.compiled.family_animations().is_empty())
        {
            // Compact family display domains may transform member coordinates;
            // until their ownership is indexed per member, defer during that domain.
            return true;
        }
        [Property::Transform, Property::Morph, Property::Presence]
            .into_iter()
            .chain(properties)
            .any(|property| {
                let channel = match property {
                    Property::EffectColor
                    | Property::EffectRadius
                    | Property::EffectIntensity
                    | Property::EffectOffset
                    | Property::EffectProgress
                    | Property::EffectSoftness
                    | Property::EffectSize
                    | Property::EffectSeed => {
                        let Some(attachment) = attachment else {
                            return false;
                        };
                        CompiledChannelKey::effect(index as u32, property, attachment)
                    }
                    _ => CompiledChannelKey::new(index as u32, property),
                };
                self.timeline_scheduler
                    .channel_changed_since(channel, since)
                    || self
                        .reactive
                        .as_ref()
                        .is_some_and(|reactive| reactive.owns_property(object, property))
            })
    }

    pub(crate) fn clear_transient_animations(&mut self) {
        if self.interactions_active() {
            self.transient_animations.revision = self
                .transient_animations
                .revision
                .checked_add(1)
                .expect("transient preparation revision exhausted");
        }
        self.transient_animations.active.clear();
        self.transient_animations.last_tick = None;
    }

    /// Exact glow attachment maintenance cannot supersede an Indicate channel.
    /// Only the validated lifecycle commit may advance this existing proof.
    pub(crate) fn rebase_transient_indicate_publication(&mut self) {
        for driver in self.transient_animations.active.values_mut() {
            if driver.glow.is_none() {
                driver.publication = self.publication;
            }
        }
    }
}

impl TransientAnimations {
    /// Whether a live transient effect owns one exact target/property lane.
    /// This intentionally does not make an effect on another object a global
    /// barrier to a drag.
    pub(crate) fn owns_property(&self, object: ObjectId, property: Property) -> bool {
        self.active.get(&object).is_some_and(|effect| {
            (effect.glow.is_some() && property == Property::EffectIntensity)
                || effect
                    .channels
                    .iter()
                    .any(|channel| channel.property == property)
        })
    }
}

#[cfg(test)]
mod tests;
