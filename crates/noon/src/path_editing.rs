//! Persistent vector edits publish new immutable content through one transaction.
mod transaction;
use crate::{AuthoringError, ExecutionSession, Mobject, MobjectFamily};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectContent, SemanticObjectState,
    SemanticStore, StoredGeometry, Vec2, VectorPath,
};
use std::{cell::RefCell, rc::Rc};
pub(crate) use transaction::PreparedPathEdits;

/// Validate that one running Scene root may admit immutable path resources.
///
/// Callers that must preserve fail-closed ordering can invoke this before
/// effective capture; [`publish_running_path_edits`] repeats the check for any
/// prepared edit that actually creates resources.
pub(crate) fn require_running_path_resource_admission(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &ExecutionSession,
) -> Result<(), AuthoringError> {
    execution
        .require_resource_creation_at_root(&store.borrow(), root)
        .map_err(AuthoringError::from)
}

/// Publish one already-prepared path edit through the running Scene authority.
///
/// Resource admission and immutable-resource rollback remain inside the same
/// `PreparedPathEdits::publish` scope as semantic publication, so a rejected
/// running mutation cannot leak newly admitted geometry resources.
pub(crate) fn publish_running_path_edits(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    prepared: PreparedPathEdits,
) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
    if prepared.creates_resources() {
        require_running_path_resource_admission(store, root, execution)?;
    }
    let mut store = store.borrow_mut();
    prepared.publish(&mut store, |store, transaction| {
        execution
            .apply_semantic_transaction_at_root(store, root, transaction)
            .map_err(AuthoringError::from)
    })
}

fn require_running_object(
    store: &Rc<RefCell<SemanticStore>>,
    object: &Mobject,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()
}

fn require_running_family(
    store: &Rc<RefCell<SemanticStore>>,
    family: &MobjectFamily,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, family.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    family.validate()
}

