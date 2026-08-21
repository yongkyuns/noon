use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, Style, TrackTiming, Transform2D,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

#[test]
fn analytic_geometry_transform_dirties_only_one_instance_without_path_work() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let style = Style::default();
    scene
        .animate_transform(
            object,
            ObjectSnapshot {
                geometry: GeometryRef::circle(1.0),
                transform: Transform2D::IDENTITY,
                style,
            },
            ObjectSnapshot {
                geometry: GeometryRef::circle(3.0),
                transform: Transform2D::IDENTITY,
                style,
            },
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert_eq!(initial.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 0);

    instance.advance_to(0.5).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(steady.stats.instances_repacked, 1);
    assert_eq!(steady.stats.dirty_instance_count, 1);
    assert_eq!(steady.circle_dirty_ranges.len(), 1);
    assert_eq!(steady.circle_dirty_ranges[0], 0..1);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(preparer.cached_path_mesh_count(), 0);
}
