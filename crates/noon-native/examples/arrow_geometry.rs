use noon::{ManimArrowOptions, MobjectFamily, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let mut arrow_options = ManimArrowOptions::arrow(-3.0, 1.5, -0.5, 1.5)?;
    arrow_options.set_buff(0.2)?;
    let arrow = MobjectFamily::manim_arrow(scene.integration_store(), arrow_options)?;

    let mut vector = MobjectFamily::manim_vector(
        scene.integration_store(),
        ManimArrowOptions::vector(2.0, 1.0)?,
    )?;
    vector.shift(0.0, -0.5)?;

    let mut double_options = ManimArrowOptions::double_arrow(-2.5, -1.5, 2.5, -1.5)?;
    double_options.set_buff(0.15)?;
    let double = MobjectFamily::manim_double_arrow(scene.integration_store(), double_options)?;

    scene.add_many(&[
        arrow.family().into(),
        vector.family().into(),
        double.family().into(),
    ])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
