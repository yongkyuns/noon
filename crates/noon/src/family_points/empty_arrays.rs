//! Zero-row array shapes use the existing shared path publication transaction.
use super::*;
use noon_core::PointArrayShape;

impl MobjectFamily {
    pub fn set_empty_point_array(&self, shape: PointArrayShape) -> Result<(), AuthoringError> {
        let path = VectorPath::from_empty_point_array(shape.clone())?;
        if let Some(mut object) = self.own_object()? {
            return object.set_empty_point_array(shape);
        }
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
}

impl crate::MobjectTarget<'_> {
    pub fn set_empty_point_array(&self, shape: PointArrayShape) -> Result<(), AuthoringError> {
        match self {
            Self::Object(object) => Mobject::clone(object).set_empty_point_array(shape),
            Self::Family(family) => family.set_empty_point_array(shape),
        }
    }
}

impl crate::Scene {
    pub fn set_empty_point_array(
        &mut self,
        target: crate::MobjectTarget<'_>,
        shape: PointArrayShape,
    ) -> Result<(), AuthoringError> {
        target.require_store(self.integration_store())?;
        let edit = crate::path_editing::PathEdit::EmptyPointArray(&shape);
        match target {
            crate::MobjectTarget::Object(object) => self.edit_object_path(object, edit),
            crate::MobjectTarget::Family(family) => {
                let path = VectorPath::from_empty_point_array(shape.clone())?;
                if let Some(object) = family.own_object()? {
                    return self.edit_object_path(&object, edit);
                }
                self.require_resource_admission()?;
                self.set_initial_family_path(family, path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_preserves_the_shape_of_new_and_existing_empty_family_paths() {
        let mut scene = crate::Scene::new();
        let family = scene.family(&[]).unwrap();
        let target = crate::MobjectTarget::from(&family);
        let initial = PointArrayShape::new(vec![0, 2]).unwrap();
        scene
            .set_empty_point_array(target, initial.clone())
            .unwrap();
        assert_eq!(target.point_array().unwrap().shape(), &initial);

        let mut running = scene.into_execution().unwrap();
        let shape = PointArrayShape::new(vec![0, 2, 3]).unwrap();
        running
            .scene_mut()
            .set_empty_point_array(target, shape.clone())
            .unwrap();
        assert_eq!(target.point_array().unwrap().shape(), &shape);
        let revision = running.scene().revision();
        let invalid = PointArrayShape::xyz(1).unwrap();
        assert!(running
            .scene_mut()
            .set_empty_point_array(target, invalid)
            .is_err());
        assert_eq!(running.scene().revision(), revision);
        assert_eq!(target.point_array().unwrap().shape(), &shape);
    }
}
