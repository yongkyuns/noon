//! Immutable observations of one retained path in an explicit coordinate space.
use crate::{AuthoringError, ExecutionSession, Mobject, UnsupportedAuthoringOperation};
use noon_core::{
    GeometryRef, GeometryResource, SemanticObjectContent, SemanticStore, SemanticTransform2_5D,
    StoredGeometry, VectorPath,
};
use noon_geometry::{canonical_outline_path, PathProportionError, PathProportionPlan};
use std::{cell::RefCell, rc::Rc};

/// A reusable snapshot of one path and its coordinate-space transform.
/// Preparation is O(path curves); proportion queries are O(log curves). This
/// observation owns derived query data, never scene or runtime authority.
#[derive(Clone, Debug)]
pub struct PathQuery {
    plan: Option<PathProportionPlan>,
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
            plan,
            transform,
            endpoints: path.endpoints(),
            unfinished_anchor: match path.commands().last() {
                Some(noon_core::PathCommand::MoveTo { to }) => Some(*to),
                _ => None,
            },
        })
    }

    /// Number of complete curves; an unfinished MoveTo is an anchor only.
    pub fn curve_count(&self) -> usize {
        self.plan
            .as_ref()
            .map_or(0, PathProportionPlan::curve_count)
    }

    /// Cubic controls in this snapshot's coordinate space. Lines and quadratics
    /// are promoted exactly; the returned points are derived observations.
    pub fn curve_points(&self, index: usize) -> Result<[(f64, f64); 4], AuthoringError> {
        let plan = self.plan.as_ref().ok_or(AuthoringError::PathQuery(
            PathProportionError::InvalidCurveIndex { index, count: 0 },
        ))?;
        Ok(plan
            .curve_points(index)
            .map_err(AuthoringError::PathQuery)?
            .map(|p| self.transform_point(p.x, p.y)))
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
            let GeometryResource::VectorPath(path) = store
                .geometry_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingGeometryResource(handle))?;
            return Ok(std::borrow::Cow::Borrowed(path));
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
        // Retained morph meshes currently use flattened sample progress for reveal.
        // Do not report different curve-parameter geometry here.
        if observed.reveal != 1.0 {
            return Err(unsupported());
        }
        path = noon_geometry::interpolate_path_preserving_order(&path, target, observed.morph)
            .map_err(AuthoringError::MorphQuery)?;
    } else if observed.morph != 0.0 {
        return Err(unsupported());
    }
    if observed.reveal != 1.0 {
        path = noon_geometry::authored_partial_path(&path, 0.0, observed.reveal);
    }
    let state = store
        .semantic_object_state_checked(object.node_id())
        .map_err(AuthoringError::from)?;
    let transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
        state.transform,
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
        prepare_content(&store, state.content, state.transform)
    }
}
