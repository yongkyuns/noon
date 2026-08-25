use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

fn rectangle_frame(reveal: f32) -> FrameState {
    let mut style = Style::default();
    style.fill = Some(Color::rgba(0.35, 0.75, 0.95, 0.35));
    style.stroke = Some(Color::rgba(0.95, 0.3, 0.75, 0.65));
    style.stroke_width = 0.08;

    FrameState {
        time: 0.0,
        objects: vec![FrameObjectState {
            id: ObjectId::new(1),
            geometry: GeometryRef::rectangle(2.0, 2.0),
            transform: Transform2D::IDENTITY,
            style,
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![reveal],
        morphs: vec![0.0],
        render_geometries: vec![None],
    }
}

#[test]
fn descending_rectangle_reveal_switches_from_analytic_to_partial_path_once() {
    let mut frame = rectangle_frame(1.0);
    let mut preparer = FramePreparer::new();

    let full = preparer.prepare(&frame);
    assert_eq!(full.rectangles.len(), 1);
    assert!(full.paths.is_empty());

    frame.time = 0.5;
    frame.reveals[0] = 0.5;
    let partial = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
    assert!(partial.rectangles.is_empty());
    assert_eq!(partial.paths.len(), 1);
    assert_eq!(partial.paths[0].path_params, [1.0, 0.0]);
    assert!(partial.path_geometry_dirty);
    assert_eq!(partial.stats.full_rebuilds, 1);

    frame.time = 0.75;
    frame.reveals[0] = 0.25;
    let later = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
    assert!(later.rectangles.is_empty());
    assert_eq!(later.paths.len(), 1);
    assert_eq!(later.paths[0].path_params, [1.0, 0.0]);
    assert!(later.path_geometry_dirty);
    assert_eq!(later.stats.full_rebuilds, 0);
}
