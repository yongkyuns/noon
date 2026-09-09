//! Paired with ordinary_family_paint.py on the shared native/direct WASM engine.
use crate::{Color, ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut a = scene.square(0.6)?;
    let mut b = scene.square(0.6)?;
    a.shift(-1.0, 0.0)?;
    b.shift(1.0, 0.0)?;
    let nested = scene.family(&[(&a).into(), (&b).into()])?;
    let family = scene.family(&[(&nested).into(), (&a).into()])?;
    family.set_fill(Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), Some(0.8))?;
    family.set_stroke(Some(Color::rgba(0.0, 0.0, 1.0, 1.0)), Some(0.04), Some(1.0))?;
    scene.add_many(&[(&family).into()])?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
        live.set_family_color(&family, 0.2, 0.6, 1.0, 1.0)
            .map_err(|e| e.to_string())?;
        live.set_family_opacity(&family, 0.5)
            .map_err(|e| e.to_string())?;
        for object in [&a, &b] {
            let effective = live.effective(object).map_err(|e| e.to_string())?;
            assert_eq!(effective.fill_opacity(), 0.5);
            assert_eq!(effective.stroke_opacity(), 0.5);
        }
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}
