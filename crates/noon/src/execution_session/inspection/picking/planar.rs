//! Sparse planar pick data from the picture actually submitted for rendering.

use std::sync::Arc;

use noon_core::{GeometryRef, GeometryResource, ObjectId, Rect, Transform2D, Vec2};
use noon_runtime::{frame_object_conservative_bounds_with_resources, RendererPublication};

use super::{PickContents, PickNode, PickRow};
use crate::execution_session::picking::{geometry_fill_contains, viewer_row_visible};

#[derive(PartialEq)]
struct PlanarRow {
    object: ObjectId,
    bounds: Rect,
    fill: Option<(GeometryRef, Transform2D)>,
}

impl PickRow for PlanarRow {
    type Bounds = Rect;

    fn bounds(&self) -> Option<Rect> {
        Some(self.bounds)
    }

    fn join(a: Rect, b: Rect) -> Rect {
        Rect {
            min: Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
            max: Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
        }
    }
}

impl PickNode<PlanarRow> {
    fn pick(&self, point: Vec2) -> Option<ObjectId> {
        if point.x < self.bounds.min.x
            || point.y < self.bounds.min.y
            || point.x > self.bounds.max.x
            || point.y > self.bounds.max.y
        {
            return None;
        }
        match &self.contents {
            PickContents::Branch(children) => children
                .iter()
                .rev()
                .flatten()
                .find_map(|child| child.pick(point)),
            PickContents::Leaf(row) => {
                let misses_fill = row.fill.as_ref().is_some_and(|(geometry, transform)| {
                    geometry_fill_contains(Some(geometry), *transform, point, None) == Ok(false)
                });
                (!misses_fill).then_some(row.object)
            }
        }
    }
}

#[derive(Clone)]
pub struct PlanarPickFrame(Option<Arc<PickNode<PlanarRow>>>);

impl PlanarPickFrame {
    pub(in crate::execution_session::inspection) fn pick(&self, point: Vec2) -> Option<ObjectId> {
        self.0.as_ref()?.pick(point)
    }
}

/// The current root plus a fixed number of host-held immutable roots. Leaf
/// positions are painter ranks, so a query can stop at the topmost visible hit.
#[derive(Default)]
pub struct PlanarPickCache {
    root: Option<Arc<PickNode<PlanarRow>>>,
    capacity: usize,
    len: usize,
}

impl PlanarPickCache {
    pub fn capture(
        &mut self,
        publication: &RendererPublication<'_>,
        snapshot: bool,
    ) -> PlanarPickFrame {
        let order = publication.painter_order();
        let frame = publication.frame();
        let resources = publication.geometry_resources();
        let row_at = |index| {
            let row: &noon_runtime::FrameObjectState = frame.objects.get(index)?;
            if Some(row.id) == publication.primary_view() || !viewer_row_visible(frame, index) {
                return None;
            }
            let bounds = frame_object_conservative_bounds_with_resources(frame, index, resources)?;
            let fill = if frame.reveal(index) == 1.0
                && frame.morph(index) == 0.0
                && row.style.fill.is_some_and(|fill| fill.alpha > 0.0)
            {
                let geometry = match frame.render_geometry(index) {
                    Some(GeometryRef::External(id)) => resources
                        .current_handle(*id)
                        .and_then(|handle| resources.get(handle))
                        .and_then(|resource| match resource {
                            GeometryResource::VectorPath(path) => {
                                Some(GeometryRef::VectorPath(Arc::clone(path)))
                            }
                            GeometryResource::Mesh(_) => None,
                        }),
                    geometry => geometry.cloned(),
                };
                geometry.map(|geometry| (geometry, frame.render_transform(index)))
            } else {
                None
            };
            Some(PlanarRow {
                object: row.id,
                bounds,
                fill,
            })
        };
        let changes = publication.changes();
        let full = self.capacity == 0 || snapshot || changes.is_all();
        if full {
            self.capacity = order.len().next_power_of_two();
            self.root = None;
        }
        while self.capacity < order.len() {
            self.capacity *= 2;
            self.root = PickNode::branch([self.root.take(), None]);
        }
        let order_change = if full {
            Some(0..order.len())
        } else {
            changes.painter_order_range()
        };
        if let Some(range) = &order_change {
            for rank in range.clone() {
                let row = order.get(rank).and_then(|&index| row_at(index as usize));
                self.root = PickNode::update(self.root.as_ref(), self.capacity, rank, row);
            }
        }
        if !full {
            for rank in order.len()..self.len {
                self.root = PickNode::update(self.root.as_ref(), self.capacity, rank, None);
            }
            for &index in changes.object_indices() {
                if let Some(rank) = publication.painter_rank(index).filter(|rank| {
                    !order_change
                        .as_ref()
                        .is_some_and(|range| range.contains(rank))
                }) {
                    self.root =
                        PickNode::update(self.root.as_ref(), self.capacity, rank, row_at(index));
                }
            }
        }
        self.len = order.len();
        PlanarPickFrame(self.root.clone())
    }
}
