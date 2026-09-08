//! Family observations and placement through the existing live publication lane.
use super::*;
use crate::{
    family_authoring::{semantic_family_leaf_ids, FamilyTranslation},
    family_layout::{bounds_critical_point, RelativePlacement},
    semantic_mobject::{authoring_xy_f64, ManimNextToArgs},
};

/// A live placement destination, observed at the current coherent publication.
#[derive(Clone, Copy)]
pub enum LiveLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a MobjectFamily),
}

impl LiveSession<'_> {
    /// Observe this family's effective bounds, including detached authored members.
    /// This query traverses only its semantic leaves and does not publish a revision.
    pub fn effective_family_layout(
        &self,
        family: &MobjectFamily,
    ) -> Result<EffectiveMobjectLayout, LiveSessionError> {
        let (_, bounds) = self.family_layout_members(family)?;
        let b = bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0));
        Ok(EffectiveMobjectLayout {
            center: bounds_critical_point(bounds, 0.0, 0.0),
            width: b.width(),
            height: b.height(),
            publication: self.session.publication_context(),
        })
    }

    fn family_layout_members(
        &self,
        family: &MobjectFamily,
    ) -> Result<(Vec<noon_core::SemanticNodeId>, Option<Bounds2D64>), LiveSessionError> {
        self.require_family(family)?;
        self.session.require_published_store(&self.store.borrow())?;
        let leaves = semantic_family_leaf_ids(&self.store.borrow(), family.node_id())
            .map_err(LiveSessionError::Mobject)?;
        let mut bounds: Option<Bounds2D64> = None;
        for &leaf in &leaves {
            let mobject = Mobject::from_node(Rc::clone(self.store), leaf)
                .map_err(LiveSessionError::Mobject)?;
            let next = self.family_member_bounds(&mobject)?;
            if let Some(next) = next {
                if let Some(total) = &mut bounds {
                    total.include(next.min_x, next.min_y);
                    total.include(next.max_x, next.max_y);
                } else {
                    bounds = Some(next);
                }
            }
        }
        Ok((leaves, bounds))
    }

    fn family_member_bounds(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Bounds2D64>, LiveSessionError> {
        self.require_mobject(mobject)?;
        if !self.session.semantic_object_is_reachable(mobject.node_id()) {
            return mobject.layout_bounds().map_err(LiveSessionError::Mobject);
        }
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(LiveSessionError::Mobject(
                "effective family layout cannot use render-content overrides".into(),
            ));
        }
        let transform = observed.object.transform;
        drop(store);
        mobject
            .layout_bounds_at(transform)
            .map_err(LiveSessionError::Mobject)
    }

    pub fn move_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.place_family(family, target, RelativePlacement::Move { edge, mask })
    }

    pub fn next_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        args: ManimNextToArgs,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.place_family(family, target, RelativePlacement::Next(args))
    }

    pub fn align_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.place_family(family, target, RelativePlacement::Align(axis))
    }

    fn place_family(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        placement: RelativePlacement,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (leaves, bounds) = self.family_layout_members(family)?;
        let delta = placement
            .delta(bounds, |x, y| match target {
                LiveLayoutTarget::Point(px, py) => {
                    let point = authoring_xy_f64(px, py)?;
                    Ok((point.x, point.y))
                }
                LiveLayoutTarget::Mobject(object) => {
                    match self
                        .family_member_bounds(object)
                        .map_err(|e| e.to_string())?
                    {
                        Some(bounds) => Ok(bounds_critical_point(Some(bounds), x, y)),
                        None if self.session.semantic_object_is_reachable(object.node_id()) => self
                            .effective_layout(object)
                            .map(|layout| layout.center)
                            .map_err(|e| e.to_string()),
                        None => object.center(),
                    }
                }
                LiveLayoutTarget::Family(family) => self
                    .family_layout_members(family)
                    .map(|(_, bounds)| bounds_critical_point(bounds, x, y))
                    .map_err(|e| e.to_string()),
            })
            .map_err(LiveSessionError::Mobject)?;
        // Relative placement cannot override a still-active affine/content driver.
        // Complete its logical segment first, as for live Mobject.move_to_point.
        for &leaf in &leaves {
            let mobject = Mobject::from_node(Rc::clone(self.store), leaf)
                .map_err(LiveSessionError::Mobject)?;
            self.placement_authored_transform(&mobject)?;
        }
        let transaction = FamilyTranslation::from_members(leaves, delta.0, delta.1)
            .and_then(|translation| translation.transaction(&self.store.borrow()))
            .map_err(LiveSessionError::Mobject)?;
        self.apply(transaction)
    }
}
