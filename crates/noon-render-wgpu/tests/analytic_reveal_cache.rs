use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn circle(id: u64, radius: f32) -> FrameObjectState {
    let style = Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.08,
        ..Style::default()
    };
    FrameObjectState {
        id: ObjectId::new(id),
        geometry: GeometryRef::circle(radius),
        transform: Transform2D::IDENTITY,
        style,
        appearance: 1.0,
    }
}

#[test]
fn active_analytic_reveal_meshes_survive_cache_pressure_on_full_rebuild() {
    let objects = vec![circle(1, 0.7), circle(2, 0.9), circle(3, 1.1)];
    let frame = FrameState {
        time: 0.5,
        presences: vec![true; objects.len()],
        reveals: vec![0.5; objects.len()],
        morphs: vec![0.0; objects.len()],
        render_geometries: vec![None; objects.len()],
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
