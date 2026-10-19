//! Explicit storage maintenance for append-only historical object slots.
use super::CompiledScene;
use crate::CompiledObject;

impl CompiledScene {
    /// Named O(existing storage) capacity barrier. Never called implicitly from
    /// prefix append or ordinary local mutation. No logical state is changed.
    pub fn reserve_prefix_object_capacity(
        &mut self,
        total: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.objects
            .try_reserve_exact(total.saturating_sub(self.objects.len()))?;
        self.family_ranks
            .try_reserve_exact(total.saturating_sub(self.family_ranks.len()))?;
        self.painter_ranks
            .try_reserve_exact(total.saturating_sub(self.painter_ranks.len()))?;
        self.family_order
            .try_reserve_exact(total.saturating_sub(self.family_order.len()))?;
        self.painter_order
            .try_reserve_exact(total.saturating_sub(self.painter_order.len()))?;
        Ok(())
    }
    /// Install source-derived tombstones, not visible membership. All identity,
    /// stream, dependency and capacity checks precede this infallible commit.
    pub fn install_prefix_tombstones(&mut self, objects: Vec<CompiledObject>) {
        for mut object in objects {
            let index = self.objects.len() as u32;
            object.live = false;
            object.dynamic = Default::default();
            self.retired_object_indices.insert(object.id, index);
            self.objects.push(object);
            self.family_ranks.push(None);
            self.painter_ranks.push(None);
        }
    }
}
