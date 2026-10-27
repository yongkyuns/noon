use super::*;

#[test]
fn independent_literal_temporal_vectors_match_shared_rust_sampling() {
    let numbers = |field: &str| {
        field
            .split(',')
            .map(|v| v.parse::<f64>().unwrap())
            .collect::<Vec<_>>()
    };
    let mut count = 0;
    for line in
        include_str!("../../../../../tests/visual-effects/paint-transition-vectors.txt").lines()
    {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('|').collect::<Vec<_>>();
        let definition = |axis: &str, stops: &str| {
            let axis = numbers(axis);
            let stops = stops.split(';').map(|stop| {
                let c = numbers(stop);
                GradientStop::new(
                    c[0],
                    Color::rgba(c[1] as f32, c[2] as f32, c[3] as f32, c[4] as f32),
                )
                .unwrap()
            });
            Some(SemanticPaint::LinearGradient(Arc::new(
                LinearGradient::new([axis[0], axis[1]], [axis[2], axis[3]], stops).unwrap(),
            )))
        };
        let opacities = numbers(fields[7]);
        let transition = PaintTransition::new(
            definition(fields[1], fields[3]),
            opacities[0],
            definition(fields[2], fields[4]),
            opacities[1],
        )
        .unwrap();
        let (Some(SemanticPaint::LinearGradient(paint)), opacity) =
            transition.sample(fields[5].parse().unwrap()).unwrap()
        else {
            panic!("gradient")
        };
        let point = numbers(fields[6]);
        let actual = paint
            .sample_premultiplied([point[0], point[1]])
            .unwrap()
            .map(|v| v * opacity);
        for (actual, expected) in actual.into_iter().zip(numbers(fields[8])) {
            assert!(
                (actual - expected).abs() <= 3e-8,
                "{}: {actual} != {expected}",
                fields[0]
            );
        }
        count += 1;
    }
    assert_eq!(count, 12);
}

fn gradient(start: [f64; 2], end: [f64; 2], colors: &[(f64, Color)]) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            start,
            end,
            colors
                .iter()
                .map(|&(p, c)| GradientStop::new(p, c).unwrap()),
        )
        .unwrap(),
    )
}
fn horizontal(color: Color) -> Arc<LinearGradient> {
    gradient([0.0, 0.5], [1.0, 0.5], &[(0.0, color), (1.0, color)])
}
fn sample_gradient(transition: &PaintTransition, alpha: f64) -> Arc<LinearGradient> {
    let (Some(SemanticPaint::LinearGradient(gradient)), _) = transition.sample(alpha).unwrap()
    else {
        panic!("interior continuous paint");
    };
    gradient
}

#[test]
fn exact_endpoints_keep_paint_kind_identity_and_full_opacity_precision() {
    let gradient = horizontal(Color::rgb(0.0, 0.0, 1.0));
    let start = Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0)));
    let end = Some(SemanticPaint::LinearGradient(gradient.clone()));
    let transition = PaintTransition::new(
        start.clone(),
        0.123456789012345,
        end.clone(),
        0.875000000000001,
    )
    .unwrap();
    assert_eq!(transition.sample(0.0).unwrap(), (start, 0.123456789012345));
    assert_eq!(transition.sample(1.0).unwrap(), (end, 0.875000000000001));
    assert!(Arc::ptr_eq(&sample_gradient(&transition, 1.0), &gradient));
}

#[test]
fn temporal_premultiplication_excludes_invisible_rgb_and_keeps_opacity_separate() {
    let from = horizontal(Color::rgba(1.0, 0.0, 0.0, 0.0));
    let transition = PaintTransition::new(
        Some(SemanticPaint::LinearGradient(from)),
        0.25,
        Some(SemanticPaint::Solid(Color::rgb(0.0, 0.0, 1.0))),
        0.75,
    )
    .unwrap();
    let (paint, opacity) = transition.sample(0.5).unwrap();
    let Some(SemanticPaint::LinearGradient(paint)) = paint else {
        panic!("gradient")
    };
    assert_eq!(opacity, 0.5);
    assert_eq!(
        paint.sample_premultiplied([0.25, 0.25]).unwrap(),
        [0.0, 0.0, 0.5, 0.5]
    );
    assert_eq!(paint.stops()[0].color(), Color::rgba(0.0, 0.0, 1.0, 0.5));
}

#[test]
fn absent_paint_fades_through_the_gradient_without_changing_exact_endpoint_enablement() {
    let to = horizontal(Color::rgb(0.0, 1.0, 0.0));
    let transition = PaintTransition::new(
        None,
        1.0,
        Some(SemanticPaint::LinearGradient(to.clone())),
        1.0,
    )
    .unwrap();
    assert_eq!(transition.sample(0.0).unwrap(), (None, 1.0));
    assert_eq!(
        sample_gradient(&transition, 0.5)
            .sample_premultiplied([0.3, 0.8])
            .unwrap(),
        [0.0, 0.5, 0.0, 0.5]
    );
    assert!(Arc::ptr_eq(&sample_gradient(&transition, 1.0), &to));
}

