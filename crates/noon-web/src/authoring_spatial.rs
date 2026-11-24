//! Thin WASM adapters to the shared Rust spatial inputs and operations.
#![cfg(target_arch = "wasm32")]
use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use noon::integration::spatial_input as input;
use noon::WorldAffineEdit;
use noon_core::{SemanticVec3, SemanticWorldTransform3D};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
fn invalid(code: &'static str, message: impl ToString) -> JsValue {
    js_error(AuthoringFailure::new("input", code, message))
}
pub(crate) fn vec3(values: &[f64], field: &str) -> Result<SemanticVec3, JsValue> {
    input::vec3(values, field).map_err(js_error)
}
pub(crate) fn about(values: &[f64]) -> Result<Option<SemanticVec3>, JsValue> {
    input::about(values).map_err(js_error)
}
pub(crate) fn color(r: f64, g: f64, b: f64, a: f64) -> Result<noon::Color, JsValue> {
    input::color(r, g, b, a).map_err(js_error)
}
pub(crate) use input::transform_values;
pub(crate) fn world_transform_from_values(
    values: &[f64],
) -> Result<SemanticWorldTransform3D, JsValue> {
    input::world_transform_from_values(values).map_err(js_error)
}

#[wasm_bindgen]
pub struct WasmMeshOptions {
    pub(crate) value: input::SpatialMeshInput,
}
impl From<input::SpatialMeshInput> for WasmMeshOptions {
    fn from(value: input::SpatialMeshInput) -> Self {
        Self { value }
    }
}
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
impl WasmMeshOptions {
    pub fn sphere(
        radius: f64,
        u_cells: usize,
        v_cells: usize,
        u_start: f64,
        u_end: f64,
        v_start: f64,
        v_end: f64,
    ) -> Result<Self, JsValue> {
        input::SpatialMeshInput::sphere(radius, u_cells, v_cells, u_start, u_end, v_start, v_end)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = line3D)]
    pub fn line_3d(
        start: &[f64],
        end: &[f64],
        thickness: f64,
        segments: usize,
    ) -> Result<Self, JsValue> {
        input::SpatialMeshInput::line_3d(start, end, thickness, segments)
            .map(Into::into)
            .map_err(js_error)
    }

    pub fn polyhedron(vertices: &[f64], faces: &[u32]) -> Result<Self, JsValue> {
        input::SpatialMeshInput::polyhedron(vertices, faces)
            .map(Into::into)
            .map_err(js_error)
    }

    pub fn cube(size: f64) -> Result<Self, JsValue> {
        input::SpatialMeshInput::cube(size)
            .map(Into::into)
            .map_err(js_error)
    }

    pub fn prism(x: f64, y: f64, z: f64) -> Result<Self, JsValue> {
        input::SpatialMeshInput::prism(x, y, z)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = prismFaces)]
    pub fn prism_faces(
        x: f64,
        y: f64,
        z: f64,
        shade_in_3d: bool,
    ) -> Result<WasmMeshFamilyOptions, JsValue> {
        input::SpatialMeshInput::prism_faces(x, y, z, shade_in_3d)
            .map(Into::into)
            .map_err(js_error)
    }

    pub fn cylinder(
        radius: f64,
        height: f64,
        segments: usize,
        show_ends: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, JsValue> {
        input::SpatialMeshInput::cylinder(
            radius,
            height,
            segments,
            show_ends,
            angle_start,
            angle_end,
        )
        .map(Into::into)
        .map_err(js_error)
    }

    pub fn cone(
        radius: f64,
        height: f64,
        segments: usize,
        show_base: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, JsValue> {
        input::SpatialMeshInput::cone(radius, height, segments, show_base, angle_start, angle_end)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setAxialPose)]
    pub fn set_axial_pose(&mut self, direction: &[f64], offset: f64) -> Result<(), JsValue> {
        self.value
            .set_axial_pose(direction, offset)
            .map_err(js_error)
    }

    pub fn torus(
        major_radius: f64,
        minor_radius: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, JsValue> {
        input::SpatialMeshInput::torus(major_radius, minor_radius, u_cells, v_cells)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.value
            .set_color(red, green, blue, alpha)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
        self.value.set_opacity(opacity).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setPointLit)]
    pub fn set_point_lit(&mut self, enabled: bool) {
        self.value.set_point_lit(enabled)
    }
}

