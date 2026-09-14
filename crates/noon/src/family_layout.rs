//! Immutable authored family observations and atomic relative placement.
use crate::AuthoringError;
use std::{cell::RefCell, rc::Rc};

use crate::{
    family_arrangement::FamilyArrangePlan,
    family_authoring::FamilyTranslation,
    semantic_mobject::{authoring_render_f64, authoring_xy_f64, ManimNextToArgs},
    Bounds2D64, ExecutionSession, ExecutionSessionPublicationError, Mobject, MobjectFamily,
    SemanticMutationTransactionResult, SemanticNodeId, Transform2D,
};
use noon_core::SemanticStore;

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
    boundary: Option<Bounds2D64>,
}

/// A typed destination for authored family placement.
#[derive(Clone, Copy)]
pub enum FamilyLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a FamilyLayout),
    Anchor(&'a LayoutAnchor),
}

/// A placement destination observed from one coherent execution publication.
///
/// The value owns no Runtime state. Running placement helpers resolve it against
/// the caller-supplied [`ExecutionSession`] immediately before one publication.
#[derive(Clone, Copy)]
pub enum LiveLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a MobjectFamily),
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
            store: Rc::clone(object.integration_store()),
            node: object.node_id(),
            index: None,
        }
    }
}

impl From<&MobjectFamily> for LayoutAnchor {
    fn from(family: &MobjectFamily) -> Self {
        Self {
            store: Rc::clone(family.integration_store()),
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

    pub(crate) fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub(crate) fn resolve(&self) -> Result<SemanticNodeId, AuthoringError> {
        let store = self.store.borrow();
        let node =
            store
                .node(self.node)
                .ok_or(noon_core::SemanticSceneOperationError::UnknownNode(
                    self.node,
                ))?;
        let Some(index) = self.index else {
            return Ok(self.node);
        };
        if !matches!(node.kind(), noon_core::SemanticNodeKind::Family(_)) {
            return Err(
                noon_core::SemanticSceneOperationError::NotSemanticFamily(self.node).into(),
            );
        }
        let members = node.members();
        let requested_index = index;
        let index = if index < 0 {
            members.len().checked_add_signed(index)
        } else {
            Some(index as usize)
        };
        index.and_then(|index| members.get(index).copied()).ok_or(
            AuthoringError::InvalidSubmobjectIndex {
                family: self.node,
                index: requested_index,
            },
        )
    }

    /// Observe the selected object/family through the shared authored layout path.
    pub fn layout(&self) -> Result<FamilyLayout, AuthoringError> {
        let node = self.resolve()?;
        if matches!(
            self.store.borrow().node(node).map(|n| n.kind()),
            Some(noon_core::SemanticNodeKind::Family(_))
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
                boundary: object.boundary_bounds()?.or(bounds),
            })
        }
    }

    /// Move this entire source using another object's/family member's bounds.
    pub fn next_to_aligned(
        &self,
        target: FamilyLayoutTarget<'_>,
        aligner: &LayoutAnchor,
        args: ManimNextToArgs,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(&self.store, &aligner.store) {
            return Err(AuthoringError::ForeignStore);
        }
        let source = self.layout()?;
        let alignment = aligner.layout()?;
        let delta = RelativePlacement::Next(args)
            .delta(alignment.boundary, |x, y| source.target_point(target, x, y))?;
        source.shift(delta.0, delta.1)
    }
}

impl MobjectFamily {
    /// Observe only this family's layout and ordered semantic leaves.
    pub fn layout(&self) -> Result<FamilyLayout, AuthoringError> {
        self.validate()?;
        let leaves = self
            .integration_store()
            .borrow()
            .ordered_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let mut bounds = None;
        let mut boundary = None;
        for &leaf in &leaves {
            let object = Mobject::from_node(Rc::clone(self.integration_store()), leaf)?;
            union_bounds(&mut bounds, object.layout_bounds()?);
            union_bounds(&mut boundary, object.boundary_bounds()?);
        }
        Ok(FamilyLayout {
            store: Rc::clone(self.integration_store()),
            leaves,
            bounds,
            boundary,
        })
    }

    /// Shift each semantic leaf once without querying its geometry.
    pub fn shift(&self, x: f64, y: f64) -> Result<(), AuthoringError> {
        let translation =
            FamilyTranslation::begin(&self.integration_store().borrow(), self.node_id(), x, y)?;
        translation.apply(&mut self.integration_store().borrow_mut())
    }
}

impl FamilyLayout {
    pub(crate) fn leaves(&self) -> &[SemanticNodeId] {
        &self.leaves
    }

    pub fn bounds(&self) -> Option<Bounds2D64> {
        self.bounds
    }

