//! Native counterpart of the smooth cylinder silhouette demo.
use super::cube_sketch_wireframe::{orbit_between, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke};

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit_between([1.54, -1.12], [0.035, -1.12], |scene| {
        scene.cylinder_sketch_silhouette(
            [1.0, 2.5],
            SemanticVec3::new(0.0, 0.0, 1.0),
            96,
            Color::from_hex(0x236B8E),
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(0.85, 27).expect("valid cylinder sketch"),
        )
    })
}
