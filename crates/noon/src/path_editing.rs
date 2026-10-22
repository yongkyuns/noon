//! Persistent vector edits publish new immutable content through one transaction.
mod quadratic;
mod scalar;
mod scalar_authoring;
pub(crate) use scalar::{ScalarPathEdit, LINE_SAMPLES};
mod transaction;
use crate::{AuthoringError, ExecutionSession, Mobject, MobjectFamily};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectContent, SemanticObjectState,
    SemanticStore, StoredGeometry, Vec2, VectorPath,
};
use std::{cell::RefCell, rc::Rc};
pub(crate) use transaction::PreparedPathEdits;

pub(crate) fn raw_points_path(points: &[(f64, f64)]) -> Result<VectorPath, AuthoringError> {
    validate_raw_points(points)?;
    Ok(VectorPath::from_manim_points_f64(points.to_vec()))
}

fn validate_raw_points(points: &[(f64, f64)]) -> Result<(), AuthoringError> {
    if points
        .iter()
        .any(|&(x, y)| !(x as f32).is_finite() || !(y as f32).is_finite())
    {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(())
}

pub(crate) fn promote_point(point: Vec2) -> (f64, f64) {
    (f64::from(point.x), f64::from(point.y))
}

fn promote_points(points: &[Vec2]) -> Vec<(f64, f64)> {
    points.iter().copied().map(promote_point).collect()
}

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
    let mut points = manim_points_f64_for_path(path)?;
    points.reverse();
    let mut reversed = VectorPath::from_manim_points_f64(points);
    if let Some(target) = path.morph_target() {
        reversed = reversed.with_morph_target(reverse_point_order(target)?);
    }
    Ok(reversed)
}

