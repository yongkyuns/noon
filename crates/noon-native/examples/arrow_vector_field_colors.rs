use noon::{
    ManimArrowVectorField, Scene, VectorFieldAxisRange, VectorFieldPoint, VectorFieldRanges2D,
    BLUE, PINK, RED, YELLOW,
};
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let colors = [RED, YELLOW, BLUE];
    let ranges = VectorFieldRanges2D::new(
        VectorFieldAxisRange::new(-2.0, 2.0, 1.0),
        VectorFieldAxisRange::new(-1.0, 1.0, 1.0),
    );
    let field = ManimArrowVectorField::create_with_color_scheme(
        Rc::clone(scene.integration_store()),
        |point| VectorFieldPoint::new(-point.y, point.x),
        ranges,
        &colors,
        -2.0,
        2.0,
        |raw| raw.x + raw.y,
    )?;

    scene.add_many(&[field.family().into()])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}

#[allow(dead_code)]
fn single_color_example(
    scene: &Scene,
) -> Result<ManimArrowVectorField, Box<dyn std::error::Error>> {
    let ranges = VectorFieldRanges2D::new(
        VectorFieldAxisRange::new(-1.0, 1.0, 1.0),
        VectorFieldAxisRange::new(-1.0, 1.0, 1.0),
    );
    Ok(ManimArrowVectorField::create_with_color(
        Rc::clone(scene.integration_store()),
        |point| VectorFieldPoint::new(-point.y, point.x),
        ranges,
        PINK,
    )?)
}
