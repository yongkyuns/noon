//! Sparse planar pick data from the picture actually submitted for rendering.

use std::sync::Arc;

use noon_core::{ObjectId, Rect, SemanticNodeId, Vec2};
use noon_runtime::{frame_object_conservative_bounds_with_resources, RendererPublication};

use super::{PickContents, PickNode, PickRow};
use crate::execution_session::picking::{viewer_row_visible, ViewerHitGeometry};

#[derive(PartialEq)]
struct PlanarRow {
    object: ObjectId,
    group: Option<SemanticNodeId>,
    bounds: Rect,
    geometry: Option<ViewerHitGeometry>,
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
    fn pick(&self, point: Vec2, stroke_slop: f64) -> Option<(ObjectId, Option<SemanticNodeId>)> {
        let contains = |slop| {
            let x = f64::from(point.x);
            let y = f64::from(point.y);
            x >= f64::from(self.bounds.min.x) - slop
                && x <= f64::from(self.bounds.max.x) + slop
                && y >= f64::from(self.bounds.min.y) - slop
                && y <= f64::from(self.bounds.max.y) + slop
        };
        if !contains(stroke_slop) {
            return None;
        }
        match &self.contents {
            PickContents::Branch(children) => children
                .iter()
                .rev()
                .flatten()
                .find_map(|child| child.pick(point, stroke_slop)),
            PickContents::Leaf(row) => row
                .geometry
                .as_ref()
                .and_then(|shape| shape.hit(point, stroke_slop))
                .unwrap_or_else(|| contains(0.0))
                .then_some((row.object, row.group)),
        }
    }
}

#[derive(Clone)]
pub struct PlanarPickFrame {
    root: Option<Arc<PickNode<PlanarRow>>>,
    stroke_slop: f64,
}

impl PlanarPickFrame {
    pub(in crate::execution_session::inspection) fn pick(
        &self,
        point: Vec2,
    ) -> Option<(ObjectId, Option<SemanticNodeId>)> {
        self.root.as_ref()?.pick(point, self.stroke_slop)
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
        world_units_per_pixel: f64,
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
            Some(PlanarRow {
                object: row.id,
                group: publication.viewer_group(row.id),
                bounds,
                geometry: ViewerHitGeometry::for_row(frame, index, resources),
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
        PlanarPickFrame {
            root: self.root.clone(),
            stroke_slop: world_units_per_pixel * 2.0,
        }
    }
}
