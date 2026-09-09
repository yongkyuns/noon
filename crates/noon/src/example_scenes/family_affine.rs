//! Paired with ordinary_family_affine.py; native and direct WASM use this session.
use crate::{ExecutionSession, ManimRotationPivot, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut a = scene.square(0.5)?;
    let mut b = scene.square(0.5)?;
    a.set_fill(0.2, 0.4, 1.0, 1.0)?;
    b.set_fill(1.0, 0.8, 0.1, 1.0)?;
    a.set_stroke_width(0.0)?;
    b.set_stroke_width(0.0)?;
    a.shift(-1.0, 0.0)?;
    b.shift(1.0, 0.0)?;
    let nested = scene.family(&[(&a).into(), (&b).into()])?;
    let family = scene.family(&[(&nested).into(), (&a).into()])?;
    family.scale(2.0, 1.0)?;
    scene.add_many(&[(&family).into()])?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
        live.rotate_family(
            &family,
            std::f64::consts::FRAC_PI_2,
            ManimRotationPivot::Center,
        )
        .map_err(|e| e.to_string())?;
        live.scale_family(&family, 0.5, 1.0)
            .map_err(|e| e.to_string())?;
        assert!(
            (live
                .effective(&a)
                .map_err(|e| e.to_string())?
                .transform
                .translation
                .y
                + 2.0)
                .abs()
                < 1e-6
        );
        assert!(
            (live
                .effective(&b)
                .map_err(|e| e.to_string())?
                .transform
                .translation
                .y
                - 2.0)
                .abs()
                < 1e-6
        );
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}
