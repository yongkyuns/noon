//! Typed spatial operations exposed by the existing execution player wrapper.

use crate::authoring_error::AuthoringFailure;
use noon::{MeshOptions, Mobject, MobjectFamily, SemanticWorldTransform3D, WorldAffineEdit};

impl super::SemanticExecutionPlayer {
    pub(crate) fn live_create_mesh(
        &mut self,
        options: MeshOptions,
    ) -> Result<Mobject, AuthoringFailure> {
        self.with_live_session(|live| live.create_mesh(options))
    }

    pub(crate) fn live_create_mesh_family(
        &mut self,
        options: Vec<MeshOptions>,
    ) -> Result<MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| live.create_mesh_family(options))
    }

    pub(crate) fn live_effective_world_transform(
        &mut self,
        object: &Mobject,
    ) -> Result<SemanticWorldTransform3D, AuthoringFailure> {
        self.with_live_session(|live| live.effective_world_transform(object))
    }

    pub(crate) fn live_set_world_transform(
        &mut self,
        object: &Mobject,
        transform: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_world_transform(object, transform))
            .map(|_| ())
    }

    pub(crate) fn live_world_affine_object(
        &mut self,
        object: &Mobject,
        edit: WorldAffineEdit,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.world_affine(object.into(), edit))
            .map(|_| ())
    }

    pub(crate) fn live_world_affine_family(
        &mut self,
        family: &MobjectFamily,
        edit: WorldAffineEdit,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.world_affine(family.into(), edit))
            .map(|_| ())
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::super::SemanticExecutionPlayer;
    use super::*;
    use crate::authoring_error::js_error;
    use crate::authoring_spatial::about as parse_about;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    impl SemanticExecutionPlayer {
        #[wasm_bindgen(js_name = createMesh)]
        pub fn create_mesh(
            &mut self,
            candidate: crate::WasmMeshOptions,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            self.live_create_mesh(candidate.options)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createMeshFamily)]
        pub fn create_mesh_family(
            &mut self,
            candidate: crate::WasmMeshFamilyOptions,
        ) -> Result<crate::WasmAuthoringFamilyHandle, JsValue> {
            self.live_create_mesh_family(candidate.options)
                .map(crate::WasmAuthoringFamilyHandle::from_semantic_family)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = effectiveWorldTransform)]
        pub fn effective_world_transform(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.live_effective_world_transform(object.semantic_mobject())
                .map(crate::authoring_spatial::transform_values)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setWorldTransform)]
        pub fn set_world_transform(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            values: &[f64],
        ) -> Result<(), JsValue> {
            let transform = crate::authoring_spatial::world_transform_from_values(values)?;
            self.live_set_world_transform(object.semantic_mobject(), transform)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = shiftWorld)]
        pub fn shift_world(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
            z: f64,
        ) -> Result<(), JsValue> {
            self.live_world_affine_object(
                object.semantic_mobject(),
                WorldAffineEdit::Shift(noon_core::SemanticVec3::new(x, y, z)),
            )
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
            self.live_world_affine_object(
                object.semantic_mobject(),
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
            self.live_world_affine_object(
                object.semantic_mobject(),
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
            self.live_world_affine_family(
                &family,
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
            self.live_world_affine_family(
                &family,
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
            self.live_world_affine_family(
                &family,
                WorldAffineEdit::Scale {
                    factor,
                    about: crate::authoring_spatial::about(about)?,
                },
            )
            .map_err(js_error)
        }
    }
}
