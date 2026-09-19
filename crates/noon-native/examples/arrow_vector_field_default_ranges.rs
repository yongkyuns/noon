use noon::{
    manim_default_vector_field_ranges_2d, ManimArrowVectorField, MobjectFamilyMember, Scene,
    VectorFieldPoint, PURPLE,
};
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let field = ManimArrowVectorField::create_with_color(
        Rc::clone(scene.integration_store()),
        |_| VectorFieldPoint::new(0.25, 0.0),
        manim_default_vector_field_ranges_2d(),
        PURPLE,
    )?;

    scene.add_many(&[MobjectFamilyMember::Family(field.family())])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
