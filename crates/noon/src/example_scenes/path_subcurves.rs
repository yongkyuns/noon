//! Detached interval copies and coherent subpath observations on the shared scene.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut source = scene.square(2.)?;
        source.shift(-2., 0.)?;
        source.set_stroke_color(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.,
        )?;
        scene.add(&source)?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let selected = live.subcurve(&source, 0.875, 0.375)?;
        live.shift(&selected, 4., 0.)?;
        live.set_stroke_color(
            &selected,
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            1.,
        )?;
        let query = selected.path_query()?;
        assert_eq!(query.subpaths().len(), 1);
        assert!(!query.is_closed()?);
        live.add(&selected)?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
