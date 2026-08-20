use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, SceneDefinition, Style, TrackTiming, Transform2D,
    Vec2, VectorPath,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn style() -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
    }
}

fn path_a() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
}

fn path_b() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0))
}

fn path_c() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
}

fn snapshot(path: VectorPath) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry: GeometryRef::path(path),
        transform: Transform2D::IDENTITY,
        style: style(),
    }
}

#[test]
fn steady_generic_path_transform_updates_instance_without_retessellation() {
    let from = snapshot(path_a());
    let mut to = snapshot(path_b());
    to.style.stroke = Some(Color::rgb(0.2, 0.7, 0.9));
    to.transform.rotation = 0.5;

    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().style = from.style;
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();

    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert_eq!(initial.stats.geometry_cache_misses, 1);
    assert!(initial.path_geometry_dirty);

    instance.advance_to(0.5).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(steady.stats.dirty_instance_count, 1);
    assert_eq!(steady.path_dirty_ranges.len(), 1);
    assert_eq!(steady.path_dirty_ranges[0], 0..1);

    instance.advance_to(1.0).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(steady.stats.dirty_instance_count, 1);
}

#[test]
fn sequential_path_pair_transition_prepares_new_geometry_once() {
    let a = snapshot(path_a());
    let b = snapshot(path_b());
    let c = snapshot(path_c());
    let mut scene = SceneDefinition::new();
    let object = scene.add(a.geometry.clone());
    scene.object_mut(object).unwrap().style = a.style;
    scene
        .animate_transform(
            object,
            a,
            b.clone(),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_transform(
            object,
            b,
            c,
            TrackTiming::new(1.0, 1.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let changes = instance.take_frame_changes();
    let first = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(first.stats.geometry_cache_misses, 1);

    instance.advance_to(0.5).unwrap();
    let changes = instance.take_frame_changes();
    let first_steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(first_steady.stats.geometry_cache_misses, 0);
    assert!(!first_steady.path_geometry_dirty);

    instance.advance_to(1.0).unwrap();
    let changes = instance.take_frame_changes();
    let transition = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(transition.stats.geometry_cache_misses, 1);
    assert!(transition.path_geometry_dirty);
    assert_eq!(preparer.cached_path_mesh_count(), 2);

    instance.advance_to(1.25).unwrap();
    let changes = instance.take_frame_changes();
    let second_steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(second_steady.stats.geometry_cache_misses, 0);
    assert!(!second_steady.path_geometry_dirty);
}
