use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, ScenePatch, Style, TrackTiming,
    Transform2D, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn snapshot(geometry: GeometryRef, transform: Transform2D) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry,
        transform,
        style: Style::default(),
        present: true,
        reveal: 1.0,
        morph: None,
    }
}

#[test]
fn rectangle_and_line_geometry_transforms_stay_on_analytic_instance_paths() {
    let mut scene = SceneDefinition::new();
    let rectangle = scene.add(GeometryRef::rectangle(Vec2::new(2.0, 1.0)));
    let line = scene.add(GeometryRef::line(
        Vec2::new(-1.0, -0.5),
        Vec2::new(1.0, 0.5),
    ));
    scene.add_track(
        rectangle,
        0.0,
        TrackTiming::linear(1.0),
        Easing::Linear,
        snapshot(
            GeometryRef::rectangle(Vec2::new(2.0, 1.0)),
            Transform2D::IDENTITY,
        ),
        snapshot(
            GeometryRef::rectangle(Vec2::new(2.0, 1.0)),
            Transform2D {
                translation: Vec2::new(0.5, 0.25),
                rotation: 0.4,
                scale: Vec2::new(1.3, 0.7),
            },
        ),
    );
    scene.add_track(
        line,
        0.0,
        TrackTiming::linear(1.0),
        Easing::Linear,
        snapshot(
            GeometryRef::line(Vec2::new(-1.0, -0.5), Vec2::new(1.0, 0.5)),
            Transform2D::IDENTITY,
        ),
        snapshot(
            GeometryRef::line(Vec2::new(-1.0, -0.5), Vec2::new(1.0, 0.5)),
            Transform2D {
                translation: Vec2::new(-0.25, 0.75),
                rotation: -0.6,
                scale: Vec2::new(0.8, 1.4),
            },
        ),
    );

    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut runtime = SceneInstance::new(compiled);
    let mut preparer = FramePreparer::new();

    let initial_changes = runtime.take_frame_changes();
    let initial = preparer.prepare_incremental(runtime.frame(), &initial_changes);
    assert_eq!(initial.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 0);

    runtime.evaluate(0.5);
    let active_changes = runtime.take_frame_changes();
    let active = preparer.prepare_incremental(runtime.frame(), &active_changes);
    assert_eq!(active.stats.dirty_instance_count, 2);
    assert_eq!(active.rectangle_dirty_ranges.len(), 1);
    assert_eq!(active.rectangle_dirty_ranges[0], 0..1);
    assert_eq!(active.line_dirty_ranges.len(), 1);
    assert_eq!(active.line_dirty_ranges[0], 0..1);
    assert_eq!(active.stats.geometry_cache_misses, 0);
    assert!(!active.path_geometry_dirty);
    assert_eq!(preparer.cached_path_mesh_count(), 0);

    runtime.evaluate(0.5);
    let steady_changes = runtime.take_frame_changes();
    let steady = preparer.prepare_incremental(runtime.frame(), &steady_changes);
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

    for edit in 0..EDIT_COUNT {
        let step = (edit + 1) as f32;
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
