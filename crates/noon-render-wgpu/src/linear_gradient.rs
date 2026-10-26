//! Disposable retained projection of effective continuous fill values.
use std::sync::Arc;

use noon_core::{LinearGradient, Rect, Transform2D};
use noon_runtime::FrameState;

use crate::{FramePreparer, PreparedFrame, RenderPrimitive};

impl PreparedFrame<'_> {
    pub(crate) fn gradient_indices(
        &self,
        primitive: RenderPrimitive,
        range: std::ops::Range<u32>,
    ) -> Option<std::collections::btree_set::Range<'_, u32>> {
        let group = match primitive {
            RenderPrimitive::Circle => 0,
            RenderPrimitive::Rectangle => 1,
            RenderPrimitive::Path { .. } => 2,
            _ => return None,
        };
        Some(self.gradient_indices[group].range(range))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedGradient {
    pub definition: Arc<LinearGradient>,
    pub bounds: Rect,
    pub transform: Transform2D,
    source_index: usize,
}

impl FramePreparer {
    pub(crate) fn sync_gradient_routes(&mut self) {
        for id in self.gradient_routes_pending.drain(..) {
            let next = self.fill_gradients.get(&id).and_then(|paint| {
                match self.slots.get(paint.source_index) {
                    Some(crate::PreparedSlot::Circle(index)) => Some((0, *index as u32)),
                    Some(crate::PreparedSlot::Rectangle(index)) => Some((1, *index as u32)),
                    Some(crate::PreparedSlot::Path { index, .. }) => Some((2, *index as u32)),
                    _ => None,
                }
            });
            let previous = self.gradient_routes.get(&id).copied();
            if previous == next {
                continue;
            }
            if let Some((group, index)) = self.gradient_routes.remove(&id) {
                self.gradient_indices[group].remove(&index);
            }
            if let Some((group, index)) = next {
                self.gradient_routes.insert(id, (group, index));
                self.gradient_indices[group].insert(index);
            }
        }
    }
    pub(crate) fn path_range_has_gradient(&self, range: &std::ops::Range<u32>) -> bool {
        !self.fill_gradients.is_empty()
            && range.clone().any(|index| {
                self.path_ids
                    .get(index as usize)
                    .is_some_and(|id| self.fill_gradients.contains_key(id))
            })
    }
    pub(crate) fn sync_fill_gradient(&mut self, frame: &FrameState, index: usize) {
        let Some(object) = frame.objects.get(index) else {
            return;
        };
        let next = object
            .fill_gradient
            .as_ref()
            .filter(|_| frame.is_drawable(index))
            .and_then(|definition| {
                object
                    .geometry()
                    .and_then(|geometry| geometry.local_bounds())
                    .map(|bounds| PreparedGradient {
                        definition: definition.clone(),
                        bounds,
                        transform: frame.render_transform(index),
                        source_index: index,
                    })
            });
        if next.is_some() || self.fill_gradients.contains_key(&object.id) {
            self.gradient_routes_pending.push(object.id);
        }
        if self.fill_gradients.get(&object.id) == next.as_ref() {
            return;
        }
        if self.fill_gradients.contains_key(&object.id) != next.is_some() {
            self.visible_projection_ready = false;
            if matches!(
                self.slots.get(index),
                Some(crate::PreparedSlot::Path { .. })
            ) {
                self.gradient_order_changes.push(index);
            }
        }
        match next {
            Some(value) => {
                self.fill_gradients.insert(object.id, value);
            }
            None => {
                self.fill_gradients.remove(&object.id);
            }
        }
        self.gradient_changes.push(object.id);
    }
}
