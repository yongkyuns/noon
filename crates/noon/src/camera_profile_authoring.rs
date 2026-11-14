//! Shared native camera-profile authoring through semantic transactions and tracks.

use noon_core::{
    AnimationOptions, ManimCamera3DProfile, SemanticMutationTransaction, SemanticObjectRole,
    SemanticObjectState, SemanticProjection3D, StoredGeometry,
};

use crate::{AuthoringError, DeclaredAnimation, Mobject, Scene};

fn perspective_clips(object: &Mobject) -> Result<(f64, f64), AuthoringError> {
    let state = object.state()?;
    if state.role() != SemanticObjectRole::Camera3D {
        return Err(AuthoringError::NonFiniteObjectState);
    }
    match state.camera_projection() {
        Some(SemanticProjection3D::Perspective { near, far, .. }) => Ok((near, far)),
        _ => Err(AuthoringError::NonFiniteObjectState),
    }
}

impl Scene {
    /// Create the scene camera from a Manim finite-perspective camera profile.
    pub fn camera_3d_profile(
        &mut self,
        profile: ManimCamera3DProfile,
        near: f64,
        far: f64,
    ) -> Result<Mobject, AuthoringError> {
        profile
            .camera(near, far)
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        self.require_camera_setup()?;
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        state.set_role(SemanticObjectRole::Camera3D);
        state
            .set_camera_profile(profile, near, far)
            .map_err(|_| AuthoringError::NonFiniteObjectState)?;
        self.create_role_object(state, true)
    }

    /// Read camera profile from the current effective frame when running, otherwise authored state.
    pub fn effective_camera_profile(
        &self,
        camera: &Mobject,
    ) -> Result<(ManimCamera3DProfile, f64, f64), AuthoringError> {
        self.require_object(camera)?;
        if let Some(execution) = self.running_execution() {
            execution.require_published_store(&self.integration_store().borrow())?;
            return execution
                .effective_camera_profile(camera.node_id())
                .ok_or(AuthoringError::NonFiniteObjectState);
        }
        let state = camera.state()?;
        let profile = state
            .camera_profile()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        let (near, far) = perspective_clips(camera)?;
        Ok((profile, near, far))
    }