/// Capture, prepare and publish one ordinary running path edit.
///
/// Resource admission remains before effective capture, preserving the existing
/// fail-closed ordering for edits that may allocate immutable geometry.
pub(crate) fn publish_running_object_edit(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    edit: PathEdit<'_>,
) -> Result<(), AuthoringError> {
    require_running_object(store, object)?;
    require_running_path_resource_admission(store, root, execution)?;
    let captured = crate::effective_capture::capture_mobject_state(store, execution, object)?;
    let store_ref = store.borrow();
    let Some(prepared) = prepare_object_edit(&store_ref, object.node_id(), captured, edit)? else {
        return Ok(());
    };
    drop(store_ref);
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

/// Apply one matrix edit from the current coherent effective object state.
pub(crate) fn publish_running_apply_matrix(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    values: &[f64],
    dimensions: (usize, usize),
    about: (f64, f64),
) -> Result<(), AuthoringError> {
    require_running_object(store, object)?;
    let captured = crate::effective_capture::capture_mobject_state(store, execution, object)?;
    let prepared = {
        let store = store.borrow();
        crate::matrix_authoring::prepare_apply_matrix(
            &store,
            object.node_id(),
            captured,
            values,
            dimensions.0,
            dimensions.1,
            about,
        )?
    };
    let Some(prepared) = prepared else {
        return Ok(());
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

/// Capture the source and replace one destination with its selected path interval.
pub(crate) fn publish_running_pointwise_partial(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    source: &Mobject,
    a: f64,
    b: f64,
) -> Result<(), AuthoringError> {
    require_running_object(store, source)?;
    let (a, b) = partial_interval(a, b)?;
    let source = crate::effective_capture::capture_mobject_state(store, execution, source)?;
    publish_running_object_edit(
        store,
        root,
        execution,
        object,
        PathEdit::Partial {
            source: &source,
            a,
            b,
        },
    )
}

/// Append a source vector object's current effective path to one destination.
pub(crate) fn publish_running_append_vectorized_mobject(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    source: &Mobject,
) -> Result<(), AuthoringError> {
    publish_running_append_vectorized_mobject_with_stride(store, root, execution, object, source, 4)
}

/// Append a vector object's current effective raw points, classifying a
/// pending destination anchor with the destination's mutable Manim stride.
pub(crate) fn publish_running_append_vectorized_mobject_with_stride(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    source: &Mobject,
    stride: i32,
) -> Result<(), AuthoringError> {
    require_running_object(store, source)?;
    let source = crate::effective_capture::capture_mobject_state(store, execution, source)?;
    publish_running_object_edit(
        store,
        root,
        execution,
        object,
        PathEdit::AppendVectorizedWithStride {
            source: &source,
            stride,
        },
    )
}

/// Change every authoritative family leaf between smooth and jagged anchors.
pub(crate) fn publish_running_family_anchor_mode(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    smooth: bool,
) -> Result<(), AuthoringError> {
    require_running_family(store, family)?;
    require_running_path_resource_admission(store, root, execution)?;
    let nodes = store
        .borrow()
        .ordered_leaf_nodes(family.node_id())
        .map_err(AuthoringError::from)?;
    let states = nodes
        .into_iter()
        .map(|node| {
            let object = Mobject::from_node(Rc::clone(store), node)?;
            Ok((
                node,
                crate::effective_capture::capture_mobject_state(store, execution, &object)?,
            ))
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;
    let prepared = {
        let store = store.borrow();
        crate::path_smoothing::prepare_anchor_edits(&store, states, smooth)?
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

/// Reverse point order for every supported geometry leaf in one authoritative
/// family transaction. Text and image resources have no shared vector-point
/// payload in Noon and are left unchanged.
pub(crate) fn publish_running_family_reverse_points(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
) -> Result<(), AuthoringError> {
    require_running_family(store, family)?;
    require_running_path_resource_admission(store, root, execution)?;
    let nodes = store
        .borrow()
        .ordered_last_occurrence_leaf_nodes(family.node_id())
        .map_err(AuthoringError::from)?;
    let states = nodes
        .into_iter()
        .map(|node| {
            let object = Mobject::from_node(Rc::clone(store), node)?;
            Ok((
                node,
                crate::effective_capture::capture_mobject_state(store, execution, &object)?,
            ))
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;
    let prepared = {
        let store = store.borrow();
        prepare_family_reverse_points(&store, states)?
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

pub(crate) fn publish_running_object_reverse_points(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
) -> Result<(), AuthoringError> {
    require_running_object(store, object)?;
    require_running_path_resource_admission(store, root, execution)?;
    let state = crate::effective_capture::capture_mobject_state(store, execution, object)?;
    let prepared = {
        let store = store.borrow();
        prepare_family_reverse_points(&store, [(object.node_id(), state)])?
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

fn prepare_family_reverse_points(
    store: &SemanticStore,
    states: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState)>,
) -> Result<PreparedPathEdits, AuthoringError> {
    let mut replacements = Vec::new();
    for (node, state) in states {
        let supported = match state.content.geometry() {
            Some(
                StoredGeometry::Circle { .. }
                | StoredGeometry::Rectangle { .. }
                | StoredGeometry::Line { .. },
            ) => true,
            Some(StoredGeometry::Resource(handle)) => matches!(
                store.geometry_resources().get(handle),
                Some(noon_core::GeometryResource::VectorPath(_))
            ),
            None => false,
        };
        if !supported {
            continue;
        }
        let path = crate::path_queries::content_path(store, state.content)?;
        replacements.push((node, state, reverse_point_order(path.as_ref())?));
    }
    PreparedPathEdits::prepare_preserving_transform(store, replacements)
}

fn reverse_point_order(path: &VectorPath) -> Result<VectorPath, AuthoringError> {
    let mut points = manim_points_for_path(path)?;
    points.reverse();
    let mut reversed = VectorPath::from_manim_points(points);
    if let Some(target) = path.morph_target() {
        reversed = reversed.with_morph_target(reverse_point_order(target)?);
    }
    Ok(reversed)
}

/// Repeat the raw point array as Manim's `apply_over_attr_arrays(repeated)` does.
fn repeat_point_array(path: &VectorPath, count: usize) -> Result<VectorPath, AuthoringError> {
    let points = manim_points_for_path(path)?;
    let total = points
        .len()
        .checked_mul(count)
        .ok_or(AuthoringError::PointArrayCapacityExceeded(usize::MAX))?;
    let mut repeated = Vec::new();
    repeated
        .try_reserve_exact(total)
        .map_err(|_| AuthoringError::PointArrayCapacityExceeded(total))?;
    for _ in 0..count {
        repeated.extend_from_slice(&points);
    }
    let mut result = VectorPath::from_manim_points(repeated);
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(repeat_point_array(target, count)?);
    }
    Ok(result)
}

fn prepare_family_repeat_points(
    store: &SemanticStore,
    states: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState)>,
    count: usize,
) -> Result<PreparedPathEdits, AuthoringError> {
    let mut replacements = Vec::new();
    for (node, state) in states {
        let supported = match state.content.geometry() {
            Some(
                StoredGeometry::Circle { .. }
                | StoredGeometry::Rectangle { .. }
                | StoredGeometry::Line { .. },
            ) => true,
            Some(StoredGeometry::Resource(handle)) => matches!(
                store.geometry_resources().get(handle),
                Some(noon_core::GeometryResource::VectorPath(_))
            ),
            None => false,
        };
        if !supported {
            continue;
        }
        let path = crate::path_queries::content_path(store, state.content)?;
        if manim_points_for_path(path.as_ref())?.is_empty() {
            continue;
        }
        replacements.push((node, state, repeat_point_array(path.as_ref(), count)?));
    }
    PreparedPathEdits::prepare_preserving_transform(store, replacements)
}

pub(crate) fn publish_running_family_repeat_points(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    family: &MobjectFamily,
    count: usize,
) -> Result<(), AuthoringError> {
    require_running_family(store, family)?;
    require_running_path_resource_admission(store, root, execution)?;
    if count == 0 {
        return Ok(());
    }
    let nodes = store
        .borrow()
        .ordered_last_occurrence_leaf_nodes(family.node_id())
        .map_err(AuthoringError::from)?;
    let states = nodes
        .into_iter()
        .map(|node| {
            let object = Mobject::from_node(Rc::clone(store), node)?;
            Ok((
                node,
                crate::effective_capture::capture_mobject_state(store, execution, &object)?,
            ))
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;
    let prepared = {
        let store = store.borrow();
        prepare_family_repeat_points(&store, states, count)?
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

pub(crate) fn publish_running_object_repeat_points(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    object: &Mobject,
    count: usize,
) -> Result<(), AuthoringError> {
    require_running_object(store, object)?;
    require_running_path_resource_admission(store, root, execution)?;
    if count == 0 {
        return Ok(());
    }
    let state = crate::effective_capture::capture_mobject_state(store, execution, object)?;
    let prepared = {
        let store = store.borrow();
        prepare_family_repeat_points(&store, [(object.node_id(), state)], count)?
    };
    publish_running_path_edits(store, root, execution, prepared).map(|_| ())
}

impl MobjectFamily {
    /// Reverse the stored point order of each supported geometry leaf while
    /// retaining the family's membership, paint, and member transforms.
    pub fn reverse_points(&self) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut store = self.integration_store().borrow_mut();
        let nodes = store
            .ordered_last_occurrence_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let states = nodes
            .into_iter()
            .map(|node| Ok((node, store.semantic_object_state_checked(node)?.clone())))
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let prepared = prepare_family_reverse_points(&store, states)?;
        prepared.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }

    /// Repeat each supported vector leaf's raw point array through one family transaction.
    pub fn repeat_points(&self, count: usize) -> Result<(), AuthoringError> {
        self.validate()?;
        if count == 0 {
            return Ok(());
        }
        let mut store = self.integration_store().borrow_mut();
        let nodes = store
            .ordered_last_occurrence_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let states = nodes
            .into_iter()
            .map(|node| Ok((node, store.semantic_object_state_checked(node)?.clone())))
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let prepared = prepare_family_repeat_points(&store, states, count)?;
        prepared.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}

impl Mobject {
    /// Reverse this object's local point order while retaining its affine state.
    pub fn reverse_points(&self) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut store = self.integration_store().borrow_mut();
        let state = store.semantic_object_state_checked(self.node_id())?.clone();
        let prepared = prepare_family_reverse_points(&store, [(self.node_id(), state)])?;
        prepared.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }

    /// Repeat this object's local raw point array while retaining its affine state.
    pub fn repeat_points(&self, count: usize) -> Result<(), AuthoringError> {
        self.validate()?;
        if count == 0 {
            return Ok(());
        }
        let mut store = self.integration_store().borrow_mut();
        let state = store.semantic_object_state_checked(self.node_id())?.clone();
        let prepared = prepare_family_repeat_points(&store, [(self.node_id(), state)], count)?;
        prepared.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}

impl crate::Scene {
    /// Apply a pointwise matrix through this Scene's running publication authority.
    pub fn apply_matrix(
        &mut self,
        object: &Mobject,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_apply_matrix(
                store,
                root,
                execution,
                object,
                values,
                (rows, columns),
                (about_x, about_y),
            )
        })
    }

    pub fn set_points(&mut self, object: &Mobject, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::RawPoints(points))
        })
    }

    pub fn append_points(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::AppendRawPoints(points),
            )
        })
    }

    pub fn set_points_smoothly(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::SmoothCorners(points),
            )
        })
    }

    pub fn make_smooth(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::AnchorMode(true))
        })
    }

    pub fn make_jagged(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::AnchorMode(false))
        })
    }

    pub fn scale_handle_to_anchor_distances(
        &mut self,
        object: &Mobject,
        factor: f64,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::ScaleHandles(factor),
            )
        })
    }

    pub fn set_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Corners(points))
        })
    }

    pub fn set_points_as_corners_with_samples(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
        bezier_t_values: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::CornersWithSamples {
                    points,
                    bezier_t_values,
                },
            )
        })
    }

    pub fn set_cubic_bezier_curves(
        &mut self,
        object: &Mobject,
        curves: &[[Vec2; 4]],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::SetCubicCurves(curves),
            )
        })
    }

    pub fn start_new_path(&mut self, object: &Mobject, point: Vec2) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Start { point, stride: 4 },
            )
        })
    }

    pub fn start_new_path_with_stride(
        &mut self,
        object: &Mobject,
        point: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Start { point, stride },
            )
        })
    }

    pub fn add_line_to(&mut self, object: &Mobject, point: Vec2) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Line(point))
        })
    }

    pub fn add_line_to_with_stride_and_samples(
        &mut self,
        object: &Mobject,
        point: Vec2,
        stride: i32,
        bezier_t_values: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::LineWithSamples {
                    point,
                    stride,
                    bezier_t_values,
                },
            )
        })
    }

    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        control: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Quadratic(control, anchor),
            )
        })
    }

    pub fn clear_points(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Clear)
        })
    }

    pub fn add_cubic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        c1: Vec2,
        c2: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Cubic(c1, c2, anchor),
            )
        })
    }

    pub fn add_smooth_curve_to(
        &mut self,
        object: &Mobject,
        handle2: Option<Vec2>,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::SmoothCurve { handle2, anchor },
            )
        })
    }

    pub fn add_cubic_bezier_curve(
        &mut self,
        object: &Mobject,
        anchor1: Vec2,
        handle1: Vec2,
        handle2: Vec2,
        anchor2: Vec2,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::CubicCurve(anchor1, handle1, handle2, anchor2),
            )
        })
    }

    pub fn add_cubic_bezier_curves(
        &mut self,
        object: &Mobject,
        curves: &[[Vec2; 4]],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::CubicCurves(curves),
            )
        })
    }

    pub fn add_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::AppendCorners(points),
            )
        })
    }

    pub fn add_points_as_corners_with_stride(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
        stride: i32,
        bezier_t_values: &[f64],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::AppendCornersWithStride {
                    points,
                    stride,
                    bezier_t_values,
                },
            )
        })
    }

    pub fn append_vectorized_mobject(
        &mut self,
        object: &Mobject,
        source: &Mobject,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_append_vectorized_mobject(store, root, execution, object, source)
        })
    }

    /// Append another vector object's points using the mutable destination stride.
    pub fn append_vectorized_mobject_with_stride(
        &mut self,
        object: &Mobject,
        source: &Mobject,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_append_vectorized_mobject_with_stride(
                store, root, execution, object, source, stride,
            )
        })
    }

    pub fn close_path(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Close)
        })
    }

    pub fn close_path_with_stride(
        &mut self,
        object: &Mobject,
        stride: i32,
        tolerance: f64,
        bezier_t_values: &[f64],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::CloseWithStride {
                    stride,
                    tolerance,
                    bezier_t_values,
                },
            )
        })
    }

    pub fn insert_n_curves(
        &mut self,
        object: &Mobject,
        additional: usize,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Subdivide(additional),
            )
        })
    }

    pub fn reverse_direction(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Reverse)
        })
    }

    pub fn pointwise_become_partial(
        &mut self,
        object: &Mobject,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_pointwise_partial(store, root, execution, object, source, a, b)
        })
    }

    pub fn make_family_smooth(&mut self, family: &MobjectFamily) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_family_anchor_mode(store, root, execution, family, true)
        })
    }

    pub fn make_family_jagged(&mut self, family: &MobjectFamily) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_family_anchor_mode(store, root, execution, family, false)
        })
    }
}

