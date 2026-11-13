//! Inert mesh/surface inputs shared by native and WASM language adapters.
//! Geometry construction, roles and validation continue through the ordinary Rust APIs.
use super::AuthoringFailure;
use crate::{
    CairoSurfaceGrid, Color, MeshOptions, SemanticPaint, SemanticSpatialMaterial,
    SpatialPathOptions, SurfaceSample, UvSurfacePlan,
};
use noon_core::{MeshResource, SemanticVec3, SemanticWorldTransform3D};

fn invalid(code: &'static str, message: impl ToString) -> AuthoringFailure {
    AuthoringFailure::new("input", code, message)
}

pub fn vec3(values: &[f64], field: &str) -> Result<SemanticVec3, AuthoringFailure> {
    if values.len() != 3 {
        return Err(invalid(
            "spatial.invalid_vector",
            format!("{field} requires 3 values"),
        ));
    }
    Ok(SemanticVec3::new(values[0], values[1], values[2]))
}

pub fn about(values: &[f64]) -> Result<Option<SemanticVec3>, AuthoringFailure> {
    if values.is_empty() {
        Ok(None)
    } else {
        vec3(values, "about").map(Some)
    }
}

pub fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<Color, AuthoringFailure> {
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

fn with_opacity(options: &mut MeshOptions, opacity: f64) -> Result<(), AuthoringFailure> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(invalid(
            "spatial.invalid_opacity",
            "opacity must be finite and in [0, 1]",
        ));
    }
    options.style.fill_opacity = opacity;
    Ok(())
}

pub fn transform_values(transform: SemanticWorldTransform3D) -> Vec<f64> {
    let t = transform.translation;
    let s = transform.scale;
    let [w, x, y, z] = transform.rotation.components();
    vec![t.x, t.y, t.z, w, x, y, z, s.x, s.y, s.z]
}

pub fn world_transform_from_values(
    values: &[f64],
) -> Result<SemanticWorldTransform3D, AuthoringFailure> {
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
    let mut positions = points.as_chunks::<3>().0.iter();
    let mut normal_values = normals.as_chunks::<3>().0.iter();
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

fn flattened_cairo_surface_samples(points: &[f64]) -> impl Iterator<Item = SemanticVec3> + '_ {
    points
        .as_chunks::<3>()
        .0
        .iter()
        .map(|point| SemanticVec3::new(point[0], point[1], point[2]))
}

pub struct SpatialMeshInput {
    pub(crate) options: MeshOptions,
}

