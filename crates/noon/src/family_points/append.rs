//! Root-only appends reuse the existing payload and coherent path publication.
use super::*;

pub fn publish_append_family_points(
    store: &Rc<std::cell::RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &MobjectFamily,
    points: &[(f64, f64)],
) -> Result<(), AuthoringError> {
    publish_append_family_points_3d(
        store,
        root,
        execution,
        family,
        &crate::path_editing::promote_raw_points_3d(points),
    )
}

impl MobjectFamily {
    /// Append only this root's coordinates, preserving identity and children.
    pub fn append_points_f64(&self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
        self.append_points_3d(&crate::path_editing::promote_raw_points_3d(points))
    }

    pub fn append_points(&self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.append_points_f64(
            &points
                .iter()
                .map(|point| (f64::from(point.x), f64::from(point.y)))
                .collect::<Vec<_>>(),
        )
    }
}

impl crate::Scene {
    /// Append family-root content through ordinary authored or live publication.
    pub fn append_family_points_f64(
        &mut self,
        family: &MobjectFamily,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), family.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        if self.running_execution().is_some() {
            return self.with_running_execution(|store, root, execution| {
                publish_append_family_points(store, root, execution, family, points)
            });
        }
        family.append_points_f64(points)
    }
}

#[cfg(test)]
mod tests;