pub(crate) fn corners_path(points: &[Vec2]) -> Result<VectorPath, AuthoringError> {
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    // Manim has no complete curve when fewer than two corners are supplied.
    if points.len() < 2 {
        return Ok(VectorPath::new());
    }
    let mut path = VectorPath::new().move_to(points[0]);
    for point in &points[1..] {
        path = path.line_to(*point);
    }
    if points.first() == points.last() {
        path = path.close();
    }
    Ok(path)
}

fn corners_path_with_samples(
    points: &[Vec2],
    bezier_t_values: &[f64; 4],
) -> Result<VectorPath, AuthoringError> {
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
        || bezier_t_values.iter().any(|t| !t.is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    if points.len() < 2 {
        return Ok(VectorPath::new());
    }
    let mut raw_points = Vec::with_capacity((points.len() - 1) * bezier_t_values.len());
    for pair in points.windows(2) {
        let start = pair[0];
        let delta = pair[1] - start;
        raw_points.extend(bezier_t_values.iter().map(|t| start + delta * (*t as f32)));
    }
    if raw_points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(VectorPath::from_manim_points(raw_points))
}

fn append_cubic_curve(path: VectorPath, curve: [Vec2; 4]) -> VectorPath {
    let [anchor1, handle1, handle2, anchor2] = curve;
    if path.endpoints().is_some_and(|(_, last)| last == anchor1) {
        path.cubic_to(handle1, handle2, anchor2)
    } else {
        path.move_to(anchor1).cubic_to(handle1, handle2, anchor2)
    }
}

fn append_vectorized_path_with_stride(
    destination: &VectorPath,
    source: &VectorPath,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    let mut destination_points = manim_points_for_path(destination)?;
    if crate::path_queries::point_array_has_new_path_started(destination_points.len(), stride) {
        destination_points.pop();
    }
    destination_points.extend(manim_points_for_path(source)?);
    let mut path = VectorPath::from_manim_points(destination_points);

    if destination.morph_target().is_some() || source.morph_target().is_some() {
        let destination_target = destination.morph_target().unwrap_or(destination);
        let source_target = source.morph_target().unwrap_or(source);
        let target = append_vectorized_path_with_stride(destination_target, source_target, stride)?;
        path = path.with_morph_target(target);
    }
    Ok(path)
}

pub(crate) fn path_replacement_state(
    mut state: SemanticObjectState,
) -> Result<SemanticObjectState, AuthoringError> {
    if !matches!(state.content, SemanticObjectContent::Geometry(_)) {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::PathEditContent,
        ));
    }
    // Input points are in world XY space. The semantic object retains its other
    // presentation and identity; its XY affine is now represented in the points.
    state.transform.translation.x = 0.;
    state.transform.translation.y = 0.;
    state.transform.scale.x = 1.;
    state.transform.scale.y = 1.;
    state.transform.orientation = noon_core::SemanticOrientation::Planar(0.);
    Ok(state)
}

pub(crate) fn path_is_unchanged(
    store: &SemanticStore,
    before: &SemanticObjectState,
    after: &SemanticObjectState,
    path: &VectorPath,
) -> bool {
    before.transform == after.transform
        && crate::path_queries::content_path(store, before.content)
            .is_ok_and(|old| old.as_ref() == path)
}

