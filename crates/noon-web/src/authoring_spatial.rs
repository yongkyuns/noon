//! Thin WASM value adapters for shared Rust spatial authoring operations.
//!
//! This API exposes explicitly indexed mesh/surface operations. It does not
//! claim Manim Cairo shaded-normal or family ordering parity.

#![cfg(target_arch = "wasm32")]

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use noon::{
    Color, MeshOptions, SemanticPaint, SemanticSpatialMaterial, SurfaceSample, UvSurfacePlan,
    WorldAffineEdit,
};
use noon_core::{MeshResource, SemanticVec3, SemanticWorldTransform3D};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn invalid(code: &'static str, message: impl ToString) -> JsValue {
    js_error(AuthoringFailure::new("input", code, message))
}

pub(crate) fn vec3(values: &[f64], field: &str) -> Result<SemanticVec3, JsValue> {
    if values.len() != 3 {
        return Err(invalid(
            "spatial.invalid_vector",
            format!("{field} requires 3 values"),
        ));
    }
    Ok(SemanticVec3::new(values[0], values[1], values[2]))
}

pub(crate) fn about(values: &[f64]) -> Result<Option<SemanticVec3>, JsValue> {
    if values.is_empty() {
        Ok(None)
    } else {
        vec3(values, "about").map(Some)
    }
}

pub(crate) fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<Color, JsValue> {
    let values = [red, green, blue, alpha];
    if values
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid(
            "spatial.invalid_color",
            "color channels must be finite and in [0, 1]",
        ));
    }
    Ok(Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}

fn paint(options: &mut MeshOptions, value: Color) {
    options.style.fill = Some(SemanticPaint::Solid(value));
}

fn with_opacity(options: &mut MeshOptions, opacity: f64) -> Result<(), JsValue> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(invalid(
            "spatial.invalid_opacity",
            "opacity must be finite and in [0, 1]",
        ));
    }
    options.style.fill_opacity = opacity;
    Ok(())
}

pub(crate) fn transform_values(transform: SemanticWorldTransform3D) -> Vec<f64> {
    let t = transform.translation;
    let s = transform.scale;
    let [w, x, y, z] = transform.rotation.components();
    vec![t.x, t.y, t.z, w, x, y, z, s.x, s.y, s.z]
}

pub(crate) fn world_transform_from_values(
    values: &[f64],
) -> Result<SemanticWorldTransform3D, JsValue> {
    if values.len() != 10 {
        return Err(invalid(
            "spatial.invalid_transform",
            "world transform requires 10 values",
        ));
    }
    let translation = SemanticVec3::new(values[0], values[1], values[2]);
    let rotation =
        noon_core::SemanticRotation3D::from_components(values[3], values[4], values[5], values[6])
            .ok_or_else(|| {
                invalid(
                    "spatial.invalid_transform",
                    "world rotation must be a finite nonzero quaternion",
                )
            })?;
    let scale = SemanticVec3::new(values[7], values[8], values[9]);
    SemanticWorldTransform3D::new(translation, rotation, scale).ok_or_else(|| {
        invalid(
            "spatial.invalid_transform",
            "world transform values must be finite",
        )
    })
}

fn mesh_options(geometry: MeshResource) -> MeshOptions {
    MeshOptions::new(geometry)
}

fn flattened_surface_samples<'a>(
    points: &'a [f64],
    normals: &'a [f64],
) -> impl Iterator<Item = SurfaceSample> + 'a {
    let mut positions = points.chunks_exact(3);
    let mut normal_values = normals.chunks_exact(3);
    std::iter::from_fn(move || {
        let position = positions.next()?;
        let position = SemanticVec3::new(position[0], position[1], position[2]);
        if normals.is_empty() {
            Some(SurfaceSample::position(position))
        } else {
            let normal = normal_values.next()?;
            Some(SurfaceSample::with_normal(
                position,
                SemanticVec3::new(normal[0], normal[1], normal[2]),
            ))
        }
    })
}

#[wasm_bindgen]
pub struct WasmMeshOptions {
    pub(crate) options: MeshOptions,
}

