//! Immutable observations of one retained path in an explicit coordinate space.
use crate::{
    AuthoringError, ExecutionSession, Mobject, MobjectTarget, UnsupportedAuthoringOperation,
};
use noon_core::{
    GeometryRef, GeometryResource, SemanticNodeId, SemanticNodeKind, SemanticObjectContent,
    SemanticStore, SemanticTransform2_5D, StoredGeometry, VectorPath,
};
use noon_geometry::{
    canonical_outline_path, PathProportionError, PathProportionPlan, MANIM_LENGTH_SAMPLE_POINTS,
};
use std::{cell::RefCell, rc::Rc};

/// A reusable snapshot of one path and its coordinate-space transform.
/// Preparation is O(path curves); proportion queries are O(log curves). This
/// observation owns derived query data, never scene or runtime authority.
#[derive(Clone, Debug)]
pub struct PathQuery {
    plan: Option<PathProportionPlan>,
    /// ManimCE's flattened four-point cubic storage, including padding between
    /// an unfinished subpath and the next MoveTo.
    manim_points: Vec<(f64, f64)>,
    transform: SemanticTransform2_5D,
    endpoints: Option<(noon_core::Vec2, noon_core::Vec2)>,
    unfinished_anchor: Option<noon_core::Vec2>,
}

impl PathQuery {
    pub(crate) fn prepare(
        path: &VectorPath,
        transform: SemanticTransform2_5D,
    ) -> Result<Self, AuthoringError> {
        let plan =
            match PathProportionPlan::with_scale(path, (transform.scale.x, transform.scale.y)) {
                Ok(plan) => Some(plan),
                Err(PathProportionError::EmptyPath) => None,
                Err(error) => return Err(AuthoringError::PathQuery(error)),
            };
        Ok(Self {
            manim_points: manim_points(path, plan.as_ref())?,
            plan,
            transform,
            endpoints: path.endpoints(),
            unfinished_anchor: match path.commands().last() {
                Some(noon_core::PathCommand::MoveTo { to }) => Some(*to),
                _ => None,
            },
        })
    }

    /// Number of complete four-point curves in Manim's stored point array.
    pub fn curve_count(&self) -> usize {
        self.manim_points.len() / 4
    }

    /// Manim's cubic point-array length, including unfinished anchors and any
    /// degenerate curve padding used to start a new subpath.
    pub fn point_count(&self) -> usize {
        self.manim_points.len()
    }

    /// Whether the path ends with a MoveTo anchor that has not joined a curve.
    /// Noon stores each cubic with four points, matching Manim's default stride.
    pub fn has_new_path_started(&self) -> bool {
        self.point_count() % 4 == 1
    }