#[allow(clippy::too_many_arguments)]
impl SpatialMeshInput {
    pub fn into_options(self) -> MeshOptions {
        self.options
    }
    /// Generic native geometry conventions; this is not full Manim class parity.
    pub fn sphere(
        radius: f64,
        u_cells: usize,
        v_cells: usize,
        u_start: f64,
        u_end: f64,
        v_start: f64,
        v_end: f64,
    ) -> Result<Self, AuthoringFailure> {
        noon_geometry::sphere_mesh_range(
            radius,
            [u_cells, v_cells],
            [u_start, u_end],
            [v_start, v_end],
        )
        .map(mesh_options)
        .map(|options| Self { options })
        .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn line_3d(
        start: &[f64],
        end: &[f64],
        thickness: f64,
        segments: usize,
    ) -> Result<Self, AuthoringFailure> {
        crate::line_3d_mesh(
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
    pub fn polyhedron(vertices: &[f64], faces: &[u32]) -> Result<Self, AuthoringFailure> {
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
        crate::triangular_polyhedron_mesh(&vertices, faces)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }

    pub fn cube(size: f64) -> Result<Self, AuthoringFailure> {
        crate::cube_mesh(size)
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn prism(x: f64, y: f64, z: f64) -> Result<Self, AuthoringFailure> {
        crate::prism_mesh(SemanticVec3::new(x, y, z))
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }

    /// Six indexed prism faces for one semantic family. Cairo appearance data
    /// is retained only when shade_in_3d is selected.
    pub fn prism_faces(
        x: f64,
        y: f64,
        z: f64,
        shade_in_3d: bool,
    ) -> Result<SpatialMeshFamilyInput, AuthoringFailure> {
        let size = SemanticVec3::new(x, y, z);
        let faces = if shade_in_3d {
            noon_geometry::prism_faces_cairo(size)
        } else {
            noon_geometry::prism_faces(size)
        }
        .map_err(|error| invalid("spatial.invalid_mesh", error))?;
        let mut options = faces.into_iter().map(mesh_options).collect::<Vec<_>>();
        if shade_in_3d {
            for option in &mut options {
                option.material = SemanticSpatialMaterial::CairoSurface;
            }
        }
        Ok(SpatialMeshFamilyInput {
            options,
            cells: Vec::new(),
            circle_caps: Vec::new(),
            axial_pose: None,
        })
    }
    pub fn cylinder(
        radius: f64,
        height: f64,
        segments: usize,
        show_ends: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, AuthoringFailure> {
        noon_geometry::cylinder_mesh_range(
            radius,
            height,
            segments,
            show_ends,
            [angle_start, angle_end],
        )
        .map(mesh_options)
        .map(|options| Self { options })
        .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn cone(
        radius: f64,
        height: f64,
        segments: usize,
        show_base: bool,
        angle_start: f64,
        angle_end: f64,
    ) -> Result<Self, AuthoringFailure> {
        noon_geometry::cone_mesh_range(
            radius,
            height,
            segments,
            show_base,
            [angle_start, angle_end],
        )
        .map(mesh_options)
        .map(|options| Self { options })
        .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    /// Configure the inert pose before a mesh is admitted to its semantic owner.
    pub fn set_axial_pose(
        &mut self,
        direction: &[f64],
        offset: f64,
    ) -> Result<(), AuthoringFailure> {
        self.options.transform =
            SemanticWorldTransform3D::from_axial_direction(vec3(direction, "direction")?, offset)
                .ok_or_else(|| {
                invalid(
                    "spatial.invalid_direction",
                    "axial pose requires a finite nonzero direction and finite offset",
                )
            })?;
        Ok(())
    }
    pub fn torus(
        major_radius: f64,
        minor_radius: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, AuthoringFailure> {
        crate::torus_mesh(major_radius, minor_radius, [u_cells, v_cells])
            .map(mesh_options)
            .map(|options| Self { options })
            .map_err(|e| invalid("spatial.invalid_mesh", e))
    }
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringFailure> {
        let value = color(red, green, blue, alpha)?;
        paint(&mut self.options, value);
        Ok(())
    }
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), AuthoringFailure> {
        with_opacity(&mut self.options, opacity)
    }
    pub fn set_point_lit(&mut self, enabled: bool) {
        self.options.material = if enabled {
            SemanticSpatialMaterial::PointLit
        } else {
            SemanticSpatialMaterial::Unlit
        };
    }
}

pub struct SpatialMeshFamilyInput {
    pub(crate) options: Vec<MeshOptions>,
    pub(crate) cells: Vec<[usize; 2]>,
    circle_caps: Vec<CircleCapRequest>,
    axial_pose: Option<(SemanticVec3, f64)>,
}

struct CircleCapRequest {
    radius: f64,
    local_z: f64,
    color: Color,
    shade_in_3d: bool,
}

#[allow(clippy::too_many_arguments)]
impl SpatialMeshFamilyInput {
    pub fn is_empty(&self) -> bool {
        self.options.is_empty()
    }
    pub fn len(&self) -> usize {
        self.options.len()
    }
    /// Stage one shared local-+Z axial pose on every family member before
    /// semantic publication. Direction and offset validation is Rust-owned.
    pub fn set_axial_pose(
        &mut self,
        direction: &[f64],
        offset: f64,
    ) -> Result<(), AuthoringFailure> {
        let direction = vec3(direction, "direction")?;
        let transform = SemanticWorldTransform3D::from_axial_direction(direction, offset)
            .ok_or_else(|| {
                invalid(
                    "spatial.invalid_direction",
                    "axial pose requires a finite nonzero direction and finite offset",
                )
            })?;
        for options in &mut self.options {
            options.transform = transform;
        }
        self.axial_pose = Some((direction, offset));
        Ok(())
    }

    /// Add an ordinary retained Circle path cap to the same family/resource
    /// transaction as sampled surface cells. `local_z` is along local +Z.
    pub fn add_circle_cap(
        &mut self,
        radius: f64,
        local_z: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        shade_in_3d: bool,
    ) -> Result<(), AuthoringFailure> {
        if !radius.is_finite() || radius <= 0.0 || !local_z.is_finite() {
            return Err(invalid(
                "spatial.invalid_circle_cap",
                "cap radius must be positive and local offset finite",
            ));
        }
        let color = color(red, green, blue, alpha)?;
        self.circle_caps.push(CircleCapRequest {
            radius,
            local_z,
            color,
            shade_in_3d,
        });
        Ok(())
    }

    pub fn into_parts(
        mut self,
    ) -> Result<(Vec<MeshOptions>, Vec<SpatialPathOptions>), AuthoringFailure> {
        self.retain_surface_roles()?;
        let (direction, offset) = self
            .axial_pose
            .unwrap_or((SemanticVec3::new(0.0, 0.0, 1.0), 0.0));
        let mut paths = Vec::with_capacity(self.circle_caps.len());
        for cap in self.circle_caps {
            let transform =
                SemanticWorldTransform3D::from_axial_direction(direction, offset + cap.local_z)
                    .ok_or_else(|| {
                        invalid("spatial.invalid_direction", "invalid cap axial pose")
                    })?;
            let options =
                SpatialPathOptions::circle(cap.radius, transform, cap.color, cap.shade_in_3d)
                    .map_err(AuthoringFailure::from)?;
            paths.push(options);
        }
        Ok((self.options, paths))
    }
    pub fn retain_surface_roles(&mut self) -> Result<(), AuthoringFailure> {
        if self.options.len() < self.cells.len() {
            return Err(invalid(
                "spatial.invalid_surface_roles",
                "UV roles must correspond one-for-one with surface cells",
            ));
        }
        for (options, cell) in self
            .options
            .iter_mut()
            .take(self.cells.len())
            .zip(&self.cells)
        {
            options.surface_uv_cell = Some(*cell);
        }
        Ok(())
    }
    pub fn has_surface_roles(&self) -> bool {
        !self.cells.is_empty()
    }
    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
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
    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: f64,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
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
    pub fn set_cairo_surface(&mut self, enabled: bool) -> Result<(), AuthoringFailure> {
        if enabled
            && self
                .options
                .iter()
                .any(|options| options.geometry.cairo_appearance().is_none())
        {
            return Err(invalid(
                "spatial.invalid_cairo_surface_material",
                "CairoSurface requires a Cairo-sampled appearance on every mesh cell",
            ));
        }
        for options in &mut self.options {
            if enabled {
                options.material = SemanticSpatialMaterial::CairoSurface;
            } else if options.material == SemanticSpatialMaterial::CairoSurface {
                options.material = SemanticSpatialMaterial::Unlit;
            }
        }
        Ok(())
    }
    pub fn set_checkerboard(
        &mut self,
        first: &[f64],
        second: &[f64],
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
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
        if self.options.len() < self.cells.len() {
            return Err(invalid(
                "spatial.invalid_surface_roles",
                "UV roles must correspond one-for-one with surface cells",
            ));
        }
        for (options, [u, v]) in self
            .options
            .iter_mut()
            .take(self.cells.len())
            .zip(&self.cells)
        {
            paint(options, if (u % 2 + v % 2) % 2 == 0 { a } else { b });
            options.style.fill_opacity = opacity;
        }
        Ok(())
    }
}

pub struct SurfaceSamplingInput {
    plan: UvSurfacePlan,
}

impl SurfaceSamplingInput {
    pub fn new(
        u_min: f64,
        u_max: f64,
        v_min: f64,
        v_max: f64,
        u_cells: usize,
        v_cells: usize,
    ) -> Result<Self, AuthoringFailure> {
        UvSurfacePlan::new([u_min, u_max], [v_min, v_max], [u_cells, v_cells])
            .map(|plan| Self { plan })
            .map_err(|e| invalid("spatial.invalid_surface", e))
    }
    pub fn parameters(&self) -> Vec<f64> {
        self.plan.coordinates().flat_map(|(u, v)| [u, v]).collect()
    }
    pub fn cairo_parameters(&self) -> Vec<f64> {
        self.plan
            .cairo_coordinates()
            .flat_map(|(u, v)| [u, v])
            .collect()
    }
    pub fn finish_mesh(
        &self,
        points: &[f64],
        normals: &[f64],
    ) -> Result<SpatialMeshInput, AuthoringFailure> {
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
            .map(|options| SpatialMeshInput { options })
            .map_err(|e| invalid("spatial.invalid_surface", e))
    }
    pub fn finish_cells(
        &self,
        points: &[f64],
        normals: &[f64],
    ) -> Result<SpatialMeshFamilyInput, AuthoringFailure> {
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
            .finish_unlit_samples(flattened_surface_samples(points, normals))
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
        Ok(SpatialMeshFamilyInput {
            options,
            cells,
            circle_caps: Vec::new(),
            axial_pose: None,
        })
    }

    pub fn finish_cairo_cells(
        &self,
        points: &[f64],
    ) -> Result<SpatialMeshFamilyInput, AuthoringFailure> {
        let expected = self
            .plan
            .cell_count()
            .checked_mul(16)
            .and_then(|count| count.checked_mul(3))
            .ok_or_else(|| invalid("spatial.invalid_surface", "surface payload is too large"))?;
        if points.len() != expected {
            return Err(invalid(
                "spatial.invalid_surface_samples",
                format!("expected {expected} Cairo position components"),
            ));
        }
        let grid: CairoSurfaceGrid = self
            .plan
            .finish_cairo_samples(flattened_cairo_surface_samples(points))
            .map_err(|e| invalid("spatial.invalid_surface_samples", e))?;
        let mut options = Vec::with_capacity(self.plan.cell_count());
        let mut cells = Vec::with_capacity(self.plan.cell_count());
        for (cell, appearance) in grid.cells() {
            let uv_cell = cell.uv_cell;
            let mesh = cell
                .into_mesh_resource()
                .map_err(|e| invalid("spatial.invalid_surface", e))?
                .with_cairo_appearance(appearance)
                .map_err(|e| invalid("spatial.invalid_surface", e))?;
            options.push(mesh_options(mesh));
            cells.push(uv_cell);
        }
        Ok(SpatialMeshFamilyInput {
            options,
            cells,
            circle_caps: Vec::new(),
            axial_pose: None,
        })
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

    #[test]
    fn surface_payload_shape_and_uv_roles_share_one_preparation() {
        let plan = SurfaceSamplingInput::new(-1.0, 1.0, -1.0, 1.0, 2, 2).unwrap();
        let parameters = plan.parameters();
        let points = parameters
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|[u, v]| [*u, *v, 0.0])
            .collect::<Vec<_>>();
        assert_eq!(points.len(), 27);
        assert_eq!(
            plan.finish_mesh(&points[..26], &[]).err().unwrap().code,
            "spatial.invalid_surface_samples"
        );
        assert!(plan.finish_mesh(&points, &[]).is_ok());
        let (cells, caps) = plan
            .finish_cells(&points, &[])
            .unwrap()
            .into_parts()
            .unwrap();
        assert_eq!(cells.len(), 4);
        assert!(caps.is_empty());
        assert!(cells.iter().all(|cell| cell.surface_uv_cell.is_some()));
    }
}

/// Inert domain spelling over the existing atomic spatial membership operations.
#[derive(Clone, Copy)]
pub enum SpatialMembershipPolicy {
    Assign(noon_core::SemanticSpatialCompositionDomain),
    DefaultWorld,
}
impl SpatialMembershipPolicy {
    pub fn from_name(value: &str) -> Result<Self, AuthoringFailure> {
        use noon_core::SemanticSpatialCompositionDomain as Domain;
        match value {
            "world_default" => Ok(Self::DefaultWorld),
            "world" => Ok(Self::Assign(Domain::World)),
            "fixed_frame" => Ok(Self::Assign(Domain::FixedFrame)),
            "fixed_orientation" => Ok(Self::Assign(Domain::FixedOrientation)),
            _ => Err("unknown spatial composition domain".into()),
        }
    }
    pub fn apply_scene(
        self,
        scene: &mut crate::Scene,
        targets: &[crate::MobjectTarget<'_>],
    ) -> Result<(), AuthoringFailure> {
        let running = scene.running_execution().is_some();
        match self {
            Self::Assign(domain) => scene.add_all_in_spatial_composition_domain(targets, domain),
            Self::DefaultWorld => scene.add_all_world_mobjects(targets),
        }
        .map(|_| ())
        .map_err(|error| match error {
            crate::SpatialCompositionError::Authoring(
                crate::AuthoringError::ExecutionPublication(error),
            ) if running => crate::LiveSessionError::Publication(error).into(),
            other => other.into(),
        })
    }
}

pub fn camera_profile(values: &[f64]) -> Result<noon_core::ManimCamera3DProfile, AuthoringFailure> {
    if values.len() != 9 {
        return Err(invalid("camera.profile", "camera profile requires [phi, theta, gamma, focalDistance, zoom, frameHeight, centerX, centerY, centerZ]"));
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid(
            "camera.profile",
            "camera profile values must be finite",
        ));
    }
    Ok(noon_core::ManimCamera3DProfile {
        phi: values[0],
        theta: values[1],
        gamma: values[2],
        focal_distance: values[3],
        zoom: values[4],
        frame_height: values[5],
        frame_center: noon_core::SemanticVec3::new(values[6], values[7], values[8]),
    })
}

pub fn camera_profile_values(profile: noon_core::ManimCamera3DProfile) -> Vec<f64> {
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
