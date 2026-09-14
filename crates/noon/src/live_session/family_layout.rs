//! Family affine edits and compatibility placement forwarding during control migration.
use super::*;
use crate::{family_layout::LiveLayoutTarget, semantic_mobject::ManimNextToArgs};

impl LiveSession<'_> {
    /// Stretch a selected live object/family through the shared world-axis operation.
    pub fn stretch(
        &mut self,
        source: &crate::LayoutAnchor,
        factor: f64,
        dimension: crate::LayoutDimension,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (x, y) = match dimension {
            crate::LayoutDimension::Width => (factor, 1.0),
            crate::LayoutDimension::Height => (1.0, factor),
        };
        self.scale_layout(source, x, y, pivot)
    }

    /// Fit from the current coherent layout and publish one local affine edit.
    /// Active affine/content drivers must finish before persistent fitting.
    pub fn rescale_to_fit(
        &mut self,
        source: &crate::LayoutAnchor,
        length: f64,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), LiveSessionError> {
        self.rescale_to_fit_with_pivot(
            source,
            length,
            dimension,
            stretch,
            crate::ManimRotationPivot::Center,
        )
    }

    /// Fit from current bounds around one shared source pivot, then publish atomically.
    pub fn rescale_to_fit_with_pivot(
        &mut self,
        source: &crate::LayoutAnchor,
        length: f64,
        dimension: crate::LayoutDimension,
        stretch: bool,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), LiveSessionError> {
        let (leaves, bounds) = self.anchor_layout_members(source)?;
        let scale = dimension
            .scale(bounds, length, stretch)
            .map_err(LiveSessionError::from)?;
        for &leaf in &leaves {
            let object =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&object)?;
        }
        let Some((x, y)) = scale else {
            return Ok(());
        };
        let (_, boundary) = self.anchor_layout_measure(source, true)?;
        let prepared = crate::family_affine::FamilyAffine::Scale(x, y, pivot).prepare(
            &self.store.borrow(),
            &leaves,
            boundary,
        )?;
        self.publish_path_edits(prepared).map(|_| ())
    }

    /// Match the effective target dimension; no wrapper computes layout ratios.
    pub fn match_dim_size(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), LiveSessionError> {
        self.match_dim_size_with_pivot(
            source,
            target,
            dimension,
            stretch,
            crate::ManimRotationPivot::Center,
        )
    }

    /// Read a coherent target extent and fit around the selected source pivot.
    pub fn match_dim_size_with_pivot(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), LiveSessionError> {
        let (_, bounds) = self.anchor_layout_members(target)?;
        self.rescale_to_fit_with_pivot(source, dimension.length(bounds), dimension, stretch, pivot)
    }

    /// Replace object/family size and center in one coherent local publication.
    /// Reads current target bounds. Persistent source edits require resolved
    /// affine/content drivers, as with ordinary dimension fitting.
    pub fn replace_layout(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), LiveSessionError> {
        let (leaves, bounds) = self.anchor_layout_members(source)?;
        let (target_nodes, target_bounds) = self.anchor_layout_members(target)?;
        if target_bounds.is_none() {
            return Err(crate::AuthoringError::MissingLayoutBounds(
                target.resolve().map_err(LiveSessionError::from)?,
            )
            .into());
        }
        for &leaf in &leaves {
            let object =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&object)?;
        }
        let target_leaves = target_nodes
            .into_iter()
            .map(|node| {
                let object = Mobject::from_node(Rc::clone(self.store), node)
                    .map_err(LiveSessionError::from)?;
                Ok((
                    node,
                    self.family_member_bounds(&object)?,
                    self.family_member_measure(&object, true)?,
                ))
            })
            .collect::<Result<Vec<_>, LiveSessionError>>()?;
        let (_, boundary) = self.anchor_layout_measure(source, true)?;
        let prepared = crate::dimension_fit::replacement_transaction(
            &self.store.borrow(),
            &leaves,
            (bounds, boundary),
            &target_leaves,
            dimension,
            stretch,
        )
        .map_err(LiveSessionError::from)?;
        self.publish_path_edits(prepared).map(|_| ())
    }

    /// Publish one alias-aware family scale after validating every local member.
    pub fn scale_family(
        &mut self,
        family: &MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_family(
            family,
            crate::family_affine::FamilyAffine::Scale(x, y, crate::ManimRotationPivot::Center),
        )
    }

    /// Apply Manim's default center-pivot scale through one coherent live publication.
    ///
    /// Native [`LiveSession::scale`] remains the origin-space affine primitive. This
    /// compatibility operation reuses the same atomic affine transaction used for
    /// families so Python never reconstructs a pivot or publishes a follow-up move.
    pub fn manim_scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.manim_scale_with_pivot(mobject, x, y, crate::ManimRotationPivot::Center)
    }

    /// Apply Manim scale around an explicit world-space point atomically.
    pub fn manim_scale_about_point(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.manim_scale_with_pivot(
            mobject,
            x,
            y,
            crate::ManimRotationPivot::Point(point_x, point_y),
        )
    }

    /// Apply Manim scale around the current critical point selected by an edge vector.
    pub fn manim_scale_about_edge(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
        edge_x: f64,
        edge_y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.manim_scale_with_pivot(
            mobject,
            x,
            y,
            crate::ManimRotationPivot::Edge(edge_x, edge_y),
        )
    }

    fn manim_scale_with_pivot(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.scale_layout(&crate::LayoutAnchor::from(mobject), x, y, pivot)
    }

    /// Publish an object or family scale about a coherent shared pivot.
    pub fn scale_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        x: f64,
        y: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Scale(x, y, pivot),
        )
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
        self.affine_layout(&crate::LayoutAnchor::from(family), operation)
    }

    /// Rotate a selected object or family using coherent live pivot bounds.
    pub fn rotate_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        angle: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Rotate(angle, pivot),
        )
    }

    /// Reflect selected leaves atomically through the ordinary semantic publication.
    pub fn flip_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        axis: crate::SemanticVec3,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Flip(axis, pivot),
        )
    }

    fn affine_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        operation: crate::family_affine::FamilyAffine,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let (leaves, bounds) = self.anchor_layout_measure(anchor, true)?;
        // As with placement, resolve an active affine driver at its logical
        // completion barrier before a persistent edit; never overwrite it midway.
        for &leaf in &leaves {
            let object =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            self.placement_authored_transform(&object)?;
        }
        let prepared = operation.prepare(&self.store.borrow(), &leaves, bounds)?;
        self.publish_path_edits(prepared)
    }

    /// Compatibility observation during the control-surface migration.
    /// New application code should use [`crate::Scene::effective_family_layout`].
    pub fn effective_family_layout(
        &self,
        family: &MobjectFamily,
    ) -> Result<EffectiveMobjectLayout, LiveSessionError> {
        self.require_family(family)?;
        crate::family_layout::effective_family_layout(self.store, self.session, family)
            .map_err(LiveSessionError::from)
    }

    fn family_layout_measure(
        &self,
        family: &MobjectFamily,
        boundary: bool,
    ) -> Result<(Vec<noon_core::SemanticNodeId>, Option<Bounds2D64>), LiveSessionError> {
        self.require_family(family)?;
        crate::family_layout::effective_family_layout_measure(
            self.store,
            self.session,
            family,
            boundary,
        )
        .map_err(LiveSessionError::from)
    }

    pub(super) fn family_member_bounds(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Bounds2D64>, LiveSessionError> {
        self.family_member_measure(mobject, false)
    }

    pub(super) fn family_member_measure(
        &self,
        mobject: &Mobject,
        boundary: bool,
    ) -> Result<Option<Bounds2D64>, LiveSessionError> {
        self.require_mobject(mobject)?;
        crate::family_layout::effective_family_member_measure(
            self.store,
            self.session,
            mobject,
            boundary,
        )
        .map_err(LiveSessionError::from)
    }

    fn anchor_layout_members(
        &self,
        anchor: &crate::LayoutAnchor,
    ) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), LiveSessionError> {
        self.anchor_layout_measure(anchor, false)
    }

    fn anchor_layout_measure(
        &self,
        anchor: &crate::LayoutAnchor,
        boundary: bool,
    ) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), LiveSessionError> {
        crate::family_layout::effective_anchor_layout_measure(
            self.store,
            self.session,
            anchor,
            boundary,
        )
        .map_err(LiveSessionError::from)
    }

    /// Compatibility forwarding while callers migrate to Scene-owned placement.
    pub fn move_to(
        &mut self,
        mobject: &Mobject,
        target: LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::family_layout::publish_move_to(
            self.store,
            self.root,
            self.session,
            mobject,
            target,
            edge,
            mask,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn move_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::family_layout::publish_move_family_to(
            self.store,
            self.root,
            self.session,
            family,
            target,
            edge,
            mask,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn next_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        args: ManimNextToArgs,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::family_layout::publish_next_family_to(
            self.store,
            self.root,
            self.session,
            family,
            target,
            args,
        )
        .map_err(LiveSessionError::from)
    }

    /// Align effective family bounds and publish only the selected leaves.
    pub fn align_family_on_frame(
        &mut self,
        family: &MobjectFamily,
        direction: (f64, f64),
        buff: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::family_layout::publish_align_family_on_frame(
            self.store,
            self.root,
            self.session,
            family,
            direction,
            buff,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn align_family_to(
        &mut self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::family_layout::publish_align_family_to(
            self.store,
            self.root,
            self.session,
            family,
            target,
            axis,
        )
        .map_err(LiveSessionError::from)
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
        crate::family_layout::publish_next_layout_to_aligned(
            self.store,
            self.root,
            self.session,
            source,
            target,
            aligner,
            args,
        )
        .map_err(LiveSessionError::from)
    }
}
