//! Shared path-aware arrange/grid example for Rust native, direct WASM and Python.
use crate::{ExecutionSession, FamilyLayoutTarget, ManimGeometryOptions, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut a = scene.geometry(ManimGeometryOptions::arc(1., -0.3, 1.8, 3, 0., 0.)?)?;
        a.set_color(1., 1., 1., 1.)?;
        let mut b = a.copy_handle()?;
        b.set_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        let c = a.copy_handle()?;
        let d = b.copy_handle()?;
        let row = scene.family(&[(&a).into(), (&b).into()])?;
        let grid = scene.family(&[(&c).into(), (&d).into()])?;
        row.arrange(1., 0., 0.4, true)?;
        row.layout()?
            .move_to(FamilyLayoutTarget::Point(-2., 0.), (0., 0.), (1., 1.))?;
        grid.arrange_in_grid(Some(2), Some(1), 0.4, 0.4)?;
        grid.layout()?
            .move_to(FamilyLayoutTarget::Point(2., 0.), (0., 0.), (1., 1.))?;
        scene.add_many(&[(&row).into(), (&grid).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
