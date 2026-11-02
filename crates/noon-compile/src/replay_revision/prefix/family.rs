//! Append-only glyph plans keep their existing stable plan/channel indices.
use super::CompiledScene;
use crate::CompiledFamilyAnimationChannel;
use noon_core::{
    CompositionTimeMapStep, FamilyAnimationLeafSpan, RetainedAnimationMember,
    RetainedFamilyAnimationLeafPlan, RetainedFamilyAnimationPlan,
};

impl CompiledScene {
    /// Logical cloned plan/channel headers, leaf/member descriptors and time maps.
    /// Excludes spare capacity, scheduler storage and allocator overhead. Heavy
    /// text/font/geometry dependencies remain in the ordinary resource closure.
    pub fn prefix_family_payload_bytes(&self, from: usize) -> Option<usize> {
        self.family_animations
            .get(from..)?
            .iter()
            .try_fold(0usize, |bytes, channel| {
                let plan = self
                    .family_animation_plans
                    .get(channel.plan_index as usize)?;
                let mut additional = std::mem::size_of::<RetainedFamilyAnimationPlan>()
                    .checked_add(std::mem::size_of::<CompiledFamilyAnimationChannel>())?
                    .checked_add(
                        plan.member_plan()
                            .leaves()
                            .len()
                            .checked_mul(std::mem::size_of::<FamilyAnimationLeafSpan>())?,
                    )?
                    .checked_add(
                        plan.leaves()
                            .len()
                            .checked_mul(std::mem::size_of::<RetainedFamilyAnimationLeafPlan>())?,
                    )?
                    .checked_add(
                        channel
                            .time_map
                            .steps
                            .len()
                            .checked_mul(std::mem::size_of::<CompositionTimeMapStep>())?,
                    )?;
                for leaf in plan.leaves() {
                    additional = additional.checked_add(
                        leaf.members()
                            .members()
                            .len()
                            .checked_mul(std::mem::size_of::<RetainedAnimationMember>())?,
                    )?;
                }
                bytes.checked_add(additional)
            })
    }

    /// Clone only new immutable plans after admission, never earlier resources.
    pub fn prefix_family_delta(
        &self,
        from: usize,
    ) -> Vec<(RetainedFamilyAnimationPlan, CompiledFamilyAnimationChannel)> {
        self.family_animations[from..]
            .iter()
            .map(|channel| {
                (
                    self.family_animation_plans[channel.plan_index as usize].clone(),
                    channel.clone(),
                )
            })
            .collect()
    }

    /// Explicit O(existing table storage) maintenance, separate from adoption.
    pub fn reserve_prefix_family_capacity(
        &mut self,
        total: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.family_animation_plans
            .try_reserve_exact(total.saturating_sub(self.family_animation_plans.len()))?;
        self.family_animations
            .try_reserve_exact(total.saturating_sub(self.family_animations.len()))
    }

    /// Install source-validated, ordered plans without evaluating their targets.
    /// Dormant rows already occupy the same stable physical indices. Their live
    /// identity maps need not exist until historical selection activates them.
    pub fn install_prefix_family_delta(
        &mut self,
        delta: Vec<(RetainedFamilyAnimationPlan, CompiledFamilyAnimationChannel)>,
    ) {
        for (plan, channel) in delta {
            debug_assert_eq!(
                channel.plan_index as usize,
                self.family_animation_plans.len()
            );
            debug_assert_eq!(
                self.objects[channel.object_index as usize].id,
                channel.target
            );
            debug_assert!(
                self.family_animation_plans.len() < self.family_animation_plans.capacity()
            );
            debug_assert!(self.family_animations.len() < self.family_animations.capacity());
            self.family_animation_plans.push(plan);
            self.family_animations.push(channel);
        }
    }
}