    /// Cubic controls in this snapshot's coordinate space. Lines and quadratics
    /// are promoted exactly; the returned points are derived observations.
    pub fn curve_points(&self, index: usize) -> Result<[(f64, f64); 4], AuthoringError> {
        if index >= self.curve_count() {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidCurveIndex {
                    index,
                    count: self.curve_count(),
                },
            ));
        }
        let start = index * 4;
        Ok(std::array::from_fn(|offset| {
            let (x, y) = self.manim_points[start + offset];
            self.transform_point(x, y)
        }))
    }

    /// Evaluate one curve at each supplied parameter in this snapshot's
    /// coordinate space. Parameters outside `[0, 1]` extrapolate the curve.
    pub fn curve_points_at(
        &self,
        index: usize,
        parameters: &[f64],
    ) -> Result<Vec<(f64, f64, f64)>, AuthoringError> {
        let controls = self.curve_points(index)?;
        Ok(parameters
            .iter()
            .map(|&t| {
                let (x, y) = evaluate_cubic(controls, t);
                (x, y, 0.0)
            })
            .collect())
    }

    /// Straight-line sample lengths for one curve, matching ManimCE's
    /// `get_nth_curve_length_pieces` default of ten Bezier samples.
    pub fn curve_length_pieces(
        &self,
        index: usize,
        sample_points: Option<usize>,
    ) -> Result<Vec<f64>, AuthoringError> {
        let controls = self.curve_points(index)?;
        let sample_points = sample_points.unwrap_or(MANIM_LENGTH_SAMPLE_POINTS);
        if sample_points == 0 {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidSampleCount,
            ));
        }
        if sample_points == 1 {
            return Ok(Vec::new());
        }
        let mut pieces = Vec::with_capacity(sample_points - 1);
        let mut previous = controls[0];
        let denominator = (sample_points - 1) as f64;
        for sample in 1..sample_points {
            let t = sample as f64 / denominator;
            let current = evaluate_cubic(controls, t);
            pieces.push((current.0 - previous.0).hypot(current.1 - previous.1));
            previous = current;
        }
        Ok(pieces)
    }

    /// Approximate length of one curve using the same samples as its pieces.
    pub fn curve_length(
        &self,
        index: usize,
        sample_points: Option<usize>,
    ) -> Result<f64, AuthoringError> {
        Ok(self
            .curve_length_pieces(index, sample_points)?
            .into_iter()
            .sum())
    }

    fn curve_column(&self, column: usize) -> Vec<(f64, f64)> {
        let mut points = (0..self.curve_count())
            .map(|index| self.curve_points(index).expect("known curve index")[column])
            .collect::<Vec<_>>();
        if column == 0 {
            if let Some(point) = self.unfinished_anchor {
                points.push(self.transform_point(f64::from(point.x), f64::from(point.y)));
            }
        }
        points
    }

    pub fn start_anchors(&self) -> Vec<(f64, f64)> {
        self.curve_column(0)
    }

    /// Return Manim's shoelace direction for the ordered start anchors.
    /// This intentionally mirrors `numpy.trapezoid(y, x)` without adding a
    /// closing edge: Manim applies that operation to the anchor array as-is.
    pub fn direction(&self) -> &'static str {
        let anchors = self.start_anchors();
        let integral = anchors
            .windows(2)
            .map(|pair| (pair[1].0 - pair[0].0) * (pair[0].1 + pair[1].1) * 0.5)
            .sum::<f64>();
        if integral > 0.0 {
            "CW"
        } else {
            "CCW"
        }
    }

    /// Manim Polygram vertex runs end whenever a curve returns to their first
    /// anchor. This also splits repeated starting vertices, as upstream does.
    pub fn vertex_groups(&self) -> Vec<Vec<(f64, f64)>> {
        let mut groups = Vec::new();
        let mut group = Vec::new();
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            group.push(points[0]);
            if points_coincide(points[3], group[0]) {
                groups.push(std::mem::take(&mut group));
            }
        }
        groups
    }
    pub fn first_handles(&self) -> Vec<(f64, f64)> {
        self.curve_column(1)
    }
    pub fn second_handles(&self) -> Vec<(f64, f64)> {
        self.curve_column(2)
    }
    pub fn end_anchors(&self) -> Vec<(f64, f64)> {
        self.curve_column(3)
    }

    /// Start anchors, first handles, second handles and end anchors. An
    /// unfinished subpath contributes only to the start-anchor column.
    pub fn anchors_and_handles(&self) -> [Vec<(f64, f64)>; 4] {
        std::array::from_fn(|column| self.curve_column(column))
    }

    /// Ordered start/end anchors, followed by any unfinished starting anchor.
    pub fn anchors(&self) -> Vec<(f64, f64)> {
        let mut anchors = Vec::with_capacity(
            self.curve_count() * 2 + usize::from(self.unfinished_anchor.is_some()),
        );
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            anchors.extend([points[0], points[3]]);
        }
        if let Some(point) = self.unfinished_anchor {
            anchors.push(self.transform_point(f64::from(point.x), f64::from(point.y)));
        }
        anchors
    }

    /// ManimCE's flattened cubic point array, including any padded subpaths.
    pub fn points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        Ok(self
            .manim_points
            .iter()
            .map(|&(x, y)| self.transform_point(x, y))
            .collect())
    }

    /// Whether first and last anchors coincide in this snapshot's coordinates.
    pub fn is_closed(&self) -> Result<bool, AuthoringError> {
        Ok(points_coincide(self.start()?, self.end()?))
    }

    /// Cubic point runs separated by noncoincident anchors, matching Manim's
    /// observable subpaths. Explicit retained contour breaks remain unchanged.
    /// A dangling anchor joins the final run only when it coincides with its end.
    pub fn subpaths(&self) -> Vec<Vec<(f64, f64)>> {
        let mut result = Vec::new();
        let mut run = Vec::new();
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            if run
                .last()
                .is_some_and(|last| !points_coincide(*last, points[0]))
            {
                result.push(std::mem::take(&mut run));
            }
            run.extend(points);
        }
        if let Some(point) = self.unfinished_anchor {
            let point = self.transform_point(f64::from(point.x), f64::from(point.y));
            if run.last().is_some_and(|last| points_coincide(*last, point)) {
                run.push(point);
            }
        }
        if !run.is_empty() {
            result.push(run);
        }
        result
    }

    /// Point in the coordinate space captured when this query was prepared.
    pub fn point_from_proportion(&self, alpha: f64) -> Result<(f64, f64), AuthoringError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidProportion(alpha as f32),
            ));
        }
        if alpha == 1.0 {
            return self.end();
        }
        let point = self
            .plan
            .as_ref()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
            .point_f64(alpha)
            .map_err(AuthoringError::PathQuery)?;
        Ok(self.transform_point(point.x, point.y))
    }

    fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        let x = x * self.transform.scale.x;
        let y = y * self.transform.scale.y;
        let (sin, cos) = self.transform.rotation_z.sin_cos();
        (
            x * cos - y * sin + self.transform.translation.x,
            x * sin + y * cos + self.transform.translation.y,
        )
    }

    pub fn start(&self) -> Result<(f64, f64), AuthoringError> {
        self.endpoint(false)
    }
    pub fn end(&self) -> Result<(f64, f64), AuthoringError> {
        self.endpoint(true)
    }

    fn endpoint(&self, end: bool) -> Result<(f64, f64), AuthoringError> {
        let (first, last) = self
            .endpoints
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?;
        let point = if end { last } else { first };
        Ok(self.transform_point(f64::from(point.x), f64::from(point.y)))
    }

    /// Ten samples per curve by default, matching Manim's path query measure.
    pub fn arc_length(
        &self,
        sample_points_per_curve: Option<usize>,
    ) -> Result<f64, AuthoringError> {
        if sample_points_per_curve.is_some_and(|count| count < 2) {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidSampleCount,
            ));
        }
        self.plan.as_ref().map_or(Ok(0.0), |plan| {
            plan.arc_length(sample_points_per_curve)
                .map_err(AuthoringError::PathQuery)
        })
    }
}

