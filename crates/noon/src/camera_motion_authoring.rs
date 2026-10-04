//! Scene-owned authored-time ambient camera rotation.

use std::sync::Arc;

use noon_core::{CameraAngularMotion, CameraRotationAxis, SemanticObjectState};

use crate::{AuthoringError, Mobject, Scene};

fn state_without_open_motion(
    state: &SemanticObjectState,
) -> Result<Vec<CameraAngularMotion>, AuthoringError> {
    let motions = state.camera_motions();
    if motions.last().is_some_and(|motion| motion.end().is_none()) {
        return Err(AuthoringError::CameraMotionAlreadyActive);
    }
    if motions.len() >= noon_core::MAX_CAMERA_MOTION_INTERVALS {
        return Err(AuthoringError::InvalidCameraMotionInput(
            "camera motion history is limited to 256 intervals per camera",
        ));
    }
    Ok(motions.to_vec())
}

type StoppedCameraMotion = (
    Arc<[CameraAngularMotion]>,
    noon_core::ManimCamera3DProfile,
    f64,
    f64,
);

pub(crate) fn ensure_camera_motion_closed(camera: &Mobject) -> Result<(), AuthoringError> {
    if camera
        .state()?
        .camera_motions()
        .last()
        .is_some_and(|motion| motion.end().is_none())
    {
        Err(AuthoringError::CameraMotionAlreadyActive)
    } else {
        Ok(())
    }
}

pub(crate) fn begin_motion(
    state: &SemanticObjectState,
    profile: noon_core::ManimCamera3DProfile,
    near: f64,
    far: f64,
    axis: CameraRotationAxis,
    rate: f64,
    start: f64,
) -> Result<Arc<[CameraAngularMotion]>, AuthoringError> {
    if !rate.is_finite() {
        return Err(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation rate must be finite",
        ));
    }
    let mut motions = state_without_open_motion(state)?;
    let motion = CameraAngularMotion::new(profile, axis, rate, start, None, near, far).ok_or(
        AuthoringError::InvalidCameraMotionInput("ambient camera rotation inputs are invalid"),
    )?;
    motions.push(motion);
    noon_core::camera_motion_history_is_valid(&motions)
        .then(|| Arc::from(motions))
        .ok_or(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation history is invalid",
        ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn begin_illusion_motion(
    state: &SemanticObjectState,
    profile: noon_core::ManimCamera3DProfile,
    near: f64,
    far: f64,
    rate: f64,
    start: f64,
    origin_phi: Option<f64>,
    origin_theta: Option<f64>,
) -> Result<Arc<[CameraAngularMotion]>, AuthoringError> {
    if !rate.is_finite() {
        return Err(AuthoringError::InvalidCameraMotionInput(
            "3D illusion camera rotation rate must be finite",
        ));
    }
    let mut motions = state_without_open_motion(state)?;
    let motion = CameraAngularMotion::three_d_illusion(
        profile,
        rate,
        start,
        None,
        near,
        far,
        origin_phi,
        origin_theta,
    )
    .ok_or(AuthoringError::InvalidCameraMotionInput(
        "3D illusion camera rotation inputs are invalid",
    ))?;
    motions.push(motion);
    noon_core::camera_motion_history_is_valid(&motions)
        .then(|| Arc::from(motions))
        .ok_or(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation history is invalid",
        ))
}

pub(crate) fn stop_motion(
    state: &SemanticObjectState,
    time: f64,
) -> Result<Option<StoppedCameraMotion>, AuthoringError> {
    let Some(last) = state.camera_motions().last().copied() else {
        return Ok(None);
    };
    if last.end().is_some() {
        return Ok(None);
    }
    let closed = last
        .close(time)
        .ok_or(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation stop time is invalid",
        ))?;
    let endpoint = closed
        .sample(time)
        .ok_or(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation endpoint is not representable",
        ))?;
    let (near, far) = closed.clips();
    let mut motions = state.camera_motions().to_vec();
    *motions.last_mut().expect("open motion was observed") = closed;
    if !noon_core::camera_motion_history_is_valid(&motions) {
        return Err(AuthoringError::InvalidCameraMotionInput(
            "ambient camera rotation history is invalid",
        ));
    }
    Ok(Some((Arc::from(motions), endpoint, near, far)))
}

fn scene_profile(
    scene: &Scene,
    camera: &Mobject,
) -> Result<(noon_core::ManimCamera3DProfile, f64, f64, f64), AuthoringError> {
    if let Some(execution) = scene.running_execution() {
        let (profile, near, far) = execution
            .effective_camera_profile(camera.node_id())
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        Ok((profile, near, far, execution.effective_time()))
    } else {
        let state = camera.state()?;
        let profile = state
            .camera_profile()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        let (near, far) = match state.camera_projection() {
            Some(noon_core::SemanticProjection3D::Perspective { near, far, .. }) => (near, far),
            _ => return Err(AuthoringError::NonFiniteObjectState),
        };
        Ok((profile, near, far, scene.time()))
    }
}

