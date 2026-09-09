use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D, TransformTrackEndpoint, Vec2, VectorPath,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn style() -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        stroke_width_mode: Default::default(),
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
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

fn endpoint(path: VectorPath) -> TransformTrackEndpoint {
    TransformTrackEndpoint {
        geometry: GeometryRef::path(path),
        transform: Transform2D::IDENTITY,
        style: style(),
    }
}

#[test]
fn steady_generic_path_transform_updates_instance_without_retessellation() {
    assert_steady_transform(noon_core::StrokeWidthMode::ScaleWithObject);
    assert_steady_transform(noon_core::StrokeWidthMode::ScreenSpace);
}

fn assert_steady_transform(stroke_mode: noon_core::StrokeWidthMode) {
    let mut from = endpoint(path_a());
    let mut to = endpoint(path_b());
    from.style.stroke_width_mode = stroke_mode;
    to.style.stroke_width_mode = stroke_mode;
    to.style.stroke = Some(Color::rgb(0.2, 0.7, 0.9));
    from.transform.translation = Vec2::new(-2.0, 1.0);
    from.transform.scale = Vec2::new(1.3, 0.7);
    to.transform.rotation = 0.5;
    to.transform.translation = Vec2::new(3.0, -1.0);
    to.transform.scale = Vec2::new(0.6, 1.9);

    let object = ObjectId::new(0);
    let objects = vec![CompiledObject::new(
        object,
        from.geometry.clone(),
        Transform2D::IDENTITY,
        from.style,
    )];
    let tracks = [TrackDefinition {
        id: TrackId::new(0),
        object,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
        time_map: Default::default(),
    }];
    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    let mut preparer = FramePreparer::new();
    instance.seek(0.1).unwrap();

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
    let a = endpoint(path_a());
    let b = endpoint(path_b());
    let c = endpoint(path_c());
    let object = ObjectId::new(0);
    let objects = vec![CompiledObject::new(
        object,
        a.geometry.clone(),
        Transform2D::IDENTITY,
        a.style,
    )];
    let tracks = [
        TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Transform,
            values: TrackValues::Object {
                from: a,
                to: b.clone(),
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: Default::default(),
        },
        TrackDefinition {
            id: TrackId::new(1),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from: b, to: c },
            timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
            time_map: Default::default(),
        },
    ];
    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
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
