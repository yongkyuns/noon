//! Native counterpart of the smooth torus silhouette demo.
use super::cube_sketch_wireframe::{orbit_between, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SketchStroke};
use std::f64::consts::FRAC_PI_2;

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit_between([0.0, -1.12], [FRAC_PI_2, -1.12], |scene| {
        scene.torus_sketch_silhouette(
            [1.5, 0.62],
            [128, 48],
            Color::from_hex(0x236B8E),
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(0.85, 27).expect("valid torus sketch"),
        )
    })
}
