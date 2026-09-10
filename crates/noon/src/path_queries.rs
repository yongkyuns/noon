//! Immutable observations of one retained path in an explicit coordinate space.
use crate::{AuthoringError, Mobject, UnsupportedAuthoringOperation};
use noon_core::{
    GeometryRef, GeometryResource, SemanticObjectContent, SemanticStore, SemanticTransform2_5D,
    StoredGeometry, VectorPath,
};
use noon_geometry::{canonical_outline_path, PathProportionError, PathProportionPlan};

/// A reusable snapshot of one path and its coordinate-space transform.
/// Preparation is O(path curves); proportion queries are O(log curves). This
/// observation owns derived query data, never scene or runtime authority.
#[derive(Clone, Debug)]
pub struct PathQuery {
    plan: Option<PathProportionPlan>,
    transform: SemanticTransform2_5D,
}

impl PathQuery {
    fn prepare(
        path: &VectorPath,
        transform: SemanticTransform2_5D,
    ) -> Result<Self, AuthoringError> {
        let plan =
            match PathProportionPlan::with_scale(path, (transform.scale.x, transform.scale.y)) {
                Ok(plan) => Some(plan),
                Err(PathProportionError::EmptyPath) => None,
                Err(error) => return Err(AuthoringError::PathQuery(error)),
            };
        Ok(Self { plan, transform })
    }

    /// Point in the coordinate space captured when this query was prepared.
    pub fn point_from_proportion(&self, alpha: f64) -> Result<(f64, f64), AuthoringError> {
        let point = self
            .plan
            .as_ref()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
            .point_f64(alpha)
            .map_err(AuthoringError::PathQuery)?;
        let x = point.x * self.transform.scale.x;
        let y = point.y * self.transform.scale.y;
        let (sin, cos) = self.transform.rotation_z.sin_cos();
        Ok((
            x * cos - y * sin + self.transform.translation.x,
            x * sin + y * cos + self.transform.translation.y,
        ))
    }

    pub fn start(&self) -> Result<(f64, f64), AuthoringError> {
        self.point_from_proportion(0.0)
    }
    pub fn end(&self) -> Result<(f64, f64), AuthoringError> {
        self.point_from_proportion(1.0)
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
    /// use `LiveSession::effective_path_query` for the current publication.
    pub fn path_query(&self) -> Result<PathQuery, AuthoringError> {
        let store = self.integration_store().borrow();
        let state = store
            .semantic_object_state_checked(self.node_id())
            .map_err(AuthoringError::from)?;
        prepare_content(&store, state.content, state.transform)
    }
}