pub(crate) fn points_coincide(left: (f64, f64), right: (f64, f64)) -> bool {
    (left.0 - right.0).abs() <= 1e-6 + 1e-5 * right.0.abs()
        && (left.1 - right.1).abs() <= 1e-6 + 1e-5 * right.1.abs()
}

fn evaluate_cubic(controls: [(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * controls[0].0
            + 3.0 * u * u * t * controls[1].0
            + 3.0 * u * t * t * controls[2].0
            + t * t * t * controls[3].0,
        u * u * u * controls[0].1
            + 3.0 * u * u * t * controls[1].1
            + 3.0 * u * t * t * controls[2].1
            + t * t * t * controls[3].1,
    )
}

fn manim_points(
    path: &VectorPath,
    plan: Option<&PathProportionPlan>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    let mut points = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    let mut curve_index = 0;

    for command in path.commands() {
        match *command {
            noon_core::PathCommand::MoveTo { to } => {
                let remainder = points.len() % 4;
                if remainder != 0 {
                    let start_index = ((points.len() - 1) / 4) * 4;
                    let start_anchor = points[start_index];
                    for _ in 0..(4 - remainder) {
                        points.push(start_anchor);
                    }
                }
                points.push((f64::from(to.x), f64::from(to.y)));
                current = Some(to);
                subpath_start = Some(to);
            }
            noon_core::PathCommand::LineTo { to }
            | noon_core::PathCommand::QuadraticTo { to, .. }
            | noon_core::PathCommand::CubicTo { to, .. } => {
                if current.is_some() {
                    let controls = plan
                        .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
                        .curve_points(curve_index)
                        .map_err(AuthoringError::PathQuery)?
                        .map(|point| (point.x, point.y));
                    append_manim_curve(&mut points, controls);
                    curve_index += 1;
                }
                current = Some(to);
            }
            noon_core::PathCommand::Close => {
                if let (Some(from), Some(start)) = (current, subpath_start) {
                    if (from - start).length() > f32::EPSILON {
                        let controls = plan
                            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
                            .curve_points(curve_index)
                            .map_err(AuthoringError::PathQuery)?
                            .map(|point| (point.x, point.y));
                        append_manim_curve(&mut points, controls);
                        curve_index += 1;
                    }
                    current = Some(start);
                }
            }
        }
    }
    Ok(points)
}

fn append_manim_curve(points: &mut Vec<(f64, f64)>, controls: [(f64, f64); 4]) {
    if points.len() % 4 == 1 {
        points.extend(controls.into_iter().skip(1));
    } else {
        points.extend(controls);
    }
}

pub(crate) fn content_path(
    store: &SemanticStore,
    content: SemanticObjectContent,
) -> Result<std::borrow::Cow<'_, VectorPath>, AuthoringError> {
    let SemanticObjectContent::Geometry(content) = content else {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PathQueryContent,
        ));
    };
    let primitive = match content {
        StoredGeometry::Circle { radius } => GeometryRef::circle(radius),
        StoredGeometry::Rectangle { size } => GeometryRef::Rectangle { size },
        StoredGeometry::Line { start, end } => GeometryRef::Line { start, end },
        StoredGeometry::Resource(handle) => {
            match store
                .geometry_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingGeometryResource(handle))?
            {
                GeometryResource::VectorPath(path) => {
                    return Ok(std::borrow::Cow::Borrowed(path));
                }
                GeometryResource::Mesh(_) => {
                    return Err(AuthoringError::Unsupported(
                        UnsupportedAuthoringOperation::PathQueryContent,
                    ));
                }
            }
        }
    };
    Ok(std::borrow::Cow::Owned(
        canonical_outline_path(&primitive).expect("analytic primitive outline"),
    ))
}

