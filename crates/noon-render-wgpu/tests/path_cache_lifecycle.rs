use noon_compile::CompiledScene;
use noon_core::{Color, GeometryRef, SceneDefinition, Style, Vec2, VectorPath};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn path(seed: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(seed, 0.0))
        .line_to(Vec2::new(seed + 1.0, 0.5))
        .line_to(Vec2::new(seed + 0.25, 1.5))
}

fn path_scene(paths: &[VectorPath]) -> SceneInstance {
    let mut scene = SceneDefinition::new();
    for path in paths {
        let object = scene.add(GeometryRef::path(path.clone()));
        scene.object_mut(object).expect("path object exists").style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.2,
            ..Style::default()
        };
    }
    SceneInstance::new(CompiledScene::compile(&scene).expect("path scene compiles"))
}

#[test]
fn path_cache_retains_recent_meshes_and_evicts_stale_entries() {
    let a = path(0.0);
    let b = path(10.0);
    let c = path(20.0);
    let mut preparer = FramePreparer::new();
    preparer.set_path_mesh_cache_limit(2);

    let scene_a = path_scene(std::slice::from_ref(&a));
    let prepared = preparer.prepare(scene_a.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 1);
    assert_eq!(preparer.cached_path_mesh_count(), 1);

    let scene_b = path_scene(std::slice::from_ref(&b));
    let prepared = preparer.prepare(scene_b.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 1);
    assert_eq!(preparer.cached_path_mesh_count(), 2);

    let scene_a = path_scene(std::slice::from_ref(&a));
    let prepared = preparer.prepare(scene_a.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 0);

    let scene_c = path_scene(std::slice::from_ref(&c));
    let prepared = preparer.prepare(scene_c.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 1);
    assert_eq!(preparer.cached_path_mesh_count(), 3);

    let scene_a = path_scene(std::slice::from_ref(&a));
    let prepared = preparer.prepare(scene_a.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 2);

    let scene_b = path_scene(std::slice::from_ref(&b));
    let prepared = preparer.prepare(scene_b.frame());
    assert_eq!(
        prepared.stats.geometry_cache_misses, 1,
        "least-recently-used stale mesh should have been evicted"
    );
}

#[test]
fn active_frame_geometry_can_exceed_retention_budget_without_retessellation() {
    let a = path(0.0);
    let b = path(10.0);
    let mut preparer = FramePreparer::new();
    preparer.set_path_mesh_cache_limit(1);

    let scene = path_scene(&[a.clone(), b.clone()]);
    let prepared = preparer.prepare(scene.frame());
    assert_eq!(prepared.path_ids.len(), 2);
    assert_eq!(prepared.stats.geometry_cache_misses, 2);
    assert_eq!(preparer.cached_path_mesh_count(), 2);

    let scene = path_scene(&[a, b]);
    let prepared = preparer.prepare(scene.frame());
    assert_eq!(prepared.path_ids.len(), 2);
    assert_eq!(
        prepared.stats.geometry_cache_misses, 0,
        "incoming-frame meshes must be pinned before stale LRU eviction"
    );
    assert_eq!(preparer.cached_path_mesh_count(), 2);
}
