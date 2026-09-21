use noon_core::{
    SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectRole,
    SemanticObjectState, SemanticStyle, StoredGeometry, Vec2, DEFAULT_FRAME_HEIGHT,
    DEFAULT_FRAME_WIDTH,
};

use crate::{AuthoringError, Bounds2D64, Mobject, Scene};

/// Pure axis-aligned camera fit result. Apply it to the ordinary camera frame
/// target and animate that target through the existing transform lifecycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAutoFrame {
    pub center: (f64, f64),
    pub height: f64,
}

fn union_camera_bounds(total: &mut Option<Bounds2D64>, next: Bounds2D64) {
    if let Some(total) = total {
        total.include(next.min_x, next.min_y);
        total.include(next.max_x, next.max_y);
    } else {
        *total = Some(next);
    }
}

impl Scene {
    /// Compute an authored camera target for the supplied objects without mutating
    /// scene, camera, or object state. Bounds are observed once from the existing
    /// semantic layout path; callers animate the returned target normally.
    pub fn camera_auto_frame(
        &self,
        objects: &[&Mobject],
        aspect: f64,
        margin: f64,
    ) -> Result<CameraAutoFrame, AuthoringError> {
        if objects.is_empty() {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "at least one object is required",
            ));
        }
        if !aspect.is_finite() || aspect <= 0.0 {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "aspect must be finite and positive",
            ));
        }
        if !margin.is_finite() || margin < 0.0 {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "margin must be finite and non-negative",
            ));
        }
        let mut total = None;
        for object in objects {
            if !std::rc::Rc::ptr_eq(self.integration_store(), object.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            object.validate()?;
            let bounds =
                object
                    .boundary_bounds()?
                    .ok_or(AuthoringError::InvalidCameraAutoFrame(
                        "target has no measurable bounds",
                    ))?;
            for value in [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y] {
                if !value.is_finite() {
                    return Err(AuthoringError::InvalidCameraAutoFrame(
                        "bounds must be finite",
                    ));
                }
            }
            union_camera_bounds(&mut total, bounds);
        }
        let bounds = total.expect("non-empty objects produced bounds");
        let width = bounds.width();
        let height = bounds.height();
        let padded_width = width + 2.0 * margin;
        let padded_height = height + 2.0 * margin;
        if !padded_width.is_finite() || !padded_height.is_finite() {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "padded bounds are not representable",
            ));
        }
        let fitted_height = padded_height.max(padded_width / aspect);
        if !fitted_height.is_finite() || fitted_height <= 0.0 || fitted_height > f64::from(f32::MAX)
        {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "result is not representable",
            ));
        }
        let center = (
            bounds.min_x + (bounds.max_x - bounds.min_x) * 0.5,
            bounds.min_y + (bounds.max_y - bounds.min_y) * 0.5,
        );
        if !center.0.is_finite()
            || !center.1.is_finite()
            || center.0.abs() > f64::from(f32::MAX)
            || center.1.abs() > f64::from(f32::MAX)
        {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "center is not representable",
            ));
        }
        Ok(CameraAutoFrame {
            center,
            height: fitted_height,
        })
    }

    /// Build an ordinary detached camera-frame target from a pure auto-frame fit.
    pub fn camera_auto_frame_target(
        &self,
        frame: &Mobject,
        objects: &[&Mobject],
        aspect: f64,
        margin: f64,
    ) -> Result<Mobject, AuthoringError> {
        if !std::rc::Rc::ptr_eq(self.integration_store(), frame.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let state = frame.state()?;
        if state.role() != noon_core::SemanticObjectRole::Camera2D {
            return Err(AuthoringError::InvalidCameraAutoFrame(
                "target requires the Scene camera frame",
            ));
        }
        let fit = self.camera_auto_frame(objects, aspect, margin)?;
        let mut target = frame.target_editor()?;
        target.set_translation(fit.center.0, fit.center.1)?;
        let scale = fit.height / f64::from(DEFAULT_FRAME_HEIGHT);
        target.set_scale(scale, scale)?;
        Ok(target)
    }

    /// Create and attach the one ordinary semantic object that defines this scene's 2D camera.
    ///
    /// Camera creation is scene initialization: the root must still be empty. Allocation and root
    /// membership then commit in one semantic transaction. The frame remains an ordinary
    /// transformable Mobject; its role only tells lowering which effective transform supplies the
    /// renderer viewport.
    pub fn camera_frame(&mut self) -> Result<Mobject, AuthoringError> {
        let store = self.integration_store().borrow();
        let root_is_empty = store
            .node(self.root())
            .ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(
                    self.root(),
                ))
            })?
            .members()
            .is_empty();
        drop(store);
        if !root_is_empty {
            return Err(AuthoringError::CameraRequiresEmptyScene(self.root()));
        }

        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT),
        });
        state.style = SemanticStyle {
            object_opacity: 0.0,
            ..SemanticStyle::default()
        };
        state.set_role(SemanticObjectRole::Camera2D);

        let mut transaction = SemanticMutationTransaction::new();
        let frame = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(self.root(), frame);
        let result = transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(AuthoringError::from)?;
        let id = result
            .resolve(frame)
            .ok_or(AuthoringError::UnresolvedCreatedNode(frame))?;
        debug_assert!(matches!(
            result.impacts(),
            [
                SemanticMutationImpact::NodeAdded { .. },
                SemanticMutationImpact::FamilyMemberAdded { .. }
            ]
        ));
        Mobject::from_node(std::rc::Rc::clone(self.integration_store()), id)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, Camera2DState, RateFunction, SemanticObjectRole, Vec2,
        DEFAULT_FRAME_HEIGHT,
    };

    use super::*;

    #[test]
    fn camera_creation_is_atomic_unique_and_uses_the_scene_identity_space() {
        let mut scene = Scene::new();
        let revision = scene.integration_store().borrow().scene_revision();
        let frame = scene.camera_frame().unwrap();
        let state = frame.state().unwrap();
        assert_eq!(state.role(), SemanticObjectRole::Camera2D);
        assert_eq!(state.style.object_opacity, 0.0);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .node(scene.root())
                .unwrap()
                .members(),
            [frame.node_id()]
        );
        assert_ne!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );

        let before = scene.integration_store().borrow().scene_revision();
        assert_eq!(
            scene.camera_frame().unwrap_err(),
            AuthoringError::CameraRequiresEmptyScene(scene.root())
        );
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    }

    #[test]
    fn camera_creation_rejects_existing_scene_content_without_mutation() {
        let mut scene = Scene::new();
        let square = scene.square(2.0).unwrap();
        scene.add(&square).unwrap();
        let before = scene.integration_store().borrow().scene_revision();

        assert_eq!(
            scene.camera_frame().unwrap_err(),
            AuthoringError::CameraRequiresEmptyScene(scene.root())
        );
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .node(scene.root())
                .unwrap()
                .members(),
            [square.node_id()]
        );
    }

    #[test]
    fn ordinary_transform_drives_the_effective_camera_and_publishes_its_endpoint() {
        let mut scene = Scene::new();
        let frame = scene.camera_frame().unwrap();
        let mut target = frame.target_editor().unwrap();
        target.set_translation(-2.0, 0.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::ZERO,
                height: DEFAULT_FRAME_HEIGHT,
            }
        );

        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                &frame,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, 0.5)
            .unwrap();
        assert_eq!(session.camera().unwrap().center, Vec2::new(-1.0, 0.0));
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(session.camera().unwrap().center, Vec2::new(-2.0, 0.0));
        assert_eq!(frame.state().unwrap().transform.translation.x, -2.0);
    }
}
