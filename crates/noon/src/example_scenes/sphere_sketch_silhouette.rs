//! Native counterpart of the Python smooth sphere silhouette.
use super::cube_sketch_wireframe::{orbit, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke, WorldAffineEdit};

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit(|scene| {
        let mut sphere = scene.sphere_sketch_silhouette(
            1.35,
            [64, 32],
            Color::from_hex(0x236B8E),
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(1.15, 27).expect("valid sphere sketch"),
        )?;
        sphere.world_affine(WorldAffineEdit::Shift(SemanticVec3::new(0.8, -0.45, 0.1)))?;
        Ok(sphere)
    })
}
