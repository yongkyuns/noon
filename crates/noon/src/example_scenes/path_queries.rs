//! Paired path sampling example for native Rust, direct Rust/WASM and Python.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let precise = 1.0 + 2.0_f64.powi(-40);
        for count in 0..10 {
            let points = vec![(precise, 0.0); count];
            let value = scene.path(
                crate::VectorPath::from_manim_points_f64(points),
                Default::default(),
            )?;
            assert_eq!(value.width()?, 0.0);
            if count != 1 && !count.is_multiple_of(4) {
                assert!(value.path_query()?.anchors().is_err());
                assert!(value.center().is_err());
            } else {
                assert_eq!(
                    value.center()?,
                    (if count == 0 { 0.0 } else { precise }, 0.0)
                );
                assert_eq!(
                    value.path_query()?.anchors()?.len(),
                    if count == 1 { 1 } else { count / 2 }
                );
            }
        }

        let mut rectangle = scene.rectangle(2., 1.)?;
        rectangle.set_scale(2., 1.)?;
        rectangle.rotate(0.25)?;
        rectangle.shift(-2.5, 0.)?;
        let mut precise_points = rectangle.path_query()?.points()?;
        precise_points[0].0 += 2.0_f64.powi(-40);
        rectangle.set_points_f64(&precise_points)?;
        assert_eq!(rectangle.path_query()?.points()?, precise_points);
        assert_eq!(rectangle.path_query()?.start()?, precise_points[0]);
        assert_eq!(
            rectangle.path_query()?.point_from_proportion(0.0)?,
            precise_points[0]
        );

        rectangle.disable_fill()?;
        rectangle.set_stroke_color(1., 1., 1., 1.)?;
        let mut ellipse = scene.circle(0.8)?;
        ellipse.set_scale(1.5, 1.)?;
        ellipse.shift(2.5, 0.)?;
        ellipse.disable_fill()?;
        ellipse.set_stroke_color(1., 1., 1., 1.)?;
        let nested = scene.family(&[(&rectangle).into(), (&ellipse).into()])?;
        let vector_family = scene.family(&[(&nested).into(), (&rectangle).into()])?;
        let expected_boundary = ellipse
            .path_query()?
            .anchors()?
            .into_iter()
            .chain(rectangle.path_query()?.anchors()?)
            .collect::<Vec<_>>();
        assert_eq!(
            crate::MobjectTarget::from(&vector_family).vector_boundary_points()?,
            expected_boundary
        );
        assert_eq!(rectangle.get_num_points()?, 16);
        assert_eq!(ellipse.get_num_points()?, 32);
        let empty = scene.path(crate::VectorPath::new(), Default::default())?;
        assert_eq!(empty.get_num_points()?, 0);
        let line = scene.line((-1.0, 0.0), (1.0, 0.0))?;
        assert_eq!(line.get_num_points()?, 4);
        let group_child = scene.circle(1.0)?;
        let group = scene.family(&[(&group_child).into()])?;
        assert_eq!(crate::MobjectTarget::from(&group).get_num_points()?, 0);
        let image = scene.image(crate::ImageMobjectOptions::rgba8(1, 1, vec![0, 0, 0, 255])?)?;
        assert_eq!(image.get_num_points()?, 4);
        #[cfg(feature = "native-text")]
        {
            let text = scene.text(crate::Text::new("A"))?;
            assert_eq!(text.get_num_points()?, 0);
        }
        for shape in [&rectangle, &ellipse] {
            scene.add(shape)?;
            let query = shape.path_query()?;
            let ranges = (0..query.curve_count())
                .map(|index| (4 * index, 4 * index + 4))
                .collect::<Vec<_>>();
            assert!(query.arc_length(None)? > 0.);
            let default_pieces = query.curve_length_pieces(0, None)?;
            assert_eq!(default_pieces.len(), 9);
            assert!(
                (query.curve_length(0, None)? - default_pieces.iter().sum::<f64>()).abs() < 1e-10
            );
            assert_eq!(query.curve_length_pieces(0, Some(4))?.len(), 3);
            assert!(query.curve_length_pieces(0, Some(1))?.is_empty());
            assert_eq!(query.curve_length(0, Some(1))?, 0.0);
            for alpha in [0.125, 0.375, 0.625, 0.875] {
                let (x, y) = query.point_from_proportion(alpha)?;
                assert!(
                    (query.proportion_from_point((x, y, 0.0))?.unwrap() - alpha).abs() < 1.0e-5
                );
                assert!(
                    (query
                        .proportion_from_point_in_ranges(&ranges, (x, y, 0.0))?
                        .unwrap()
                        - alpha)
                        .abs()
                        < 1.0e-5
                );
                let mut marker = scene.circle(0.08)?;
                marker.set_fill(1., 204. / 255., 68. / 255., 1.)?;
                marker.set_stroke_width(0.)?;
                marker.set_translation(x, y)?;
                scene.add(&marker)?;
            }
        }
        assert_eq!(
            rectangle
                .path_query()?
                .proportion_from_point((0.0, 0.0, 0.0))?,
            None
        );
        let loop_path = scene.path(
            crate::VectorPath::new()
                .move_to(crate::Vec2::ZERO)
                .cubic_to(
                    crate::Vec2::new(1.0, 1.0),
                    crate::Vec2::new(-1.0, 1.0),
                    crate::Vec2::ZERO,
                ),
            Default::default(),
        )?;
        assert_eq!(
            loop_path
                .path_query()?
                .proportion_from_point((0.0, 0.0, 0.0))?,
            Some(1.0)
        );
        let mut endpoint_path = scene.path(
            crate::VectorPath::new()
                .move_to(crate::Vec2::new(-3.0, -1.0))
                .cubic_to(
                    crate::Vec2::new(-1.0, 2.0),
                    crate::Vec2::new(1.0, -2.0),
                    crate::Vec2::new(3.0, 1.0),
                ),
            Default::default(),
        )?;
        endpoint_path.rotate(0.1)?;
        let endpoint_query = endpoint_path.path_query()?;
        let endpoint = endpoint_query.end()?;
        assert_eq!(
            endpoint_query.proportion_from_point((endpoint.0, endpoint.1, 0.0))?,
            Some(1.0)
        );
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
        let expected_boundary = ellipse
            .path_query()?
            .anchors()?
            .into_iter()
            .chain(captured.anchors()?)
            .collect::<Vec<_>>();
        assert_eq!(
            crate::integration::effective_vector_boundary_points(
                scene.integration_store(),
                &session,
                (&vector_family).into(),
            )?,
            expected_boundary
        );
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