/// Inputs to shared authoring preparation, published using the normal content
/// replacement transaction. This is not retained scene or runtime state.
pub(crate) enum PathEdit<'a> {
    Clear,
    RawPoints(&'a [Vec2]),
    AppendRawPoints(&'a [Vec2]),
    Corners(&'a [Vec2]),
    CornersWithSamples {
        points: &'a [Vec2],
        bezier_t_values: &'a [f64; 4],
    },
    SmoothCorners(&'a [Vec2]),
    AnchorMode(bool),
    Start {
        point: Vec2,
        stride: i32,
    },
    Line(Vec2),
    LineWithStride(Vec2, i32),
    LineWithSamples {
        point: Vec2,
        stride: i32,
        bezier_t_values: &'a [f64; 4],
    },
    AppendCorners(&'a [Vec2]),
    Quadratic(Vec2, Vec2),
    QuadraticWithStride(Vec2, Vec2, i32),
    Cubic(Vec2, Vec2, Vec2),
    CubicWithStride(Vec2, Vec2, Vec2, i32),
    CubicCurve(Vec2, Vec2, Vec2, Vec2),
    SmoothCurve {
        handle2: Option<Vec2>,
        anchor: Vec2,
    },
    SmoothCurveWithStride {
        handle2: Option<Vec2>,
        anchor: Vec2,
        stride: i32,
    },
    CubicCurves(&'a [[Vec2; 4]]),
    SetCubicCurves(&'a [[Vec2; 4]]),
    AppendVectorized(&'a SemanticObjectState),
    AppendVectorizedWithStride {
        source: &'a SemanticObjectState,
        stride: i32,
    },
    Close,
    CloseWithStride {
        stride: i32,
        tolerance: f64,
        bezier_t_values: &'a [f64],
    },
    Reverse,
    AppendCornersWithStride {
        points: &'a [Vec2],
        stride: i32,
        bezier_t_values: &'a [f64],
    },
    Subdivide(usize),
    ScaleHandles(f64),
    Partial {
        source: &'a SemanticObjectState,
        a: f32,
        b: f32,
    },
}

impl PathEdit<'_> {
    pub(crate) fn prepare(
        self,
        store: &SemanticStore,
        state: &SemanticObjectState,
    ) -> Result<Option<VectorPath>, AuthoringError> {
        if matches!(self, Self::Clear) {
            return Ok(Some(VectorPath::new()));
        }
        if let Self::CloseWithStride {
            stride,
            tolerance,
            bezier_t_values,
        } = self
        {
            let path = world_path(store, state)?;
            return close_path_with_stride(path, stride, tolerance, bezier_t_values);
        }
        if let Self::LineWithSamples {
            point,
            stride,
            bezier_t_values,
        } = self
        {
            return append_manim_line_to_path_with_samples(
                world_path(store, state)?,
                point,
                stride,
                bezier_t_values,
            )
            .map(Some);
        }
        if let Self::RawPoints(points) = self {
            if points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(VectorPath::from_manim_points(points.to_vec())));
        }
        if let Self::AppendRawPoints(points) = self {
            if points.is_empty() {
                return Ok(None);
            }
            if points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let path = world_path(store, state)?;
            let mut raw_points = crate::path_queries::PathQuery::prepare(
                &path,
                noon_core::SemanticTransform2_5D::default(),
            )?
            .points()?
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32, y as f32))
            .collect::<Vec<_>>();
            raw_points.extend_from_slice(points);
            return Ok(Some(VectorPath::from_manim_points(raw_points)));
        }
        if matches!(&self, Self::ScaleHandles(factor) if *factor == 1.0) {
            return Ok(None);
        }
        if let Self::CubicCurve(anchor1, handle1, handle2, anchor2) = self {
            let path = append_cubic_curve(
                world_path(store, state)?,
                [anchor1, handle1, handle2, anchor2],
            );
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::CubicCurves(curves) = self {
            if curves.is_empty() {
                return Ok(None);
            }
            let path = curves
                .iter()
                .copied()
                .fold(world_path(store, state)?, append_cubic_curve);
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::SetCubicCurves(curves) = self {
            if curves
                .iter()
                .flatten()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let path = curves
                .iter()
                .copied()
                .fold(VectorPath::new(), append_cubic_curve);
            return Ok(Some(path));
        }
        if let Self::AppendCorners(points) = self {
            let mut path = world_path(store, state)?;
            let Some((_, _)) = path.endpoints() else {
                return Err(AuthoringError::PathQuery(
                    noon_geometry::PathProportionError::EmptyPath,
                ));
            };
            if points.is_empty() {
                return Ok(None);
            }
            if points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            path = path.open_last_subpath();
            for point in points {
                path = path.line_to(*point);
            }
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::AppendCornersWithStride {
            points,
            stride,
            bezier_t_values,
        } = self
        {
            return append_corners_with_stride(
                world_path(store, state)?,
                points,
                stride,
                bezier_t_values,
            )
            .map(Some);
        }
        if let Self::AppendVectorized(source) = self {
            let destination = world_path(store, state)?;
            let source = world_path(store, source)?;
            let path = append_vectorized_path_with_stride(&destination, &source, 4)?;
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::AppendVectorizedWithStride { source, stride } = self {
            let destination = world_path(store, state)?;
            let source = world_path(store, source)?;
            let path = append_vectorized_path_with_stride(&destination, &source, stride)?;
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::SmoothCurve { handle2, anchor } = self {
            let path = world_path(store, state)?;
            return append_smooth_curve_path(path, handle2, anchor, 4).map(Some);
        }
        if let Self::SmoothCurveWithStride {
            handle2,
            anchor,
            stride,
        } = self
        {
            let path = world_path(store, state)?;
            return append_smooth_curve_path(path, handle2, anchor, stride).map(Some);
        }
        if let Self::Corners(points) = self {
            return corners_path(points).map(Some);
        }
        if let Self::CornersWithSamples {
            points,
            bezier_t_values,
        } = self
        {
            return corners_path_with_samples(points, bezier_t_values).map(Some);
        }
        if let Self::SmoothCorners(points) = self {
            return noon_geometry::change_path_anchor_mode(&corners_path(points)?, true)
                .map(Some)
                .map_err(AuthoringError::PathQuery);
        }
        if let Self::Partial { source, a, b } = self {
            let path = world_path(store, source)?;
            // Manim leaves the destination unchanged when the source has no
            // complete curve, except when the whole point set was requested.
            if !(a == 0. && b == 1.)
                && !path.commands().iter().any(|command| {
                    matches!(
                        command,
                        noon_core::PathCommand::LineTo { .. }
                            | noon_core::PathCommand::QuadraticTo { .. }
                            | noon_core::PathCommand::CubicTo { .. }
                    )
                })
            {
                return Ok(None);
            }
            return Ok(Some(noon_geometry::authored_partial_path(&path, a, b)));
        }
        if let Self::Subdivide(0) = self {
            return Ok(None);
        }
        let mut path = world_path(store, state)?;
        if let Self::AnchorMode(smooth) = self {
            return noon_geometry::change_path_anchor_mode(&path, smooth)
                .map(Some)
                .map_err(AuthoringError::PathQuery);
        }
        if let Self::Subdivide(additional) = self {
            return noon_geometry::subdivide_path(&path, additional)
                .map(Some)
                .map_err(AuthoringError::PathQuery);
        }
        if let Self::ScaleHandles(factor) = self {
            return scale_handle_distances(&path, factor).map(Some);
        }
        if let Self::Reverse = self {
            return Ok(Some(noon_geometry::reverse_path(&path)));
        }
        if let Self::Start { point, stride } = self {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            if stride == 0 {
                return Err(AuthoringError::NonPositiveNumber {
                    name: "n_points_per_cubic_curve".to_owned(),
                    value: 0.0,
                });
            }

            // Manim's start_new_path is a raw point-array append. Preserve an
            // existing raw array (including a partial tail), or derive the
            // point array once from the current shared path when it was built
            // from compact path commands.
            let mut points = manim_points_for_path(&path)?;

            // For positive strides, pad a partial final curve by repeating
            // its last start anchor until the next curve boundary. A negative
            // Python slice stride makes Manim's computed repeat count
            // non-positive, so it appends only the new point.
            if stride > 0 {
                let stride = stride as usize;
                let remainder = points.len() % stride;
                if remainder != 0 {
                    let anchor_index = ((points.len() - 1) / stride) * stride;
                    let anchor = points[anchor_index];
                    points.extend(std::iter::repeat_n(anchor, stride - remainder));
                }
            }
            points.push(point);
            path = VectorPath::from_manim_points(points);
        } else if let Self::Line(to) = self {
            if !to.x.is_finite() || !to.y.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            let start = points.last().copied().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            let (control1, control2) = cubic_line_controls(start, to);
            path = append_manim_cubic_to_path(points, control1, control2, to)?;
        } else if let Self::LineWithStride(to, stride) = self {
            if !to.x.is_finite() || !to.y.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            let start = points.last().copied().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            let (control1, control2) = cubic_line_controls(start, to);
            path = append_manim_cubic_to_path_with_stride(points, control1, control2, to, stride)?;
        } else if let Self::Quadratic(control, to) = self {
            if [control, to]
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            let start = points.last().copied().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            let control1 = control * (2.0 / 3.0) + start / 3.0;
            let control2 = control * (2.0 / 3.0) + to / 3.0;
            path = append_manim_cubic_to_path(points, control1, control2, to)?;
        } else if let Self::QuadraticWithStride(control, to, stride) = self {
            if [control, to]
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            let start = points.last().copied().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            let control1 = control * (2.0 / 3.0) + start / 3.0;
            let control2 = control * (2.0 / 3.0) + to / 3.0;
            path = append_manim_cubic_to_path_with_stride(points, control1, control2, to, stride)?;
        } else if let Self::Cubic(control1, control2, to) = self {
            if [control1, control2, to]
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            path = append_manim_cubic_to_path(points, control1, control2, to)?;
        } else if let Self::CubicWithStride(control1, control2, to, stride) = self {
            if [control1, control2, to]
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let points = manim_points_for_path(&path)?;
            path = append_manim_cubic_to_path_with_stride(points, control1, control2, to, stride)?;
        } else {
            let (first, last) = path.endpoints().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            path = match self {
                Self::Line(to) => path.open_last_subpath().line_to(to),
                Self::LineWithStride(to, _) => path.open_last_subpath().line_to(to),
                Self::LineWithSamples { point, .. } => path.open_last_subpath().line_to(point),
                Self::Quadratic(control, to) => path.open_last_subpath().quadratic_to(control, to),
                Self::QuadraticWithStride(control, to, _) => {
                    path.open_last_subpath().quadratic_to(control, to)
                }
                Self::Cubic(c1, c2, to) => path.open_last_subpath().cubic_to(c1, c2, to),
                Self::CubicWithStride(c1, c2, to, _) => {
                    path.open_last_subpath().cubic_to(c1, c2, to)
                }
                Self::Close => {
                    if (last - first).length() < 1e-6 {
                        path
                    } else {
                        // Only complete curves define Manim's subpaths; a
                        // dangling MoveTo does not hide the previous subpath.
                        let mut current_start = None;
                        let mut completed_start = None;
                        for command in path.commands() {
                            match *command {
                                noon_core::PathCommand::MoveTo { to } => current_start = Some(to),
                                _ => completed_start = current_start,
                            }
                        }
                        let start = completed_start.unwrap_or(last);
                        let close_contour = current_start == Some(start);
                        path = path.open_last_subpath().line_to(start);
                        if close_contour {
                            path.close()
                        } else {
                            path
                        }
                    }
                }
                Self::Corners(_)
                | Self::CornersWithSamples { .. }
                | Self::SmoothCorners(_)
                | Self::Clear
                | Self::RawPoints(_)
                | Self::AppendRawPoints(_)
                | Self::AnchorMode(_)
                | Self::Start { .. }
                | Self::Reverse
                | Self::Subdivide(_)
                | Self::ScaleHandles(_)
                | Self::CubicCurve(_, _, _, _)
                | Self::SmoothCurve { .. }
                | Self::SmoothCurveWithStride { .. }
                | Self::CubicCurves(_)
                | Self::SetCubicCurves(_)
                | Self::AppendCorners(_)
                | Self::AppendCornersWithStride { .. }
                | Self::AppendVectorized(_)
                | Self::AppendVectorizedWithStride { .. }
                | Self::CloseWithStride { .. }
                | Self::Partial { .. } => {
                    unreachable!()
                }
            };
        }
        if !path.is_finite() {
            return Err(AuthoringError::NonFiniteGeometry);
        }
        Ok(Some(path))
    }
}

fn manim_points_for_path(path: &VectorPath) -> Result<Vec<Vec2>, AuthoringError> {
    if let Some(points) = path.manim_points() {
        return Ok(points.to_vec());
    }
    Ok(
        crate::path_queries::PathQuery::prepare(path, noon_core::SemanticTransform2_5D::default())?
            .points()?
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32, y as f32))
            .collect(),
    )
}

fn close_path_with_stride(
    path: VectorPath,
    stride: i32,
    tolerance: f64,
    bezier_t_values: &[f64],
) -> Result<Option<VectorPath>, AuthoringError> {
    let morph_target = path.morph_target().cloned();
    let mut points = manim_points_for_path(&path)?;
    let Some(first) = points.first().copied() else {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::EmptyPath,
        ));
    };
    let last = *points.last().expect("non-empty point array was checked");
    if crate::path_queries::manim_points_2d_close(
        [f64::from(first.x), f64::from(first.y)],
        [f64::from(last.x), f64::from(last.y)],
        tolerance,
    ) {
        return Ok(None);
    }

    let coordinates = points
        .iter()
        .flat_map(|point| [f64::from(point.x), f64::from(point.y)])
        .collect::<Vec<_>>();
    let ranges = crate::path_queries::manim_subpath_ranges(
        &coordinates,
        points.len(),
        2,
        2,
        f64::from(stride),
        tolerance,
        true,
    )?;
    let Some(start_index) = ranges.get(ranges.len().saturating_sub(2)).copied() else {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::EmptyPath,
        ));
    };
    let target = points[start_index];
    let start = last;
    let line_points = bezier_t_values
        .iter()
        .skip(1)
        .map(|t| start + (target - start) * (*t as f32))
        .collect::<Vec<_>>();

    if !crate::path_queries::point_array_has_new_path_started(points.len(), stride) {
        points.push(last);
    }
    points.extend(line_points);
    let mut result = VectorPath::from_manim_points(points);
    if let Some(target) = morph_target {
        let closed_target =
            close_path_with_stride(target.clone(), stride, tolerance, bezier_t_values)?
                .unwrap_or(target);
        result = result.with_morph_target(closed_target);
    }
    if !result.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(Some(result))
}

fn cubic_line_controls(start: Vec2, end: Vec2) -> (Vec2, Vec2) {
    let direction = end - start;
    (start + direction / 3.0, start + direction * (2.0 / 3.0))
}

fn append_manim_cubic_to_path(
    points: Vec<Vec2>,
    control1: Vec2,
    control2: Vec2,
    anchor: Vec2,
) -> Result<VectorPath, AuthoringError> {
    append_manim_cubic_to_path_with_stride(points, control1, control2, anchor, 4)
}

fn append_smooth_curve_path(
    path: VectorPath,
    handle2: Option<Vec2>,
    anchor: Vec2,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    if !anchor.x.is_finite()
        || !anchor.y.is_finite()
        || handle2.is_some_and(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    let points = manim_points_for_path(&path)?;
    if points.is_empty() {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::EmptyPath,
        ));
    }
    if manim_remainder_is_one(points.len(), stride) {
        // Manim treats a pending one-point subpath as a straight line, even
        // when the optional second control point was supplied.
        let start = *points.last().expect("the path was checked above");
        let (control1, control2) = cubic_line_controls(start, anchor);
        append_manim_cubic_to_path_with_stride(points, control1, control2, anchor, stride)
    } else {
        let last_handle2 = points[points.len() - 2];
        let last = points[points.len() - 1];
        let tangent = last - last_handle2;
        let handle1 = last + tangent;
        let handle2 = handle2.unwrap_or_else(|| {
            let direction = anchor - last;
            // Manim rotates the incoming tangent by pi about the
            // anchor-to-anchor axis. In the planar model this is the
            // reflection across that axis; a zero axis leaves it unchanged,
            // matching Manim's zero-rotvec fallback.
            let reflected = direction.normalized().map_or(tangent, |axis| {
                axis * (2.0 * (tangent.x * axis.x + tangent.y * axis.y)) - tangent
            });
            anchor - reflected
        });
        append_manim_cubic_to_path_with_stride(points, handle1, handle2, anchor, stride)
    }
}

fn manim_remainder_is_one(point_count: usize, stride: i32) -> bool {
    crate::path_queries::point_array_has_new_path_started(point_count, stride)
}

fn append_manim_cubic_to_path_with_stride(
    mut points: Vec<Vec2>,
    control1: Vec2,
    control2: Vec2,
    anchor: Vec2,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    let last = points.last().copied().ok_or(AuthoringError::PathQuery(
        noon_geometry::PathProportionError::EmptyPath,
    ))?;
    // add_cubic_bezier_curve_to appends three points after a pending new
    // subpath anchor, and otherwise prefixes those three with get_last_point.
    if !manim_remainder_is_one(points.len(), stride) {
        points.push(last);
    }
    points.extend([control1, control2, anchor]);
    let path = VectorPath::from_manim_points(points);
    if !path.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(path)
}

fn append_manim_line_to_path_with_samples(
    path: VectorPath,
    point: Vec2,
    stride: i32,
    bezier_t_values: &[f64; 4],
) -> Result<VectorPath, AuthoringError> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || bezier_t_values.iter().any(|value| !value.is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }

    let morph_target = path.morph_target().cloned();
    let mut points = manim_points_for_path(&path)?;
    let start = points.last().copied().ok_or(AuthoringError::PathQuery(
        noon_geometry::PathProportionError::EmptyPath,
    ))?;
    if !manim_remainder_is_one(points.len(), stride) {
        points.push(start);
    }
    points.extend(
        bezier_t_values
            .iter()
            .skip(1)
            .map(|t| start + (point - start) * (*t as f32)),
    );

    let mut result = VectorPath::from_manim_points(points);
    if let Some(target) = morph_target {
        let target =
            append_manim_line_to_path_with_samples(target, point, stride, bezier_t_values)?;
        result = result.with_morph_target(target);
    }
    if !result.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(result)
}