    /// Replace a camera profile, deriving world pose and projection in one mutation.
    pub fn set_camera_profile(
        &mut self,
        camera: &Mobject,
        profile: ManimCamera3DProfile,
    ) -> Result<(), AuthoringError> {
        self.require_object(camera)?;
        let (near, far) = perspective_clips(camera)?;
        if profile.camera(near, far).is_none() {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        if camera
            .state()?
            .camera_motions()
            .last()
            .is_some_and(|motion| motion.end().is_none())
        {
            return Err(AuthoringError::CameraMotionAlreadyActive);
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_camera_profile(camera.node_id(), profile, near, far);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Set unwrapped Manim orientation angles while retaining the camera optics and center.
    pub fn set_camera_orientation(
        &mut self,
        camera: &Mobject,
        phi: f64,
        theta: f64,
        gamma: f64,
    ) -> Result<(), AuthoringError> {
        self.require_object(camera)?;
        let mut profile = self.effective_camera_profile(camera)?.0;
        profile.phi = phi;
        profile.theta = theta;
        profile.gamma = gamma;
        self.set_camera_profile(camera, profile)
    }

    /// Declare an ordinary camera-profile animation on this Scene.
    pub fn declare_camera_profile_move(
        &mut self,
        camera: &Mobject,
        profile: ManimCamera3DProfile,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        self.require_object(camera)?;
        let state = camera.state()?;
        let (near, far) = match state.camera_projection() {
            Some(SemanticProjection3D::Perspective { near, far, .. }) => (near, far),
            _ => {
                return Err(AuthoringError::InvalidAnimationDeclaration(
                    "camera profile tracks require a perspective Camera3D",
                ))
            }
        };
        if state.role() != SemanticObjectRole::Camera3D
            || state.camera_profile().is_none()
            || profile.camera(near, far).is_none()
        {
            return Err(AuthoringError::InvalidAnimationDeclaration(
                "camera profile endpoint is invalid for this camera",
            ));
        }
        self.declare_animation(
            noon_core::SemanticAnimationIntent::CameraProfileTo {
                target: camera.node_id(),
                profile,
            },
            options,
        )
    }
}

impl crate::LiveSession<'_> {
    /// Atomically declare and activate a profile endpoint with the ordinary composition scheduler.
    pub fn move_camera_profile(
        &mut self,
        camera: &Mobject,
        profile: ManimCamera3DProfile,
        options: AnimationOptions,
    ) -> Result<crate::ExecutionSegment, crate::LiveSessionError> {
        self.require_mobject(camera)?;
        let (near, far) = perspective_clips(camera)?;
        if camera.state()?.camera_profile().is_none() || profile.camera(near, far).is_none() {
            return Err(AuthoringError::NonFiniteObjectState.into());
        }
        let request = crate::AnimationCompositionRequest::CameraProfile {
            target: camera,
            profile,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{SemanticAnimationIntent, SemanticVec3};

    fn profile() -> ManimCamera3DProfile {
        ManimCamera3DProfile {
            phi: 0.7,
            theta: -1.1,
            gamma: 0.2,
            focal_distance: 5.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::new(0.5, -0.25, 0.75),
        }
    }

    #[test]
    fn profile_camera_creation_and_orientation_are_derived_and_atomic() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        let state = camera.state().unwrap();
        let expected = profile().camera(0.1, 100.0).unwrap();
        assert_eq!(state.camera_profile(), Some(profile()));
        assert_eq!(state.transform.translation, expected.position);
        assert_eq!(state.camera_projection(), Some(expected.projection));

        scene
            .set_camera_orientation(&camera, 0.9, 4.0, -0.3)
            .unwrap();
        let changed = camera.state().unwrap();
        let mut endpoint = profile();
        endpoint.phi = 0.9;
        endpoint.theta = 4.0;
        endpoint.gamma = -0.3;
        assert_eq!(changed.camera_profile(), Some(endpoint));
        assert_eq!(
            changed.transform.translation,
            endpoint.camera(0.1, 100.0).unwrap().position
        );

        let revision = scene.integration_store().borrow().scene_revision();
        assert!(scene
            .set_camera_orientation(&camera, f64::NAN, 0.0, 0.0)
            .is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(camera.state().unwrap().camera_profile(), Some(endpoint));
    }

    #[test]
    fn camera_profile_goal_is_a_standard_replayable_animation_intent() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        let mut endpoint = profile();
        endpoint.theta += std::f64::consts::TAU;
        endpoint.zoom = 1.5;
        let animation = scene
            .declare_camera_profile_move(&camera, endpoint, AnimationOptions::new().run_time(2.0))
            .unwrap();
        let store = scene.integration_store().borrow();
        assert!(matches!(
            store.semantic_animation_state(animation.node_id()).unwrap().intent(),
            SemanticAnimationIntent::CameraProfileTo { target, profile: actual }
                if *target == camera.node_id() && *actual == endpoint
        ));
    }

    #[test]
    fn camera_profile_rejects_a_stale_running_publication() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        let marker = scene.circle(0.5).unwrap();
        scene.add(&marker).unwrap();
        let execution = scene.into_execution().unwrap();

        assert_eq!(
            execution.scene().effective_camera_profile(&camera).unwrap(),
            (profile(), 0.1, 100.0)
        );
        // A low-level authored edit makes this runtime's revision stale.
        let mut external = SemanticMutationTransaction::new();
        external.set_property(
            marker.node_id(),
            noon_core::SemanticObjectProperty::RotationZ,
            0.5,
        );
        external
            .apply(&mut execution.scene().integration_store().borrow_mut())
            .unwrap();
        let error = execution
            .scene()
            .effective_camera_profile(&camera)
            .unwrap_err();
        assert!(matches!(
            error,
            AuthoringError::ExecutionPublication(
                crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
            )
        ));
    }

    #[test]
    fn orientation_reads_effective_optics_and_edits_require_segment_completion() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        let mut target = profile();
        target.phi += 0.8;
        target.zoom = 2.25;
        target.focal_distance = 7.5;
        target.frame_height = 6.0;
        target.frame_center = SemanticVec3::new(-1.0, 0.4, 2.0);

        let mut execution = scene.into_execution().unwrap();
        let segment = execution
            .live()
            .move_camera_profile(&camera, target, AnimationOptions::new().run_time(2.0))
            .unwrap();
        execution
            .live()
            .advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        let before = execution.scene().effective_camera_profile(&camera).unwrap();
        assert_eq!(before.0.zoom, (profile().zoom + target.zoom) * 0.5);
        assert!(matches!(
            execution
                .scene_mut()
                .set_camera_orientation(&camera, 1.2, -0.7, 0.3),
            Err(AuthoringError::ExecutionPublication(
                crate::ExecutionSessionPublicationError::SegmentCompletionPending
            ))
        ));
        assert_eq!(
            execution.scene().effective_camera_profile(&camera).unwrap(),
            before
        );
        execution
            .live()
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        execution.live().complete_segment(segment).unwrap();
        let before = execution.scene().effective_camera_profile(&camera).unwrap();
        assert_eq!(before.0, target);
        execution
            .scene_mut()
            .set_camera_orientation(&camera, 1.2, -0.7, 0.3)
            .unwrap();
        let after = execution.scene().effective_camera_profile(&camera).unwrap();

        assert_eq!(after.0.zoom, before.0.zoom);
        assert_eq!(after.0.focal_distance, before.0.focal_distance);
        assert_eq!(after.0.frame_height, before.0.frame_height);
        assert_eq!(after.0.frame_center, before.0.frame_center);
        assert_eq!((after.1, after.2), (before.1, before.2));
    }
}
