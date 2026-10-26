//! Full-coordinate critical points for retained content and mixed families.
use super::*;
use crate::SemanticVec3;

impl LayoutAnchor {
    /// Read content boundary extrema without converting opaque content to paths.
    pub fn critical_point_3d(
        &self,
        direction: SemanticVec3,
    ) -> Result<SemanticVec3, AuthoringError> {
        if !direction.is_finite() {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        let layout = self.layout()?;
        let (x, y) = layout.critical_point(direction.x, direction.y);
        let z = crate::semantic_mobject::boundary_depth_for_leaves(
            &self.store.borrow(),
            &layout.leaves,
            direction.z,
        )?;
        Ok(SemanticVec3::new(x, y, z))
    }
}

/// Read mixed-content boundaries from one coherent execution publication.
pub(crate) fn effective_layout_critical_point_3d(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    source: &LayoutAnchor,
    direction: SemanticVec3,
) -> Result<SemanticVec3, AuthoringError> {
    if !direction.is_finite() {
        return Err(AuthoringError::NonFiniteObjectState);
    }
    let (leaves, boundary) = effective_anchor_layout_measure(store, execution, source, true)?;
    let (x, y) = bounds_critical_point(boundary, direction.x, direction.y);
    // The planar runtime has no independent Z driver. The coherent layout read
    // above validates content/affine overrides before canonical depth.
    let z =
        crate::semantic_mobject::boundary_depth_for_leaves(&store.borrow(), &leaves, direction.z)?;
    Ok(SemanticVec3::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImageMobjectOptions, Scene};

    #[test]
    fn opaque_content_depth_survives_authored_and_live_mixed_family_queries() {
        let mut scene = Scene::new();
        let mut options = ImageMobjectOptions::rgba8(1, 1, vec![255; 4]).unwrap();
        options.set_height(2.).unwrap();
        let mut image = scene.image(options).unwrap();
        image.shift_3d(SemanticVec3::new(-3., 0., 1.125)).unwrap();
        let mut circle = scene.circle(0.2).unwrap();
        circle.shift_3d(SemanticVec3::new(2., 0., -3.5)).unwrap();
        let nested = scene.family(&[(&image).into(), (&circle).into()]).unwrap();
        let family = scene.family(&[(&nested).into(), (&image).into()]).unwrap();
        let anchor = LayoutAnchor::from(&family);
        let up = SemanticVec3::new(0., 0., 1.);
        let down = SemanticVec3::new(0., 0., -1.);
        assert_eq!(anchor.critical_point_3d(up).unwrap().z, 1.125);
        assert_eq!(anchor.critical_point_3d(down).unwrap().z, -3.5);
        assert_eq!(
            LayoutAnchor::from(&image).critical_point_3d(down).unwrap(),
            SemanticVec3::new(-3., 0., 1.125)
        );
        scene.add_many(&[(&family).into()]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let before = execution.publication_context();
        let expected = anchor.critical_point_3d(up).unwrap();
        assert_eq!(
            scene
                .live(&mut execution)
                .layout_critical_point_3d(&anchor, up)
                .unwrap(),
            expected
        );
        assert_eq!(execution.publication_context(), before);
        scene
            .live(&mut execution)
            .shift_3d(&image, SemanticVec3::new(1.5, 0., 2.))
            .unwrap();
        let before = execution.publication_context();
        let actual = scene
            .live(&mut execution)
            .layout_critical_point_3d(&anchor, up)
            .unwrap();
        assert_eq!(actual.z, 3.125);
        assert_eq!(actual.xy(), anchor.layout().unwrap().center());
        assert_eq!(execution.publication_context(), before);
        let foreign = Scene::new().square(1.).unwrap();
        assert!(scene
            .live(&mut execution)
            .layout_critical_point_3d(&LayoutAnchor::from(&foreign), up)
            .is_err());
        assert!(scene
            .live(&mut execution)
            .layout_critical_point_3d(&anchor, SemanticVec3::new(0., 0., f64::NAN))
            .is_err());
        assert_eq!(execution.publication_context(), before);
    }
}
