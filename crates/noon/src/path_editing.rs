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
    require_running_object(store, source)?;
    let source = crate::effective_capture::capture_mobject_state(store, execution, source)?;
    publish_running_object_edit(
        store,
        root,
        execution,
        object,
        PathEdit::AppendVectorized(&source),
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

    pub fn append_vectorized_mobject(
        &mut self,
        object: &Mobject,
        source: &Mobject,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_append_vectorized_mobject(store, root, execution, object, source)
        })
    }

    pub fn close_path(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Close)
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

fn append_cubic_curve(path: VectorPath, curve: [Vec2; 4]) -> VectorPath {
    let [anchor1, handle1, handle2, anchor2] = curve;
    if path.endpoints().is_some_and(|(_, last)| last == anchor1) {
        path.cubic_to(handle1, handle2, anchor2)
    } else {
        path.move_to(anchor1).cubic_to(handle1, handle2, anchor2)
    }
}

fn append_path_command(path: VectorPath, command: noon_core::PathCommand) -> VectorPath {
    match command {
        noon_core::PathCommand::MoveTo { to } => path.move_to(to),
        noon_core::PathCommand::LineTo { to } => path.line_to(to),
        noon_core::PathCommand::QuadraticTo { control, to } => path.quadratic_to(control, to),
        noon_core::PathCommand::CubicTo {
            control1,
            control2,
            to,
        } => path.cubic_to(control1, control2, to),
        noon_core::PathCommand::Close => path.close(),
    }
}

fn append_vectorized_path(destination: VectorPath, source: &VectorPath) -> VectorPath {
    let morph_target = (destination.morph_target().is_some() || source.morph_target().is_some())
        .then(|| {
            let destination_target = destination
                .morph_target()
                .cloned()
                .unwrap_or_else(|| destination.clone());
            let source_target = source
                .morph_target()
                .cloned()
                .unwrap_or_else(|| source.clone());
            append_vectorized_path(destination_target, &source_target)
        });
    let mut destination_commands = destination.commands().to_vec();
    if matches!(
        destination_commands.last(),
        Some(noon_core::PathCommand::MoveTo { .. })
    ) {
        destination_commands.pop();
    }
    let path = destination_commands
        .into_iter()
        .chain(source.commands().iter().copied())
        .fold(VectorPath::new(), append_path_command);
    morph_target.map_or(path.clone(), |target| path.with_morph_target(target))
}