fn append_corners_with_stride(
    path: VectorPath,
    corners: &[Vec2],
    stride: i32,
    bezier_t_values: &[f64],
) -> Result<VectorPath, AuthoringError> {
    let Some((_, _)) = path.endpoints() else {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::EmptyPath,
        ));
    };
    if corners.is_empty() {
        return Ok(path);
    }
    if corners
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
        || bezier_t_values.iter().any(|value| !value.is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    if stride <= 0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: "n_points_per_cubic_curve".to_owned(),
            value: f64::from(stride),
        });
    }
    let stride = stride as usize;
    let mut points = manim_points_for_path(&path)?;
    let mut start = points.last().copied().ok_or(AuthoringError::PathQuery(
        noon_geometry::PathProportionError::EmptyPath,
    ))?;
    if manim_remainder_is_one(points.len(), stride as i32) {
        // Manim computes the first start corner before removing a pending
        // anchor from the existing point array, then emits it in the new run.
        points.pop();
    }
    for end in corners {
        for index in 0..stride {
            // Manim initializes the final slot to the endpoint, then overlays
            // each constructor-time interpolation sample. If a mutable stride
            // leaves an unsampled interior slot, choosing the endpoint keeps
            // Rust's stored geometry deterministic; Python gates that unstable
            // source profile before it reaches this helper.
            let mut point = *end;
            if let Some(t) = bezier_t_values.get(index) {
                let t = *t as f32;
                point = start + (*end - start) * t;
            }
            points.push(point);
        }
        start = *end;
    }
    let path = VectorPath::from_manim_points(points);
    if !path.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(path)
}

