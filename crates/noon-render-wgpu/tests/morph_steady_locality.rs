use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2, VectorPath,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

const OBJECT_COUNT: usize = 600;
const VARIANT_COUNT: usize = 12;

fn style() -> Style {
    Style {
        fill: Some(Color::WHITE),
        stroke: Some(Color::WHITE),
        stroke_width: 0.02,
        stroke_width_mode: Default::default(),
        opacity: 0.8,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    }
}

fn square(scale: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-scale, -scale))
        .line_to(Vec2::new(scale, -scale))
        .line_to(Vec2::new(scale, scale))
        .line_to(Vec2::new(-scale, scale))
        .close()
}

fn diamond(scale: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, scale))
        .line_to(Vec2::new(-scale, 0.0))
        .line_to(Vec2::new(0.0, -scale))
        .line_to(Vec2::new(scale, 0.0))
        .close()
}

fn endpoint(path: VectorPath) -> TransformTrackEndpoint {
    TransformTrackEndpoint {
        geometry: GeometryRef::path(path),
        transform: Transform2D::IDENTITY,
        style: style(),
    }
}

#[test]
fn six_hundred_object_steady_morph_updates_instances_only() {
    let variants: Vec<_> = (0..VARIANT_COUNT)
        .map(|variant| {
            let source_scale = 0.75 + variant as f32 * 0.02;
            let target_scale = 0.82 + variant as f32 * 0.02;
            (
                endpoint(square(source_scale)),
                endpoint(diamond(target_scale)),
            )
        })
        .collect();

    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    let mut tracks = Vec::with_capacity(OBJECT_COUNT);
    for index in 0..OBJECT_COUNT {
        let object = ObjectId::new(index as u64);
        let (a, b) = &variants[index % VARIANT_COUNT];
        let (from, to) = if index % 2 == 0 {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        objects.push(CompiledObject::new(
            object,
            from.geometry.clone(),
            from.transform,
            from.style,
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(index as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        });
    }

    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    let mut preparer = FramePreparer::new();

    instance.seek(0.10).unwrap();
    let activation_changes = instance.take_frame_changes();
    let activation = preparer.prepare_incremental(instance.frame(), &activation_changes);
    assert!(activation.stats.geometry_cache_misses > 0);
    assert!(activation.path_geometry_dirty);

    instance.advance_to(0.20).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);

    assert_eq!(steady.stats.full_rebuilds, 0);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert_eq!(steady.stats.instances_repacked, OBJECT_COUNT);
    assert_eq!(steady.stats.dirty_instance_count, OBJECT_COUNT);
    assert_eq!(steady.stats.path_vertices_repacked, 0);
    assert_eq!(steady.stats.path_indices_repacked, 0);
    assert_eq!(steady.stats.mega_path_indices_repacked, 0);
    assert_eq!(steady.stats.render_order_positions_visited, 0);
    assert_eq!(steady.stats.render_order_chunks_rebuilt, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(steady.path_dirty_ranges.len(), 1);
    assert_eq!(steady.path_dirty_ranges[0], 0..OBJECT_COUNT);

    instance.advance_to(0.30).unwrap();
    let changes = instance.take_frame_changes();
    let steady_again = preparer.prepare_incremental(instance.frame(), &changes);

    assert_eq!(steady_again.stats.full_rebuilds, 0);
    assert_eq!(steady_again.stats.geometry_cache_misses, 0);
    assert_eq!(steady_again.stats.instances_repacked, OBJECT_COUNT);
    assert_eq!(steady_again.stats.dirty_instance_count, OBJECT_COUNT);
    assert_eq!(steady_again.stats.path_vertices_repacked, 0);
    assert_eq!(steady_again.stats.path_indices_repacked, 0);
    assert_eq!(steady_again.stats.mega_path_indices_repacked, 0);
    assert_eq!(steady_again.stats.render_order_positions_visited, 0);
    assert_eq!(steady_again.stats.render_order_chunks_rebuilt, 0);
    assert!(!steady_again.path_geometry_dirty);
    assert_eq!(steady_again.path_dirty_ranges.len(), 1);
    assert_eq!(steady_again.path_dirty_ranges[0], 0..OBJECT_COUNT);
}

#[test]
fn six_hundred_object_second_morph_steady_state_remains_instance_only() {
    let variants: Vec<_> = (0..VARIANT_COUNT)
        .map(|variant| {
            let source_scale = 0.75 + variant as f32 * 0.02;
            let first_target_scale = 0.82 + variant as f32 * 0.02;
            let second_square_scale = 0.88 + variant as f32 * 0.02;
            let second_diamond_scale = 0.91 + variant as f32 * 0.02;
            (
                endpoint(square(source_scale)),
                endpoint(diamond(first_target_scale)),
                endpoint(square(second_square_scale)),
                endpoint(diamond(second_diamond_scale)),
            )
        })
        .collect();

    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    let mut tracks = Vec::with_capacity(OBJECT_COUNT * 2);
    for index in 0..OBJECT_COUNT {
        let object = ObjectId::new(index as u64);
        let (square_a, diamond_a, square_b, diamond_b) = &variants[index % VARIANT_COUNT];
        let (from, middle, to) = if index % 2 == 0 {
            (square_a.clone(), diamond_a.clone(), square_b.clone())
        } else {
            (diamond_a.clone(), square_a.clone(), diamond_b.clone())
        };
        objects.push(CompiledObject::new(
            object,
            from.geometry.clone(),
            from.transform,
            from.style,
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new((index * 2) as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object {
                from: from.clone(),
                to: middle.clone(),
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        });
        tracks.push(TrackDefinition {
            id: TrackId::new((index * 2 + 1) as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from: middle, to },
            timing: TrackTiming::new(1.0, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        });
    }

    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    let mut preparer = FramePreparer::new();

    instance.seek(0.10).unwrap();
    let changes = instance.take_frame_changes();
    let _first_activation = preparer.prepare_incremental(instance.frame(), &changes);

    instance.advance_to(0.20).unwrap();
    let changes = instance.take_frame_changes();
    let _first_steady = preparer.prepare_incremental(instance.frame(), &changes);

    instance.advance_to(1.10).unwrap();
    let changes = instance.take_frame_changes();
    let _second_activation = preparer.prepare_incremental(instance.frame(), &changes);

    instance.advance_to(1.20).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);

    assert_eq!(steady.stats.full_rebuilds, 0);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert_eq!(steady.stats.instances_repacked, OBJECT_COUNT);
    assert_eq!(steady.stats.dirty_instance_count, OBJECT_COUNT);
    assert_eq!(steady.stats.path_vertices_repacked, 0);
    assert_eq!(steady.stats.path_indices_repacked, 0);
    assert_eq!(steady.stats.mega_path_indices_repacked, 0);
    assert_eq!(steady.stats.render_order_positions_visited, 0);
    assert_eq!(steady.stats.render_order_chunks_rebuilt, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(steady.path_dirty_ranges.len(), 1);
    assert_eq!(steady.path_dirty_ranges[0], 0..OBJECT_COUNT);

    instance.advance_to(1.30).unwrap();
    let changes = instance.take_frame_changes();
    let steady_again = preparer.prepare_incremental(instance.frame(), &changes);

    assert_eq!(steady_again.stats.full_rebuilds, 0);
    assert_eq!(steady_again.stats.geometry_cache_misses, 0);
    assert_eq!(steady_again.stats.instances_repacked, OBJECT_COUNT);
    assert_eq!(steady_again.stats.dirty_instance_count, OBJECT_COUNT);
    assert_eq!(steady_again.stats.path_vertices_repacked, 0);
    assert_eq!(steady_again.stats.path_indices_repacked, 0);
    assert_eq!(steady_again.stats.mega_path_indices_repacked, 0);
    assert_eq!(steady_again.stats.render_order_positions_visited, 0);
    assert_eq!(steady_again.stats.render_order_chunks_rebuilt, 0);
    assert!(!steady_again.path_geometry_dirty);
    assert_eq!(steady_again.path_dirty_ranges.len(), 1);
    assert_eq!(steady_again.path_dirty_ranges[0], 0..OBJECT_COUNT);
}