#[wasm_bindgen]
impl WasmMeshOptions {
    /// Generic native geometry conventions; this is not full Manim class parity.
    pub fn sphere(radius: f64, u_cells: usize, v_cells: usize) -> Result<Self, JsValue> {
        noon::sphere_mesh(radius, [u_cells, v_cells])
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    #[wasm_bindgen(js_name = line3D)]
    pub fn line_3d(
        start: &[f64],
        end: &[f64],
        thickness: f64,
        segments: usize,
    ) -> Result<Self, JsValue> {
        noon::line_3d_mesh(
            vec3(start, "start")?,
            vec3(end, "end")?,
            thickness,
            segments,
        )
        .map(mesh_options)
        .map(|options| Self { options })
        .map_err(|e| invalid("spatial.invalid_mesh", e))
    }

    /// Explicit triangular faces preserve winding and use flat normals.
    pub fn polyhedron(vertices: &[f64], faces: &[u32]) -> Result<Self, JsValue> {
        let (vertices, vertex_tail) = vertices.as_chunks::<3>();
        let (faces, face_tail) = faces.as_chunks::<3>();
        if !vertex_tail.is_empty() || !face_tail.is_empty() {
            return Err(invalid(
                "spatial.invalid_mesh",
                "polyhedron inputs require triples",
            ));
        }
        if vertices.len() > noon_geometry::MAX_SURFACE_VERTICES
            || faces.len() > noon_geometry::MAX_SURFACE_CELLS
        {
            return Err(invalid(
                "spatial.invalid_mesh",
                "polyhedron exceeds the bounded mesh size",
            ));
        }
        let vertices = vertices
            .iter()
            .map(|v| SemanticVec3::new(v[0], v[1], v[2]))
            .collect::<Vec<_>>();
        noon::triangular_polyhedron_mesh(&vertices, faces)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }

    pub fn cube(size: f64) -> Result<Self, JsValue> {
        noon::cube_mesh(size)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn prism(x: f64, y: f64, z: f64) -> Result<Self, JsValue> {
        noon::prism_mesh(SemanticVec3::new(x, y, z))
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn cylinder(radius: f64, height: f64, segments: usize) -> Result<Self, JsValue> {
        noon::cylinder_mesh(radius, height, segments)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn cone(radius: f64, height: f64, segments: usize) -> Result<Self, JsValue> {
        noon::cone_mesh(radius, height, segments)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn torus(
        major_radius: f64,
        minor_radius: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, JsValue> {
        noon::torus_mesh(major_radius, minor_radius, [u_cells, v_cells])
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        let value = color(red, green, blue, alpha)?;
        paint(&mut self.options, value);
        Ok(())
    }
    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
        with_opacity(&mut self.options, opacity)
    }
    #[wasm_bindgen(js_name = setPointLit)]
    pub fn set_point_lit(&mut self, enabled: bool) {
        self.options.material = if enabled {
            SemanticSpatialMaterial::PointLit
        } else {
            SemanticSpatialMaterial::Unlit
        };
    }
}

#[wasm_bindgen]
pub struct WasmMeshFamilyOptions {
    pub(crate) options: Vec<MeshOptions>,
    pub(crate) cells: Vec<[usize; 2]>,
}

#[wasm_bindgen]
impl WasmMeshFamilyOptions {
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.options.len()
    }
    pub(crate) fn retain_surface_roles(&mut self) -> Result<(), JsValue> {
        if self.options.len() != self.cells.len() {
            return Err(invalid(
                "spatial.invalid_surface_roles",
                "UV roles must correspond one-for-one with surface cells",
            ));
        }
        for (options, cell) in self.options.iter_mut().zip(&self.cells) {
            options.surface_uv_cell = Some(*cell);
        }
        Ok(())
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
        let fill = color(red, green, blue, alpha)?;
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(invalid(
                "spatial.invalid_opacity",
                "opacity must be finite and in [0, 1]",
            ));
        }
        for options in &mut self.options {
            paint(options, fill);
            options.style.fill_opacity = opacity;
        }
        Ok(())
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
        let stroke = color(red, green, blue, alpha)?;
        if !width.is_finite() || width < 0.0 {
            return Err(invalid(
                "spatial.invalid_stroke_width",
                "stroke width must be finite and non-negative",
            ));
        }
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(invalid(
                "spatial.invalid_opacity",
                "opacity must be finite and in [0, 1]",
            ));
        }
        for options in &mut self.options {
            options.style.stroke = Some(SemanticPaint::Solid(stroke));
            options.style.stroke_width = width;
            options.style.stroke_opacity = opacity;
            options.style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
        }
        Ok(())
    }
    #[wasm_bindgen(js_name = setPointLit)]
    pub fn set_point_lit(&mut self, enabled: bool) {
        let material = if enabled {
            SemanticSpatialMaterial::PointLit
        } else {
            SemanticSpatialMaterial::Unlit
        };
        for options in &mut self.options {
            options.material = material;
        }
    }
    #[wasm_bindgen(js_name = setCheckerboard)]
    pub fn set_checkerboard(
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
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(invalid(
                "spatial.invalid_opacity",
                "opacity must be finite and in [0, 1]",
            ));
        }
        let a = color(first[0], first[1], first[2], first[3])?;
        let b = color(second[0], second[1], second[2], second[3])?;
        if self.options.len() != self.cells.len() {
            return Err(invalid(
                "spatial.invalid_surface_roles",
                "UV roles must correspond one-for-one with surface cells",
            ));
        }
        for (options, [u, v]) in self.options.iter_mut().zip(&self.cells) {
            paint(options, if (u % 2 + v % 2) % 2 == 0 { a } else { b });
            options.style.fill_opacity = opacity;
        }
        Ok(())
    }
}

#[wasm_bindgen]
pub struct WasmSurfaceSamplingPlan {
    plan: UvSurfacePlan,
}

#[wasm_bindgen]
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
        UvSurfacePlan::new([u_min, u_max], [v_min, v_max], [u_cells, v_cells])
            .map(|plan| Self { plan })
            .map_err(|e| invalid("spatial.invalid_surface", e))
    }
    #[wasm_bindgen(js_name = parameters)]
    pub fn parameters(&self) -> Vec<f64> {
        self.plan.coordinates().flat_map(|(u, v)| [u, v]).collect()
    }
    #[wasm_bindgen(js_name = finishMesh)]
    pub fn finish_mesh(&self, points: &[f64], normals: &[f64]) -> Result<WasmMeshOptions, JsValue> {
        let expected =
            self.plan.vertex_count().checked_mul(3).ok_or_else(|| {
                invalid("spatial.invalid_surface", "surface payload is too large")
            })?;
        if points.len() != expected || (!normals.is_empty() && normals.len() != expected) {
            return Err(invalid(
                "spatial.invalid_surface_samples",
                format!("expected {expected} position and optional normal components"),
            ));
        }
        let grid = self
            .plan
            .finish_samples(flattened_surface_samples(points, normals))
            .map_err(|e| invalid("spatial.invalid_surface_samples", e))?;
        grid.into_mesh_resource()
            .map(mesh_options)
            .map(|options| WasmMeshOptions { options })
            .map_err(|e| invalid("spatial.invalid_surface", e))
    }
    #[wasm_bindgen(js_name = finishCells)]
    pub fn finish_cells(
        &self,
        points: &[f64],
        normals: &[f64],
    ) -> Result<WasmMeshFamilyOptions, JsValue> {
        let expected =
            self.plan.vertex_count().checked_mul(3).ok_or_else(|| {
                invalid("spatial.invalid_surface", "surface payload is too large")
            })?;
        if points.len() != expected || (!normals.is_empty() && normals.len() != expected) {
            return Err(invalid(
                "spatial.invalid_surface_samples",
                format!("expected {expected} position and optional normal components"),
            ));
        }
        let grid = self
            .plan
            .finish_samples(flattened_surface_samples(points, normals))
            .map_err(|e| invalid("spatial.invalid_surface_samples", e))?;
        let cell_count = grid.plan().cell_count();
        let mut options = Vec::with_capacity(cell_count);
        let mut cells = Vec::with_capacity(cell_count);
        for cell in grid.cells() {
            let uv_cell = cell.uv_cell;
            let mesh = cell
                .into_mesh_resource()
                .map_err(|e| invalid("spatial.invalid_surface", e))?;
            options.push(mesh_options(mesh));
            cells.push(uv_cell);
        }
        Ok(WasmMeshFamilyOptions { options, cells })
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createMesh)]
    pub fn create_mesh(
        &self,
        candidate: WasmMeshOptions,
    ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
        noon::Mobject::from_mesh(Rc::clone(&self.semantics), candidate.options)
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = createMeshFamily)]
    pub fn create_mesh_family(
        &self,
        mut candidate: WasmMeshFamilyOptions,
    ) -> Result<crate::WasmAuthoringFamilyHandle, JsValue> {
        candidate.retain_surface_roles()?;
        let family =
            noon::MobjectFamily::from_meshes(Rc::clone(&self.semantics), candidate.options)
                .map_err(js_error)?;
        let surface = noon::SurfaceFamily::from_family(family).map_err(js_error)?;
        Ok(crate::WasmAuthoringFamilyHandle::from_surface_family(
            surface,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_and_color_conversion_is_exact_and_validated() {
        assert_eq!(
            vec3(&[1.0, 2.0, 3.0], "test").unwrap(),
            SemanticVec3::new(1.0, 2.0, 3.0)
        );
        assert!(vec3(&[1.0, 2.0], "test").is_err());
        assert!(color(1.1, 0.0, 0.0, 1.0).is_err());
        assert!(color(0.0, f64::NAN, 0.0, 1.0).is_err());
        assert_eq!(about(&[]).unwrap(), None);
    }

    #[test]
    fn transform_adapter_has_documented_component_order() {
        let q = noon_core::SemanticRotation3D::from_components(0.5, 0.5, 0.5, 0.5).unwrap();
        let value = SemanticWorldTransform3D::new(
            SemanticVec3::new(1.0, 2.0, 3.0),
            q,
            SemanticVec3::new(4.0, 5.0, 6.0),
        )
        .unwrap();
        assert_eq!(
            transform_values(value),
            vec![1.0, 2.0, 3.0, 0.5, 0.5, 0.5, 0.5, 4.0, 5.0, 6.0]
        );
    }
}
