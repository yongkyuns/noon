//! Paired with ordinary_family_replacement.py on native and direct Rust/WASM.
use crate::{ExecutionSession, LayoutAnchor, LayoutDimension::Width, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut first = scene.rectangle(2.0, 1.0).map_err(|e| e.to_string())?;
    let mut second = scene.square(1.0).map_err(|e| e.to_string())?;
    let mut target = scene.rectangle(4.0, 2.0).map_err(|e| e.to_string())?;
    for (object, color) in [
        (&mut first, (68.0 / 255.0, 136.0 / 255.0, 1.0)),
        (&mut second, (1.0, 204.0 / 255.0, 68.0 / 255.0)),
        (&mut target, (1.0, 68.0 / 255.0, 102.0 / 255.0)),
    ] {
        object
            .set_fill(color.0, color.1, color.2, 1.0)
            .map_err(|e| e.to_string())?;
        object.set_stroke_width(0.0).map_err(|e| e.to_string())?;
    }
    first.shift(-2.0, 0.0).map_err(|e| e.to_string())?;
    second.shift(2.0, 0.0).map_err(|e| e.to_string())?;
    target.shift(0.0, -2.0).map_err(|e| e.to_string())?;
    let nested = scene
        .family(&[(&first).into(), (&second).into()])
        .map_err(|e| e.to_string())?;
    let family = scene
        .family(&[(&first).into(), (&nested).into()])
        .map_err(|e| e.to_string())?;
    LayoutAnchor::from(&family)
        .replace_layout(&(&target).into(), Width, false)
        .map_err(|e| e.to_string())?;
    scene
        .add_many(&[(&family).into(), (&target).into()])
        .map_err(|e| e.to_string())?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        live.replace_layout(&(&family).into(), &(&target).into(), Width, true)
            .map_err(|e| e.to_string())?;
        live.shift_family(&family, 0.0, 4.0)
            .map_err(|e| e.to_string())?;
        let wait = live.wait_segment(0.2).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}
