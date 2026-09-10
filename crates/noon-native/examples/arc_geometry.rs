use noon::{Mobject, Scene};
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let store = Rc::clone(scene.integration_store());

    let mut arc = Mobject::manim_arc(store.clone(), 1.25, -0.3, 1.8, 9, -2.0, 0.8)?;
    arc.set_stroke_color(0.345, 0.769, 0.867, 1.0)?;
    arc.set_stroke_width(0.06)?;

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
    between.set_stroke_color(0.969, 0.851, 0.435, 1.0)?;
    between.set_stroke_width(0.06)?;

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
    negative_radius.set_stroke_color(0.988, 0.384, 0.333, 1.0)?;
    negative_radius.set_stroke_width(0.06)?;

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