pub(crate) fn prepare_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
    transform: SemanticTransform2_5D,
) -> Result<PathQuery, AuthoringError> {
    PathQuery::prepare(content_path(store, content)?.as_ref(), transform)
}

fn family_point_occurrences(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, AuthoringError> {
    fn visit(
        store: &SemanticStore,
        id: SemanticNodeId,
        result: &mut Vec<SemanticNodeId>,
    ) -> Result<(), AuthoringError> {
        let node = store
            .node(id)
            .ok_or_else(|| AuthoringError::from(noon_core::SemanticStoreError::UnknownNode(id)))?;
        match node.kind() {
            SemanticNodeKind::AuthoringObject => result.push(id),
            SemanticNodeKind::Family(_) => {
                for member in node.members() {
                    visit(store, member, result)?;
                }
            }
            SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_)
            | SemanticNodeKind::Effect(_) => {}
        }
        Ok(())
    }

    let mut result = Vec::new();
    visit(store, root, &mut result)?;
    Ok(result)
}

fn authored_points_for_occurrences(
    store: &SemanticStore,
    occurrences: &[SemanticNodeId],
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    let mut result = Vec::new();
    for id in occurrences {
        let state = store
            .semantic_object_state_checked(*id)
            .map_err(AuthoringError::from)?;
        let transform = state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        result.extend(prepare_content(store, state.content, transform)?.points()?);
    }
    Ok(result)
}

