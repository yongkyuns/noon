//! Native counterpart of the Python Line3D sketch silhouette.
use super::cube_sketch_wireframe::{orbit_between, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke};

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    let end_phi = 0.6_f64.atan2(0.8);
    orbit_between(
        [end_phi + std::f64::consts::FRAC_PI_2, 0.0],
        [end_phi, 0.0],
        |scene| {
            scene.line_3d_sketch_silhouette(
                [
                    SemanticVec3::new(-0.9, 0.0, -1.2),
                    SemanticVec3::new(0.9, 0.0, 1.2),
                ],
                0.62,
                96,
                Color::from_hex(0x236B8E),
                Color::from_hex(0xACEAD7),
                0.05,
                SketchStroke::new(0.85, 27).expect("valid tube sketch"),
            )
        },
    )
}
