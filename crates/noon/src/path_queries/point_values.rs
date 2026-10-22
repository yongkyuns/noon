//! Full-coordinate observations use the captured semantic array directly.
use super::*;

impl PathQuery {
    /// Every semantic coordinate in this snapshot's captured coordinate space.
    /// The returned values are derived observations, never another scene store.
    pub fn points_3d(&self) -> Result<Vec<SemanticVec3>, AuthoringError> {
        self.manim_points
            .iter()
            .map(|&point| self.point_3d(point))
            .collect()
    }

    /// Full cubic controls; incomplete trailing points do not invent a curve.
    pub fn curve_points_3d(&self, index: usize) -> Result<[SemanticVec3; 4], AuthoringError> {
        if index >= self.curve_count() {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidCurveIndex {
                    index,
                    count: self.curve_count(),
                },
            ));
        }
        let start = index * 4;
        let mut result = [SemanticVec3::ZERO; 4];
        for (offset, value) in result.iter_mut().enumerate() {
            *value = self.point_3d(self.manim_points[start + offset])?;
        }
        Ok(result)
    }

    pub fn start_3d(&self) -> Result<SemanticVec3, AuthoringError> {
        self.manim_points
            .first()
            .copied()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))
            .and_then(|point| self.point_3d(point))
    }

    pub fn end_3d(&self) -> Result<SemanticVec3, AuthoringError> {
        self.manim_points
            .last()
            .copied()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))
            .and_then(|point| self.point_3d(point))
    }

    fn point_3d(&self, point: SemanticVec3) -> Result<SemanticVec3, AuthoringError> {
        let point = self.transform.transform_xyz(point);
        if point.is_finite() {
            Ok(point)
        } else {
            Err(AuthoringError::NonFiniteGeometry)
        }
    }
}

// Reuse the established CE polynomial arithmetic for every coordinate.
// XY projection is explicit and never replaces the captured three-axis data.
pub(super) fn evaluate_cubic_xyz(controls: [SemanticVec3; 4], t: f64) -> SemanticVec3 {
    let (x, y) = evaluate_cubic(controls.map(SemanticVec3::xy), t);
    let (z, _) = evaluate_cubic(controls.map(|point| (point.z, 0.0)), t);
    SemanticVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn native_snapshot_retains_z_after_authoring_moves_and_keeps_its_old_values() {
        let mut scene = Scene::new();
        let points = vec![
            SemanticVec3::new(0.125, 0.25, 0.123456789012345),
            SemanticVec3::new(0.5, 0.75, 1.25),
            SemanticVec3::new(1.0, 1.5, 2.25),
            SemanticVec3::new(2.0, 3.0, 4.25),
            SemanticVec3::new(5.0, 6.0, 7.25),
        ];
        let mut object = scene
            .path(
                VectorPath::from_manim_points_3d(points.clone()),
                Default::default(),
            )
            .unwrap();
        let snapshot = object.path_query().unwrap();
        object.shift(1.5, -2.5).unwrap();
        let moved = object.path_query().unwrap();
        assert_eq!(snapshot.points_3d().unwrap(), points);
        assert_eq!(
            moved.start_3d().unwrap(),
            SemanticVec3::new(1.625, -2.25, 0.123456789012345)
        );
        assert_eq!(moved.end_3d().unwrap(), SemanticVec3::new(6.5, 3.5, 7.25));
        assert_eq!(moved.point_count(), 5);
        assert_eq!(moved.curve_count(), 1);
        assert_eq!(
            moved.points().unwrap(),
            moved
                .points_3d()
                .unwrap()
                .into_iter()
                .map(SemanticVec3::xy)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn native_closing_and_curve_samples_use_the_actual_third_coordinate() {
        let mut scene = Scene::new();
        let points = vec![
            SemanticVec3::new(0.0, 0.0, 0.0),
            SemanticVec3::new(1.0, 0.0, 0.25),
            SemanticVec3::new(1.0, 1.0, 0.75),
            SemanticVec3::new(0.0, 0.0, 1.0),
        ];
        let object = scene
            .path(
                VectorPath::from_manim_points_3d(points.clone()),
                Default::default(),
            )
            .unwrap();
        let query = object.path_query().unwrap();
        assert!(
            !query.is_closed().unwrap(),
            "coincident XY endpoints do not close a three-axis path"
        );
        assert_eq!(query.curve_points_3d(0).unwrap(), points.as_slice());
        assert_eq!(
            query.curve_points_at(0, &[0.0, 0.5, 1.0]).unwrap(),
            [(0.0, 0.0, 0.0), (0.75, 0.375, 0.5), (0.0, 0.0, 1.0)]
        );
    }

    #[test]
    fn empty_queries_fail_without_inventing_controls_or_endpoints() {
        let query = PathQuery::prepare(&VectorPath::new(), Default::default()).unwrap();
        assert!(query.points_3d().unwrap().is_empty());
        assert!(query.start_3d().is_err());
        assert!(query.end_3d().is_err());
        assert!(query.curve_points_3d(0).is_err());
    }
}

impl MobjectTarget<'_> {
    /// Full-coordinate points in the same recursive order and alias multiplicity
    /// as the planar convenience. Traversal stays in the semantic store.
    pub fn all_points_3d(&self) -> Result<Vec<SemanticVec3>, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let occurrences = FamilyPointObservation::RecursivePoints.nodes(&store, self.node_id())?;
        authored_points_for_occurrences(&store, &occurrences, PathQuery::points_3d)
    }
}

/// Read full-coordinate family occurrences from one coherent publication.
pub fn effective_all_points_3d(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<Vec<SemanticVec3>, AuthoringError> {
    effective_point_observation_map(
        store,
        execution,
        target,
        FamilyPointObservation::RecursivePoints,
        PathQuery::points_3d,
    )
}

#[cfg(test)]
mod family_tests;
