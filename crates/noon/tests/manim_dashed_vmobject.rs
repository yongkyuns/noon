use noon::{DashedVMobjectOptions, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

#[test]
fn generic_dashed_vmobject_preserves_style_and_open_dash_topology() {
    let scene = Scene::new();
    let source_path = VectorPath::new()
        .move_to(Vec2::new(-2.0, 0.0))
        .line_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(2.0, 0.0));
    let mut source = scene
        .geometry(ManimGeometryOptions::path(source_path).unwrap())
        .unwrap();
    source.set_stroke_width(0.07).unwrap();
    let source_state = source.state().unwrap();

    let dashed = source
        .dashed_vmobject(DashedVMobjectOptions {
            num_dashes: 4,
            dashed_ratio: 0.5,
            dash_offset: 0.0,
            equal_lengths: true,
        })
        .unwrap();
    let query = dashed.path_query().unwrap();

    assert_eq!(query.subpaths().len(), 4);
    assert_eq!(dashed.state().unwrap().style, source_state.style);
    assert_eq!(source.state().unwrap(), source_state);
    assert_ne!(dashed.node_id(), source.node_id());
    assert!((query.start().unwrap().0 + 2.0).abs() < 1e-6);
    assert!((query.end().unwrap().0 - 2.0).abs() < 1e-6);
}

#[test]
fn generic_dashed_vmobject_equal_lengths_differs_from_curve_parameter_spacing() {
    let scene = Scene::new();
    let source_path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(10.0, 0.0));
    let source = scene
        .geometry(ManimGeometryOptions::path(source_path).unwrap())
        .unwrap();

    let equal = source
        .dashed_vmobject(DashedVMobjectOptions {
            num_dashes: 1,
            dashed_ratio: 0.25,
            equal_lengths: true,
            ..Default::default()
        })
        .unwrap();
    let direct = source
        .dashed_vmobject(DashedVMobjectOptions {
            num_dashes: 1,
            dashed_ratio: 0.25,
            equal_lengths: false,
            ..Default::default()
        })
        .unwrap();

    assert!((equal.path_query().unwrap().end().unwrap().0 - 2.5).abs() < 1e-5);
    assert!((direct.path_query().unwrap().end().unwrap().0 - 0.5).abs() < 1e-5);
}
