//! Sparse disposable projection of image fill parameters; resources stay in the
//! existing immutable raster cache and geometry owns coverage/painter order.
use noon_core::{ImagePaint, Rect, Transform2D};
use noon_runtime::FrameState;

use crate::{FramePreparer, PreparedFrame, RenderPrimitive};

impl PreparedFrame<'_> {
    pub(crate) fn image_paint_indices(
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
        Some(self.image_paint_indices[group].range(range))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedImagePaint {
    pub definition: ImagePaint,
    pub bounds: Rect,
    pub transform: Transform2D,
    source_index: usize,
}

impl FramePreparer {
    pub(crate) fn sync_image_paint_routes(&mut self) {
        for id in self.image_paint_routes_pending.drain(..) {
            let next = self.fill_image_paints.get(&id).and_then(|paint| {
                match self.slots.get(paint.source_index) {
                    Some(crate::PreparedSlot::Circle(index)) => Some((0, *index as u32)),
                    Some(crate::PreparedSlot::Rectangle(index)) => Some((1, *index as u32)),
                    Some(crate::PreparedSlot::Path { index, .. }) => Some((2, *index as u32)),
                    _ => None,
                }
            });
            let previous = self.image_paint_routes.get(&id).copied();
            if previous == next {
                continue;
            }
            if let Some((group, index)) = self.image_paint_routes.remove(&id) {
                self.image_paint_indices[group].remove(&index);
            }
            if let Some((group, index)) = next {
                self.image_paint_routes.insert(id, (group, index));
                self.image_paint_indices[group].insert(index);
            }
        }
    }
    pub(crate) fn sync_fill_image_paint(&mut self, frame: &FrameState, index: usize) {
        let Some(object) = frame.objects.get(index) else {
            return;
        };
        let next = object
            .fill_image
            .as_deref()
            .filter(|_| frame.is_drawable(index))
            .and_then(|definition| {
                object
                    .geometry()
                    .and_then(|geometry| geometry.local_bounds())
                    .map(|bounds| PreparedImagePaint {
                        definition: *definition,
                        bounds,
                        transform: frame.render_transform(index),
                        source_index: index,
                    })
            });
        if next.is_some() || self.fill_image_paints.contains_key(&object.id) {
            self.image_paint_routes_pending.push(object.id);
        }
        if self.fill_image_paints.get(&object.id) == next.as_ref() {
            return;
        }
        if self.fill_image_paints.contains_key(&object.id) != next.is_some() {
            self.visible_projection_ready = false;
            if matches!(
                self.slots.get(index),
                Some(crate::PreparedSlot::Path { .. })
            ) {
                self.image_paint_order_changes.push(index);
            }
        }
        match next {
            Some(value) => {
                self.fill_image_paints.insert(object.id, value);
            }
            None => {
                self.fill_image_paints.remove(&object.id);
            }
        }
        self.image_paint_changes.push(object.id);
    }
}
