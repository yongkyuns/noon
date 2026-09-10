use noon::{Mobject, Scene};
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let store = Rc::clone(scene.integration_store());

    let mut arc = Mobject::manim_arc(store.clone(), 1.25, -0.3, 1.8, 9, -2.0, 0.8)?;
    arc.set_color(88.0 / 255.0, 196.0 / 255.0, 221.0 / 255.0, 1.0)?;

    let mut between = Mobject::manim_arc_between_points(
        store.clone(),
        -0.5,
        -1.5,
        2.5,
        1.0,
        std::f64::consts::FRAC_PI_2,
        None,
        9,
    )?;
    between.set_color(247.0 / 255.0, 217.0 / 255.0, 111.0 / 255.0, 1.0)?;

    let mut negative_radius = Mobject::manim_arc_between_points(
        store,
        0.5,
        -2.0,
        3.0,
        -2.0,
        0.1,
        Some(-2.0),
        9,
    )?;
    negative_radius.set_color(252.0 / 255.0, 98.0 / 255.0, 85.0 / 255.0, 1.0)?;

    scene.add_many(&[
        (&arc).into(),
        (&between).into(),
        (&negative_radius).into(),
    ])?;
    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
    }
    noon_native::run(session)?;
    Ok(())
}
