//! Explicit capacity maintenance; append itself never relocates scene storage.
use super::{PrefixError, PrefixReceiver, ReplayError};
use crate::{FrameObjectState, SceneInstance};

impl PrefixReceiver {
    /// Named O(existing storage) maintenance barrier, separate from ordinary
    /// prefix delivery. Reserve ahead using a bounded workload capacity or the
    /// next batch's required_object_capacity(). Returns requested total slots.
    /// Allocation failure changes capacity only, never frame/history/publication.
    /// Payload and revision limits are entry counts, not a physical-memory cap.
    pub fn reserve_object_capacity(
        &mut self,
        runtime: &mut SceneInstance,
        total: usize,
    ) -> Result<usize, PrefixError> {
        self.validate_runtime(runtime)?;
        if total <= self.object_capacity {
            return Ok(self.object_capacity);
        }
        if total > u32::MAX as usize
            || total.saturating_sub(self.initial_object_slots) > self.limits.payloads
        {
            return Err(ReplayError::RetentionLimit.into());
        }
        runtime
            .compiled
            .reserve_prefix_object_capacity(total)
            .map_err(|_| PrefixError::Allocation)?;
        macro_rules! reserve {
            ($values:expr) => {
                $values
                    .try_reserve_exact(total.saturating_sub($values.len()))
                    .map_err(|_| PrefixError::Allocation)?;
            };
        }
        reserve!(runtime.frame.objects);
        reserve!(runtime.frame.presences);
        reserve!(runtime.frame.reveals);
        reserve!(runtime.frame.morphs);
        reserve!(runtime.frame.render_geometries);
        reserve!(runtime.frame.render_transforms);
        reserve!(runtime.frame.family_animations);
        reserve!(runtime.frame.family_animation_plan_indices);
        reserve!(runtime.painter_ranks);
        reserve!(runtime.painter_order);
        self.object_capacity = total;
        Ok(total)
    }
}
impl SceneInstance {
    /// Materialize only previously reserved tombstone rows, upon a requested
    /// seek or forward advance, never during append. Actual membership/content is
    /// selected by historical revision exchange before any renderer publication.
    pub(super) fn extend_prefix_frame_storage(&mut self) {
        while self.frame.objects.len() < self.compiled.objects().len() {
            let object = &self.compiled.objects()[self.frame.objects.len()];
            debug_assert!(!object.live);
            self.frame.objects.push(FrameObjectState {
                draw_enabled: object.draw_enabled,
                glow: None,
                id: object.id,
                content: object.content.clone(),
                text_bounds: object.text_bounds,
                transform: object.base_transform,
                spatial: None,
                style: object.base_style,
                z_index: object.base_z_index,
                appearance: 1.0,
            });
            self.frame.presences.push(false);
            self.frame.reveals.push(1.0);
            self.frame.morphs.push(0.0);
            self.frame.render_geometries.push(None);
            self.frame.render_transforms.push(None);
            self.frame.family_animations.push(None);
            self.frame.family_animation_plan_indices.push(None);
            self.painter_ranks.push(None);
        }
    }
}
