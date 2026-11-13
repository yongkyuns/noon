//! Immutable authored family observations and atomic relative placement.

mod placement;
mod point_queries;
use crate::AuthoringError;
pub(crate) use placement::effective_placement_transform;
pub(crate) use point_queries::effective_layout_critical_point_3d;
use std::{cell::RefCell, rc::Rc};

use crate::{
    family_authoring::FamilyTranslation,
    semantic_mobject::{authoring_render_f64, authoring_xy_f64, ManimNextToArgs},
    Bounds2D64, ExecutionSession, ExecutionSessionPublicationError, Mobject, MobjectFamily,
    SemanticNodeId, Transform2D,
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

/// A placement destination resolved by the owning [`crate::Scene`].
///
/// During playback, the Scene observes the current publication before applying placement.
#[derive(Clone, Copy)]
pub enum LiveLayoutTarget<'a> {
    Point(f64, f64),
    Mobject(&'a Mobject),
    Family(&'a MobjectFamily),
    Anchor(&'a LayoutAnchor),
}

/// A layout reference into the existing semantic store, optionally selecting a
/// member of Manim's split view. Point-bearing geometry includes itself before
/// its direct children; families and empty geometry start with their children.
/// Negative indices count from the end. Selection is resolved on each request,
/// without retaining a bounds or membership snapshot.
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
    /// Place at a selected target edge and fit the result to the default frame.
    /// Both adjustments are prepared before one atomic translation.
    pub fn next_to_edge_in_frame(
        &self,
        target: &LayoutAnchor,
        edge: (f64, f64),
        args: ManimNextToArgs,
        frame_buff: f64,
    ) -> Result<(), AuthoringError> {
        self.layout()?.place(
            FamilyLayoutTarget::Anchor(target),
            RelativePlacement::NextInFrame {
                args,
                edge,
                frame_buff,
            },
        )
    }

    /// Check that this anchor belongs to an expected semantic store without
    /// resolving its identity there.
    pub fn belongs_to_store(&self, expected: &Rc<RefCell<SemanticStore>>) -> bool {
        Rc::ptr_eq(&self.store, expected)
    }

    /// Resolve this checked layout anchor to its semantic node identity.
    ///
    /// Callers that accept handles from another authoring context should first
    /// use [`Self::belongs_to_store`] to ensure resolution occurs in the expected
    /// store.
    pub fn resolve_checked(&self) -> Result<SemanticNodeId, AuthoringError> {
        self.resolve()
    }

    /// Select an entry in the shared split view, without traversing wrapper trees.
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
        let includes_self = match node.kind() {
            noon_core::SemanticNodeKind::Family(_) => false,
            noon_core::SemanticNodeKind::AuthoringObject => {
                let state = store.semantic_object_state_checked(self.node)?;
                let path = crate::path_queries::content_path(&store, state.content)?;
                crate::path_queries::PathQuery::prepare(
                    &path,
                    noon_core::SemanticTransform2_5D::default(),
                )?
                .point_count()
                    > 0
            }
            _ => {
                return Err(
                    noon_core::SemanticSceneOperationError::NotSemanticFamily(self.node).into(),
                )
            }
        };
        let members = node.members();
        let count = members.len() + usize::from(includes_self);
        let requested_index = index;
        let index = if index < 0 {
            count.checked_add_signed(index)
        } else {
            Some(index as usize)
        };
        index
            .filter(|&index| index < count)
            .map(|index| {
                if includes_self && index == 0 {
                    self.node
                } else {
                    members[index - usize::from(includes_self)]
                }
            })
            .ok_or(AuthoringError::InvalidSubmobjectIndex {
                family: self.node,
                index: requested_index,
            })
    }

    /// Observe the selected root's own content and unique ordered descendants.
    pub fn layout(&self) -> Result<FamilyLayout, AuthoringError> {
        authored_layout(&self.store, self.resolve()?)
    }

    /// Measure content extents without requiring valid vector anchors.
    pub(crate) fn dimension_bounds(&self) -> Result<Option<Bounds2D64>, AuthoringError> {
        let leaves = self
            .store
            .borrow()
            .ordered_leaf_nodes(self.resolve()?)
            .map_err(AuthoringError::from)?;
        let mut bounds = None;
        for leaf in leaves {
            let object = Mobject::from_node(Rc::clone(&self.store), leaf)?;
            union_bounds(&mut bounds, object.content_layout_bounds()?);
        }
        Ok(bounds)
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
    /// Observe only this family's layout and ordered semantic content.
    pub fn layout(&self) -> Result<FamilyLayout, AuthoringError> {
        self.validate()?;
        authored_layout(self.integration_store(), self.node_id())
    }

    /// Shift each semantic leaf once without querying its geometry.
    pub fn shift(&self, x: f64, y: f64) -> Result<(), AuthoringError> {
        self.shift_3d(noon_core::SemanticVec3::new(x, y, 0.0))
    }

    pub fn shift_3d(&self, delta: noon_core::SemanticVec3) -> Result<(), AuthoringError> {
        let translation =
            FamilyTranslation::begin_3d(&self.integration_store().borrow(), self.node_id(), delta)?;
        translation.apply(&mut self.integration_store().borrow_mut())
    }
}

/// One root-scoped collection; per-content reads must not retraverse descendants.
fn authored_layout(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
) -> Result<FamilyLayout, AuthoringError> {
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(root)
        .map_err(AuthoringError::from)?;
    let mut bounds = None;
    let mut boundary = None;
    for &leaf in &leaves {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        union_bounds(&mut bounds, object.content_layout_bounds()?);
        union_bounds(&mut boundary, object.content_boundary_bounds()?);
    }
    if bounds.is_none()
        && store.borrow().node(root).is_some_and(|node| {
            matches!(node.kind(), noon_core::SemanticNodeKind::AuthoringObject)
                && node.members_iter().next().is_none()
        })
    {
        let (x, y) = Mobject::from_node(Rc::clone(store), root)?.content_center()?;
        bounds = Some(Bounds2D64::point(x, y));
        boundary = boundary.or(bounds);
    }
    Ok(FamilyLayout {
        store: Rc::clone(store),
        leaves,
        bounds,
        boundary,
    })
}

impl FamilyLayout {
    pub(crate) fn into_placement(self) -> (Vec<SemanticNodeId>, Option<Bounds2D64>) {
        (self.leaves, self.boundary)
    }

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
        self.shift_3d(noon_core::SemanticVec3::new(x, y, 0.0))
    }

    pub fn shift_3d(&self, delta: noon_core::SemanticVec3) -> Result<(), AuthoringError> {
        FamilyTranslation::from_members_3d(self.leaves.clone(), delta)?
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
    Move {
        edge: (f64, f64),
        mask: (f64, f64),
    },
    Next(ManimNextToArgs),
    NextInFrame {
        args: ManimNextToArgs,
        edge: (f64, f64),
        frame_buff: f64,
    },
    Align((f64, f64)),
}

impl RelativePlacement {
    pub(crate) fn delta<E: From<AuthoringError>>(
        self,
        bounds: Option<Bounds2D64>,
        target: impl FnOnce(f64, f64) -> Result<(f64, f64), E>,
    ) -> Result<(f64, f64), E> {
        let fitting = if let Self::NextInFrame {
            edge, frame_buff, ..
        } = &self
        {
            let edge = authoring_xy_f64(edge.0, edge.1)?;
            Some(((edge.x, edge.y), *frame_buff))
        } else {
            None
        };
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
            Self::Next(args) | Self::NextInFrame { args, .. } => {
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
        let target_axis = fitting.map_or(target_axis, |(edge, _)| edge);
        let target = target(target_axis.0, target_axis.1)?;
        let mut delta = (
            (target.0 - source.0 + offset.0) * mask.0,
            (target.1 - source.1 + offset.1) * mask.1,
        );
        if let Some((_, buff)) = fitting {
            fit_translation_to_default_frame(bounds, &mut delta, buff)?;
        }
        Ok(delta)
    }
}

/// Preserve Manim's UP, DOWN, LEFT, RIGHT correction order, including labels
/// larger than the frame. This is preparation math; it performs no publication.
pub(crate) fn fit_translation_to_default_frame(
    bounds: Option<Bounds2D64>,
    delta: &mut (f64, f64),
    buff: f64,
) -> Result<(), AuthoringError> {
    let buff = authoring_render_f64("frame buffer", buff)?;
    if buff < 0.0 {
        return Err(AuthoringError::NonFiniteTransform);
    }
    let half_width = f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5;
    let half_height = f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5;
    for (direction, limit) in [
        ((0.0, 1.0), half_height - buff),
        ((0.0, -1.0), half_height - buff),
        ((-1.0, 0.0), half_width - buff),
        ((1.0, 0.0), half_width - buff),
    ] {
        let point = bounds_critical_point(bounds, direction.0, direction.1);
        let projection = (point.0 + delta.0) * direction.0 + (point.1 + delta.1) * direction.1;
        if projection > limit {
            delta.0 += (limit - projection) * direction.0;
            delta.1 += (limit - projection) * direction.1;
        }
    }
    authoring_xy_f64(delta.0, delta.1)?;
    Ok(())
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

/// Read a fitting extent from the addressed layout scope.
pub(crate) fn effective_dimension_length(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    anchor: &LayoutAnchor,
    dimension: crate::LayoutDimension,
) -> Result<f64, AuthoringError> {
    if dimension != crate::LayoutDimension::Depth {
        let (_, bounds) = effective_anchor_layout_measure(store, execution, anchor, false)?;
        return Ok(dimension.length(bounds));
    }
    if !anchor.belongs_to_store(store) {
        return Err(AuthoringError::ForeignStore);
    }
    execution.require_published_store(&store.borrow())?;
    let root = anchor.resolve()?;
    let store = store.borrow();
    let leaves = store.ordered_leaf_nodes(root)?;
    // The planar runtime has no independent depth driver. Validate the
    // content publication before reading its canonical Z data.
    for &leaf in &leaves {
        if execution.semantic_object_is_reachable(leaf) {
            let observed = execution.effective_semantic_object(&store, leaf)?;
            if !observed.authored_content_layout_applicable() {
                return Err(AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
                ));
            }
        }
    }
    crate::dimension_fit::depth_length_for_leaves(&store, &leaves)
}

/// Observe an object's own content and descendants at one effective publication.
pub(crate) fn effective_mobject_layout(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<crate::EffectiveMobjectLayout, AuthoringError> {
    crate::MobjectTarget::Object(object).require_store(store)?;
    if object.has_members()? {
        let anchor = LayoutAnchor::from(object);
        let (_, bounds) = effective_anchor_layout_measure(store, execution, &anchor, false)?;
        let (_, boundary) = effective_anchor_layout_measure(store, execution, &anchor, true)?;
        return Ok(crate::EffectiveMobjectLayout {
            center: bounds_critical_point(boundary, 0.0, 0.0),
            width: bounds.map_or(0.0, Bounds2D64::width),
            height: bounds.map_or(0.0, Bounds2D64::height),
            publication: execution.publication_context(),
        });
    }
    let store_ref = store.borrow();
    let observed = execution
        .effective_semantic_object(&store_ref, object.node_id())
        .map_err(AuthoringError::from)?;
    if !observed.authored_content_layout_applicable() {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveLayoutRenderOverride,
        ));
    }
    let transform = observed.object.transform;
    let publication = observed.publication;
    drop(store_ref);
    let bounds = object.layout_bounds_at(transform)?;
    let boundary = object.boundary_bounds_at(transform)?;
    Ok(crate::EffectiveMobjectLayout {
        center: crate::semantic_mobject::boundary_center(boundary, (0.0, 0.0)),
        width: bounds.map_or(0.0, Bounds2D64::width),
        height: bounds.map_or(0.0, Bounds2D64::height),
        publication,
    })
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
    effective_root_layout_measure(store, execution, family.node_id(), boundary)
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
            object.content_boundary_bounds()
        } else {
            object.content_layout_bounds()
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
    effective_root_layout_measure(store, execution, anchor.resolve()?, boundary)
}

/// Collect own-content measurements once per unique addressed identity. Callers
/// validate store ownership and the coherent publication before entering here.
fn effective_root_layout_measure(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    node: SemanticNodeId,
    boundary: bool,
) -> Result<(Vec<SemanticNodeId>, Option<Bounds2D64>), AuthoringError> {
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(node)
        .map_err(AuthoringError::from)?;
    let mut bounds = None;
    for &leaf in &leaves {
        let object = Mobject::from_node(Rc::clone(store), leaf)?;
        union_bounds(
            &mut bounds,
            effective_family_member_measure(store, execution, &object, boundary)?,
        );
    }
    if bounds.is_none()
        && store.borrow().node(node).is_some_and(|node| {
            matches!(node.kind(), noon_core::SemanticNodeKind::AuthoringObject)
                && node.members_iter().next().is_none()
        })
    {
        let object = Mobject::from_node(Rc::clone(store), node)?;
        let (x, y) = effective_member_center(store, execution, &object)?;
        bounds = Some(Bounds2D64::point(x, y));
    }
    Ok((leaves, bounds))
}

/// Validate that persistent composite placement can safely use an entry's state.
/// Existing affine/reactive drivers cannot be baked into a new authored position.
#[cfg(any(feature = "native-text", feature = "latex", test))]
pub(crate) fn composite_entry_state(
    store: &Rc<RefCell<SemanticStore>>,
    execution: Option<&ExecutionSession>,
    root: SemanticNodeId,
    object: &Mobject,
) -> Result<noon_core::SemanticObjectState, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    if let Some(execution) = execution {
        execution.require_resource_creation_at_root(&store.borrow(), root)?;
        placement_authored_transform(store, execution, object)?;
        return crate::effective_capture::capture_mobject_state(store, execution, object);
    }
    object.state()
}

pub(crate) fn placement_authored_transform(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<Transform2D, AuthoringError> {
    effective_placement_transform(store, execution, object, false).map(|(transform, _)| transform)
}

fn effective_member_center(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<(f64, f64), AuthoringError> {
    if !execution.semantic_object_is_reachable(object.node_id()) {
        return object.content_center();
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

pub(crate) fn placement_target_point(
    store: &Rc<RefCell<SemanticStore>>,
    execution: Option<&ExecutionSession>,
    target: LiveLayoutTarget<'_>,
    x: f64,
    y: f64,
) -> Result<(f64, f64), AuthoringError> {
    match (execution, target) {
        (_, LiveLayoutTarget::Point(px, py)) => {
            let point = authoring_xy_f64(px, py)?;
            Ok((point.x, point.y))
        }
        (Some(execution), LiveLayoutTarget::Mobject(object)) => {
            effective_anchor_layout_measure(store, execution, &LayoutAnchor::from(object), true)
                .map(|(_, bounds)| bounds_critical_point(bounds, x, y))
        }
        (Some(execution), LiveLayoutTarget::Anchor(anchor)) => {
            effective_anchor_layout_measure(store, execution, anchor, true)
                .map(|(_, bounds)| bounds_critical_point(bounds, x, y))
        }
        (Some(execution), LiveLayoutTarget::Family(family)) => {
            effective_family_layout_measure(store, execution, family, true)
                .map(|(_, bounds)| bounds_critical_point(bounds, x, y))
        }
        (None, LiveLayoutTarget::Mobject(object)) => {
            if !Rc::ptr_eq(store, object.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            object.validate()?;
            object.critical_point(x, y)
        }
        (None, LiveLayoutTarget::Family(family)) => {
            if !Rc::ptr_eq(store, family.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            Ok(family.layout()?.critical_point(x, y))
        }
        (None, LiveLayoutTarget::Anchor(anchor)) => {
            if !anchor.belongs_to_store(store) {
                return Err(AuthoringError::ForeignStore);
            }
            Ok(anchor.layout()?.critical_point(x, y))
        }
    }
}

#[cfg(test)]
mod placement_callback_tests;
