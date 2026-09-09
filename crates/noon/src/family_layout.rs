//! Immutable authored family observations and atomic relative placement.
use std::{cell::RefCell, rc::Rc};

use crate::{
    family_authoring::FamilyTranslation,
    semantic_mobject::{authoring_render_f64, authoring_xy_f64, ManimNextToArgs},
    Bounds2D64, Mobject, MobjectFamily, SemanticNodeId, SemanticStore,
};

/// Bounds and ordered leaf identities observed at one authored point in time.
///
/// This is a query result, not scene state. Mutations use its observed identities
/// and bounds; obtain a fresh observation to place from updated bounds. Each
/// operation validates all leaves and commits through one semantic transaction.
#[derive(Clone, Debug)]
pub struct FamilyLayout {
    store: Rc<RefCell<SemanticStore>>,
    leaves: Vec<SemanticNodeId>,
    bounds: Option<Bounds2D64>,
}

/// A typed destination for authored family placement.
#[derive(Clone, Copy)]
pub enum FamilyLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a FamilyLayout),
    Anchor(&'a LayoutAnchor),
}

/// A layout reference into the existing semantic store, optionally selecting a
/// direct family member. It owns no bounds or membership snapshot. Negative
/// indices count from the end; selection is resolved when placement is requested.
#[derive(Clone, Debug)]
pub struct LayoutAnchor {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
    index: Option<isize>,
}

impl From<&Mobject> for LayoutAnchor {
    fn from(object: &Mobject) -> Self {
        Self {
            store: Rc::clone(object.store()),
            node: object.node_id(),
            index: None,
        }
    }
}

impl From<&MobjectFamily> for LayoutAnchor {
    fn from(family: &MobjectFamily) -> Self {
        Self {
            store: Rc::clone(family.store()),
            node: family.node_id(),
            index: None,
        }
    }
}

impl LayoutAnchor {
    /// Select a direct semantic family member, without traversing wrapper trees.
    pub fn member(mut self, index: isize) -> Self {
        self.index = Some(index);
        self
    }

    pub(crate) fn from_node(store: Rc<RefCell<SemanticStore>>, node: SemanticNodeId) -> Self {
        Self {
            store,
            node,
            index: None,
        }
    }

    pub(crate) fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub(crate) fn resolve(&self) -> Result<SemanticNodeId, String> {
        let store = self.store.borrow();
        let node = store
            .node(self.node)
            .ok_or_else(|| format!("stale layout anchor {:?}", self.node))?;
        let Some(index) = self.index else {
            return Ok(self.node);
        };
        if !matches!(node.kind(), crate::SemanticNodeKind::Family) {
            return Err("alignment submobject index requires a semantic family".into());
        }
        let members = node.members();
        let index = if index < 0 {
            members.len().checked_add_signed(index)
        } else {
            Some(index as usize)
        };
        index
            .and_then(|index| members.get(index).copied())
            .ok_or_else(|| "alignment submobject index is unavailable".into())
    }

    /// Observe the selected object/family through the shared authored layout path.
    pub fn layout(&self) -> Result<FamilyLayout, String> {
        let node = self.resolve()?;
        if matches!(
            self.store.borrow().node(node).map(|n| n.kind()),
            Some(crate::SemanticNodeKind::Family)
        ) {
            MobjectFamily::from_node(Rc::clone(&self.store), node)?.layout()
        } else {
            let object = Mobject::from_node(Rc::clone(&self.store), node)?;
            let bounds = match object.layout_bounds()? {
                Some(bounds) => Some(bounds),
                None => {
                    let (x, y) = object.center()?;
                    Some(Bounds2D64::point(x, y))
                }
            };
            Ok(FamilyLayout {
                store: Rc::clone(&self.store),
                leaves: vec![node],
                bounds,
            })
        }
    }

    /// Move this entire source using another object's/family member's bounds.
    pub fn next_to_aligned(
        &self,
        target: FamilyLayoutTarget<'_>,
        aligner: &LayoutAnchor,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, &aligner.store) {
            return Err("layout anchors belong to different authoring stores".into());
        }
        let source = self.layout()?;
        let alignment = aligner.layout()?;
        let delta = RelativePlacement::Next(args)
            .delta(alignment.bounds, |x, y| source.target_point(target, x, y))?;
        source.shift(delta.0, delta.1)
    }
}

impl MobjectFamily {
    /// Observe only this family's layout and ordered semantic leaves.
    pub fn layout(&self) -> Result<FamilyLayout, String> {
        self.validate().map_err(|error| error.to_string())?;
        let leaves = self
            .store()
            .borrow()
            .ordered_leaf_nodes(self.node_id())
            .map_err(|e| e.to_string())?;
        let mut bounds: Option<Bounds2D64> = None;
        for &leaf in &leaves {
            let Some(next) = Mobject::from_node(Rc::clone(self.store()), leaf)?.layout_bounds()?
            else {
                continue;
            };
            if let Some(total) = &mut bounds {
                total.include(next.min_x, next.min_y);
                total.include(next.max_x, next.max_y);
            } else {
                bounds = Some(next);
            }
        }
        Ok(FamilyLayout {
            store: Rc::clone(self.store()),
            leaves,
            bounds,
        })
    }

    /// Shift each semantic leaf once without querying its geometry.
    pub fn shift(&self, x: f64, y: f64) -> Result<(), String> {
        let translation = FamilyTranslation::begin(&self.store().borrow(), self.node_id(), x, y)?;
        translation.apply(&mut self.store().borrow_mut())
    }
}

