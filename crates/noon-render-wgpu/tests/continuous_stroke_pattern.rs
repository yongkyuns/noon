use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{Color, GeometryRef, ObjectId, Property, RateFunction, StrokePattern, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2};
use noon_render_wgpu::{FramePreparer, LineInstance};
use noon_runtime::SceneInstance;

#[test]
fn stroke_pattern_updates_one_instance_without_changing_geometry_or_other_rows() {
    let geometry = GeometryRef::line(Vec2::new(-3.0, 0.0), Vec2::new(3.0, 0.0));
    let source = TransformTrackEndpoint {
        style: Style {
            fill: None, stroke: Some(Color::WHITE), stroke_width: 0.1,
            stroke_pattern: Some(StrokePattern::dashed(0.25, 0.25).unwrap()),
            ..Style::default()
        },
        ..TransformTrackEndpoint::new(geometry.clone())
    };
    let mut target = source.clone();
    target.style.stroke_pattern = Some(source.style.stroke_pattern.unwrap().with_phase(2.0).unwrap());
    let animated = CompiledObject::new(ObjectId::new(0), geometry.clone(), Transform2D::IDENTITY, source.style);
    let mut objects = vec![animated];
    for id in 1..101 {
        objects.push(CompiledObject::new(ObjectId::new(id), geometry.clone(), Transform2D::IDENTITY,
            Style { fill: None, stroke: Some(Color::WHITE), stroke_width: 0.1, ..Style::default() }));
    }
    let track = TrackDefinition {
        id: TrackId::new(0), object: ObjectId::new(0), property: Property::Transform,
        values: TrackValues::Object { from: source, to: target },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear), time_map: Default::default(),
    };
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[track]).unwrap());
    let mut renderer = FramePreparer::new();
    runtime.advance_to(0.0).unwrap();
    let initial_changes = runtime.take_frame_changes();
    let prepared = renderer.prepare_incremental(runtime.frame(), &initial_changes);
    let untouched = prepared.lines[1..].to_vec();
    assert_eq!(std::mem::size_of::<LineInstance>(), 88);
    for time in [0.125, 0.375, 0.75, 1.125, 1.875] {
        runtime.advance_to(time).unwrap();
        let changes = runtime.take_frame_changes();
        let prepared = renderer.prepare_incremental(runtime.frame(), &changes);
        assert_eq!(prepared.lines.len(), 101);
        assert_eq!(&prepared.lines[1..], untouched);
        assert_eq!(prepared.lines[0].style.fill_enabled, 4);
        assert_eq!(prepared.lines[0].style.fill[2], (time as f32).rem_euclid(0.5));
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert!(!prepared.path_geometry_dirty);
    }
}
