//! Authored-time camera-motion lifecycle on the existing LiveSession publication path.

use super::*;
use crate::camera_motion_authoring::{begin_illusion_motion, begin_motion, stop_motion};
use crate::AuthoringError;
use noon_core::{CameraRotationAxis, SemanticMutationTransaction};

impl LiveSession<'_> {
    pub fn effective_camera_profile(
        &self,
        camera: &Mobject,
    ) -> Result<(noon_core::ManimCamera3DProfile, f64, f64), LiveSessionError> {
        self.require_mobject(camera)?;
        self.session
            .effective_camera_profile(camera.node_id())
            .ok_or(AuthoringError::NonFiniteObjectState.into())
    }

    pub fn begin_ambient_camera_rotation(
        &mut self,
        camera: &Mobject,
        axis: CameraRotationAxis,
        rate: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(camera)?;
        let time = self.session.effective_time();
        let (profile, near, far) = self
            .session
            .effective_camera_profile(camera.node_id())
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        let state = camera.state()?;
        let motions = begin_motion(&state, profile, near, far, axis, rate, time)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        self.apply(transaction).map(|_| ())
    }

    /// Close the active interval and publish its exact sampled endpoint in one transaction.
    /// Calling stop with no open interval is idempotent.
    pub fn stop_ambient_camera_rotation(
        &mut self,
        camera: &Mobject,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(camera)?;
        let state = camera.state()?;
        let Some((motions, profile, near, far)) =
            stop_motion(&state, self.session.effective_time())?
        else {
            return Ok(());
        };
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        transaction.set_camera_profile(camera.node_id(), profile, near, far);
        self.apply(transaction).map(|_| ())
    }

    pub fn begin_3dillusion_camera_rotation(
        &mut self,
        camera: &Mobject,
        rate: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(camera)?;
        let time = self.session.effective_time();
        let (profile, near, far) = self
            .session
            .effective_camera_profile(camera.node_id())
            .ok_or(AuthoringError::NonFiniteObjectState)?;
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
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_camera_motions(camera.node_id(), motions);
        self.apply(transaction).map(|_| ())
    }

    pub fn stop_3dillusion_camera_rotation(
        &mut self,
        camera: &Mobject,
    ) -> Result<(), LiveSessionError> {
        self.stop_ambient_camera_rotation(camera)
    }
}