/// Prepare one persistent path edit against an already-selected authored or
/// coherent effective snapshot. This centralizes the exact single-object no-op
/// contract used by cold and live authoring before either side publishes.
pub(crate) fn prepare_object_edit(
    store: &SemanticStore,
    object: SemanticNodeId,
    captured: SemanticObjectState,
    edit: PathEdit<'_>,
) -> Result<Option<PreparedPathEdits>, AuthoringError> {
    let before = store
        .semantic_object_state_checked(object)
        .map_err(AuthoringError::from)?;
    let after = path_replacement_state(captured.clone())?;
    let Some(path) = edit.prepare(store, &captured)? else {
        return Ok(None);
    };
    if path_is_unchanged(store, before, &after, &path) {
        return Ok(None);
    }
    PreparedPathEdits::prepare(store, [(object, captured, path)]).map(Some)
}

pub(crate) fn world_path(
    store: &SemanticStore,
    state: &SemanticObjectState,
) -> Result<VectorPath, AuthoringError> {
    let checked = |name, value| {
        crate::semantic_mobject::authoring_render_f64(name, value).map(|value| value as f32)
    };
    let transform = state
        .transform
        .as_planar()
        .ok_or(AuthoringError::NonFiniteObjectState)?;
    let transform = noon_core::Transform2D {
        translation: Vec2::new(
            checked("path translation x", transform.translation.x)?,
            checked("path translation y", transform.translation.y)?,
        ),
        scale: Vec2::new(
            checked("path scale x", transform.scale.x)?,
            checked("path scale y", transform.scale.y)?,
        ),
        rotation: checked("path rotation", transform.rotation_z)?,
    };
    let path = crate::path_queries::content_path(store, state.content)?.transformed(transform);
    Ok(path)
}