#[test]
fn coincident_hard_stops_move_continuously_and_remain_right_continuous() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let make = |position| {
        gradient(
            [0.0, 0.5],
            [1.0, 0.5],
            &[(0.0, red), (position, red), (position, blue), (1.0, blue)],
        )
    };
    let transition = PaintTransition::new(
        Some(SemanticPaint::LinearGradient(make(0.25))),
        1.0,
        Some(SemanticPaint::LinearGradient(make(0.75))),
        1.0,
    )
    .unwrap();
    let middle = sample_gradient(&transition, 0.5);
    assert_eq!(middle.stops()[1].position(), 0.5);
    assert_eq!(middle.stops()[2].position(), 0.5);
    assert_eq!(
        middle.sample_premultiplied([0.49, 0.5]).unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        middle.sample_premultiplied([0.5, 0.5]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn axis_collapse_away_from_midpoint_is_rejected_before_sampling() {
    let from = gradient(
        [0.0, 0.5],
        [1.0, 0.5],
        &[(0.0, Color::WHITE), (1.0, Color::BLACK)],
    );
    let to = gradient(
        [0.5, 0.5],
        [0.25, 0.5],
        &[(0.0, Color::WHITE), (1.0, Color::BLACK)],
    );
    // Axis lengths 1 -> -1/4 cancel at alpha=4/5, not at the midpoint.
    assert_eq!(
        PaintTransition::new(
            Some(SemanticPaint::LinearGradient(from)),
            1.0,
            Some(SemanticPaint::LinearGradient(to)),
            1.0
        )
        .unwrap_err(),
        PaintTransitionError::DegenerateAxis
    );
}

#[test]
fn valid_axis_rotation_and_reverse_sample_order_are_deterministic() {
    let from = horizontal(Color::rgb(1.0, 0.0, 0.0));
    let to = gradient(
        [0.5, 0.0],
        [0.5, 1.0],
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    );
    let transition = PaintTransition::new(
        Some(SemanticPaint::LinearGradient(from)),
        0.25,
        Some(SemanticPaint::LinearGradient(to)),
        0.75,
    )
    .unwrap();
    let expected = transition.sample(0.375).unwrap();
    for alpha in [0.0, 1.0, 0.9, 0.1, 0.375] {
        assert!(sample_gradient(&transition, alpha)
            .sample_premultiplied([0.25, 0.25])
            .unwrap()
            .into_iter()
            .all(f64::is_finite));
    }
    assert_eq!(transition.sample(0.375).unwrap(), expected);
}

#[test]
fn unchanged_definition_reuses_stop_storage_for_opacity_only_animation() {
    let from = horizontal(Color::WHITE);
    let transition = PaintTransition::new(
        Some(SemanticPaint::LinearGradient(from.clone())),
        0.25,
        Some(SemanticPaint::LinearGradient(from.clone())),
        0.75,
    )
    .unwrap();
    assert!(Arc::ptr_eq(&sample_gradient(&transition, 0.375), &from));
}

#[test]
fn invalid_progress_opacity_resource_and_stop_topology_fail_closed() {
    let paint = Some(SemanticPaint::LinearGradient(horizontal(Color::WHITE)));
    let to = Some(SemanticPaint::LinearGradient(gradient(
        [0.0, 0.0],
        [1.0, 0.0],
        &[
            (0.0, Color::WHITE),
            (0.5, Color::WHITE),
            (1.0, Color::BLACK),
        ],
    )));
    assert_eq!(
        PaintTransition::new(paint.clone(), 1.0, to, 1.0).unwrap_err(),
        PaintTransitionError::DifferentStopCounts
    );
    assert_eq!(
        PaintTransition::new(
            paint.clone(),
            1.0,
            Some(SemanticPaint::Image(std::sync::Arc::new(
                crate::ImagePaint::new(
                    crate::RasterImageResourceHandle {
                        arena: 1,
                        id: crate::RasterImageResourceId::new(7),
                        version: 0
                    },
                    crate::ImageMapping::default(),
                    crate::RasterImageSampling::Linear
                )
            ))),
            1.0
        )
        .unwrap_err(),
        PaintTransitionError::UnsupportedPaint
    );
    for opacity in [-1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            PaintTransition::new(paint.clone(), opacity, paint.clone(), 1.0).unwrap_err(),
            PaintTransitionError::InvalidOpacity
        );
    }
    let transition = PaintTransition::new(paint.clone(), 1.0, paint, 1.0).unwrap();
    for alpha in [-1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            transition.sample(alpha).unwrap_err(),
            PaintTransitionError::InvalidProgress
        );
    }
}
