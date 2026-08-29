use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, ScenePatch, Style, TrackTiming, Transform2D,
    Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn snapshot(geometry: GeometryRef, style: Style) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry,
        transform: Transform2D::IDENTITY,
        style,
    }
}

#[test]
fn analytic_geometry_transform_dirties_only_one_instance_without_path_work() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let style = Style::default();
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::circle(1.0), style),
            snapshot(GeometryRef::circle(3.0), style),
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

#[test]
fn rectangle_and_line_geometry_transforms_stay_on_analytic_instance_paths() {
    let mut scene = SceneDefinition::new();
    let style = Style::default();

    let rectangle = scene.add(GeometryRef::rectangle(2.0, 4.0));
    scene
        .animate_transform(
            rectangle,
            snapshot(GeometryRef::rectangle(2.0, 4.0), style),
            snapshot(GeometryRef::rectangle(6.0, 8.0), style),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let line = scene.add(GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)));
    scene
        .animate_transform(
            line,
            snapshot(
                GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
                style,
            ),
            snapshot(
                GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
                style,
            ),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let initial_changes = instance.take_frame_changes();
    preparer.prepare_incremental(instance.frame(), &initial_changes);

    instance.advance_to(0.5).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(steady.stats.instances_repacked, 2);
    assert_eq!(steady.stats.dirty_instance_count, 2);
    assert_eq!(steady.rectangle_dirty_ranges.len(), 1);
    assert_eq!(steady.rectangle_dirty_ranges[0], 0..1);
    assert_eq!(steady.line_dirty_ranges.len(), 1);
    assert_eq!(steady.line_dirty_ranges[0], 0..1);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(preparer.cached_path_mesh_count(), 0);
}

#[test]
fn repeated_line_transform_patches_keep_preparation_bounded_and_local() {
    const EDIT_COUNT: usize = 1_000;
    const STATIC_INDEX: usize = 0;
    const MOVING_INDEX: usize = 1;

    let mut scene = SceneDefinition::new();
    let _static_line = scene.add(GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)));
    let moving_line = scene.add(GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)));

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let static_before = instance.frame().objects[STATIC_INDEX].clone();
    let mut preparer = FramePreparer::new();
    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert_eq!(initial.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 0);

    let mut step = 0.0_f32;
    for _ in 0..EDIT_COUNT {
        step += 1.0;
        let transform = Transform2D {
            translation: Vec2::new(step * 0.001, -step * 0.0005),
            ..Transform2D::IDENTITY
        };
        instance
            .apply_patch(&ScenePatch::SetTransform {
                object: moving_line,
                transform,
            })
            .unwrap();

        let changes = instance.take_frame_changes();
        assert_eq!(changes.object_indices(), &[MOVING_INDEX]);
        let prepared = preparer.prepare_incremental(instance.frame(), &changes);

        assert_eq!(instance.frame().objects[MOVING_INDEX].transform, transform);
        assert_eq!(instance.frame().objects[STATIC_INDEX], static_before);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert_eq!(prepared.line_dirty_ranges.len(), 1);
        assert_eq!(prepared.line_dirty_ranges[0].start, MOVING_INDEX);
        assert_eq!(prepared.line_dirty_ranges[0].end, MOVING_INDEX + 1);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(preparer.cached_path_mesh_count(), 0);
    }
}
