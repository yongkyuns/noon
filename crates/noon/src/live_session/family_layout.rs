//! Family observations and placement through the existing live publication lane.
use super::*;
use crate::{
    family_authoring::FamilyTranslation,
    family_layout::{bounds_critical_point, RelativePlacement},
    semantic_mobject::{authoring_xy_f64, ManimNextToArgs},
};

/// A live placement destination, observed at the current coherent publication.
#[derive(Clone, Copy)]
pub enum LiveLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a MobjectFamily),
    Anchor(&'a crate::LayoutAnchor),
}

impl LiveSession<'_> {
    /// Fit from the current coherent layout and publish one local affine edit.
    /// Active affine/content drivers must finish before persistent fitting.
    pub fn rescale_to_fit(
        &mut self,
        source: &crate::LayoutAnchor,
        length: f64,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), LiveSessionError> {
        let (leaves, bounds) = self.anchor_layout_members(source)?;
        let scale = dimension
            .scale(bounds, length, stretch)
            .map_err(LiveSessionError::from)?;
        for &leaf in &leaves {
            let object =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&object)?;
            crate::dimension_fit::validate_fit_stretch(
                self.authored(&object)?.transform.rotation_z,
                stretch,
            )
            .map_err(LiveSessionError::from)?;
        }
        let Some((x, y)) = scale else {
            return Ok(());
        };
        let transaction = crate::family_affine::FamilyAffine::Scale(x, y)
            .transaction(&self.store.borrow(), &leaves, bounds)
            .map_err(LiveSessionError::from)?;
        self.apply(transaction).map(|_| ())
    }

    /// Match the effective target dimension; no wrapper computes layout ratios.
    pub fn match_dim_size(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), LiveSessionError> {
        let (_, bounds) = self.anchor_layout_members(target)?;
        self.rescale_to_fit(source, dimension.length(bounds), dimension, stretch)
    }

    /// Publish one alias-aware family scale after validating every local member.
    pub fn scale_family(
        &mut self,
        family: &MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_family(family, crate::family_affine::FamilyAffine::Scale(x, y))
    }

    /// Rotate a family through the same authored transaction and live publication lane.
    pub fn rotate_family(
        &mut self,
        family: &MobjectFamily,
        angle: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_family(
            family,
            crate::family_affine::FamilyAffine::Rotate(angle, pivot),
        )
    }

    fn affine_family(
        &mut self,
        family: &MobjectFamily,
        operation: crate::family_affine::FamilyAffine,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (leaves, bounds) = self.family_layout_members(family)?;
        // As with placement, resolve an active affine driver at its logical
        // completion barrier before a persistent edit; never overwrite it midway.
        for &leaf in &leaves {
            let object =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&object)?;
        }
        let transaction = operation
            .transaction(&self.store.borrow(), &leaves, bounds)
            .map_err(LiveSessionError::from)?;
        self.apply(transaction)
    }

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
        let leaves = self
            .store
            .borrow()
            .ordered_leaf_nodes(family.node_id())
            .map_err(crate::AuthoringError::from)?;
        let mut bounds: Option<Bounds2D64> = None;
        for &leaf in &leaves {
            let mobject =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
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

    pub(super) fn family_member_bounds(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Bounds2D64>, LiveSessionError> {
        self.require_mobject(mobject)?;
        if !self.session.semantic_object_is_reachable(mobject.node_id()) {
            return mobject.layout_bounds().map_err(LiveSessionError::from);
        }
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(crate::AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
            )
            .into());
        }
        let transform = observed.object.transform;
        drop(store);
        mobject
            .layout_bounds_at(transform)
            .map_err(LiveSessionError::from)
    }

    /// Move one live object or detached target using shared edge/mask semantics.
    /// Reads only the source and destination bounds and publishes one translation.
    pub fn move_to(
        &mut self,
        mobject: &Mobject,
        target: LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let transform = self.placement_authored_transform(mobject)?;
        let bounds = mobject
            .layout_bounds_at(transform)
            .map_err(LiveSessionError::from)?
            .unwrap_or_else(|| {
                Bounds2D64::point(
                    f64::from(transform.translation.x),
                    f64::from(transform.translation.y),
                )
            });
        self.place_layout_members(
            vec![mobject.node_id()],
            Some(bounds),
            target,
            RelativePlacement::Move { edge, mask },
        )
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

    fn anchor_layout_members(
        &self,
        anchor: &crate::LayoutAnchor,
    ) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), LiveSessionError> {
        if !Rc::ptr_eq(self.store, anchor.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore.into());
        }
        self.session.require_published_store(&self.store.borrow())?;
        let node = anchor.resolve().map_err(LiveSessionError::from)?;
        if matches!(
            self.store.borrow().node(node).map(|n| n.kind()),
            Some(noon_core::SemanticNodeKind::Family)
        ) {
            let family = MobjectFamily::from_node(Rc::clone(self.store), node)
                .map_err(LiveSessionError::from)?;
            self.family_layout_members(&family)
        } else {
            let object =
                Mobject::from_node(Rc::clone(self.store), node).map_err(LiveSessionError::from)?;
            let bounds = self.family_member_bounds(&object)?;
            let bounds = match bounds {
                Some(bounds) => Some(bounds),
                None => {
                    let (x, y) = if self.session.semantic_object_is_reachable(node) {
                        self.effective_layout(&object)?.center
                    } else {
                        object.center().map_err(LiveSessionError::from)?
                    };
                    Some(Bounds2D64::point(x, y))
                }
            };
            Ok((vec![node], bounds))
        }
    }

    /// Place an entire object/family from selected effective bounds. All anchor
    /// resolution and driver validation precede one local mutation transaction.
    pub fn next_layout_to_aligned(
        &mut self,
        source: &crate::LayoutAnchor,
        target: LiveLayoutTarget<'_>,
        aligner: &crate::LayoutAnchor,
        args: ManimNextToArgs,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (leaves, _) = self.anchor_layout_members(source)?;
        let (_, bounds) = self.anchor_layout_members(aligner)?;
        self.place_layout_members(leaves, bounds, target, RelativePlacement::Next(args))
    }

    fn place_family(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        placement: RelativePlacement,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (leaves, bounds) = self.family_layout_members(family)?;
        self.place_layout_members(leaves, bounds, target, placement)
    }

    fn place_layout_members(
        &mut self,
        leaves: Vec<SemanticNodeId>,
        bounds: Option<Bounds2D64>,
        target: LiveLayoutTarget<'_>,
        placement: RelativePlacement,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let delta = placement.delta(bounds, |x, y| match target {
            LiveLayoutTarget::Point(px, py) => {
                let point = authoring_xy_f64(px, py)?;
                Ok((point.x, point.y))
            }
            LiveLayoutTarget::Mobject(object) => match self.family_member_bounds(object)? {
                Some(bounds) => Ok(bounds_critical_point(Some(bounds), x, y)),
                None if self.session.semantic_object_is_reachable(object.node_id()) => {
                    self.effective_layout(object).map(|layout| layout.center)
                }
                None => object.center().map_err(LiveSessionError::from),
            },
            LiveLayoutTarget::Anchor(anchor) => self
                .anchor_layout_members(anchor)
                .map(|(_, bounds)| bounds_critical_point(bounds, x, y)),
            LiveLayoutTarget::Family(family) => self
                .family_layout_members(family)
                .map(|(_, bounds)| bounds_critical_point(bounds, x, y)),
        })?;
        // Relative placement cannot override a still-active affine/content driver.
        // Complete its logical segment first, as for live Mobject.move_to_point.
        for &leaf in &leaves {
            let mobject =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&mobject)?;
        }
        let transaction = FamilyTranslation::from_members(leaves, delta.0, delta.1)
            .and_then(|translation| translation.transaction(&self.store.borrow()))
            .map_err(LiveSessionError::from)?;
        self.apply(transaction)
    }
}
