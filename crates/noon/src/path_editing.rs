//! Persistent vector edits publish new immutable content through one transaction.
use crate::{AuthoringError, Mobject};
use noon_core::{
    SemanticMutationTransaction, SemanticObjectContent, SemanticObjectState, SemanticStore,
    StoredGeometry, Vec2, VectorPath,
};

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
    state.transform.rotation_z = 0.;
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

pub(crate) fn path_transaction(
    object: noon_core::SemanticNodeId,
    before: &SemanticObjectState,
    mut after: SemanticObjectState,
    handle: noon_core::GeometryResourceHandle,
) -> SemanticMutationTransaction {
    after.content = StoredGeometry::Resource(handle).into();
    let mut transaction = SemanticMutationTransaction::new();
    crate::semantic_mobject::stage_state_changes(&mut transaction, object, before, &after);
    transaction
}

/// Inputs to shared authoring preparation, published using the normal content
/// replacement transaction. This is not retained scene or runtime state.
pub(crate) enum PathEdit<'a> {
    Corners(&'a [Vec2]),
    Start(Vec2),
    Line(Vec2),
    Quadratic(Vec2, Vec2),
    Cubic(Vec2, Vec2, Vec2),
    Close,
    Reverse,
    Subdivide(usize),
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
        if let Self::Corners(points) = self {
            return corners_path(points).map(Some);
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
        if let Self::Subdivide(additional) = self {
            return noon_geometry::subdivide_path(&path, additional)
                .map(Some)
                .map_err(AuthoringError::PathQuery);
        }
        if let Self::Reverse = self {
            return Ok(Some(noon_geometry::reverse_path(&path)));
        }
        if let Self::Start(point) = self {
            if let Some(noon_core::PathCommand::MoveTo { to }) = path.commands().last().copied() {
                // Complete an unfinished anchor as one degenerate curve before
                // beginning another subpath, matching Manim's point semantics.
                path = path.line_to(to);
            }
            path = path.move_to(point);
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
                | Self::Start(_)
                | Self::Reverse
                | Self::Subdivide(_)
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

pub(crate) fn world_path(
    store: &SemanticStore,
    state: &SemanticObjectState,
) -> Result<VectorPath, AuthoringError> {
    let checked = |name, value| {
        crate::semantic_mobject::authoring_render_f64(name, value).map(|value| value as f32)
    };
    let transform = state.transform;
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

impl Mobject {
    /// Set a polyline from world-space corners, preserving identity and paint.
    /// Copies retain their previous immutable path; no temporary object is used.
    pub fn set_points_as_corners(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Corners(points))
    }
    /// Begin a new world-space subpath without connecting it to the previous one.
    pub fn start_new_path(&mut self, point: Vec2) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Start(point))
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
        let before = self.state()?;
        let after = path_replacement_state(before.clone())?;
        let mut store = self.integration_store().borrow_mut();
        let Some(path) = edit.prepare(&store, &before)? else {
            return Ok(());
        };
        if path_is_unchanged(&store, &before, &after, &path) {
            return Ok(());
        }
        store.with_geometry_path(path, |store, handle| {
            path_transaction(self.node_id(), &before, after, handle)
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}
