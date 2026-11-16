use noon::{ArrowVectorFieldOptions, Scene, VectorFieldAxisRange, BLUE, RED, YELLOW};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut field = ArrowVectorFieldOptions::new(
        Some(VectorFieldAxisRange::new(-2.0, 2.0, 1.0)),
        Some(VectorFieldAxisRange::new(-1.0, 1.0, 1.0)),
        false,
    )?;
    field.set_color_gradient(-2.0, 2.0, true);
    for color in [RED, YELLOW, BLUE] {
        field.add_color_gradient_stop(
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            1.0,
        )?;
    }
    for index in 0..field.sample_count() {
        let point = field.sample_point(index)?;
        field.set_vector(index, -point.y, point.x)?;
        field.set_color_value(index, -point.y + point.x)?;
    }

    let field = scene.manim_arrow_vector_field(field)?;
    scene.add_many(&[field.family().into()])?;
    noon_native::run(scene.into_execution()?)?;
    Ok(())
}