/// Repeat the raw point array as Manim's `apply_over_attr_arrays(repeated)` does.
fn repeat_point_array(path: &VectorPath, count: usize) -> Result<VectorPath, AuthoringError> {
    let points = manim_points_f64_for_path(path)?;
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
    let mut result = VectorPath::from_manim_points_f64(repeated);
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
        self.set_points_f64(object, &promote_points(points))
    }

    pub fn append_points(
        &mut self,
        object: &Mobject,
        points: &[Vec2],
    ) -> Result<(), AuthoringError> {
        self.append_points_f64(object, &promote_points(points))
    }

    pub fn set_points_f64(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::RawPoints(points))
        })
    }

    pub fn append_points_f64(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
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
        self.add_quadratic_bezier_curve_to_f64(
            object,
            promote_point(control),
            promote_point(anchor),
        )
    }

    /// Append a quadratic while preserving semantic control-point precision.
    pub fn add_quadratic_bezier_curve_to_f64(
        &mut self,
        object: &Mobject,
        control: (f64, f64),
        anchor: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::QuadraticWithStride(control, anchor, 4),
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

fn append_vectorized_path_with_stride(
    destination: &VectorPath,
    source: &VectorPath,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    let mut destination_points = manim_points_f64_for_path(destination)?;
    if crate::path_queries::point_array_has_new_path_started(destination_points.len(), stride) {
        destination_points.pop();
    }
    destination_points.extend(manim_points_f64_for_path(source)?);
    let mut path = VectorPath::from_manim_points_f64(destination_points);

    if destination.morph_target().is_some() || source.morph_target().is_some() {
        let destination_target = destination.morph_target().unwrap_or(destination);
        let source_target = source.morph_target().unwrap_or(source);
        let target = append_vectorized_path_with_stride(destination_target, source_target, stride)?;
        path = path.with_morph_target(target);
    }
    if !path.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
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
    Scalar(ScalarPathEdit<'a>),
    Clear,
    RawPoints(&'a [(f64, f64)]),
    AppendRawPoints(&'a [(f64, f64)]),
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
    QuadraticWithStride((f64, f64), (f64, f64), i32),
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
        use scalar::{ScalarPathEdit as Edit, LINE_SAMPLES};
        let current = || world_path(store, state);
        match self {
            Self::Scalar(edit) => {
                let path = if matches!(
                    edit,
                    Edit::Corners { .. } | Edit::Cubics { replace: true, .. }
                ) {
                    VectorPath::new()
                } else {
                    current()?
                };
                scalar::prepare(path, edit)
            }
            Self::Clear => Ok(Some(VectorPath::new())),
            Self::RawPoints(points) => raw_points_path(points).map(Some),
            Self::AppendRawPoints(points) => {
                validate_raw_points(points)?;
                if points.is_empty() {
                    return Ok(None);
                }
                let mut raw = manim_points_f64_for_path(&current()?)?;
                raw.extend_from_slice(points);
                Ok(Some(VectorPath::from_manim_points_f64(raw)))
            }
            Self::Corners(points) => scalar::prepare(
                VectorPath::new(),
                Edit::Corners {
                    points: &promote_points(points),
                    samples: &LINE_SAMPLES,
                },
            ),
            Self::CornersWithSamples {
                points,
                bezier_t_values,
            } => scalar::prepare(
                VectorPath::new(),
                Edit::Corners {
                    points: &promote_points(points),
                    samples: bezier_t_values,
                },
            ),
            Self::SmoothCorners(points) => {
                noon_geometry::change_path_anchor_mode(&corners_path(points)?, true)
                    .map(Some)
                    .map_err(AuthoringError::PathQuery)
            }
            Self::AnchorMode(smooth) => noon_geometry::change_path_anchor_mode(&current()?, smooth)
                .map(Some)
                .map_err(AuthoringError::PathQuery),
            Self::Start { point, stride } => scalar::prepare(
                current()?,
                Edit::Start {
                    point: promote_point(point),
                    stride,
                },
            ),
            Self::Line(point) => scalar::prepare(
                current()?,
                Edit::Line {
                    point: promote_point(point),
                    stride: 4,
                    samples: &LINE_SAMPLES,
                },
            ),
            Self::LineWithStride(point, stride) => scalar::prepare(
                current()?,
                Edit::Line {
                    point: promote_point(point),
                    stride,
                    samples: &LINE_SAMPLES,
                },
            ),
            Self::LineWithSamples {
                point,
                stride,
                bezier_t_values,
            } => scalar::prepare(
                current()?,
                Edit::Line {
                    point: promote_point(point),
                    stride,
                    samples: bezier_t_values,
                },
            ),
            Self::QuadraticWithStride(control, anchor, stride) => {
                quadratic::append_quadratic_f64(current()?, control, anchor, stride).map(Some)
            }
            Self::Cubic(c1, c2, anchor) => scalar::prepare(
                current()?,
                Edit::CubicTo {
                    controls: [c1, c2, anchor].map(promote_point),
                    stride: 4,
                },
            ),
            Self::CubicWithStride(c1, c2, anchor, stride) => scalar::prepare(
                current()?,
                Edit::CubicTo {
                    controls: [c1, c2, anchor].map(promote_point),
                    stride,
                },
            ),
            Self::CubicCurve(a1, h1, h2, a2) => {
                scalar::prepare(current()?, Edit::Cubic([a1, h1, h2, a2].map(promote_point)))
            }
            Self::CubicCurves(curves) => {
                if curves.is_empty() {
                    return Ok(None);
                }
                let curves = curves
                    .iter()
                    .map(|curve| (*curve).map(promote_point))
                    .collect::<Vec<_>>();
                scalar::prepare(
                    current()?,
                    Edit::Cubics {
                        curves: &curves,
                        replace: false,
                    },
                )
            }
            Self::SetCubicCurves(curves) => {
                let curves = curves
                    .iter()
                    .map(|curve| (*curve).map(promote_point))
                    .collect::<Vec<_>>();
                scalar::prepare(
                    VectorPath::new(),
                    Edit::Cubics {
                        curves: &curves,
                        replace: true,
                    },
                )
            }
            Self::AppendCorners(points) => {
                if points.is_empty() {
                    current()?.endpoints().ok_or(AuthoringError::PathQuery(
                        noon_geometry::PathProportionError::EmptyPath,
                    ))?;
                    return Ok(None);
                }
                scalar::prepare(
                    current()?,
                    Edit::AppendCorners {
                        points: &promote_points(points),
                        stride: 4,
                        samples: &LINE_SAMPLES,
                    },
                )
            }
            Self::AppendCornersWithStride {
                points,
                stride,
                bezier_t_values,
            } => scalar::prepare(
                current()?,
                Edit::AppendCorners {
                    points: &promote_points(points),
                    stride,
                    samples: bezier_t_values,
                },
            ),
            Self::AppendVectorized(source) => {
                append_vectorized_path_with_stride(&current()?, &world_path(store, source)?, 4)
                    .map(Some)
            }
            Self::AppendVectorizedWithStride { source, stride } => {
                append_vectorized_path_with_stride(&current()?, &world_path(store, source)?, stride)
                    .map(Some)
            }
            Self::SmoothCurve { handle2, anchor } => {
                append_smooth_curve_path(current()?, handle2, anchor, 4).map(Some)
            }
            Self::SmoothCurveWithStride {
                handle2,
                anchor,
                stride,
            } => append_smooth_curve_path(current()?, handle2, anchor, stride).map(Some),
            Self::Close => scalar::prepare(
                current()?,
                Edit::Close {
                    stride: 4,
                    tolerance: 1e-6,
                    samples: &LINE_SAMPLES,
                },
            ),
            Self::CloseWithStride {
                stride,
                tolerance,
                bezier_t_values,
            } => scalar::prepare(
                current()?,
                Edit::Close {
                    stride,
                    tolerance,
                    samples: bezier_t_values,
                },
            ),
            Self::Reverse => Ok(Some(noon_geometry::reverse_path(&current()?))),
            Self::Subdivide(0) => Ok(None),
            Self::Subdivide(additional) => noon_geometry::subdivide_path(&current()?, additional)
                .map(Some)
                .map_err(AuthoringError::PathQuery),
            Self::ScaleHandles(1.0) => Ok(None),
            Self::ScaleHandles(factor) => scale_handle_distances(&current()?, factor).map(Some),
            Self::Partial { source, a, b } => {
                let path = world_path(store, source)?;
                if !(a == 0.0 && b == 1.0)
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
                Ok(Some(noon_geometry::authored_partial_path(&path, a, b)))
            }
        }
    }
}

pub(crate) fn manim_points_f64_for_path(
    path: &VectorPath,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    if let Some(points) = path.manim_points() {
        return Ok(points.to_vec());
    }
    crate::path_queries::PathQuery::prepare(path, noon_core::SemanticTransform2_5D::default())?
        .points()
}

pub(crate) fn manim_points_for_path(path: &VectorPath) -> Result<Vec<Vec2>, AuthoringError> {
    Ok(manim_points_f64_for_path(path)?
        .into_iter()
        .map(|(x, y)| Vec2::new(x as f32, y as f32))
        .collect())
}

fn cubic_line_controls(start: Vec2, end: Vec2) -> (Vec2, Vec2) {
    let direction = end - start;
    (start + direction / 3.0, start + direction * (2.0 / 3.0))
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
    points: Vec<Vec2>,
    control1: Vec2,
    control2: Vec2,
    anchor: Vec2,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    append_manim_cubic_points_with_stride(
        points,
        [control1, control2, anchor],
        stride,
        VectorPath::from_manim_points,
    )
}

fn append_manim_cubic_points_with_stride<T: Copy>(
    mut points: Vec<T>,
    tail: [T; 3],
    stride: i32,
    build: impl FnOnce(Vec<T>) -> VectorPath,
) -> Result<VectorPath, AuthoringError> {
    let last = points.last().copied().ok_or(AuthoringError::PathQuery(
        noon_geometry::PathProportionError::EmptyPath,
    ))?;
    // add_cubic_bezier_curve_to appends three points after a pending new
    // subpath anchor, and otherwise prefixes those three with get_last_point.
    if !manim_remainder_is_one(points.len(), stride) {
        points.push(last);
    }
    points.extend(tail);
    let path = build(points);
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
    let transform = state
        .transform
        .as_planar()
        .ok_or(AuthoringError::NonFiniteObjectState)?;
    for (name, value) in [
        ("path translation x", transform.translation.x),
        ("path translation y", transform.translation.y),
        ("path scale x", transform.scale.x),
        ("path scale y", transform.scale.y),
        ("path rotation", transform.rotation_z),
    ] {
        crate::semantic_mobject::authoring_render_f64(name, value)?;
    }
    Ok(crate::path_queries::content_path(store, state.content)?.transformed_semantic(transform))
}

/// Scale each cubic handle around its associated anchor, promoting lines and
/// quadratic segments to their exact cubic representation first. Manim stores
/// all VMobject curves as four-point cubics, while Noon retains compact line
/// and quadratic commands until an edit needs handle-level control.
fn scale_handle_distances(path: &VectorPath, factor: f64) -> Result<VectorPath, AuthoringError> {
    if !factor.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    if factor == 1.0 {
        return Ok(path.clone());
    }
    let mut points = manim_points_f64_for_path(path)?;
    for curve in points.as_chunks_mut::<4>().0 {
        for (anchor, handle) in [(0, 1), (3, 2)] {
            curve[handle] = (
                curve[anchor].0 + (curve[handle].0 - curve[anchor].0) * factor,
                curve[anchor].1 + (curve[handle].1 - curve[anchor].1) * factor,
            );
        }
    }
    let mut result = VectorPath::from_manim_points_f64(points);
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(scale_handle_distances(target, factor)?);
    }
    if !result.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(result)
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
        self.set_points_f64(&promote_points(points))
    }

    /// Append to the exact Manim-style world-space point array.
    pub fn append_points(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.append_points_f64(&promote_points(points))
    }

    /// Replace raw world-space points at semantic precision, including an
    /// incomplete cubic tail. Renderer coordinates are lowered explicitly.
    pub fn set_points_f64(&mut self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::RawPoints(points))
    }

    /// Append raw coordinates without lowering the existing or incoming array.
    pub fn append_points_f64(&mut self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
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
        self.add_quadratic_bezier_curve_to_f64(promote_point(control), promote_point(anchor))
    }
    pub fn add_quadratic_bezier_curve_to_with_stride(
        &mut self,
        control: Vec2,
        anchor: Vec2,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.add_quadratic_bezier_curve_to_f64_with_stride(
            promote_point(control),
            promote_point(anchor),
            stride,
        )
    }

    /// Append a quadratic without lowering caller coordinates to renderer precision.
    pub fn add_quadratic_bezier_curve_to_f64(
        &mut self,
        control: (f64, f64),
        anchor: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.add_quadratic_bezier_curve_to_f64_with_stride(control, anchor, 4)
    }

    pub fn add_quadratic_bezier_curve_to_f64_with_stride(
        &mut self,
        control: (f64, f64),
        anchor: (f64, f64),
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
