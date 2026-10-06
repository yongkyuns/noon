//! Deterministic renderer-independent geometry for Noon.

#![forbid(unsafe_code)]

mod arrow;
pub use arrow::{arrow_tip_vertices, ArrowGeometry};
mod boolean;
mod coordinates;
mod flatten;
pub use boolean::{boolean_paths, BooleanOperation, BooleanPathError, BOOLEAN_FLATTEN_TOLERANCE};
mod geometry_proportion;
mod isoline;
mod morph;
mod outline;
mod partial;
mod polygon_fill;
pub use polygon_fill::polygon_fill_contains;
mod plotting;
mod reverse;
mod smoothing;
mod solids;
mod surface;
pub use reverse::reverse_path;
pub use smoothing::{
    change_path_anchor_mode, change_path_anchor_mode_with_boundary, smooth_curve_handles,
    SplineBoundary,
};
mod mesh_helpers;
pub use mesh_helpers::{line_3d_mesh, triangular_polyhedron_mesh};

pub use solids::{
    cone_mesh, cone_mesh_range, cone_parts, cone_parts_range, cube_mesh, cylinder_mesh,
    cylinder_mesh_range, cylinder_parts, cylinder_parts_range, prism_faces, prism_faces_cairo,
    prism_mesh, sphere_mesh, sphere_mesh_range, surface_mesh, torus_mesh,
};
pub use surface::{
    CairoSurfaceAppearance, CairoSurfaceCoordinates, CairoSurfaceGrid, SurfaceCell,
    SurfaceCoordinates, SurfaceError, SurfaceGrid, SurfaceSample, UvSurfacePlan,
    CAIRO_SURFACE_HANDLE_SCALE, MAX_CAIRO_SURFACE_SAMPLES, MAX_SURFACE_CELLS, MAX_SURFACE_VERTICES,
};
mod tessellation;
mod vector_field;

pub use coordinates::*;
pub use geometry_proportion::*;
pub use isoline::*;
pub use morph::*;
pub use outline::*;
pub use partial::*;
pub use plotting::*;
pub use tessellation::*;
pub use vector_field::*;