#[wasm_bindgen]
pub struct WasmMeshFamilyOptions {
    pub(crate) value: input::SpatialMeshFamilyInput,
}
impl From<input::SpatialMeshFamilyInput> for WasmMeshFamilyOptions {
    fn from(value: input::SpatialMeshFamilyInput) -> Self {
        Self { value }
    }
}
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
impl WasmMeshFamilyOptions {
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.value.len()
    }
    #[wasm_bindgen(js_name = setAxialPose)]
    pub fn set_axial_pose(&mut self, direction: &[f64], offset: f64) -> Result<(), JsValue> {
        self.value
            .set_axial_pose(direction, offset)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = addCircleCap)]
    pub fn add_circle_cap(
        &mut self,
        radius: f64,
        local_z: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        shade_in_3d: bool,
    ) -> Result<(), JsValue> {
        self.value
            .add_circle_cap(radius, local_z, red, green, blue, alpha, shade_in_3d)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setFill)]
    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.value
            .set_fill(red, green, blue, alpha, opacity)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setStroke)]
    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: f64,
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.value
            .set_stroke(red, green, blue, alpha, width, opacity)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setPointLit)]
    pub fn set_point_lit(&mut self, enabled: bool) {
        self.value.set_point_lit(enabled)
    }
    #[wasm_bindgen(js_name = setCairoSurface)]
    pub fn set_cairo_surface(&mut self, enabled: bool) -> Result<(), JsValue> {
        self.value.set_cairo_surface(enabled).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setCheckerboard)]
    pub fn set_checkerboard(
        &mut self,
        first: &[f64],
        second: &[f64],
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.value
            .set_checkerboard(first, second, opacity)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
pub struct WasmSurfaceSamplingPlan {
    pub(crate) value: input::SurfaceSamplingInput,
}
impl From<input::SurfaceSamplingInput> for WasmSurfaceSamplingPlan {
    fn from(value: input::SurfaceSamplingInput) -> Self {
        Self { value }
    }
}
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
impl WasmSurfaceSamplingPlan {
    #[wasm_bindgen(constructor)]
    pub fn new(
        u_min: f64,
        u_max: f64,
        v_min: f64,
        v_max: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, JsValue> {
        input::SurfaceSamplingInput::new(u_min, u_max, v_min, v_max, u_cells, v_cells)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = parameters)]
    pub fn parameters(&self) -> Vec<f64> {
        self.value.parameters()
    }
    #[wasm_bindgen(js_name = cairoParameters)]
    pub fn cairo_parameters(&self) -> Vec<f64> {
        self.value.cairo_parameters()
    }
    #[wasm_bindgen(js_name = finishMesh)]
    pub fn finish_mesh(&self, points: &[f64], normals: &[f64]) -> Result<WasmMeshOptions, JsValue> {
        self.value
            .finish_mesh(points, normals)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = finishCells)]
    pub fn finish_cells(
        &self,
        points: &[f64],
        normals: &[f64],
    ) -> Result<WasmMeshFamilyOptions, JsValue> {
        self.value
            .finish_cells(points, normals)
            .map(Into::into)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = finishCairoCells)]
    pub fn finish_cairo_cells(&self, points: &[f64]) -> Result<WasmMeshFamilyOptions, JsValue> {
        self.value
            .finish_cairo_cells(points)
            .map(Into::into)
            .map_err(js_error)
    }
}
impl WasmMeshFamilyOptions {
    pub(crate) fn has_surface_roles(&self) -> bool {
        self.value.has_surface_roles()
    }
    pub(crate) fn into_parts(
        self,
    ) -> Result<(Vec<noon::MeshOptions>, Vec<noon::SpatialPathOptions>), JsValue> {
        self.value.into_parts().map_err(js_error)
    }
}
#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createMesh)]
    pub fn create_mesh(
        &self,
        candidate: WasmMeshOptions,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        noon::Mobject::from_mesh(Rc::clone(&self.semantics), candidate.value.into_options())
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMeshFamily)]
    pub fn create_mesh_family(
        &self,
        candidate: WasmMeshFamilyOptions,
    ) -> Result<crate::WasmAuthoringFamilyHandle, JsValue> {
        let surface = candidate.has_surface_roles();
        let (meshes, paths) = candidate.into_parts()?;
        let family = if paths.is_empty() {
            noon::MobjectFamily::from_meshes(Rc::clone(&self.semantics), meshes)
        } else {
            noon::MobjectFamily::from_meshes_and_paths(Rc::clone(&self.semantics), meshes, paths)
        }
        .map_err(js_error)?;
        if surface {
            let surface = noon::SurfaceFamily::from_family(family).map_err(js_error)?;
            Ok(crate::WasmAuthoringFamilyHandle::from_surface_family(
                surface,
            ))
        } else {
            Ok(crate::WasmAuthoringFamilyHandle::from_semantic_family(
                family,
            ))
        }
    }
}

