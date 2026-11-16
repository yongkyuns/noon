//! XYZ family payloads reuse the same identity and path publication as leaves.
use super::*;
use noon_core::SemanticVec3;

impl MobjectFamily {
    pub fn set_points_3d(&self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        if let Some(mut object) = self.own_object()? {
            return object.set_points_3d(points);
        }
        let path = crate::path_editing::raw_points_path_3d(points)?;
        publish_initial_family_path(
            &mut self.integration_store().borrow_mut(),
            self.node_id(),
            path,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }

    pub fn append_points_3d(&self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        match self.own_object()? {
            Some(mut object) => object.append_points_3d(points),
            None if points.is_empty() => Ok(()),
            None => self.set_points_3d(points),
        }
    }
}

impl crate::Scene {
    pub fn set_family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.family_points_3d(family, points, false)
    }

    pub fn append_family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.family_points_3d(family, points, true)
    }

    fn family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
        append: bool,
    ) -> Result<(), AuthoringError> {
        crate::MobjectTarget::from(family).require_store(self.integration_store())?;
        let object = family.own_object()?;
        self.require_resource_admission()?;
        if append && points.is_empty() {
            return Ok(());
        }
        match object {
            Some(object) if append => self.append_points_3d(&object, points),
            Some(object) => self.set_points_3d(&object, points),
            None => self
                .set_initial_family_path(family, crate::path_editing::raw_points_path_3d(points)?),
        }
    }
}

#[cfg(test)]
mod tests;
