//! Player forwarding for shared semantic camera-profile operations.

use crate::authoring_error::AuthoringFailure;

impl super::SemanticExecutionPlayer {
    pub(crate) fn live_effective_camera_profile(
        &mut self,
        camera: &noon::Mobject,
    ) -> Result<(noon_core::ManimCamera3DProfile, f64, f64), AuthoringFailure> {
        self.with_live_session(|live| live.effective_camera_profile(camera))
    }

    pub(crate) fn live_set_camera_profile(
        &mut self,
        camera: &noon::Mobject,
        profile: noon_core::ManimCamera3DProfile,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_camera_profile(camera, profile))
    }

    pub(crate) fn live_begin_ambient_camera_rotation(
        &mut self,
        camera: &noon::Mobject,
        axis: noon_core::CameraRotationAxis,
        rate: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.begin_ambient_camera_rotation(camera, axis, rate))
    }

    pub(crate) fn live_stop_ambient_camera_rotation(
        &mut self,
        camera: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.stop_ambient_camera_rotation(camera))
    }

    pub(crate) fn live_begin_3dillusion_camera_rotation(
        &mut self,
        camera: &noon::Mobject,
        rate: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.begin_3dillusion_camera_rotation(camera, rate, origin_phi, origin_theta)
        })
    }

    pub(crate) fn live_stop_3dillusion_camera_rotation(
        &mut self,
        camera: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.stop_3dillusion_camera_rotation(camera))
    }

    pub(crate) fn live_set_camera_orientation(
        &mut self,
        camera: &noon::Mobject,
        phi: f64,
        theta: f64,
        gamma: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_camera_orientation(camera, phi, theta, gamma))
    }
}
