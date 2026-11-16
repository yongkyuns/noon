use noon::{ArrowVectorFieldOptions, Scene, PURPLE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut field = ArrowVectorFieldOptions::new(None, None, false)?;
    field.set_field_color(
        PURPLE.red.into(),
        PURPLE.green.into(),
        PURPLE.blue.into(),
        1.0,
    )?;
    for index in 0..field.sample_count() {
        field.set_vector(index, 0.25, 0.0)?;
    }

    let field = scene.manim_arrow_vector_field(field)?;
    scene.add_many(&[field.family().into()])?;
    noon_native::run(scene.into_execution()?)?;
    Ok(())
}
