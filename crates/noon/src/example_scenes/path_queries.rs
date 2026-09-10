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
        let target = rectangle.target_editor()?;
        crate::LayoutAnchor::from(&target).stretch(
            1.2,
            crate::LayoutDimension::Width,
            crate::ManimRotationPivot::Center,
        )?;
        let start = rectangle.path_query()?.start()?;
        let end = target.path_query()?.start()?;
        let animation = scene.declare_transform_to(
            &rectangle,
            &target,
            crate::AnimationOptions::new()
                .run_time(1.)
                .rate_func(crate::RateFunction::Linear),
        )?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation)?;
        live.advance_segment_to(segment, 0.5)?;
        let captured = live.effective_path_query(&rectangle)?;
        let midpoint = captured.start()?;
        assert!((midpoint.0 - (start.0 + end.0) * 0.5).abs() < 2e-6);
        assert!((midpoint.1 - (start.1 + end.1) * 0.5).abs() < 2e-6);
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
        assert_eq!(captured.start()?, midpoint);
        let before = live.effective_path_query(&rectangle)?.start()?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        assert_eq!(live.effective_path_query(&rectangle)?.start()?, before);
        let reveal = live.declare_and_activate_create(
            &ellipse,
            crate::AnimationOptions::new()
                .run_time(1.)
                .rate_func(crate::RateFunction::Linear),
        )?;
        live.advance_segment_to(reveal, reveal.end_time() - 0.5)?;
        let endpoint = live.effective_path_query(&ellipse)?.end()?;
        assert!((endpoint.0 - 1.3).abs() < 2e-6 && endpoint.1.abs() < 2e-6);
        live.advance_segment_to(reveal, reveal.end_time())?;
        live.complete_segment(reveal)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
