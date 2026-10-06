//! Canonical-scene bindings for spatial objects and world-space edits.

use super::{CanonicalAuthoringSceneContext, WasmDeclaredAnimationHandle};
use crate::{
    authoring_error::js_error, authoring_spatial::about, authoring_spatial::vec3,
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmMeshFamilyOptions, WasmMeshOptions,
};
use noon::{SemanticCamera3D, SemanticProjection3D, WorldAffineEdit};
use noon_core::{Color, SemanticRotation3D, SemanticVec3};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn invalid(message: impl ToString) -> JsValue {
    js_error(crate::authoring_error::AuthoringFailure::new(
        "input",
        "spatial.invalid_input",
        message,
    ))
}

fn world_values(values: &[f64]) -> Result<noon::SemanticWorldTransform3D, JsValue> {
    crate::authoring_spatial::world_transform_from_values(values)
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = setSurfaceCheckerboard)]
    pub fn set_surface_checkerboard(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        first: &[f64],
        second: &[f64],
        opacity: f64,
    ) -> Result<(), JsValue> {
        if first.len() != 4 || second.len() != 4 {
            return Err(invalid(
                "surface checkerboard colors require RGBA quadruples",
            ));
        }
        let surface = family
            .semantic_surface_family()?
            .ok_or_else(|| invalid("checkerboard fills require a Surface family"))?;
        let colors = [
            crate::authoring_spatial::color(first[0], first[1], first[2], first[3])?,
            crate::authoring_spatial::color(second[0], second[1], second[2], second[3])?,
        ];
        self.inner
            .set_surface_checkerboard(surface, colors, opacity)
            .map_err(js_error)
    }

    /// Create a canonical indexed mesh through the cold Scene or active LiveSession owner.
    #[wasm_bindgen(js_name = createMesh)]
    pub fn create_mesh(
        &mut self,
        candidate: WasmMeshOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.inner
            .create_mesh(candidate.options)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = createMeshFamily)]
    pub fn create_mesh_family(
        &mut self,
        candidate: WasmMeshFamilyOptions,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let surface = candidate.has_surface_roles();
        let (meshes, paths) = candidate.into_parts()?;
        let family = self
            .inner
            .create_mesh_family(meshes, paths)
            .map_err(js_error)?;
        if surface {
            noon::SurfaceFamily::from_family(family)
                .map(WasmAuthoringFamilyHandle::from_surface_family)
                .map_err(js_error)
        } else {
            Ok(WasmAuthoringFamilyHandle::from_semantic_family(family))
        }
    }

    /// Create and bind the one camera declaration before execution begins.
    #[wasm_bindgen(js_name = createCamera3D)]
    pub fn create_camera_3d(
        &mut self,
        wrapper_id: &str,
        position: &[f64],
        rotation: &[f64],
        perspective_fov: f64,
        near: f64,
        far: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let id = super::parse_object_id("camera object ID", wrapper_id)?;
        let position = vec3(position, "camera position")?;
        if rotation.len() != 4 {
            return Err(invalid("camera rotation requires quaternion [w, x, y, z]"));
        }
        let orientation =
            SemanticRotation3D::from_components(rotation[0], rotation[1], rotation[2], rotation[3])
                .ok_or_else(|| invalid("camera rotation must be a finite nonzero quaternion"))?;
        let camera = SemanticCamera3D::new(
            position,
            orientation,
            SemanticProjection3D::Perspective {
                vertical_fov_radians: perspective_fov,
                near,
                far,
            },
        )
        .ok_or_else(|| invalid("camera projection or pose is invalid"))?;
        self.inner
            .create_camera_3d(id, camera)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    /// Detached light; the ordinary membership API attaches it to the scene.
    #[wasm_bindgen(js_name = createPointLight3D)]
    pub fn create_point_light_3d(
        &mut self,
        position: &[f64],
        rgba: &[f64],
        intensity: f64,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        if rgba.len() != 4 {
            return Err(invalid("point light color requires RGBA components"));
        }
        let channels = [rgba[0], rgba[1], rgba[2], rgba[3]];
        if channels
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(invalid("point light color components must be in [0, 1]"));
        }
        let color = Color::rgba(
            rgba[0] as f32,
            rgba[1] as f32,
            rgba[2] as f32,
            rgba[3] as f32,
        );
        self.inner
            .create_point_light_3d(vec3(position, "light position")?, color, intensity)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = effectiveWorldTransform)]
    pub fn effective_world_transform(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
    ) -> Result<Vec<f64>, JsValue> {
        self.inner
            .effective_world_transform(object.semantic_mobject())
            .map(crate::authoring_spatial::transform_values)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = effectiveWorldCenter)]
    pub fn effective_world_center(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
    ) -> Result<Vec<f64>, JsValue> {
        self.inner
            .effective_world_center(object.semantic_mobject())
            .map(|center| vec![center.x, center.y, center.z])
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = effectiveWorldFamilyCenter)]
    pub fn effective_world_family_center(
        &mut self,
        family: &crate::WasmAuthoringFamilyHandle,
    ) -> Result<Vec<f64>, JsValue> {
        self.inner
            .effective_world_family_center(family.semantic_family_ref())
            .map(|center| vec![center.x, center.y, center.z])
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setWorldTransform)]
    pub fn set_world_transform(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        values: &[f64],
    ) -> Result<(), JsValue> {
        self.inner
            .set_world_transform(
                object.semantic_mobject(),
                crate::authoring_spatial::world_transform_from_values(values)?,
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = shiftWorld)]
    pub fn shift_world(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<(), JsValue> {
        self.inner
            .world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Shift(SemanticVec3::new(x, y, z)),
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = rotateWorld)]
    pub fn rotate_world(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        radians: f64,
        pivot: &[f64],
    ) -> Result<(), JsValue> {
        self.inner
            .world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Rotate {
                    axis: SemanticVec3::new(axis_x, axis_y, axis_z),
                    radians,
                    about: about(pivot)?,
                },
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = scaleWorld)]
    pub fn scale_world(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        factor: f64,
        pivot: &[f64],
    ) -> Result<(), JsValue> {
        self.inner
            .world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Scale {
                    factor,
                    about: about(pivot)?,
                },
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = shiftFamilyWorld)]
    pub fn shift_family_world(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<(), JsValue> {
        let family = family.semantic_family()?;
        self.inner
            .world_affine(
                (&family).into(),
                WorldAffineEdit::Shift(SemanticVec3::new(x, y, z)),
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = rotateFamilyWorld)]
    pub fn rotate_family_world(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        radians: f64,
        pivot: &[f64],
    ) -> Result<(), JsValue> {
        let family = family.semantic_family()?;
        self.inner
            .world_affine(
                (&family).into(),
                WorldAffineEdit::Rotate {
                    axis: SemanticVec3::new(axis_x, axis_y, axis_z),
                    radians,
                    about: about(pivot)?,
                },
            )
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = scaleFamilyWorld)]
    pub fn scale_family_world(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        factor: f64,
        pivot: &[f64],
    ) -> Result<(), JsValue> {
        let family = family.semantic_family()?;
        self.inner
            .world_affine(
                (&family).into(),
                WorldAffineEdit::Scale {
                    factor,
                    about: about(pivot)?,
                },
            )
            .map_err(js_error)
    }

    /// Declare an activatable shared world-transform intent before execution.
    #[wasm_bindgen(js_name = declareWorldTransform)]
    pub fn declare_world_transform(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        values: &[f64],
        run_time: f64,
        rate_function: &str,
    ) -> Result<WasmDeclaredAnimationHandle, JsValue> {
        let node = object.id_in_store(self.inner.scene.integration_store(), "world animation")?;
        let is_bound = self
            .inner
            .identities
            .get(&node)
            .is_some_and(|wrapper_id| self.inner.bindings.get(wrapper_id) == Some(&node));
        if !is_bound {
            return Err(invalid(
                "world-transform target is not bound to this canonical scene",
            ));
        }
        let rate = noon_core::RateFunction::from_semantic_id(rate_function)
            .ok_or_else(|| invalid(format!("unsupported rate function {rate_function:?}")))?;
        let declaration = self
            .inner
            .declare_world_transform(
                object.semantic_mobject(),
                world_values(values)?,
                noon_core::AnimationOptions::new()
                    .run_time(run_time)
                    .rate_func(rate),
            )
            .map_err(js_error)?;
        Ok(WasmDeclaredAnimationHandle {
            declaration,
            store: Rc::clone(self.inner.scene.integration_store()),
        })
    }
}
