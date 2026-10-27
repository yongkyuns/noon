//! Explicit capacity maintenance; append itself never relocates scene storage.
use super::object_budget;
use super::{PrefixError, PrefixReceiver, ReplayError, Revision};
use crate::{FrameObjectState, SceneInstance};

/// Constant-time admission for the index allocation only, including spare slots.
/// Kept separate from payload/resource accounting: Revision contains Vec headers,
/// not the allocations owned by those vectors. No history traversal or allocation.
pub(super) fn preflight_history_index_capacity(capacity: usize) -> Result<usize, PrefixError> {
    let bytes_per_revision = std::mem::size_of::<Revision>();
    let limit_bytes = SceneInstance::MAX_PREFIX_HISTORY_INDEX_BYTES;
    capacity
        .checked_mul(bytes_per_revision)
        .filter(|&bytes| bytes <= limit_bytes)
        .ok_or(PrefixError::HistoryIndexBudgetExceeded {
            revision_capacity: capacity,
            bytes_per_revision,
            limit_bytes,
        })
}

/// One storage-growth implementation for bootstrap and later maintenance.
/// Requested over-budget storage fails before any reservation. Allocation failure
/// or allocator over-reservation may leave larger capacities, but cannot change
/// rows, history, publication or the receiver's advertised object capacity.
pub(super) fn reserve_object_vectors(
    runtime: &mut SceneInstance,
    total: usize,
    limit_bytes: usize,
) -> Result<(), PrefixError> {
    object_budget::preflight_reserved(runtime, total, limit_bytes)?;
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
    object_budget::preflight_reserved(runtime, 0, limit_bytes)?;
    Ok(())
}

impl PrefixReceiver {
    /// Named O(existing storage) maintenance barrier, separate from ordinary
    /// prefix delivery. Reserve ahead using a bounded workload capacity or the
    /// next batch's required_object_capacity(). Returns requested total slots.
    /// Allocation failure changes capacity only, never frame/history/publication.
    /// The separate object-vector byte cap includes existing spare capacity.
    /// No-op requests also check it; previously published history stays usable.
    /// It excludes maps, indirect payloads and allocator/whole-process peaks.
    pub fn reserve_object_capacity(
        &mut self,
        runtime: &mut SceneInstance,
        total: usize,
    ) -> Result<usize, PrefixError> {
        self.validate_runtime(runtime)?;
        if total <= self.object_capacity {
            // Failed growth may leave excess spare capacity without committing
            // object_capacity. A no-op must not report that storage as admitted.
            object_budget::preflight_reserved(runtime, total, self.object_vector_budget_bytes)?;
            return Ok(self.object_capacity);
        }
        if total > u32::MAX as usize
            || total.saturating_sub(self.initial_object_slots) > self.limits.payloads
        {
            return Err(ReplayError::RetentionLimit.into());
        }
        reserve_object_vectors(runtime, total, self.object_vector_budget_bytes)?;
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