fn transform_array(object: &noon::Mobject) -> Result<Vec<f64>, JsValue> {
    object
        .world_transform()
        .map(transform_values)
        .map_err(js_error)
}
fn rotate(radians: f64, axis: &[f64], about_values: &[f64]) -> Result<WorldAffineEdit, JsValue> {
    Ok(WorldAffineEdit::Rotate {
        axis: vec3(axis, "axis")?,
        radians,
        about: about(about_values)?,
    })
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = worldTransform)]
    pub fn world_transform(&self) -> Result<Vec<f64>, JsValue> {
        transform_array(self.semantic_mobject())
    }
    #[wasm_bindgen(js_name = worldCenter)]
    pub fn world_center(&self) -> Result<Vec<f64>, JsValue> {
        self.semantic_mobject()
            .world_center()
            .map(|center| vec![center.x, center.y, center.z])
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = shiftWorld)]
    pub fn shift_world(&mut self, x: f64, y: f64, z: f64) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .world_affine(WorldAffineEdit::Shift(SemanticVec3::new(x, y, z)))
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = rotateWorld)]
    pub fn rotate_world(
        &mut self,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        radians: f64,
        about_values: &[f64],
    ) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .world_affine(WorldAffineEdit::Rotate {
                axis: SemanticVec3::new(axis_x, axis_y, axis_z),
                radians,
                about: about(about_values)?,
            })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = scaleWorld)]
    pub fn scale_world(&mut self, factor: f64, about_values: &[f64]) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .world_affine(WorldAffineEdit::Scale {
                factor,
                about: about(about_values)?,
            })
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = setFillByCheckerboard)]
    pub fn set_fill_by_checkerboard(
        &mut self,
        first: &[f64],
        second: &[f64],
        opacity: f64,
    ) -> Result<(), JsValue> {
        if first.len() != 4 || second.len() != 4 {
            return Err(invalid(
                "spatial.invalid_color",
                "checkerboard colors require RGBA quadruples",
            ));
        }
        let first = color(first[0], first[1], first[2], first[3])?;
        let second = color(second[0], second[1], second[2], second[3])?;
        let surface = self.semantic_surface_family()?.ok_or_else(|| {
            invalid(
                "spatial.not_a_surface",
                "checkerboard fills require a Surface family",
            )
        })?;
        surface
            .set_fill_by_checkerboard([first, second], opacity)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = shiftWorld)]
    pub fn shift_world(&mut self, x: f64, y: f64, z: f64) -> Result<(), JsValue> {
        self.semantic_family()?
            .world_affine(WorldAffineEdit::Shift(SemanticVec3::new(x, y, z)))
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = rotateWorld)]
    pub fn rotate_world(
        &mut self,
        radians: f64,
        axis: &[f64],
        about_values: &[f64],
    ) -> Result<(), JsValue> {
        self.semantic_family()?
            .world_affine(rotate(radians, axis, about_values)?)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = scaleWorld)]
    pub fn scale_world(&mut self, factor: f64, about_values: &[f64]) -> Result<(), JsValue> {
        self.semantic_family()?
            .world_affine(WorldAffineEdit::Scale {
                factor,
                about: about(about_values)?,
            })
            .map_err(js_error)
    }
}
