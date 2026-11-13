//! Scene-owned affine placement and fitting over authored or effective layout.
use super::*;
use crate::{
    family_layout::{
        effective_anchor_layout_measure, effective_family_member_measure,
        placement_authored_transform,
    },
    semantic_mobject::ManimNextToArgs,
};

impl Scene {
    /// Place and fit from one effective snapshot, then publish one local edit.
    pub fn next_layout_to_edge_in_frame(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        edge: (f64, f64),
        args: ManimNextToArgs,
        frame_buff: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.require_layout_anchor(source)?;
        self.require_layout_anchor(target)?;
        if self.running_execution().is_none() {
            let transaction = crate::family_layout::prepare_next_layout_to_edge_in_frame(
                &self.store,
                source,
                target,
                edge,
                args,
                frame_buff,
            )?;
            return self.apply_semantic_transaction(transaction);
        }
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_next_layout_to_edge_in_frame(
                store, root, execution, source, target, edge, args, frame_buff,
            )
        })
    }

    /// Stretch a selected live object/family through the shared world-axis operation.
    pub fn stretch(
        &mut self,
        source: &crate::LayoutAnchor,
        factor: f64,
        dimension: crate::LayoutDimension,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        let (x, y) = match dimension {
            crate::LayoutDimension::Width => (factor, 1.0),
            crate::LayoutDimension::Height => (1.0, factor),
            crate::LayoutDimension::Depth => {
                return self.affine_layout(
                    source,
                    crate::family_affine::FamilyAffine::StretchDepth(factor, pivot),
                    None,
                );
            }
        };
        self.affine_layout(
            source,
            crate::family_affine::FamilyAffine::Scale(x, y, pivot),
            None,
        )
    }

    /// Fit from the current coherent layout and publish one local affine edit.
    /// Active affine/content drivers must finish before persistent fitting.
    pub fn rescale_to_fit(
        &mut self,
        source: &crate::LayoutAnchor,
        length: f64,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.require_layout_anchor(source)?;
        let previous = self.layout_dimension_length(source, dimension)?;
        let Some(factor) = crate::dimension_fit::fitting_factor(previous, length)? else {
            return Ok(());
        };
        let factors = crate::dimension_fit::fitting_factors(factor, dimension, stretch);
        let table_layout_scope = (!stretch).then(|| source.resolve()).transpose()?;
        self.affine_layout(
            source,
            crate::family_affine::FamilyAffine::Scale3d(factors, pivot),
            table_layout_scope,
        )
        .map(|_| ())
    }

    /// Match the effective target dimension; no wrapper computes layout ratios.
    pub fn match_dim_size(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.require_layout_anchor(target)?;
        let length = self.layout_dimension_length(target, dimension)?;
        self.rescale_to_fit_with_pivot(source, length, dimension, stretch, pivot)
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
    ) -> Result<(), AuthoringError> {
        self.replace_layout_with_circle_buffer(source, target, dimension, stretch, None)
    }

    /// Publish Circle.surround from coherent live bounds with one revision.
    /// The source must have resolved affine/content drivers, as with replace.
    pub fn surround_circle(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: u32,
        stretch: bool,
        buffer_factor: f64,
    ) -> Result<(), AuthoringError> {
        let dimension = crate::dimension_fit::circle_surround_dimension(dimension, stretch)?;
        self.replace_layout_with_circle_buffer(
            source,
            target,
            dimension,
            stretch,
            Some(buffer_factor),
        )
    }

    fn replace_layout_with_circle_buffer(
        &mut self,
        source: &crate::LayoutAnchor,
        target: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
        stretch: bool,
        circle_buffer_factor: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.require_layout_anchor(source)?;
        self.require_layout_anchor(target)?;
        let Some(execution) = self.running_execution() else {
            return source.replace_layout_with_circle_buffer(
                target,
                dimension,
                stretch,
                circle_buffer_factor,
            );
        };
        let (leaves, bounds) =
            effective_anchor_layout_measure(&self.store, execution, source, false)?;
        let (target_nodes, target_bounds) =
            effective_anchor_layout_measure(&self.store, execution, target, false)?;
        if target_bounds.is_none() {
            return Err(AuthoringError::MissingLayoutBounds(target.resolve()?));
        }
        validate_layout_drivers(&self.store, execution, &leaves)?;
        let target_leaves = target_nodes
            .into_iter()
            .map(|node| {
                let object = Mobject::from_node(Rc::clone(&self.store), node)?;
                Ok((
                    node,
                    effective_family_member_measure(&self.store, execution, &object, false)?,
                    effective_family_member_measure(&self.store, execution, &object, true)?,
                ))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let (_, boundary) = effective_anchor_layout_measure(&self.store, execution, source, true)?;
        let prepared = crate::dimension_fit::replacement_transaction(
            &self.store.borrow(),
            &leaves,
            (bounds, boundary),
            &target_leaves,
            dimension,
            stretch,
            circle_buffer_factor,
        )?;
        self.publish_path_edits(prepared).map(|_| ())
    }

    /// Publish one alias-aware family scale after validating every local member.
    pub fn scale_family(
        &mut self,
        family: &MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.affine_layout(
            &crate::LayoutAnchor::from(family),
            crate::family_affine::FamilyAffine::Scale(x, y, crate::ManimRotationPivot::Center),
            Some(family.node_id()),
        )
    }

    /// Apply Manim's default center-pivot scale through one coherent live publication.
    ///
    /// Uses the shared layout transaction to capture the pivot before publishing.
    pub fn manim_scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
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
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
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
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
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
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.scale_layout(&crate::LayoutAnchor::from(mobject), x, y, pivot)
    }

    /// Publish an object or family scale about a coherent shared pivot.
    pub fn scale_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        x: f64,
        y: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        let table_layout_scope = anchor.resolve()?;
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Scale(x, y, pivot),
            Some(table_layout_scope),
        )
    }

    /// Scale full semantic XYZ coordinates from one coherent live publication.
    pub fn scale_layout_3d(
        &mut self,
        anchor: &crate::LayoutAnchor,
        factors: crate::SemanticVec3,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        let table_layout_scope = anchor.resolve()?;
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Scale3d(factors, pivot),
            Some(table_layout_scope),
        )
    }

    /// Rotate a family through the same authored transaction and live publication lane.
    pub fn rotate_family(
        &mut self,
        family: &MobjectFamily,
        angle: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.rotate_layout(&crate::LayoutAnchor::from(family), angle, pivot)
    }

    /// Rotate a selected object or family using coherent live pivot bounds.
    pub fn rotate_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        angle: f64,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Rotate(angle, pivot),
            None,
        )
    }

    /// Rotate an object about its effective geometry center.
    pub fn rotate(
        &mut self,
        object: &Mobject,
        angle: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.rotate_layout(
            &crate::LayoutAnchor::from(object),
            angle,
            crate::ManimRotationPivot::Center,
        )
    }

    /// Reflect selected leaves atomically through the ordinary semantic publication.
    pub fn flip_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        axis: crate::SemanticVec3,
        pivot: crate::ManimRotationPivot,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.affine_layout(
            anchor,
            crate::family_affine::FamilyAffine::Flip(axis, pivot),
            None,
        )
    }

    fn affine_layout(
        &mut self,
        anchor: &crate::LayoutAnchor,
        operation: crate::family_affine::FamilyAffine,
        table_layout_scope: Option<SemanticNodeId>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.require_layout_anchor(anchor)?;
        let prepared = match self.running_execution() {
            Some(execution) => prepare_effective_affine(
                &self.store,
                execution,
                anchor,
                operation,
                table_layout_scope,
            )?,
            None => anchor.prepare_affine(operation, table_layout_scope)?,
        };
        self.publish_path_edits(prepared)
    }

    fn require_layout_anchor(&self, anchor: &crate::LayoutAnchor) -> Result<(), AuthoringError> {
        if !anchor.belongs_to_store(&self.store) {
            return Err(AuthoringError::ForeignStore);
        }
        Ok(())
    }

    fn layout_dimension_length(
        &self,
        anchor: &crate::LayoutAnchor,
        dimension: crate::LayoutDimension,
    ) -> Result<f64, AuthoringError> {
        match self.running_execution() {
            Some(_) => self.effective_dimension_length(anchor, dimension),
            None => anchor.dimension_length(dimension),
        }
    }
}

fn prepare_effective_affine(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    anchor: &crate::LayoutAnchor,
    operation: crate::family_affine::FamilyAffine,
    table_layout_scope: Option<SemanticNodeId>,
) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
    let (leaves, bounds) = if operation.needs_xy_bounds() {
        effective_anchor_layout_measure(store, execution, anchor, true)?
    } else {
        if !anchor.belongs_to_store(store) {
            return Err(AuthoringError::ForeignStore);
        }
        execution.require_published_store(&store.borrow())?;
        let root = anchor.resolve()?;
        (store.borrow().ordered_leaf_nodes(root)?, None)
    };
    validate_layout_drivers(store, execution, &leaves)?;
    operation.prepare_for_scope(&store.borrow(), &leaves, bounds, table_layout_scope)
}

fn validate_layout_drivers(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    leaves: &[SemanticNodeId],
) -> Result<(), AuthoringError> {
    for &leaf in leaves {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        placement_authored_transform(store, execution, &object)?;
    }
    Ok(())
}
