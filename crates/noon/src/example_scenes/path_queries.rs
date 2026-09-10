//! Paired path sampling example for native Rust, direct Rust/WASM and Python.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut rectangle = scene.rectangle(2., 1.)?;
        rectangle.set_scale(2., 1.)?;
        rectangle.rotate(0.25)?;
        rectangle.shift(-2.5, 0.)?;
        rectangle.disable_fill()?;
        rectangle.set_stroke_color(1., 1., 1., 1.)?;
        let mut ellipse = scene.circle(0.8)?;
        ellipse.set_scale(1.5, 1.)?;
        ellipse.shift(2.5, 0.)?;
        ellipse.disable_fill()?;
        ellipse.set_stroke_color(1., 1., 1., 1.)?;
        for shape in [&rectangle, &ellipse] {
            scene.add(shape)?;
            let query = shape.path_query()?;
            assert!(query.arc_length(None)? > 0.);
            for alpha in [0.125, 0.375, 0.625, 0.875] {
                let (x, y) = query.point_from_proportion(alpha)?;
                let mut marker = scene.circle(0.08)?;
                marker.set_fill(1., 204. / 255., 68. / 255., 1.)?;
                marker.set_stroke_width(0.)?;
                marker.set_translation(x, y)?;
                scene.add(&marker)?;
            }
        }
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let before = live.effective_path_query(&rectangle)?.start()?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        assert_eq!(live.effective_path_query(&rectangle)?.start()?, before);
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