fn last_cubic_second_control(path: &VectorPath) -> Option<Vec2> {
    let mut current = None;
    let mut subpath_start = None;
    let mut control2 = None;
    for command in path.commands() {
        match *command {
            noon_core::PathCommand::MoveTo { to } => {
                current = Some(to);
                subpath_start = Some(to);
                control2 = None;
            }
            noon_core::PathCommand::LineTo { to } => {
                if let Some(from) = current {
                    control2 = Some(from + (to - from) * (2.0 / 3.0));
                }
                current = Some(to);
            }
            noon_core::PathCommand::QuadraticTo { control, to } => {
                control2 = Some(control * (2.0 / 3.0) + to * (1.0 / 3.0));
                current = Some(to);
            }
            noon_core::PathCommand::CubicTo {
                control2: next_control2,
                to,
                ..
            } => {
                control2 = Some(next_control2);
                current = Some(to);
            }
            noon_core::PathCommand::Close => {
                if let (Some(from), Some(start)) = (current, subpath_start) {
                    if from != start {
                        control2 = Some(from + (start - from) * (2.0 / 3.0));
                    }
                    current = Some(start);
                }
            }
        }
    }
    control2
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
    SmoothCorners(&'a [Vec2]),
    AnchorMode(bool),
    Start {
        point: Vec2,
        stride: i32,
    },
    Line(Vec2),
    AppendCorners(&'a [Vec2]),
    Quadratic(Vec2, Vec2),
    Cubic(Vec2, Vec2, Vec2),
    CubicCurve(Vec2, Vec2, Vec2, Vec2),
    SmoothCurve {
        handle2: Option<Vec2>,
        anchor: Vec2,
    },
    CubicCurves(&'a [[Vec2; 4]]),
    SetCubicCurves(&'a [[Vec2; 4]]),
    AppendVectorized(&'a SemanticObjectState),
    Close,
    Reverse,
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
        if let Self::AppendVectorized(source) = self {
            let destination = world_path(store, state)?;
            let source = world_path(store, source)?;
            let path = append_vectorized_path(destination, &source);
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::SmoothCurve { handle2, anchor } = self {
            if !anchor.x.is_finite()
                || !anchor.y.is_finite()
                || handle2.is_some_and(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            let mut path = world_path(store, state)?;
            if path.endpoints().is_none() {
                return Err(AuthoringError::PathQuery(
                    noon_geometry::PathProportionError::EmptyPath,
                ));
            }
            if matches!(
                path.commands().last(),
                Some(noon_core::PathCommand::MoveTo { .. })
            ) {
                // Manim treats a pending one-point subpath as a straight line,
                // even when the optional second control point was supplied.
                path = path.line_to(anchor);
            } else {
                let (_, last) = path.endpoints().expect("the path was checked above");
                let last_handle2 = last_cubic_second_control(&path).ok_or(
                    AuthoringError::PathQuery(noon_geometry::PathProportionError::EmptyPath),
                )?;
                let tangent = last - last_handle2;
                let handle1 = last + tangent;
                let handle2 = handle2.unwrap_or_else(|| {
                    let direction = anchor - last;
                    // Manim rotates the incoming tangent by pi about the
                    // anchor-to-anchor axis. In the planar model this is the
                    // reflection across that axis; a zero axis leaves it
                    // unchanged, matching Manim's zero-rotvec fallback.
                    let reflected = direction.normalized().map_or(tangent, |axis| {
                        axis * (2.0 * (tangent.x * axis.x + tangent.y * axis.y)) - tangent
                    });
                    anchor - reflected
                });
                path = path.open_last_subpath().cubic_to(handle1, handle2, anchor);
            }
            if !path.is_finite() {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            return Ok(Some(path));
        }
        if let Self::Corners(points) = self {
            return corners_path(points).map(Some);
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
            let mut points = if let Some(points) = path.manim_points() {
                points.to_vec()
            } else {
                crate::path_queries::PathQuery::prepare(
                    &path,
                    noon_core::SemanticTransform2_5D::default(),
                )?
                .points()?
                .into_iter()
                .map(|(x, y)| Vec2::new(x as f32, y as f32))
                .collect()
            };

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
                    points.extend(std::iter::repeat(anchor).take(stride - remainder));
                }
            }
            points.push(point);
            path = VectorPath::from_manim_points(points);
        } else {
            let (first, last) = path.endpoints().ok_or(AuthoringError::PathQuery(
                noon_geometry::PathProportionError::EmptyPath,
            ))?;
            path = match self {
                Self::Line(to) => path.open_last_subpath().line_to(to),
                Self::Quadratic(control, to) => path.open_last_subpath().quadratic_to(control, to),
                Self::Cubic(c1, c2, to) => path.open_last_subpath().cubic_to(c1, c2, to),
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
                | Self::CubicCurves(_)
                | Self::SetCubicCurves(_)
                | Self::AppendCorners(_)
                | Self::AppendVectorized(_)
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
    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        control: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Quadratic(control, anchor))
    }
    pub fn add_cubic_bezier_curve_to(
        &mut self,
        control1: Vec2,
        control2: Vec2,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Cubic(control1, control2, anchor))
    }
    pub fn add_smooth_curve_to(
        &mut self,
        handle2: Option<Vec2>,
        anchor: Vec2,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::SmoothCurve { handle2, anchor })
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
    /// Append another vector object's current world-space path.
    pub fn append_vectorized_mobject(&mut self, source: &Mobject) -> Result<(), AuthoringError> {
        self.require_same_store(source)?;
        let source = source.state()?;
        self.edit_path(PathEdit::AppendVectorized(&source))
    }
    pub fn close_path(&mut self) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Close)
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