impl FamilyLayout {
    pub fn bounds(&self) -> Option<Bounds2D64> {
        self.bounds
    }

    pub fn center(&self) -> (f64, f64) {
        let bounds = self.bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0));
        (
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        )
    }

    pub fn width(&self) -> f64 {
        self.bounds.map_or(0.0, Bounds2D64::width)
    }
    pub fn height(&self) -> f64 {
        self.bounds.map_or(0.0, Bounds2D64::height)
    }

    pub fn critical_point(&self, x: f64, y: f64) -> (f64, f64) {
        bounds_critical_point(self.bounds, x, y)
    }

    pub fn shift(&self, x: f64, y: f64) -> Result<(), String> {
        FamilyTranslation::from_members(self.leaves.clone(), x, y)?
            .apply(&mut self.store.borrow_mut())
    }

    pub fn move_to(
        &self,
        target: FamilyLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<(), String> {
        self.place(target, RelativePlacement::Move { edge, mask })
    }

    /// Manim placement preserves the supplied direction magnitude and coordinate mask.
    pub fn next_to(
        &self,
        target: FamilyLayoutTarget<'_>,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        self.place(target, RelativePlacement::Next(args))
    }

    pub fn align_to(&self, target: FamilyLayoutTarget<'_>, axis: (f64, f64)) -> Result<(), String> {
        self.place(target, RelativePlacement::Align(axis))
    }

    fn place(
        &self,
        target: FamilyLayoutTarget<'_>,
        placement: RelativePlacement,
    ) -> Result<(), String> {
        let delta = placement.delta(self.bounds, |x, y| self.target_point(target, x, y))?;
        self.shift(delta.0, delta.1)
    }

    fn target_point(
        &self,
        target: FamilyLayoutTarget<'_>,
        x: f64,
        y: f64,
    ) -> Result<(f64, f64), String> {
        let target_store = match target {
            FamilyLayoutTarget::Point(px, py) => {
                let point = authoring_xy_f64(px, py)?;
                return Ok((point.x, point.y));
            }
            FamilyLayoutTarget::Mobject(object) => object.store(),
            FamilyLayoutTarget::Family(family) => &family.store,
            FamilyLayoutTarget::Anchor(anchor) => anchor.store(),
        };
        if !Rc::ptr_eq(&self.store, target_store) {
            return Err(
                "family placement source and target belong to different authoring stores".into(),
            );
        }
        match target {
            FamilyLayoutTarget::Mobject(object) => object.critical_point(x, y),
            FamilyLayoutTarget::Family(family) => Ok(family.critical_point(x, y)),
            FamilyLayoutTarget::Anchor(anchor) => Ok(anchor.layout()?.critical_point(x, y)),
            FamilyLayoutTarget::Point(..) => unreachable!(),
        }
    }
}

/// Shared relative-placement math; publication remains owned by the caller.
pub(crate) enum RelativePlacement {
    Move { edge: (f64, f64), mask: (f64, f64) },
    Next(ManimNextToArgs),
    Align((f64, f64)),
}

impl RelativePlacement {
    pub(crate) fn delta(
        self,
        bounds: Option<Bounds2D64>,
        target: impl FnOnce(f64, f64) -> Result<(f64, f64), String>,
    ) -> Result<(f64, f64), String> {
        let (source_axis, target_axis, offset, mask) = match self {
            Self::Move { edge, mask } => {
                let edge = authoring_xy_f64(edge.0, edge.1)?;
                let mask = authoring_xy_f64(mask.0, mask.1)?;
                (
                    (edge.x, edge.y),
                    (edge.x, edge.y),
                    (0.0, 0.0),
                    (mask.x, mask.y),
                )
            }
            Self::Next(args) => {
                let direction = authoring_xy_f64(args.direction.0, args.direction.1)?;
                let edge = authoring_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
                let mask = authoring_xy_f64(args.mask.0, args.mask.1)?;
                let buff = authoring_render_f64("buffer", args.buff)?;
                (
                    (edge.x - direction.x, edge.y - direction.y),
                    (edge.x + direction.x, edge.y + direction.y),
                    (direction.x * buff, direction.y * buff),
                    (mask.x, mask.y),
                )
            }
            Self::Align(axis) => {
                let axis = authoring_xy_f64(axis.0, axis.1)?;
                (
                    (axis.x, axis.y),
                    (axis.x, axis.y),
                    (0.0, 0.0),
                    (
                        if axis.x == 0.0 { 0.0 } else { 1.0 },
                        if axis.y == 0.0 { 0.0 } else { 1.0 },
                    ),
                )
            }
        };
        let source = bounds_critical_point(bounds, source_axis.0, source_axis.1);
        let target = target(target_axis.0, target_axis.1)?;
        Ok((
            (target.0 - source.0 + offset.0) * mask.0,
            (target.1 - source.1 + offset.1) * mask.1,
        ))
    }
}

pub(crate) fn bounds_critical_point(bounds: Option<Bounds2D64>, x: f64, y: f64) -> (f64, f64) {
    let b = bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0));
    (
        if x < 0.0 {
            b.min_x
        } else if x > 0.0 {
            b.max_x
        } else {
            (b.min_x + b.max_x) * 0.5
        },
        if y < 0.0 {
            b.min_y
        } else if y > 0.0 {
            b.max_y
        } else {
            (b.min_y + b.max_y) * 0.5
        },
    )
}
