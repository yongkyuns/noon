//! WASM facade for typed Manim finite-perspective camera profiles.

use super::{CanonicalAuthoringSceneContext, WasmAnimationCompositionBuilder};
use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle};
use noon_core::{ManimCamera3DProfile, SemanticVec3};
use wasm_bindgen::prelude::*;

fn invalid(message: impl ToString) -> JsValue {
    js_error(crate::authoring_error::AuthoringFailure::new(
        "input",
        "camera.profile",
        message,
    ))
}

fn profile(values: &[f64]) -> Result<ManimCamera3DProfile, JsValue> {
    if values.len() != 9 {
        return Err(invalid("camera profile requires [phi, theta, gamma, focalDistance, zoom, frameHeight, centerX, centerY, centerZ]"));
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid("camera profile values must be finite"));
    }
    Ok(ManimCamera3DProfile {
        phi: values[0],
        theta: values[1],
        gamma: values[2],
        focal_distance: values[3],
        zoom: values[4],
        frame_height: values[5],
        frame_center: SemanticVec3::new(values[6], values[7], values[8]),
    })
}

fn profile_values(profile: ManimCamera3DProfile) -> Vec<f64> {
    vec![
        profile.phi,
        profile.theta,
        profile.gamma,
        profile.focal_distance,
        profile.zoom,
        profile.frame_height,
        profile.frame_center.x,
        profile.frame_center.y,
        profile.frame_center.z,
    ]
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = createCamera3DProfile)]
    pub fn create_camera_3d_profile(
        &mut self,
        wrapper_id: &str,
        values: &[f64],
        near: f64,
        far: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let id = super::parse_object_id("camera object ID", wrapper_id)?;
        self.inner
            .create_camera_profile(id, profile(values)?, near, far)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setCameraOrientation)]
    pub fn set_camera_orientation(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        phi: f64,
        theta: f64,
        gamma: f64,
    ) -> Result<(), JsValue> {
        self.inner
            .set_camera_orientation(camera.semantic_mobject(), phi, theta, gamma)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = effectiveCameraProfile)]
    pub fn effective_camera_profile(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
    ) -> Result<Vec<f64>, JsValue> {
        let (profile, _, _) = self
            .inner
            .effective_camera_profile(camera.semantic_mobject())
            .map_err(js_error)?;
        Ok(profile_values(profile))
    }

    #[wasm_bindgen(js_name = setCameraProfile)]
    pub fn set_camera_profile(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        values: &[f64],
    ) -> Result<(), JsValue> {
        self.inner
            .set_camera_profile(camera.semantic_mobject(), profile(values)?)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = beginAmbientCameraRotation)]
    pub fn begin_ambient_camera_rotation(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        rate: f64,
        axis: &str,
    ) -> Result<(), JsValue> {
        let axis = match axis {
            "phi" => noon_core::CameraRotationAxis::Phi,
            "theta" => noon_core::CameraRotationAxis::Theta,
            "gamma" => noon_core::CameraRotationAxis::Gamma,
            _ => return Err(invalid("ambient camera axis must be phi, theta, or gamma")),
        };
        self.inner
            .begin_ambient_camera_rotation(camera.semantic_mobject(), axis, rate)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = stopAmbientCameraRotation)]
    pub fn stop_ambient_camera_rotation(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .stop_ambient_camera_rotation(camera.semantic_mobject())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = declareCameraProfileMove)]
    pub fn declare_camera_profile_move(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        values: &[f64],
        run_time: f64,
        rate_function: &str,
    ) -> Result<super::WasmDeclaredAnimationHandle, JsValue> {
        let node = camera.id_in_store(
            self.inner.scene.integration_store(),
            "camera profile animation",
        )?;
        if !self
            .inner
            .identities
            .get(&node)
            .is_some_and(|wrapper_id| self.inner.bindings.get(wrapper_id) == Some(&node))
        {
            return Err(invalid(
                "camera profile target is not bound to this canonical scene",
            ));
        }
        let rate = noon_core::RateFunction::from_semantic_id(rate_function)
            .ok_or_else(|| invalid(format!("unsupported rate function {rate_function:?}")))?;
        let declaration = self
            .inner
            .declare_camera_profile_move(
                camera.semantic_mobject(),
                profile(values)?,
                noon_core::AnimationOptions::new()
                    .run_time(run_time)
                    .rate_func(rate),
            )
            .map_err(js_error)?;
        Ok(super::WasmDeclaredAnimationHandle {
            declaration,
            store: std::rc::Rc::clone(self.inner.scene.integration_store()),
        })
    }

    #[wasm_bindgen(js_name = moveCameraProfile)]
    pub fn move_camera_profile(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        values: &[f64],
        run_time: f64,
        rate_function: &str,
    ) -> Result<f64, JsValue> {
        let node = camera.id_in_store(
            self.inner.scene.integration_store(),
            "camera profile animation",
        )?;
        if !self
            .inner
            .identities
            .get(&node)
            .is_some_and(|wrapper_id| self.inner.bindings.get(wrapper_id) == Some(&node))
        {
            return Err(invalid(
                "camera profile target is not bound to this canonical scene",
            ));
        }
        let rate = noon_core::RateFunction::from_semantic_id(rate_function)
            .ok_or_else(|| invalid(format!("unsupported rate function {rate_function:?}")))?;
        self.inner
            .move_camera_profile(
                camera.semantic_mobject(),
                profile(values)?,
                noon_core::AnimationOptions::new()
                    .run_time(run_time)
                    .rate_func(rate),
            )
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAnimationCompositionBuilder {
    #[wasm_bindgen(js_name = appendCameraProfile)]
    pub fn append_camera_profile(
        &mut self,
        camera: &WasmAuthoringMobjectHandle,
        values: &[f64],
        run_time: f64,
        rate_function: &str,
    ) -> Result<(), JsValue> {
        let endpoint = profile(values)?;
        let rate = noon_core::RateFunction::from_semantic_id(rate_function)
            .ok_or_else(|| invalid(format!("unsupported rate function {rate_function:?}")))?;
        self.children
            .push(super::super::OrdinaryCompositionChild::CameraProfile {
                target: camera.semantic_mobject().clone(),
                profile: endpoint,
                options: noon_core::AnimationOptions::new()
                    .run_time(run_time)
                    .rate_func(rate),
            });
        Ok(())
    }
}
