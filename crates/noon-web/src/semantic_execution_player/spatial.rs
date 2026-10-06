//! Typed spatial operations exposed by the existing execution player wrapper.

use crate::authoring_error::AuthoringFailure;
#[cfg(any(target_arch = "wasm32", test))]
use noon::MobjectFamily;
use noon::{MeshOptions, Mobject, SemanticWorldTransform3D, WorldAffineEdit};

impl super::SemanticExecutionPlayer {
    pub(crate) fn live_set_surface_checkerboard(
        &mut self,
        surface: &noon::SurfaceFamily,
        colors: [noon::Color; 2],
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_surface_checkerboard(surface, colors, opacity))
            .map(|_| ())
    }

    pub(crate) fn live_add_spatial_membership(
        &mut self,
        targets: &[noon::MobjectTarget<'_>],
        policy: crate::canonical_authoring_scene::SpatialMembershipPolicy,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(match policy {
                crate::canonical_authoring_scene::SpatialMembershipPolicy::Assign(domain) => {
                    live.add_all_in_spatial_composition_domain(targets, domain)
                }
                crate::canonical_authoring_scene::SpatialMembershipPolicy::DefaultWorld => {
                    live.add_all_world_mobjects(targets)
                }
            })
        })?
        .map(|_| ())
        .map_err(AuthoringFailure::from)
    }

    pub(crate) fn live_create_mesh(
        &mut self,
        options: MeshOptions,
    ) -> Result<Mobject, AuthoringFailure> {
        self.with_live_session(|live| live.create_mesh(options))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_manim_arrow(
        &mut self,
        options: noon::ManimArrowOptions,
    ) -> Result<noon::ManimArrow, AuthoringFailure> {
        self.with_live_session(|live| live.create_manim_arrow(options))
    }

    #[cfg(target_arch = "wasm32")]
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

    pub(crate) fn live_effective_world_center(
        &mut self,
        object: &Mobject,
    ) -> Result<noon_core::SemanticVec3, AuthoringFailure> {
        self.with_live_session(|live| live.effective_world_center(object))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_world_family_center(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<noon_core::SemanticVec3, AuthoringFailure> {
        self.with_live_session(|live| live.effective_world_family_center(family))
    }

    pub(crate) fn live_set_world_transform(
        &mut self,
        object: &Mobject,
        transform: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_world_transform(object, transform))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_world_affine_object(
        &mut self,
        object: &Mobject,
        edit: WorldAffineEdit,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.world_affine(object.into(), edit))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
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

        #[wasm_bindgen(js_name = effectiveWorldCenter)]
        pub fn effective_world_center(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.live_effective_world_center(object.semantic_mobject())
                .map(|center| vec![center.x, center.y, center.z])
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

        #[wasm_bindgen(js_name = effectiveWorldFamilyCenter)]
        pub fn effective_world_family_center(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<Vec<f64>, JsValue> {
            self.live_effective_world_family_center(family.semantic_family_ref())
                .map(|center| vec![center.x, center.y, center.z])
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