    pub(crate) fn boundary_bounds(&self) -> Option<Bounds2D64> {
        self.boundary
    }

    pub fn center(&self) -> (f64, f64) {
        let bounds = self.boundary.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0));
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
        bounds_critical_point(self.boundary, x, y)
    }

    pub fn shift(&self, x: f64, y: f64) -> Result<(), AuthoringError> {
        FamilyTranslation::from_members(self.leaves.clone(), x, y)?
            .apply(&mut self.store.borrow_mut())
    }

    /// Align selected family edges to the default frame in one transaction.
    /// Direction magnitude scales the buffer, as with object frame alignment.
    pub fn align_on_frame(&self, direction: (f64, f64), buff: f64) -> Result<(), AuthoringError> {
        let target = frame_alignment_target(direction, buff)?;
        self.align_to(FamilyLayoutTarget::Point(target.0, target.1), direction)
    }

    pub fn move_to(
        &self,
        target: FamilyLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.place(target, RelativePlacement::Move { edge, mask })
    }

    /// Manim placement preserves the supplied direction magnitude and coordinate mask.
    pub fn next_to(
        &self,
        target: FamilyLayoutTarget<'_>,
        args: ManimNextToArgs,
    ) -> Result<(), AuthoringError> {
        self.place(target, RelativePlacement::Next(args))
    }

    pub fn align_to(
        &self,
        target: FamilyLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.place(target, RelativePlacement::Align(axis))
    }

    fn place(
        &self,
        target: FamilyLayoutTarget<'_>,
        placement: RelativePlacement,
    ) -> Result<(), AuthoringError> {
        let delta = placement.delta(self.boundary, |x, y| self.target_point(target, x, y))?;
        self.shift(delta.0, delta.1)
    }

    fn target_point(
        &self,
        target: FamilyLayoutTarget<'_>,
        x: f64,
        y: f64,
    ) -> Result<(f64, f64), AuthoringError> {
        let target_store = match target {
            FamilyLayoutTarget::Point(px, py) => {
                let point = authoring_xy_f64(px, py)?;
                return Ok((point.x, point.y));
            }
            FamilyLayoutTarget::Mobject(object) => object.integration_store(),
            FamilyLayoutTarget::Family(family) => &family.store,
            FamilyLayoutTarget::Anchor(anchor) => anchor.integration_store(),
        };
        if !Rc::ptr_eq(&self.store, target_store) {
            return Err(AuthoringError::ForeignStore);
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
    pub(crate) fn delta<E: From<AuthoringError>>(
        self,
        bounds: Option<Bounds2D64>,
        target: impl FnOnce(f64, f64) -> Result<(f64, f64), E>,
    ) -> Result<(f64, f64), E> {
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

/// Shared frame target; placement owns bounds, alias selection and publication.
pub(crate) fn frame_alignment_target(
    direction: (f64, f64),
    buff: f64,
) -> Result<(f64, f64), AuthoringError> {
    let direction = authoring_xy_f64(direction.0, direction.1)?;
    let buff = authoring_render_f64("buffer", buff)?;
    let coordinate = |direction: f64, extent: f32| {
        if direction == 0.0 {
            0.0
        } else {
            direction.signum() * f64::from(extent) * 0.5 - direction * buff
        }
    };
    Ok((
        coordinate(direction.x, noon_core::DEFAULT_FRAME_WIDTH),
        coordinate(direction.y, noon_core::DEFAULT_FRAME_HEIGHT),
    ))
}

pub(crate) fn union_bounds(total: &mut Option<Bounds2D64>, next: Option<Bounds2D64>) {
    if let Some(next) = next {
        if let Some(total) = total {
            total.include(next.min_x, next.min_y);
            total.include(next.max_x, next.max_y);
        } else {
            *total = Some(next);
        }
    }
}

/// Observe one semantic family's effective layout from an existing coherent execution.
///
/// Reachable leaves use Runtime's current affine. Detached leaves preserve their exact
/// authored layout. Render-content overrides remain explicit failures because authored
/// geometry bounds no longer describe the visible result in that state.
pub(crate) fn effective_family_layout(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    family: &MobjectFamily,
) -> Result<crate::EffectiveMobjectLayout, AuthoringError> {
    let (_, bounds) = effective_family_layout_measure(store, execution, family, false)?;
    let (_, boundary) = effective_family_layout_measure(store, execution, family, true)?;
    let layout = bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0));
    Ok(crate::EffectiveMobjectLayout {
        center: bounds_critical_point(boundary, 0.0, 0.0),
        width: layout.width(),
        height: layout.height(),
        publication: execution.publication_context(),
    })
}

pub(crate) fn effective_family_layout_measure(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    family: &MobjectFamily,
    boundary: bool,
) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), AuthoringError> {
    if !Rc::ptr_eq(store, family.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    family.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(family.node_id())
        .map_err(AuthoringError::from)?;
    let mut bounds = None;
    for &leaf in &leaves {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        union_bounds(
            &mut bounds,
            effective_family_member_measure(store, execution, &object, boundary)?,
        );
    }
    Ok((leaves, bounds))
}

pub(crate) fn effective_family_member_measure(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
    boundary: bool,
) -> Result<Option<Bounds2D64>, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()?;
    if !execution.semantic_object_is_reachable(object.node_id()) {
        return if boundary {
            object.boundary_bounds()
        } else {
            object.layout_bounds()
        };
    }
    let store_ref = store.borrow();
    let observed = execution
        .effective_semantic_object(&store_ref, object.node_id())
        .map_err(AuthoringError::from)?;
    if !observed.authored_content_layout_applicable() {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
        ));
    }
    let transform = observed.object.transform;
    drop(store_ref);
    if boundary {
        object.boundary_bounds_at(transform)
    } else {
        object.layout_bounds_at(transform)
    }
}

