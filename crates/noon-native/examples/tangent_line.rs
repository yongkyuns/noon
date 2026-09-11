use noon::{ManimGeometryOptions, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let circle = scene.geometry(ManimGeometryOptions::circle(2.0)?)?;

    let mut first_options = circle.manim_tangent_line_options(0.0, 4.0, 1.0e-6)?;
    first_options.set_color(41.0 / 255.0, 171.0 / 255.0, 202.0 / 255.0, 1.0)?;
    let first = scene.geometry(first_options)?;

    let mut second_options = circle.manim_tangent_line_options(0.4, 4.0, 1.0e-6)?;
    second_options.set_color(131.0 / 255.0, 193.0 / 255.0, 103.0 / 255.0, 1.0)?;
    let second = scene.geometry(second_options)?;

    scene.add_many(&[(&circle).into(), (&first).into(), (&second).into()])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
