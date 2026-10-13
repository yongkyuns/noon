//! Paired polygon constructors on the ordinary native/direct-WASM path.
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, ManimGeometryOptions, Scene,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let requests = [
        (
            ManimGeometryOptions::polygon(&[(-1.0, -0.5), (1.0, -0.5), (0.0, 1.0)])?,
            -4.0,
            1.5,
        ),
        (
            ManimGeometryOptions::regular_polygon(6, 1.0, None)?,
            -1.5,
            1.5,
        ),
        (
            ManimGeometryOptions::regular_polygon(5, 1.0, None)?,
            1.0,
            1.5,
        ),
        (
            ManimGeometryOptions::star(5, 1.0, None, 2, Some(std::f64::consts::FRAC_PI_2))?,
            3.5,
            1.5,
        ),
        (
            ManimGeometryOptions::regular_polygram(6, 2, 1.0, None)?,
            -2.0,
            -1.5,
        ),
        (
            ManimGeometryOptions::star(7, 1.0, Some(0.6), 3, Some(0.3))?,
            2.0,
            -1.5,
        ),
    ];
    for (mut options, x, y) in requests {
        options.set_translation(x, y)?;
        options.set_fill_opacity(0.2)?;
        let object = scene.geometry(options)?;
        scene.add(&object)?;
    }
    Ok(scene)
}

pub struct PolygonPlayback(bool);
impl LiveContinuation for PolygonPlayback {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.0 {
            return Ok(ContinuationStep::Finished);
        }
        self.0 = true;
        Ok(ContinuationStep::Await(
            live.wait_segment(0.2).map_err(|e| e.to_string())?,
        ))
    }
}

pub fn program() -> Result<LiveProgram<PolygonPlayback>, Box<dyn std::error::Error>> {
    Ok(scene()?.into_live_program(PolygonPlayback(false))?)
}
