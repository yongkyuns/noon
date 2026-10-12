//! Paired polygon constructors on the ordinary native/direct-WASM path.
use crate::{LiveProgram, ManimGeometryOptions, Scene};

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

pub fn program(
) -> Result<LiveProgram<crate::example_scenes::static_hold::Hold>, Box<dyn std::error::Error>> {
    Ok(crate::example_scenes::static_hold::program(scene()?, 0.2)?)
}
