use noon::{ManimGeometryOptions, Scene, Vec2};

fn midpoint(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    ((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5)
}

#[test]
fn tangent_line_samples_generic_retained_paths_in_authored_world_space() {
    let scene = Scene::new();
    let mut source_options = ManimGeometryOptions::rectangle(4.0, 2.0).unwrap();
    source_options.set_scale(1.25, 0.75).unwrap();
    source_options.set_rotation(0.3).unwrap();
    source_options.set_translation(-0.4, 0.6).unwrap();
    let source = scene.geometry(source_options).unwrap();

    let query = source.path_query().unwrap();
    let a = query.point_from_proportion(0.3 - 1.0e-4).unwrap();
    let b = query.point_from_proportion(0.3 + 1.0e-4).unwrap();
    let expected_midpoint = midpoint(a, b);

    let tangent = scene
        .geometry(
            source
                .manim_tangent_line_options(0.3, 3.5, 1.0e-4)
                .unwrap(),
        )
        .unwrap();
    let endpoints = tangent.manim_line_endpoints().unwrap();
    let actual_midpoint = midpoint(endpoints.start, endpoints.end);
    let dx = endpoints.end.0 - endpoints.start.0;
    let dy = endpoints.end.1 - endpoints.start.1;

    assert!((dx.hypot(dy) - 3.5).abs() < 1.0e-5);
    assert!((actual_midpoint.0 - expected_midpoint.0).abs() < 1.0e-6);
    assert!((actual_midpoint.1 - expected_midpoint.1).abs() < 1.0e-6);
}

#[test]
fn endpoint_alpha_clamps_like_manim() {
    let scene = Scene::new();
    let source = scene.circle(2.0).unwrap();
    let query = source.path_query().unwrap();
    let first = query.point_from_proportion(0.0).unwrap();
    let nearby = query.point_from_proportion(1.0e-6).unwrap();
    let expected = midpoint(first, nearby);

    let tangent = scene
        .geometry(
            source
                .manim_tangent_line_options(0.0, 4.0, 1.0e-6)
                .unwrap(),
        )
        .unwrap();
    let endpoints = tangent.manim_line_endpoints().unwrap();
    let center = midpoint(endpoints.start, endpoints.end);
    assert!((center.0 - expected.0).abs() < 1.0e-6);
    assert!((center.1 - expected.1).abs() < 1.0e-6);
}

#[test]
fn signed_requested_length_preserves_manim_line_scaling_orientation() {
    let scene = Scene::new();
    let source = scene.circle(1.5).unwrap();
    let positive = scene
        .geometry(
            source
                .manim_tangent_line_options(0.25, 2.0, 1.0e-5)
                .unwrap(),
        )
        .unwrap();
    let negative = scene
        .geometry(
            source
                .manim_tangent_line_options(0.25, -2.0, 1.0e-5)
                .unwrap(),
        )
        .unwrap();
    let p = positive.manim_line_endpoints().unwrap();
    let n = negative.manim_line_endpoints().unwrap();

    assert!((p.start.0 - n.end.0).abs() < 1.0e-6);
    assert!((p.start.1 - n.end.1).abs() < 1.0e-6);
    assert!((p.end.0 - n.start.0).abs() < 1.0e-6);
    assert!((p.end.1 - n.start.1).abs() < 1.0e-6);
}

#[test]
fn tangent_line_sampling_is_read_only_until_candidate_publication() {
    let scene = Scene::new();
    let source = scene.circle(2.0).unwrap();
    let state = source.state().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();

    let candidate = source
        .manim_tangent_line_options(0.4, 2.5, 1.0e-5)
        .unwrap();
    assert_eq!(source.state().unwrap(), state);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );

    let tangent = scene.geometry(candidate).unwrap();
    assert_eq!(source.state().unwrap(), state);
    assert_ne!(tangent.node_id(), source.node_id());
}

#[test]
fn degenerate_sample_rejects_without_semantic_publication() {
    let scene = Scene::new();
    let source = scene.circle(2.0).unwrap();
    let revision = scene.revision();
    let result = source.manim_tangent_line_options(0.5, 1.0, 0.0);
    assert!(result.is_err());
    assert_eq!(scene.revision(), revision);
}

#[test]
fn zero_requested_length_is_a_valid_degenerate_line() {
    let scene = Scene::new();
    let source = scene.circle(2.0).unwrap();
    let tangent = scene
        .geometry(
            source
                .manim_tangent_line_options(0.2, 0.0, 1.0e-5)
                .unwrap(),
        )
        .unwrap();
    let endpoints = tangent.manim_line_endpoints().unwrap();
    assert_eq!(
        Vec2::new(endpoints.start.0 as f32, endpoints.start.1 as f32),
        Vec2::new(endpoints.end.0 as f32, endpoints.end.1 as f32)
    );
}