/// Scale each cubic handle around its associated anchor, promoting lines and
/// quadratic segments to their exact cubic representation first. Manim stores
/// all VMobject curves as four-point cubics, while Noon retains compact line
/// and quadratic commands until an edit needs handle-level control.
fn scale_handle_distances(path: &VectorPath, factor: f64) -> Result<VectorPath, AuthoringError> {
    if !factor.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    let factor = factor as f32;
    if !factor.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    if factor == 1.0 {
        return Ok(path.clone());
    }

    let mut result = VectorPath::new();
    let mut current = None;
    let mut subpath_start = None;
    for command in path.commands().iter().copied() {
        match command {
            noon_core::PathCommand::MoveTo { to } => {
                result = result.move_to(to);
                current = Some(to);
                subpath_start = Some(to);
            }
            noon_core::PathCommand::LineTo { to } => {
                if let Some(from) = current {
                    let delta = to - from;
                    let handle1 = from + delta * (1.0 / 3.0);
                    let handle2 = from + delta * (2.0 / 3.0);
                    result = result.cubic_to(
                        scale_control(from, handle1, factor),
                        scale_control(to, handle2, factor),
                        to,
                    );
                } else {
                    result = result.line_to(to);
                }
                current = Some(to);
            }
            noon_core::PathCommand::QuadraticTo { control, to } => {
                if let Some(from) = current {
                    let handle1 = from + (control - from) * (2.0 / 3.0);
                    let handle2 = to + (control - to) * (2.0 / 3.0);
                    result = result.cubic_to(
                        scale_control(from, handle1, factor),
                        scale_control(to, handle2, factor),
                        to,
                    );
                } else {
                    result = result.quadratic_to(control, to);
                }
                current = Some(to);
            }
            noon_core::PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                if let Some(from) = current {
                    result = result.cubic_to(
                        scale_control(from, control1, factor),
                        scale_control(to, control2, factor),
                        to,
                    );
                } else {
                    result = result.cubic_to(control1, control2, to);
                }
                current = Some(to);
            }
            noon_core::PathCommand::Close => {
                if let (Some(from), Some(to)) = (current, subpath_start) {
                    if from != to {
                        let delta = to - from;
                        let handle1 = from + delta * (1.0 / 3.0);
                        let handle2 = from + delta * (2.0 / 3.0);
                        result = result.cubic_to(
                            scale_control(from, handle1, factor),
                            scale_control(to, handle2, factor),
                            to,
                        );
                    }
                }
                result = result.close();
                current = subpath_start;
            }
        }
    }
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(scale_handle_distances(target, f64::from(factor))?);
    }
    if !result.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(result)
}

fn scale_control(anchor: Vec2, handle: Vec2, factor: f32) -> Vec2 {
    anchor + (handle - anchor) * factor
}

pub(crate) fn partial_interval(a: f64, b: f64) -> Result<(f32, f32), AuthoringError> {
    for alpha in [a, b] {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::InvalidProportion(alpha as f32),
            ));
        }
    }
    if a > b {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::InvalidInterval,
        ));
    }
    Ok((a as f32, b as f32))
}

pub(crate) fn prepare_subcurve(
    store: &SemanticStore,
    state: &SemanticObjectState,
    a: f64,
    b: f64,
) -> Result<(SemanticObjectState, Option<VectorPath>), AuthoringError> {
    partial_interval(a, a)?;
    partial_interval(b, b)?;
    let query = crate::path_queries::prepare_content(
        store,
        state.content,
        state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?,
    )?;
    let closed = query.is_closed()?;
    // A singleton has no complete curve and retains its original content.
    if query.curve_count() == 0 {
        return Ok((state.clone(), None));
    }
    if a > b && !closed {
        return Err(AuthoringError::PathQuery(
            noon_geometry::PathProportionError::InvalidInterval,
        ));
    }
    if a == 0. && b == 1. {
        return Ok((state.clone(), None));
    }
    let path = world_path(store, state)?;
    let selected = noon_geometry::subcurve_path(&path, a as f32, b as f32)
        .map_err(AuthoringError::PathQuery)?;
    Ok((path_replacement_state(state.clone())?, Some(selected)))
}

pub(crate) fn subcurve_creation(state: SemanticObjectState) -> SemanticMutationTransaction {
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_node(noon_core::SemanticNodeCreation::object(state));
    transaction
}

pub(crate) fn created_subcurve_id(
    result: &noon_core::SemanticMutationTransactionResult,
) -> noon_core::SemanticNodeId {
    let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
        unreachable!("one subcurve has one semantic creation")
    };
    *node
}

