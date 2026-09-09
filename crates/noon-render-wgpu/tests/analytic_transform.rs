use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn endpoint(geometry: GeometryRef, style: Style) -> TransformTrackEndpoint {
    TransformTrackEndpoint {
        geometry,
        transform: Transform2D::IDENTITY,
        style,
    }
}

#[test]
fn analytic_geometry_transform_dirties_only_one_instance_without_path_work() {
    let object = ObjectId::new(0);
    let style = Style::default();
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            style,
        )],
        &[TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Transform,
            values: TrackValues::Object {
                from: endpoint(GeometryRef::circle(1.0), style),
                to: endpoint(GeometryRef::circle(3.0), style),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        }],
    )
    .unwrap();
    let mut instance = SceneInstance::new(compiled);
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
    let style = Style::default();
    let rectangle = ObjectId::new(0);
    let line = ObjectId::new(1);
    let rectangle_from = GeometryRef::rectangle(2.0, 4.0);
    let line_from = GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0));
    let objects = vec![
        CompiledObject::new(
            rectangle,
            rectangle_from.clone(),
            Transform2D::IDENTITY,
            style,
        ),
        CompiledObject::new(line, line_from.clone(), Transform2D::IDENTITY, style),
    ];
    let tracks = [
        TrackDefinition {
            id: TrackId::new(0),
            object: rectangle,
            property: Property::Transform,
            values: TrackValues::Object {
                from: endpoint(rectangle_from, style),
                to: endpoint(GeometryRef::rectangle(6.0, 8.0), style),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        },
        TrackDefinition {
            id: TrackId::new(1),
            object: line,
            property: Property::Transform,
            values: TrackValues::Object {
                from: endpoint(line_from, style),
                to: endpoint(
                    GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
                    style,
                ),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        },
    ];
    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
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

    let moving_line = ObjectId::new(MOVING_INDEX as u64);
    let objects = (0..2)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(index),
                GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let mut instance = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
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
            .apply_execution_patch(&ExecutionPatch::SetTransform {
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