pub(crate) fn effective_anchor_layout_measure(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    anchor: &LayoutAnchor,
    boundary: bool,
) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), AuthoringError> {
    if !Rc::ptr_eq(store, anchor.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let node = anchor.resolve()?;
    if matches!(
        store.borrow().node(node).map(|n| n.kind()),
        Some(noon_core::SemanticNodeKind::Family(_))
    ) {
        let family = MobjectFamily::from_node(Rc::clone(store), node)?;
        effective_family_layout_measure(store, execution, &family, boundary)
    } else {
        let object = Mobject::from_node(Rc::clone(store), node)?;
        let bounds = effective_family_member_measure(store, execution, &object, boundary)?;
        let bounds = match bounds {
            Some(bounds) => Some(bounds),
            None => {
                let (x, y) = effective_member_center(store, execution, &object)?;
                Some(Bounds2D64::point(x, y))
            }
        };
        Ok((vec![node], bounds))
    }
}

pub(crate) fn placement_authored_transform(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<Transform2D, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    let authored = object.state()?;
    let authored_transform = Transform2D {
        translation: authored.transform.translation.lower_xy_f32()?,
        rotation: authoring_render_f64(
            "move_to authored rotation",
            authored.transform.rotation_z,
        )? as f32,
        scale: authored.transform.scale.lower_xy_f32()?,
    };
    let store_ref = store.borrow();
    match execution.effective_semantic_object(&store_ref, object.node_id()) {
        Ok(observed) if !observed.authored_content_layout_applicable() => {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::PlacementRenderOverride,
            ));
        }
        Ok(observed) if observed.object.transform != authored_transform => {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver,
            ));
        }
        Ok(_) | Err(ExecutionSessionPublicationError::UnknownObject(_)) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(authored_transform)
}

fn effective_member_center(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<(f64, f64), AuthoringError> {
    if !execution.semantic_object_is_reachable(object.node_id()) {
        return object.center();
    }
    let store_ref = store.borrow();
    let observed = execution
        .effective_semantic_object(&store_ref, object.node_id())
        .map_err(AuthoringError::from)?;
    if !observed.authored_content_layout_applicable() {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
        ));
    }
    Ok((
        f64::from(observed.object.transform.translation.x),
        f64::from(observed.object.transform.translation.y),
    ))
}

fn live_target_point(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: LiveLayoutTarget<'_>,
    x: f64,
    y: f64,
) -> Result<(f64, f64), AuthoringError> {
    match target {
        LiveLayoutTarget::Point(px, py) => {
            let point = authoring_xy_f64(px, py)?;
            Ok((point.x, point.y))
        }
        LiveLayoutTarget::Mobject(object) => {
            match effective_family_member_measure(store, execution, object, true)? {
                Some(bounds) => Ok(bounds_critical_point(Some(bounds), x, y)),
                None => effective_member_center(store, execution, object),
            }
        }
        LiveLayoutTarget::Anchor(anchor) => effective_anchor_layout_measure(
            store,
            execution,
            anchor,
            true,
        )
        .map(|(_, bounds)| bounds_critical_point(bounds, x, y)),
        LiveLayoutTarget::Family(family) => effective_family_layout_measure(
            store,
            execution,
            family,
            true,
        )
        .map(|(_, bounds)| bounds_critical_point(bounds, x, y)),
    }
}

