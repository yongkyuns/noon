//! Native counterpart of the smooth cone silhouette demo.
use super::cube_sketch_wireframe::{orbit_between, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke, WorldAffineEdit};

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    use std::f64::consts::{FRAC_PI_2, PI};
    orbit_between([FRAC_PI_2, -1.12], [PI, -1.12], |scene| {
        let mut cone = scene.cone_sketch_silhouette(
            [1.0, 2.5],
            SemanticVec3::new(0.0, 0.0, 1.0),
            96,
            Color::from_hex(0x236B8E),
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(0.85, 27).expect("valid cone sketch"),
        )?;
        cone.world_affine(WorldAffineEdit::Shift(SemanticVec3::new(0.0, 0.0, 1.25)))?;
        Ok(cone)
    })
}
