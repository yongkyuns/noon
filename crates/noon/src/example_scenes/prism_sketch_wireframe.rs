//! Native counterpart of the Python prism sketch wireframe.
use super::cube_sketch_wireframe::{orbit, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke};

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit(|scene| {
        scene.prism_sketch_wireframe(
            SemanticVec3::new(3.5, 1.75, 1.25),
            Color::from_hex(0x236B8E),
            1.0,
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(1.15, 27).expect("valid prism sketch"),
        )
    })
}
