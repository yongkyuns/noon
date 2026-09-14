use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D, Vec2, VectorPath, PI,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

const OBJECT_COUNT: usize = 600;
const TRACKS_PER_OBJECT: usize = 5;
const SAMPLE_HZ: f64 = 60.0;
const TURBULENCE_START: f64 = 2.72;
const TURBULENCE_DURATION: f64 = 0.45;
const FIRST_SAMPLE_INDEX: usize = 164;
const SAMPLE_COUNT: usize = 27;

const PALETTE: [Color; 8] = [
    Color::BLUE,
    Color::TEAL,
    Color::GREEN,
    Color::YELLOW,
    Color::ORANGE,
    Color::RED,
    Color::PINK,
    Color::PURPLE,
];

fn style(color: Color) -> Style {
    Style {
        fill: Some(color),
        stroke: Some(color),
        stroke_width: 0.02,
        stroke_width_mode: Default::default(),
        opacity: 0.72,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    }
}

fn retained_path(index: usize) -> VectorPath {
    let variant = index % 12;
    let scale = 0.75 + variant as f32 * 0.02;
    if index.is_multiple_of(2) {
        VectorPath::new()
            .move_to(Vec2::new(-scale, -scale))
            .line_to(Vec2::new(scale, -scale))
            .line_to(Vec2::new(scale, scale))
            .line_to(Vec2::new(-scale, scale))
            .close()
    } else {
        VectorPath::new()
            .move_to(Vec2::new(0.0, scale))
            .line_to(Vec2::new(-scale, 0.0))
            .line_to(Vec2::new(0.0, -scale))
            .line_to(Vec2::new(scale, 0.0))
            .close()
    }
}

fn track(id: usize, object: ObjectId, property: Property, values: TrackValues) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id as u64),
        object,
        property,
        values,
        timing: TrackTiming::new(0.0, TURBULENCE_DURATION, RateFunction::Linear),
        time_map: Default::default(),
    }
}

#[test]
fn six_hundred_object_turbulence_remains_instance_only_for_every_measured_sample() {
    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    let mut tracks = Vec::with_capacity(OBJECT_COUNT * TRACKS_PER_OBJECT);

    for index in 0..OBJECT_COUNT {
        let object = ObjectId::new(index as u64);
        let row = index / 30;
        let col = index % 30;
        let base_color = PALETTE[(index + 6) % PALETTE.len()];
        let target_color = PALETTE[(index * 5 + 1) % PALETTE.len()];
        let factor = 0.92 + 0.02 * ((index * 7 + col) % 9) as f32;
        let angle = if (index + row).is_multiple_of(2) {
            PI / 3.0
        } else {
            -PI / 3.0
        };
        let dx = (((index * 23 + col) % 7) as f32 - 3.0) * 0.022;
        let dy = (((index * 31 + row) % 7) as f32 - 3.0) * 0.020;
        let first_track = index * TRACKS_PER_OBJECT;

        objects.push(CompiledObject::new(
            object,
            GeometryRef::path(retained_path(index)),
            Transform2D::IDENTITY,
            style(base_color),
        ));
        tracks.push(track(
            first_track,
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(dx, dy),
            },
        ));
        tracks.push(track(
            first_track + 1,
            object,
            Property::Rotation,
            TrackValues::Scalar {
                from: 0.0,
                to: angle,
            },
        ));
        tracks.push(track(
            first_track + 2,
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(factor, factor),
            },
        ));
        tracks.push(track(
            first_track + 3,
            object,
            Property::Fill,
            TrackValues::Color {
                from: Some(base_color),
                to: Some(target_color),
            },
        ));
        tracks.push(track(
            first_track + 4,
            object,
            Property::Stroke,
            TrackValues::Color {
                from: Some(base_color),
                to: Some(target_color),
            },
        ));
    }

    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    let mut preparer = FramePreparer::new();

    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert!(initial.stats.geometry_cache_misses > 0);
    assert!(initial.path_geometry_dirty);

    // The benchmark phase starts at 2.72 s, which is 163.2 samples on its
    // fixed 60 Hz grid. Samples 164 through 190 are therefore the exact 27
    // measured turbulence frames, expressed here relative to phase start.
    for sample_index in FIRST_SAMPLE_INDEX..FIRST_SAMPLE_INDEX + SAMPLE_COUNT {
        let local_time = sample_index as f64 / SAMPLE_HZ - TURBULENCE_START;
        instance.advance_to(local_time).unwrap();
        let changes = instance.take_frame_changes();
        let prepared = preparer.prepare_incremental(instance.frame(), &changes);

        assert_eq!(prepared.stats.full_rebuilds, 0, "sample {sample_index}");
        assert_eq!(
            prepared.stats.geometry_cache_misses, 0,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.instances_repacked, OBJECT_COUNT,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.dirty_instance_count, OBJECT_COUNT,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.path_vertices_repacked, 0,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.path_indices_repacked, 0,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.mega_path_indices_repacked, 0,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.render_order_positions_visited, 0,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.render_order_chunks_rebuilt, 0,
            "sample {sample_index}"
        );
        assert!(!prepared.path_geometry_dirty, "sample {sample_index}");
        assert_eq!(prepared.path_dirty_ranges.len(), 1, "sample {sample_index}");
        assert_eq!(
            prepared.path_dirty_ranges[0],
            0..OBJECT_COUNT,
            "sample {sample_index}"
        );
    }
}
