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
        let segment = scene.live(&mut session).play_animation(&animation)?;
        scene.live(&mut session).advance_segment_to(segment, 0.5)?;
        let captured = crate::integration::effective_path_query(
            scene.integration_store(),
            &session,
            &rectangle,
        )?;
        let midpoint = captured.start()?;
        assert!((midpoint.0 - (start.0 + end.0) * 0.5).abs() < 2e-6);
        assert!((midpoint.1 - (start.1 + end.1) * 0.5).abs() < 2e-6);
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time())?;
        scene.live(&mut session).complete_segment(segment)?;
        assert_eq!(captured.start()?, midpoint);
        let before = crate::integration::effective_path_query(
            scene.integration_store(),
            &session,
            &rectangle,
        )?
        .start()?;
        let wait = scene.live(&mut session).wait_segment(0.2)?;
        scene
            .live(&mut session)
            .advance_segment_to(wait, wait.end_time())?;
        scene.live(&mut session).complete_segment(wait)?;
        assert_eq!(
            crate::integration::effective_path_query(
                scene.integration_store(),
                &session,
                &rectangle
            )?
            .start()?,
            before
        );
        scene.live(&mut session).remove(&ellipse)?;
        let reveal = scene.live(&mut session).declare_and_activate_create(
            &ellipse,
            crate::AnimationOptions::new()
                .run_time(1.)
                .rate_func(crate::RateFunction::Linear),
        )?;
        scene
            .live(&mut session)
            .advance_segment_to(reveal, reveal.end_time() - 0.5)?;
        let endpoint = crate::integration::effective_path_query(
            scene.integration_store(),
            &session,
            &ellipse,
        )?
        .end()?;
        assert!((endpoint.0 - 1.3).abs() < 2e-6 && endpoint.1.abs() < 2e-6);
        scene
            .live(&mut session)
            .advance_segment_to(reveal, reveal.end_time())?;
        scene.live(&mut session).complete_segment(reveal)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
