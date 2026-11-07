//! Native typed wrappers over the same bounded Rust mesh/surface inputs as WASM.
use crate::engine_error;
use noon::integration::spatial_input as input;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct MeshInput {
    value: Option<input::SpatialMeshInput>,
}
impl From<input::SpatialMeshInput> for MeshInput {
    fn from(value: input::SpatialMeshInput) -> Self {
        Self { value: Some(value) }
    }
}
impl MeshInput {
    pub(crate) fn take(&mut self) -> PyResult<input::SpatialMeshInput> {
        self.value
            .take()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
    fn value_mut(&mut self) -> PyResult<&mut input::SpatialMeshInput> {
        self.value
            .as_mut()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl MeshInput {
    fn free(&mut self) {
        self.value.take();
    }
    #[staticmethod]
    fn sphere(
        radius: f64,
        u_cells: usize,
        v_cells: usize,
        u_start: f64,
        u_end: f64,
        v_start: f64,
        v_end: f64,
    ) -> Result<Self, PyErr> {
        input::SpatialMeshInput::sphere(radius, u_cells, v_cells, u_start, u_end, v_start, v_end)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "line3D")]
    fn line_3d(
        start: Vec<f64>,
        end: Vec<f64>,
        thickness: f64,
        segments: usize,
    ) -> Result<Self, PyErr> {
        input::SpatialMeshInput::line_3d(&start, &end, thickness, segments)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    fn polyhedron(vertices: Vec<f64>, faces: Vec<u32>) -> Result<Self, PyErr> {
        input::SpatialMeshInput::polyhedron(&vertices, &faces)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    fn cube(size: f64) -> Result<Self, PyErr> {
        input::SpatialMeshInput::cube(size)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    fn prism(x: f64, y: f64, z: f64) -> Result<Self, PyErr> {
        input::SpatialMeshInput::prism(x, y, z)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "prismFaces")]
    fn prism_faces(x: f64, y: f64, z: f64, shade_in_3d: bool) -> Result<MeshFamilyInput, PyErr> {
        input::SpatialMeshInput::prism_faces(x, y, z, shade_in_3d)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[staticmethod]
    fn cylinder(
        radius: f64,
        height: f64,
        segments: usize,
        show_ends: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, PyErr> {
        input::SpatialMeshInput::cylinder(
            radius,
            height,
            segments,
            show_ends,
            angle_start,
            angle_end,
        )
        .map(Into::into)
        .map_err(engine_error)
    }
    #[staticmethod]
    fn cone(
        radius: f64,
        height: f64,
        segments: usize,
        show_base: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, PyErr> {
        input::SpatialMeshInput::cone(radius, height, segments, show_base, angle_start, angle_end)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[pyo3(name = "setAxialPose")]
    fn set_axial_pose(&mut self, direction: Vec<f64>, offset: f64) -> Result<(), PyErr> {
        self.value_mut()?
            .set_axial_pose(&direction, offset)
            .map_err(engine_error)
    }
    #[staticmethod]
    fn torus(
        major_radius: f64,
        minor_radius: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, PyErr> {
        input::SpatialMeshInput::torus(major_radius, minor_radius, u_cells, v_cells)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[pyo3(name = "setColor")]
    fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.value_mut()?
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "setOpacity")]
    fn set_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.value_mut()?.set_opacity(opacity).map_err(engine_error)
    }
    #[pyo3(name = "setPointLit")]
    fn set_point_lit(&mut self, enabled: bool) -> PyResult<()> {
        self.value_mut()?.set_point_lit(enabled);
        Ok(())
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct MeshFamilyInput {
    value: Option<input::SpatialMeshFamilyInput>,
}
impl From<input::SpatialMeshFamilyInput> for MeshFamilyInput {
    fn from(value: input::SpatialMeshFamilyInput) -> Self {
        Self { value: Some(value) }
    }
}
impl MeshFamilyInput {
    pub(crate) fn take(&mut self) -> PyResult<input::SpatialMeshFamilyInput> {
        self.value
            .take()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
    fn value(&self) -> PyResult<&input::SpatialMeshFamilyInput> {
        self.value
            .as_ref()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
    fn value_mut(&mut self) -> PyResult<&mut input::SpatialMeshFamilyInput> {
        self.value
            .as_mut()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl MeshFamilyInput {
    fn free(&mut self) {
        self.value.take();
    }
    #[getter]
    fn len(&self) -> PyResult<usize> {
        Ok(self.value()?.len())
    }
    #[pyo3(name = "setAxialPose")]
    fn set_axial_pose(&mut self, direction: Vec<f64>, offset: f64) -> Result<(), PyErr> {
        self.value_mut()?
            .set_axial_pose(&direction, offset)
            .map_err(engine_error)
    }
    #[pyo3(name = "addCircleCap")]
    fn add_circle_cap(
        &mut self,
        radius: f64,
        local_z: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        shade_in_3d: bool,
    ) -> Result<(), PyErr> {
        self.value_mut()?
            .add_circle_cap(radius, local_z, red, green, blue, alpha, shade_in_3d)
            .map_err(engine_error)
    }
    #[pyo3(name = "setFill")]
    fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<(), PyErr> {
        self.value_mut()?
            .set_fill(red, green, blue, alpha, opacity)
            .map_err(engine_error)
    }
    #[pyo3(name = "setStroke")]
    fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: f64,
        opacity: f64,
    ) -> Result<(), PyErr> {
        self.value_mut()?
            .set_stroke(red, green, blue, alpha, width, opacity)
            .map_err(engine_error)
    }
    #[pyo3(name = "setPointLit")]
    fn set_point_lit(&mut self, enabled: bool) -> PyResult<()> {
        self.value_mut()?.set_point_lit(enabled);
        Ok(())
    }
    #[pyo3(name = "setCairoSurface")]
    fn set_cairo_surface(&mut self, enabled: bool) -> Result<(), PyErr> {
        self.value_mut()?
            .set_cairo_surface(enabled)
            .map_err(engine_error)
    }
    #[pyo3(name = "setCheckerboard")]
    fn set_checkerboard(
        &mut self,
        first: Vec<f64>,
        second: Vec<f64>,
        opacity: f64,
    ) -> Result<(), PyErr> {
        self.value_mut()?
            .set_checkerboard(&first, &second, opacity)
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct SurfaceSamplingPlan {
    value: Option<input::SurfaceSamplingInput>,
}
impl From<input::SurfaceSamplingInput> for SurfaceSamplingPlan {
    fn from(value: input::SurfaceSamplingInput) -> Self {
        Self { value: Some(value) }
    }
}
impl SurfaceSamplingPlan {
    fn value(&self) -> PyResult<&input::SurfaceSamplingInput> {
        self.value
            .as_ref()
            .ok_or_else(|| engine_error("spatial input has been consumed"))
    }
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl SurfaceSamplingPlan {
    fn free(&mut self) {
        self.value.take();
    }
    #[new]
    fn new(
        u_min: f64,
        u_max: f64,
        v_min: f64,
        v_max: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, PyErr> {
        input::SurfaceSamplingInput::new(u_min, u_max, v_min, v_max, u_cells, v_cells)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[pyo3(name = "parameters")]
    fn parameters(&self) -> PyResult<Vec<f64>> {
        Ok(self.value()?.parameters())
    }
    #[pyo3(name = "cairoParameters")]
    fn cairo_parameters(&self) -> PyResult<Vec<f64>> {
        Ok(self.value()?.cairo_parameters())
    }
    #[pyo3(name = "finishMesh")]
    fn finish_mesh(&self, points: Vec<f64>, normals: Vec<f64>) -> Result<MeshInput, PyErr> {
        self.value()?
            .finish_mesh(&points, &normals)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[pyo3(name = "finishCells")]
    fn finish_cells(&self, points: Vec<f64>, normals: Vec<f64>) -> Result<MeshFamilyInput, PyErr> {
        self.value()?
            .finish_cells(&points, &normals)
            .map(Into::into)
            .map_err(engine_error)
    }
    #[pyo3(name = "finishCairoCells")]
    fn finish_cairo_cells(&self, points: Vec<f64>) -> Result<MeshFamilyInput, PyErr> {
        self.value()?
            .finish_cairo_cells(&points)
            .map(Into::into)
            .map_err(engine_error)
    }
}

pub(crate) fn create_mesh(
    context: &mut crate::context::Context,
    mut candidate: PyRefMut<'_, MeshInput>,
) -> PyResult<crate::mobject::MobjectHandle> {
    context.available()?;
    let options = candidate.take()?.into_options();
    if context.execution.is_some() {
        context.with_live(|live| live.create_mesh(options))
    } else {
        context.scene.mesh(options).map_err(engine_error)
    }
    .map(crate::mobject::MobjectHandle::from_semantic_mobject)
}
pub(crate) fn create_family(
    context: &mut crate::context::Context,
    mut candidate: PyRefMut<'_, MeshFamilyInput>,
) -> PyResult<crate::family::FamilyHandle> {
    context.available()?;
    let candidate = candidate.take()?;
    let surface = candidate.has_surface_roles();
    let (meshes, paths) = candidate.into_parts().map_err(engine_error)?;
    let family = if context.execution.is_some() {
        context.with_live(|live| live.create_mesh_family_with_paths(meshes, paths))
    } else {
        context
            .scene
            .mesh_family_with_paths(meshes, paths)
            .map_err(engine_error)
    }?;
    if surface {
        noon::SurfaceFamily::from_family(family.clone()).map_err(engine_error)?;
    }
    Ok(crate::family::FamilyHandle { family })
}
pub(crate) fn world_affine(
    context: &mut crate::context::Context,
    target: noon::MobjectTarget<'_>,
    edit: noon::WorldAffineEdit,
) -> PyResult<()> {
    context.available()?;
    if context.execution.is_some() {
        context.with_live(|live| live.world_affine(target, edit))
    } else {
        context
            .scene
            .world_affine(target, edit)
            .map_err(engine_error)
    }
}
pub(crate) fn checkerboard(first: &[f64], second: &[f64]) -> PyResult<[noon::Color; 2]> {
    let color = |v: &[f64]| {
        let [r, g, b, a] = v else {
            return Err(engine_error("checkerboard colors require RGBA quadruples"));
        };
        input::color(*r, *g, *b, *a).map_err(engine_error)
    };
    Ok([color(first)?, color(second)?])
}
pub(crate) fn authored_mesh(
    store: &crate::context::Store,
    mut candidate: PyRefMut<'_, MeshInput>,
) -> PyResult<crate::mobject::MobjectHandle> {
    noon::Mobject::from_mesh(
        std::rc::Rc::clone(&store.semantics),
        candidate.take()?.into_options(),
    )
    .map(crate::mobject::MobjectHandle::from_semantic_mobject)
    .map_err(engine_error)
}
pub(crate) fn authored_family(
    store: &crate::context::Store,
    mut candidate: PyRefMut<'_, MeshFamilyInput>,
) -> PyResult<crate::family::FamilyHandle> {
    let candidate = candidate.take()?;
    let surface = candidate.has_surface_roles();
    let (meshes, paths) = candidate.into_parts().map_err(engine_error)?;
    let family = noon::MobjectFamily::from_meshes_and_paths(
        std::rc::Rc::clone(&store.semantics),
        meshes,
        paths,
    )
    .map_err(engine_error)?;
    if surface {
        noon::SurfaceFamily::from_family(family.clone()).map_err(engine_error)?;
    }
    Ok(crate::family::FamilyHandle { family })
}
