use noon_compile::CompiledScene;
use noon_core::{
    Color, GeometryRef, SceneDefinition, StrokeCap, StrokeJoin, Style, Vec2, VectorPath,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn style(join: StrokeJoin, cap: StrokeCap) -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.2,
        stroke_join: join,
        stroke_cap: cap,
        opacity: 1.0,
    }
}

#[test]
fn path_cache_key_includes_join_and_cap_policy() {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 1.0));
    let styles = [
        style(StrokeJoin::Round, StrokeCap::Round),
        style(StrokeJoin::Miter, StrokeCap::Round),
        style(StrokeJoin::Round, StrokeCap::Butt),
        style(StrokeJoin::Round, StrokeCap::Round),
    ];
    let mut scene = SceneDefinition::new();
    for path_style in styles {
        let object = scene.add(GeometryRef::path(path.clone()));
        scene.object_mut(object).unwrap().style = path_style;
    }
    let instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(instance.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 3);
    assert_eq!(preparer.cached_path_mesh_count(), 3);
}
