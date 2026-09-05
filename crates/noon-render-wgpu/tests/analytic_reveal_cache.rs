use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn rectangle(id: u64, width: f32, height: f32) -> FrameObjectState {
    let style = Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.08,
        stroke_width_mode: Default::default(),
        ..Style::default()
    };
    FrameObjectState {
        id: ObjectId::new(id),
        content: noon_core::ObjectContentRef::Geometry(GeometryRef::rectangle(width, height)),
        text_bounds: None,
        transform: Transform2D::IDENTITY,
        style,
        appearance: 1.0,
    }
}

#[test]
fn active_analytic_reveal_meshes_survive_cache_pressure_on_full_rebuild() {
    // Circle Create is now fully analytic and intentionally allocates no path
    // mesh. Rectangles still exercise the transient analytic->path reveal cache
    // that this regression is intended to protect.
    let objects = vec![
        rectangle(1, 1.4, 1.0),
        rectangle(2, 1.8, 1.2),
        rectangle(3, 2.2, 1.4),
    ];
    let frame = FrameState {
        time: 0.5,
        presences: vec![true; objects.len()],
        reveals: vec![0.5; objects.len()],
        morphs: vec![0.0; objects.len()],
        render_geometries: vec![None; objects.len()],
        render_transforms: vec![None; objects.len()],
        objects,
    };
    let mut preparer = FramePreparer::new();
    preparer.set_path_mesh_cache_limit(1);

    let cold = preparer.prepare(&frame);
    assert_eq!(cold.stats.geometry_cache_misses, 3);
    assert_eq!(cold.paths.len(), 3);
    assert_eq!(preparer.cached_path_mesh_count(), 3);

    let warm = preparer.prepare(&frame);
    assert_eq!(warm.paths.len(), 3);
    assert_eq!(warm.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 3);
}
