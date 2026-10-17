//! Paired with ordinary_family_paint.py on the shared native/direct WASM engine.
use crate::{Color, ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut a = scene.square(0.6).map_err(|error| error.to_string())?;
    let mut b = scene.square(0.6).map_err(|error| error.to_string())?;
    a.shift(-1.0, 0.0).map_err(|error| error.to_string())?;
    b.shift(1.0, 0.0).map_err(|error| error.to_string())?;
    let nested = scene
        .family(&[(&a).into(), (&b).into()])
        .map_err(|error| error.to_string())?;
    let family = scene
        .family(&[(&nested).into(), (&a).into()])
        .map_err(|error| error.to_string())?;
    family
        .set_fill(Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), Some(0.8))
        .map_err(|error| error.to_string())?;
    family
        .set_stroke(Some(Color::rgba(0.0, 0.0, 1.0, 1.0)), Some(0.04), Some(1.0))
        .map_err(|error| error.to_string())?;
    a.fade(0.5).map_err(|error| error.to_string())?;
    assert_eq!(a.fill_opacity().map_err(|e| e.to_string())?, 0.4);
    scene
        .add_many(&[(&family).into()])
        .map_err(|error| error.to_string())?;
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
        live.fade(&a, 0.5).map_err(|e| e.to_string())?;
        live.fade_family(&family, 0.5).map_err(|e| e.to_string())?;
        for (object, opacity) in [(&a, 0.0625), (&b, 0.25)] {
            let effective = live.effective(object).map_err(|e| e.to_string())?;
            assert_eq!(effective.fill_opacity(), opacity);
            assert_eq!(effective.stroke_opacity(), opacity);
        }
        live.fade_to_family(&family, Color::rgb(1.0, 0.0, 0.0), 0.5)
            .map_err(|e| e.to_string())?;
        for (object, color, opacity) in [
            (&a, Color::rgba(0.8, 0.15, 0.25, 0.0625), 0.0625),
            (&b, Color::rgba(0.6, 0.3, 0.5, 0.25), 0.25),
        ] {
            let effective = live.effective(object).map_err(|e| e.to_string())?;
            assert_eq!(effective.style.fill, Some(color));
            assert_eq!(effective.fill_opacity(), opacity);
            assert_eq!(effective.style.stroke, Some(color));
            assert_eq!(effective.stroke_opacity(), opacity);
        }
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fade_and_fade_to_fixture_matches_recursive_family_expectations() {
        super::session().unwrap();
    }
}