impl MobjectTarget<'_> {
    /// Return authored points for this object/family in Manim's recursive
    /// pre-order, preserving repeated family aliases as repeated point runs.
    pub fn all_points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let occurrences = family_point_occurrences(&store, self.node_id())?;
        authored_points_for_occurrences(&store, &occurrences)
    }
}

/// Observe current effective points for all object occurrences below a target.
/// Membership is snapshotted once; each immutable path query reads the same
/// execution session and preserves Manim's recursive alias multiplicity.
pub fn effective_all_points(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    if !Rc::ptr_eq(store, target.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    target.validate()?;
    let occurrences = {
        let semantics = store.borrow();
        family_point_occurrences(&semantics, target.node_id())?
    };
    let mut result = Vec::new();
    for id in occurrences {
        let object = Mobject::from_node(Rc::clone(store), id)?;
        result.extend(effective_path_query(store, execution, &object)?.points()?);
    }
    Ok(result)
}

/// Observe one object's exact current path from an existing coherent execution.
///
/// This is explicit low-level integration over the existing Semantic Scene and Runtime
/// authorities. It owns no scene/session state and performs no publication.
pub fn effective_path_query(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<PathQuery, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()?;
    let store = store.borrow();
    let observed = execution
        .effective_semantic_object(&store, object.node_id())
        .map_err(AuthoringError::from)?;
    let unsupported =
        || AuthoringError::Unsupported(UnsupportedAuthoringOperation::EffectivePathRenderOverride);
    let geometry = observed
        .render_geometry
        .or_else(|| observed.object.content.geometry())
        .ok_or(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PathQueryContent,
        ))?;
    let mut path = noon_geometry::canonical_outline_path(geometry).ok_or(
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::PathQueryContent),
    )?;
    if let Some(target) = path.morph_target() {
        path = noon_geometry::interpolate_revealed_path_preserving_order(
            &path,
            target,
            observed.morph,
            observed.reveal,
        )
        .map_err(AuthoringError::MorphQuery)?;
    } else if observed.morph != 0.0 {
        return Err(unsupported());
    } else if observed.reveal != 1.0 {
        path = noon_geometry::authored_partial_path(&path, 0.0, observed.reveal);
    }
    let state = store
        .semantic_object_state_checked(object.node_id())
        .map_err(AuthoringError::from)?;
    let transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
        state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?,
        observed
            .render_transform
            .unwrap_or(observed.object.transform),
    );
    PathQuery::prepare(&path, transform)
}

impl Mobject {
    /// Prepare a query over immutable local/content-space geometry.
    pub fn local_path_query(&self) -> Result<PathQuery, AuthoringError> {
        let store = self.integration_store().borrow();
        let state = store
            .semantic_object_state_checked(self.node_id())
            .map_err(AuthoringError::from)?;
        prepare_content(&store, state.content, SemanticTransform2_5D::default())
    }

    /// Prepare a query over authored world-space geometry. During execution,
    /// use [`crate::Scene::effective_path_query`] for the current Runtime publication.
    pub fn path_query(&self) -> Result<PathQuery, AuthoringError> {
        let store = self.integration_store().borrow();
        let state = store
            .semantic_object_state_checked(self.node_id())
            .map_err(AuthoringError::from)?;
        prepare_content(
            &store,
            state.content,
            state
                .transform
                .as_planar()
                .ok_or(AuthoringError::NonFiniteObjectState)?,
        )
    }
}
