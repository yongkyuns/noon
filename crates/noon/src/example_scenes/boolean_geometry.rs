//! Four filled-region operations share ordinary retained paths and rendering.
use crate::{BooleanOperation, ExecutionSession, ManimGeometryOptions, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut a = scene.circle(0.9)?;
        let mut b = scene.circle(0.9)?;
        a.shift(-0.4, 0.)?;
        b.shift(0.4, 0.)?;
        for (op, x, y, color) in [
            (BooleanOperation::Union, -2.5, 1.5, noon_core::BLUE),
            (BooleanOperation::Intersection, 2.5, 1.5, noon_core::GREEN),
            (BooleanOperation::Difference, -2.5, -1.5, noon_core::YELLOW),
            (BooleanOperation::Exclusion, 2.5, -1.5, noon_core::RED),
        ] {
            let mut options = ManimGeometryOptions::boolean_geometry(op, &[a.clone(), b.clone()])?;
            options.set_color(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                1.,
            )?;
            options.set_fill_opacity(0.7)?;
            options.set_stroke_width(2.)?;
            let mut result = scene.geometry(options)?;
            result.shift(x, y)?;
            scene.add(&result)?;
        }
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|e| e.to_string())
}
