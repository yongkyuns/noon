//! Bounded input-driven use of ordinary animation channels on a monotonic time domain.
//! Authored time and retained replay history are untouched. One driver per target;
//! completion releases it, and seeking reconstructs the ordinary authored frame.
use crate::{
    apply_evaluated_value, interpolate_track_values, EffectivePropertyWrite, EvaluationStats,
    FrameRowState, RuntimeIdentity, SceneInstance,
};
use noon_compile::{
    lower_indicate_channels_with_bindings, EffectiveAnimationProperties, LoweredAffineChannel,
};
use noon_core::{
    ObjectId, PublicationContext, RateFunction, SemanticClickIndicate, SemanticVec3, Style,
    Transform2D,
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
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TransientAnimations {
    active: BTreeMap<ObjectId, PreparedTransientAnimation>,
    last_tick: Option<f64>,
}

impl SceneInstance {
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
        let index = self
            .frame_index_for_object(object)
            .ok_or("interaction target is not live")?;
        let row = &self.frame.objects[index];
        let channels = lower_indicate_channels_with_bindings(
            &[],
            EffectiveAnimationProperties {
                z_index: row.z_index,
                transform: row.transform,
                style: row.style,
                appearance: row.appearance,
                reveal: self.frame.reveal(index),
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
            || prepared.authored_time != self.frame.time
            || !self
                .frame_index_for_object(prepared.object)
                .is_some_and(|index| {
                    self.object_slot_is_live(index)
                        && self.frame.objects[index].transform == prepared.transform
                        && self.frame.objects[index].style == prepared.style
                })
        {
            return Err("foreign or stale prepared click animation".into());
        }
        self.transient_animations
            .active
            .entry(prepared.object)
            .or_insert(prepared);
        Ok(())
    }

    /// Platform monotonic seconds, independent of paused/completed authored time.
    pub fn advance_interactions(&mut self, wall_time: f64) -> Result<(), String> {
        if !wall_time.is_finite() || wall_time < 0.0 {
            return Err("interaction tick must be finite nonnegative monotonic seconds".into());
        }
        if !self.interactions_active() {
            self.transient_animations.last_tick = None;
            return Ok(());
        }
        if self
            .transient_animations
            .last_tick
            .is_some_and(|last| wall_time < last)
        {
            return Err("interaction clock moved backwards".into());
        }
        let mut writes = Vec::with_capacity(self.transient_animations.active.len() * 2);
        let mut completed = Vec::new();
        let mut stale = Vec::new();
        for (&object, driver) in &self.transient_animations.active {
            let Some(index) = self
                .frame_index_for_object(object)
                .filter(|&i| self.object_slot_is_live(i))
            else {
                completed.push(object);
                continue;
            };
            if driver.publication.scene_revision() != self.publication.scene_revision()
                || driver.publication.execution_revision() != self.publication.execution_revision()
                || driver.authored_time != self.frame.time
            {
                stale.push(index);
                completed.push(object);
                continue;
            }
            let elapsed = wall_time - driver.origin.unwrap_or(wall_time);
            let mut row = FrameRowState::from_frame(&self.frame, index);
            row.transform = driver.transform;
            row.style = driver.style;
            if elapsed < driver.duration {
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
            writes.push(EffectivePropertyWrite::Transform {
                object,
                transform: row.transform,
            });
            writes.push(EffectivePropertyWrite::Style {
                object,
                style: row.style,
            });
        }
        let next_epoch = if stale.is_empty() {
            None
        } else {
            Some(
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or("interaction frame epoch exhausted")?,
            )
        };
        let prepared = self
            .prepare_effective_property_batch(&writes)
            .map_err(|error| error.to_string())?;
        self.commit_transient_effective_properties(prepared)
            .map_err(|error| error.to_string())?;
        // A semantic revision supersedes captured values. Reconstruct only affected
        // active rows from the current plan, never write a stale captured baseline.
        let mut stale_changed = false;
        for index in stale {
            let before = FrameRowState::from_frame(&self.frame, index);
            self.relower_object(index, self.frame.time, &mut EvaluationStats::default());
            self.reapply_reactive_for_object(index);
            if before.differs_from_frame(&self.frame, index) {
                self.mark_changed(index);
                stale_changed = true;
            }
        }
        if stale_changed {
            self.publication = self
                .publication
                .with_frame_epoch(next_epoch.expect("stale rows reserved an epoch"));
        }
        for object in completed {
            self.transient_animations.active.remove(&object);
        }
        for driver in self.transient_animations.active.values_mut() {
            driver.origin.get_or_insert(wall_time);
        }
        self.transient_animations.last_tick = Some(wall_time);
        Ok(())
    }

    pub(crate) fn clear_transient_animations(&mut self) {
        self.transient_animations = TransientAnimations::default();
    }
}
