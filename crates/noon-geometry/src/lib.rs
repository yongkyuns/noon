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
    cone_mesh, cone_parts, cube_mesh, cylinder_mesh, cylinder_parts, prism_faces, prism_mesh,
    sphere_mesh, surface_mesh, torus_mesh,
};
pub use surface::{
    SurfaceCell, SurfaceCoordinates, SurfaceError, SurfaceGrid, SurfaceSample, UvSurfacePlan,
    MAX_SURFACE_CELLS, MAX_SURFACE_VERTICES,
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
