//! Typed spatial operations exposed by the existing execution player wrapper.

use crate::authoring_error::AuthoringFailure;

impl super::SemanticExecutionPlayer {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn world_affine(
        &mut self,
        target: noon::MobjectTarget<'_>,
        edit: noon::WorldAffineEdit,
    ) -> Result<(), AuthoringFailure> {
        self.scene_mut()
            .world_affine(target, edit)
            .map_err(|error| AuthoringFailure::from_scene_mutation(error, true))
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::super::SemanticExecutionPlayer;
    use super::*;
    use crate::authoring_error::js_error;
    use crate::authoring_spatial::about as parse_about;
    use noon::WorldAffineEdit;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    impl SemanticExecutionPlayer {
        #[wasm_bindgen(js_name = createMesh)]
        pub fn create_mesh(
            &mut self,
            candidate: crate::WasmMeshOptions,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            self.scene_mut()
                .mesh(candidate.value.into_options())
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(noon::LiveSessionError::from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createMeshFamily)]
        pub fn create_mesh_family(
            &mut self,
            candidate: crate::WasmMeshFamilyOptions,
        ) -> Result<crate::WasmAuthoringFamilyHandle, JsValue> {
            let surface = candidate.has_surface_roles();
            let (meshes, paths) = candidate.into_parts()?;
            let family = self
                .scene_mut()
                .mesh_family_with_paths(meshes, paths)
                .map_err(noon::LiveSessionError::from)
                .map_err(js_error)?;
            if surface {
                noon::SurfaceFamily::from_family(family)
                    .map(crate::WasmAuthoringFamilyHandle::from_surface_family)
                    .map_err(js_error)
            } else {
                Ok(crate::WasmAuthoringFamilyHandle::from_semantic_family(
                    family,
                ))
            }
        }

        #[wasm_bindgen(js_name = effectiveWorldTransform)]
        pub fn effective_world_transform(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.scene_mut()
                .effective_world_transform(object.semantic_mobject())
                .map(crate::authoring_spatial::transform_values)
                .map_err(noon::LiveSessionError::from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = effectiveWorldCenter)]
        pub fn effective_world_center(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.scene_mut()
                .effective_world_center(object.semantic_mobject())
                .map(|center| vec![center.x, center.y, center.z])
                .map_err(noon::LiveSessionError::from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setWorldTransform)]
        pub fn set_world_transform(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            values: &[f64],
        ) -> Result<(), JsValue> {
            let transform = crate::authoring_spatial::world_transform_from_values(values)?;
            self.scene_mut()
                .set_world_transform(object.semantic_mobject(), transform)
                .map_err(|error| js_error(AuthoringFailure::from_scene_mutation(error, true)))
        }

        #[wasm_bindgen(js_name = shiftWorld)]
        pub fn shift_world(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
            z: f64,
        ) -> Result<(), JsValue> {
            self.world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Shift(noon_core::SemanticVec3::new(x, y, z)),
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = effectiveWorldFamilyCenter)]
        pub fn effective_world_family_center(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.scene_mut()
                .effective_world_family_center(family.semantic_family_ref())
                .map(|center| vec![center.x, center.y, center.z])
                .map_err(noon::LiveSessionError::from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = rotateWorld)]
        pub fn rotate_world(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
            radians: f64,
            about: &[f64],
        ) -> Result<(), JsValue> {
            let about = parse_about(about)?;
            self.world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Rotate {
                    axis: noon_core::SemanticVec3::new(axis_x, axis_y, axis_z),
                    radians,
                    about,
                },
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = scaleWorld)]
        pub fn scale_world(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            factor: f64,
            about: &[f64],
        ) -> Result<(), JsValue> {
            self.world_affine(
                object.semantic_mobject().into(),
                WorldAffineEdit::Scale {
                    factor,
                    about: parse_about(about)?,
                },
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = shiftFamilyWorld)]
        pub fn shift_family_world(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
            x: f64,
            y: f64,
            z: f64,
        ) -> Result<(), JsValue> {
            let family = family.semantic_family()?;
            self.world_affine(
                (&family).into(),
                WorldAffineEdit::Shift(noon_core::SemanticVec3::new(x, y, z)),
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = rotateFamilyWorld)]
        pub fn rotate_family_world(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
            radians: f64,
            about: &[f64],
        ) -> Result<(), JsValue> {
            let family = family.semantic_family()?;
            self.world_affine(
                (&family).into(),
                WorldAffineEdit::Rotate {
                    axis: noon_core::SemanticVec3::new(axis_x, axis_y, axis_z),
                    radians,
                    about: parse_about(about)?,
                },
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = scaleFamilyWorld)]
        pub fn scale_family_world(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
            factor: f64,
            about: &[f64],
        ) -> Result<(), JsValue> {
            let family = family.semantic_family()?;
            self.world_affine(
                (&family).into(),
                WorldAffineEdit::Scale {
                    factor,
                    about: crate::authoring_spatial::about(about)?,
                },
            )
            .map_err(js_error)
        }
    }
}
