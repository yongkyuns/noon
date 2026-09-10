//! Paired with ordinary_family_membership_order.py on native and direct WASM.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<ExecutionSession, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut a = scene.square(2.0)?;
        let mut b = scene.square(2.0)?;
        a.set_fill(1.0, 0.0, 0.0, 1.0)?;
        a.set_stroke_width(0.0)?;
        b.set_fill(0.0, 0.0, 1.0, 1.0)?;
        b.set_stroke_width(0.0)?;
        b.shift(1.0, 0.0)?;
        let family = scene.family(&[(&a).into(), (&b).into(), (&a).into()])?;
        family.add((&b).into())?;
        scene.add_many(&[(&family).into()])?;
        let mut session = scene.execution_session()?;
        {
            let mut live = scene.live(&mut session);
            live.add_family_members(&family, &[(&a).into()])?;
            let wait = live.wait_segment(0.2)?;
            live.advance_segment_to(wait, wait.end_time())?;
            live.complete_segment(wait)?;
        }
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