pub(crate) fn publish_running_subcurve(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    source: &Mobject,
    a: f64,
    b: f64,
) -> Result<Mobject, AuthoringError> {
    if !Rc::ptr_eq(store, source.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    source.validate()?;
    require_running_path_resource_admission(store, root, execution)?;
    let captured = crate::effective_capture::capture_mobject_state(store, execution, source)?;
    let mut store_mut = store.borrow_mut();
    let (mut state, path) = prepare_subcurve(&store_mut, &captured, a, b)?;
    let mut publish = |store: &mut SemanticStore, state| {
        execution
            .apply_semantic_transaction_at_root(store, root, subcurve_creation(state))
            .map_err(AuthoringError::from)
    };
    let result = if let Some(path) = path {
        store_mut.with_geometry_path(path, |store, handle| {
            state.content = StoredGeometry::Resource(handle).into();
            publish(store, state)
        })?
    } else {
        publish(&mut store_mut, state)?
    };
    let id = created_subcurve_id(&result);
    drop(store_mut);
    Mobject::from_node(Rc::clone(store), id)
}

impl Mobject {
    /// Create a detached copy of this path's interval, preserving paint/priority.
    /// Closed paths allow a > b to select across the seam; open paths require
    /// an ordered interval. Source identity, geometry and membership are untouched.
    pub fn subcurve(&self, a: f64, b: f64) -> Result<Self, AuthoringError> {
        let state = self.state()?;
        let mut store = self.integration_store().borrow_mut();
        let (mut state, path) = prepare_subcurve(&store, &state, a, b)?;
        let result = if let Some(path) = path {
            store.with_geometry_path(path, |store, handle| {
                state.content = StoredGeometry::Resource(handle).into();
                subcurve_creation(state)
                    .apply(store)
                    .map_err(AuthoringError::from)
            })?
        } else {
            subcurve_creation(state).apply(&mut store)?
        };
        let id = created_subcurve_id(&result);
        drop(store);
        Self::from_node(std::rc::Rc::clone(self.integration_store()), id)
    }

    /// Replace this path with the exact Manim-style world-space point array.
    pub fn set_points(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::RawPoints(points))
    }

    /// Append to the exact Manim-style world-space point array.
    pub fn append_points(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AppendRawPoints(points))
    }

    /// Replace world-space corners with one smooth retained cubic spline.
    pub fn set_points_smoothly(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::SmoothCorners(points))
    }
    pub fn make_smooth(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AnchorMode(true))
    }
    pub fn make_jagged(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AnchorMode(false))
    }

    /// Scale cubic handles around their associated anchors. A factor of one is
    /// a path-preserving no-op; zero and negative factors follow ManimCE.
    pub fn scale_handle_to_anchor_distances(&mut self, factor: f64) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::ScaleHandles(factor))
    }

    /// Set a polyline from world-space corners, preserving identity and paint.
    /// Copies retain their previous immutable path; no temporary object is used.
    pub fn set_points_as_corners(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Corners(points))
    }
    pub fn set_points_as_corners_with_samples(
        &mut self,
        points: &[Vec2],
        bezier_t_values: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::CornersWithSamples {
            points,
            bezier_t_values,
        })
    }
    /// Replace the path with cubic curves defined by four points each.
    pub fn set_cubic_bezier_curves(&mut self, curves: &[[Vec2; 4]]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::SetCubicCurves(curves))
    }
    /// Remove this object's path while retaining its semantic identity and style.
    pub fn clear_points(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Clear)
    }
    /// Begin a new world-space subpath without connecting it to the previous one.
    pub fn start_new_path(&mut self, point: Vec2) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Start { point, stride: 4 })
    }
    /// Begin a new subpath using Manim's current mutable point-array stride.
    pub fn start_new_path_with_stride(
        &mut self,
        point: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Start { point, stride })
    }
    pub fn add_line_to(&mut self, point: Vec2) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Line(point))
    }
    pub fn add_line_to_with_stride(
        &mut self,
        point: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::LineWithStride(point, stride))
    }
    pub fn add_line_to_with_stride_and_samples(
        &mut self,
        point: Vec2,
        stride: i32,
        bezier_t_values: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::LineWithSamples {
            point,
            stride,
            bezier_t_values,
        })
    }
    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        control: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Quadratic(control, anchor))
    }
    pub fn add_quadratic_bezier_curve_to_with_stride(
        &mut self,
        control: Vec2,
        anchor: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::QuadraticWithStride(control, anchor, stride))
    }
    pub fn add_cubic_bezier_curve_to(
        &mut self,
        control1: Vec2,
        control2: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Cubic(control1, control2, anchor))
    }
    pub fn add_cubic_bezier_curve_to_with_stride(
        &mut self,
        control1: Vec2,
        control2: Vec2,
        anchor: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::CubicWithStride(
            control1, control2, anchor, stride,
        ))
    }
    pub fn add_smooth_curve_to(
        &mut self,
        handle2: Option<Vec2>,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::SmoothCurve { handle2, anchor })
    }
    pub fn add_smooth_curve_to_with_stride(
        &mut self,
        handle2: Option<Vec2>,
        anchor: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::SmoothCurveWithStride {
            handle2,
            anchor,
            stride,
        })
    }
    /// Append a cubic curve from its start anchor and two controls.
    pub fn add_cubic_bezier_curve(
        &mut self,
        anchor1: Vec2,
        handle1: Vec2,
        handle2: Vec2,
        anchor2: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::CubicCurve(anchor1, handle1, handle2, anchor2))
    }
    /// Append a batch of four-control-point cubic curves atomically.
    pub fn add_cubic_bezier_curves(&mut self, curves: &[[Vec2; 4]]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::CubicCurves(curves))
    }
    /// Append straight segments from the current endpoint through each corner.
    pub fn add_points_as_corners(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AppendCorners(points))
    }
    /// Append line segments using the mutable Manim stride and constructor-time samples.
    pub fn add_points_as_corners_with_stride(
        &mut self,
        points: &[Vec2],
        stride: i32,
        bezier_t_values: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AppendCornersWithStride {
            points,
            stride,
            bezier_t_values,
        })
    }
    /// Append another vector object's current world-space path.
    pub fn append_vectorized_mobject(&mut self, source: &Mobject) -> Result<(), AuthoringError> {
        self.require_same_store(source)?;
        let source = source.state()?;
        self.edit_path(PathEdit::AppendVectorized(&source))
    }
    /// Append another vector object's points using the mutable destination stride.
    pub fn append_vectorized_mobject_with_stride(
        &mut self,
        source: &Mobject,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.require_same_store(source)?;
        let source = source.state()?;
        self.edit_path(PathEdit::AppendVectorizedWithStride {
            source: &source,
            stride,
        })
    }
    pub fn close_path(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Close)
    }
    pub fn close_path_with_stride(
        &mut self,
        stride: i32,
        tolerance: f64,
        bezier_t_values: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::CloseWithStride {
            stride,
            tolerance,
            bezier_t_values,
        })
    }
    /// Add curves through deterministic subdivision, preserving the shape.
    pub fn insert_n_curves(&mut self, additional: usize) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Subdivide(additional))
    }

    /// Reverse this object's curves and subpath order, retaining paint and identity.
    pub fn reverse_direction(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Reverse)
    }
    /// Replace world points with a curve-count parameter interval of another
    /// vector object. Unlike point_from_proportion, this is not arc-length based.
    /// The supported interval is finite and ordered: 0 <= a <= b <= 1.
    pub fn pointwise_become_partial(
        &mut self,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), AuthoringError> {
        self.require_same_store(source)?;
        let (a, b) = partial_interval(a, b)?;
        let source = source.state()?;
        self.edit_path(PathEdit::Partial {
            source: &source,
            a,
            b,
        })
    }
    fn edit_path(&mut self, edit: PathEdit<'_>) -> Result<(), AuthoringError> {
        let captured = self.state()?;
        let mut store = self.integration_store().borrow_mut();
        let Some(prepared) = prepare_object_edit(&store, self.node_id(), captured, edit)? else {
            return Ok(());
        };
        prepared.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}