fn publish_layout_translation(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    leaves: Vec<SemanticNodeId>,
    bounds: Option<Bounds2D64>,
    target: LiveLayoutTarget<'_>,
    placement: RelativePlacement,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let delta = placement.delta(bounds, |x, y| {
        live_target_point(store, execution, target, x, y)
    })?;
    for &leaf in &leaves {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        placement_authored_transform(store, execution, &object)?;
    }
    let transaction = FamilyTranslation::from_members(leaves, delta.0, delta.1)?
        .transaction(&store.borrow())?;
    crate::Scene::publish_running_transaction(store, root, execution, transaction)
        .map_err(AuthoringError::from)
}

pub(crate) fn publish_shift_family(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    x: f64,
    y: f64,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    if !Rc::ptr_eq(store, family.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    family.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let transaction = FamilyTranslation::begin(&store.borrow(), family.node_id(), x, y)?
        .transaction(&store.borrow())?;
    crate::Scene::publish_running_transaction(store, root, execution, transaction)
        .map_err(AuthoringError::from)
}

pub(crate) fn publish_arrange_family(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    options: &crate::FamilyArrangeOptions,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    if !Rc::ptr_eq(store, family.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    family.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let plan = FamilyArrangePlan::begin(family, options)?;
    publish_family_arrangement(store, root, execution, plan)
}

pub(crate) fn publish_arrange_family_in_grid(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    options: &crate::FamilyGridOptions,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    if !Rc::ptr_eq(store, family.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    family.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let plan = FamilyArrangePlan::grid(family, options)?;
    publish_family_arrangement(store, root, execution, plan)
}

fn publish_family_arrangement(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    mut plan: FamilyArrangePlan,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    plan.observe_leaf_bounds(|leaf| {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        Ok::<_, AuthoringError>(crate::family_arrangement::ArrangementBounds {
            dimensions: effective_family_member_measure(store, execution, &object, false)?,
            anchors: effective_family_member_measure(store, execution, &object, true)?,
        })
    })?;
    let transaction = plan.transaction(|leaf| {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        placement_authored_transform(store, execution, &object)?;
        object.state().map(|state| state.transform.translation)
    })?;
    crate::Scene::publish_running_transaction(store, root, execution, transaction)
        .map_err(AuthoringError::from)
}

pub(crate) fn publish_move_to(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    target: LiveLayoutTarget<'_>,
    edge: (f64, f64),
    mask: (f64, f64),
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let transform = placement_authored_transform(store, execution, object)?;
    let bounds = object.boundary_bounds_at(transform)?.unwrap_or_else(|| {
        Bounds2D64::point(
            f64::from(transform.translation.x),
            f64::from(transform.translation.y),
        )
    });
    publish_layout_translation(
        store,
        root,
        execution,
        vec![object.node_id()],
        Some(bounds),
        target,
        RelativePlacement::Move { edge, mask },
    )
}

pub(crate) fn publish_move_family_to(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    target: LiveLayoutTarget<'_>,
    edge: (f64, f64),
    mask: (f64, f64),
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    let (leaves, bounds) = effective_family_layout_measure(store, execution, family, true)?;
    publish_layout_translation(
        store,
        root,
        execution,
        leaves,
        bounds,
        target,
        RelativePlacement::Move { edge, mask },
    )
}

pub(crate) fn publish_next_family_to(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    target: LiveLayoutTarget<'_>,
    args: ManimNextToArgs,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    let (leaves, bounds) = effective_family_layout_measure(store, execution, family, true)?;
    publish_layout_translation(
        store,
        root,
        execution,
        leaves,
        bounds,
        target,
        RelativePlacement::Next(args),
    )
}

pub(crate) fn publish_align_family_on_frame(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    direction: (f64, f64),
    buff: f64,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    let target = frame_alignment_target(direction, buff)?;
    publish_align_family_to(
        store,
        root,
        execution,
        family,
        LiveLayoutTarget::Point(target.0, target.1),
        direction,
    )
}

pub(crate) fn publish_align_family_to(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    target: LiveLayoutTarget<'_>,
    axis: (f64, f64),
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    let (leaves, bounds) = effective_family_layout_measure(store, execution, family, true)?;
    publish_layout_translation(
        store,
        root,
        execution,
        leaves,
        bounds,
        target,
        RelativePlacement::Align(axis),
    )
}

pub(crate) fn publish_next_layout_to_aligned(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    source: &LayoutAnchor,
    target: LiveLayoutTarget<'_>,
    aligner: &LayoutAnchor,
    args: ManimNextToArgs,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    let (leaves, _) = effective_anchor_layout_measure(store, execution, source, false)?;
    let (_, bounds) = effective_anchor_layout_measure(store, execution, aligner, true)?;
    publish_layout_translation(
        store,
        root,
        execution,
        leaves,
        bounds,
        target,
        RelativePlacement::Next(args),
    )
}
