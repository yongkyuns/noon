use noon::{ManimArrowOptions, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let mut arrow_options = ManimArrowOptions::arrow(-3.0, 1.5, -0.5, 1.5)?;
    arrow_options.set_buff(0.2)?;
    arrow_options.set_color(88.0 / 255.0, 196.0 / 255.0, 221.0 / 255.0, 1.0)?;
    let arrow = scene.manim_arrow(arrow_options)?;
    arrow.scale(0.65, false)?;

    let mut vector_options = ManimArrowOptions::vector(2.0, 1.0)?;
    vector_options.set_translation(0.0, -0.5)?;
    vector_options.set_color(247.0 / 255.0, 217.0 / 255.0, 111.0 / 255.0, 1.0)?;
    let vector = scene.manim_arrow(vector_options)?;
    vector.scale(0.65, true)?;

    let mut short_options = ManimArrowOptions::arrow(-0.2, 0.25, 0.2, 0.25)?;
    short_options.set_buff(0.0)?;
    short_options.set_color(131.0 / 255.0, 193.0 / 255.0, 103.0 / 255.0, 1.0)?;
    let short = scene.manim_arrow(short_options)?;
    short.scale(3.0, false)?;

    let mut double_options = ManimArrowOptions::double_arrow(-2.5, -1.5, 2.5, -1.5)?;
    double_options.set_buff(0.15)?;
    double_options.set_color(252.0 / 255.0, 98.0 / 255.0, 85.0 / 255.0, 1.0)?;
    let double = scene.manim_arrow(double_options)?;
    double.scale(0.65, false)?;

    scene.add_many(&[
        arrow.family().into(),
        vector.family().into(),
        short.family().into(),
        double.family().into(),
    ])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
