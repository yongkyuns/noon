//! Paired with web/python/examples/ordinary_arc_geometry.py and the Manim parity fixture.
use crate::{ExecutionSession, Mobject, Scene};
use std::rc::Rc;

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let store = Rc::clone(scene.integration_store());

    let mut arc = Mobject::manim_arc(store.clone(), 1.25, -0.3, 1.8, 9, -2.0, 0.8)
        .map_err(|error| error.to_string())?;
    arc.set_stroke_color(0.345, 0.769, 0.867, 1.0)
        .map_err(|error| error.to_string())?;
    arc.set_stroke_width(0.06)
        .map_err(|error| error.to_string())?;

    let mut between = Mobject::manim_arc_between_points(
        store.clone(),
        -0.5,
        -1.5,
        2.5,
        1.0,
        std::f64::consts::FRAC_PI_2,
        None,
        9,
    )
    .map_err(|error| error.to_string())?;
    between
        .set_stroke_color(0.969, 0.851, 0.435, 1.0)
        .map_err(|error| error.to_string())?;
    between
        .set_stroke_width(0.06)
        .map_err(|error| error.to_string())?;

    let mut negative_radius = Mobject::manim_arc_between_points(
        store,
        0.5,
        -2.0,
        3.0,
        -2.0,
        0.1,
        Some(-2.0),
        9,
    )
    .map_err(|error| error.to_string())?;
    negative_radius
        .set_stroke_color(0.988, 0.384, 0.333, 1.0)
        .map_err(|error| error.to_string())?;
    negative_radius
        .set_stroke_width(0.06)
        .map_err(|error| error.to_string())?;

    scene
        .add_many(&[(&arc).into(), (&between).into(), (&negative_radius).into()])
        .map_err(|error| error.to_string())?;
    let mut session = scene.execution_session().map_err(|error| error.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2).map_err(|error| error.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|error| error.to_string())?;
        live.complete_segment(wait)
            .map_err(|error| error.to_string())?;
    }
    Ok(session)
}