impl Scene {
    /// Begin one authored-time unwrapped ambient angular interval.
    pub fn begin_ambient_camera_rotation(
        &mut self,
        camera: &Mobject,
        axis: CameraRotationAxis,
        rate: f64,
    ) -> Result<(), AuthoringError> {
        self.require_object(camera)?;
        let (profile, near, far, time) = scene_profile(self, camera)?;
        let state = camera.state()?;
        let motions = begin_motion(&state, profile, near, far, axis, rate, time)?;
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Close the active ambient interval and atomically author its sampled profile endpoint.
    /// Calling stop without an open interval is idempotent.
    pub fn stop_ambient_camera_rotation(&mut self, camera: &Mobject) -> Result<(), AuthoringError> {
        self.require_object(camera)?;
        let time = self
            .running_execution()
            .map_or_else(|| self.time(), |execution| execution.effective_time());
        let state = camera.state()?;
        let Some((motions, profile, near, far)) = stop_motion(&state, time)? else {
            return Ok(());
        };
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        transaction.set_camera_profile(camera.node_id(), profile, near, far);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Begin Manim's authored-time camera-orbit illusion around a captured pose.
    pub fn begin_3dillusion_camera_rotation(
        &mut self,
        camera: &Mobject,
        rate: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.require_object(camera)?;
        let (profile, near, far, time) = scene_profile(self, camera)?;
        let state = camera.state()?;
        let motions = begin_illusion_motion(
            &state,
            profile,
            near,
            far,
            rate,
            time,
            origin_phi,
            origin_theta,
        )?;
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Stop the illusion at the exact sampled effective endpoint.
    pub fn stop_3dillusion_camera_rotation(
        &mut self,
        camera: &Mobject,
    ) -> Result<(), AuthoringError> {
        self.stop_ambient_camera_rotation(camera)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> noon_core::ManimCamera3DProfile {
        noon_core::ManimCamera3DProfile {
            phi: 0.7,
            theta: -1.2,
            gamma: 0.1,
            focal_distance: 5.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: noon_core::SemanticVec3::ZERO,
        }
    }

    #[test]
    fn ambient_camera_rotation_closes_with_one_exact_authored_endpoint() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        scene
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Theta, 0.25)
            .unwrap();
        assert_eq!(camera.state().unwrap().camera_motions().len(), 1);
        assert!(scene
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Gamma, 0.1)
            .is_err());
        assert!(scene
            .set_camera_orientation(&camera, 1.0, 0.0, 0.0)
            .is_err());

        scene.wait(2.0).unwrap();
        scene.stop_ambient_camera_rotation(&camera).unwrap();
        let state = camera.state().unwrap();
        let motion = state.camera_motions()[0];
        assert_eq!(motion.end(), Some(2.0));
        let mut expected = profile();
        expected.theta += 0.5;
        assert_eq!(state.camera_profile(), Some(expected));
        assert_eq!(
            state.transform.translation,
            expected.camera(0.1, 100.0).unwrap().position
        );
        scene.stop_ambient_camera_rotation(&camera).unwrap();
        assert_eq!(camera.state().unwrap().camera_motions()[0].end(), Some(2.0));
    }

    #[test]
    fn illusion_camera_rotation_is_authored_time_owned_and_stops_at_its_sample() {
        let source = noon_core::ManimCamera3DProfile {
            phi: 75_f64.to_radians(),
            theta: 30_f64.to_radians(),
            ..profile()
        };
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(source, 0.1, 100.0).unwrap();
        scene
            .begin_3dillusion_camera_rotation(&camera, 2.0, None, None)
            .unwrap();
        assert!(scene
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Theta, 1.0)
            .is_err());

        scene.wait(std::f64::consts::FRAC_PI_2).unwrap();
        let expected = noon_core::CameraAngularMotion::three_d_illusion(
            source,
            2.0,
            0.0,
            Some(std::f64::consts::FRAC_PI_2),
            0.1,
            100.0,
            None,
            None,
        )
        .unwrap()
        .sample(std::f64::consts::FRAC_PI_2)
        .unwrap();
        scene.stop_3dillusion_camera_rotation(&camera).unwrap();

        let state = camera.state().unwrap();
        assert_eq!(
            state.camera_motions()[0].end(),
            Some(std::f64::consts::FRAC_PI_2)
        );
        assert_eq!(state.camera_profile(), Some(expected));
        assert!(scene
            .begin_3dillusion_camera_rotation(&camera, 1.0, Some(f64::NAN), None)
            .is_err());
        assert_eq!(camera.state().unwrap().camera_profile(), Some(expected));
    }

    #[test]
    fn invalid_ambient_inputs_do_not_publish_or_open_driver_state() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();
        let before = scene.integration_store().borrow().scene_revision();
        assert!(scene
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Theta, f64::NAN)
            .is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
        assert!(camera.state().unwrap().camera_motions().is_empty());
    }

    #[test]
    fn camera_motion_history_limit_rejects_the_next_interval_atomically() {
        let mut scene = Scene::new();
        let camera = scene.camera_3d_profile(profile(), 0.1, 100.0).unwrap();

        for _ in 0..noon_core::MAX_CAMERA_MOTION_INTERVALS {
            scene
                .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Theta, 0.25)
                .unwrap();
            scene.wait(0.01).unwrap();
            scene.stop_ambient_camera_rotation(&camera).unwrap();
        }

        let before = camera.state().unwrap();
        assert_eq!(
            before.camera_motions().len(),
            noon_core::MAX_CAMERA_MOTION_INTERVALS
        );
        assert!(before
            .camera_motions()
            .iter()
            .all(|motion| motion.end().is_some()));
        let revision = scene.integration_store().borrow().scene_revision();
        let authored_profile = before.camera_profile();
        let authored_pose = before.transform;
        let authored_projection = before.camera_projection();

        // Stopping again after the final closed interval is a no-op, even at the cap.
        scene.stop_ambient_camera_rotation(&camera).unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );

        let error = scene
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Gamma, 0.1)
            .unwrap_err();
        assert!(matches!(
            error,
            AuthoringError::InvalidCameraMotionInput(
                "camera motion history is limited to 256 intervals per camera"
            )
        ));

        let after = camera.state().unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(after.camera_motions(), before.camera_motions());
        assert_eq!(after.camera_profile(), authored_profile);
        assert_eq!(after.transform, authored_pose);
        assert_eq!(after.camera_projection(), authored_projection);
    }
}
