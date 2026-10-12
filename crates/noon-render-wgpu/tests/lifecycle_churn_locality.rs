use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{FramePreparer, PreparedFrame, RenderPrimitive};
use noon_runtime::SceneInstance;

const SHAPE_COUNT: usize = 600;
const LEAVING_COUNT: usize = SHAPE_COUNT / 3;
const PULSE_COUNT: usize = LEAVING_COUNT;
const MAX_VISIBLE_COUNT: usize = SHAPE_COUNT + PULSE_COUNT;
const SAMPLE_HZ: f64 = 60.0;
const LIFECYCLE_START: f64 = 3.41;
const FIRST_SAMPLE_INDEX: usize = 205;
const SAMPLE_COUNT: usize = 63;

fn presence_track(id: usize, object: ObjectId, at: f64, from: bool, to: bool) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id as u64),
        object,
        property: Property::Presence,
        values: TrackValues::Bool { from, to },
        timing: TrackTiming::instant(at),
        time_map: Default::default(),
    }
}

fn retained_shape(index: usize) -> VectorPath {
    let scale = 0.075 + (index % 12) as f32 * 0.002;
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

fn shape_style(index: usize) -> Style {
    let color = if index.is_multiple_of(2) {
        Color::BLUE
    } else {
        Color::TEAL
    };
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

fn expected_path_count(sample_index: usize) -> usize {
    match sample_index {
        205..=234 => SHAPE_COUNT,
        235..=240 => SHAPE_COUNT - LEAVING_COUNT,
        241..=267 => SHAPE_COUNT,
        _ => unreachable!("sample is outside lifecycle-churn"),
    }
}

fn expected_pulse_count(sample_index: usize) -> usize {
    match sample_index {
        205..=210 => 0,
        211..=267 => PULSE_COUNT,
        _ => unreachable!("sample is outside lifecycle-churn"),
    }
}

fn submitted_primitive_counts(prepared: &PreparedFrame<'_>) -> (usize, usize) {
    let mut paths = 0usize;
    let mut circles = 0usize;
    for ordered in prepared.ordered_render_batches() {
        let count =
            (ordered.batch.instance_range.end - ordered.batch.instance_range.start) as usize;
        match ordered.batch.primitive {
            RenderPrimitive::Circle => circles += count,
            RenderPrimitive::Path { .. } => paths += count,
            RenderPrimitive::MegaPath { .. } => {
                paths += ordered
                    .mega_path_batch
                    .expect("mega-path draw metadata must resolve")
                    .path_count;
            }
            RenderPrimitive::Rectangle | RenderPrimitive::Line => {}
        }
    }
    (paths, circles)
}

#[test]
fn lifecycle_churn_presence_transitions_keep_morphed_paths_resident_on_the_exact_sample_grid() {
    let mut objects = Vec::with_capacity(MAX_VISIBLE_COUNT);
    let mut tracks = Vec::with_capacity(LEAVING_COUNT * 2 + PULSE_COUNT * 2);
    let mut next_track = 0usize;

    // The real Dynamic Load lifecycle phase starts after two Square↔Circle
    // transforms. At that point the 600 primary shapes use retained vector-path
    // morph resources, while the 200 transient pulses are analytic circles.
    // Model that split directly so path residency/repack assertions are not
    // vacuous during presence churn.
    for index in 0..SHAPE_COUNT {
        let object = ObjectId::new(index as u64);
        let row = index / 30;
        let col = index % 30;
        let transform = Transform2D {
            translation: Vec2::new(col as f32 * 0.38, -(row as f32) * 0.255),
            ..Transform2D::IDENTITY
        };
        objects.push(CompiledObject::new(
            object,
            GeometryRef::path(retained_shape(index)),
            transform,
            shape_style(index),
        ));

        if index.is_multiple_of(3) {
            tracks.push(presence_track(next_track, object, 0.50, true, false));
            next_track += 1;
            tracks.push(presence_track(next_track, object, 0.60, false, true));
            next_track += 1;
        }
    }

    for pulse_index in 0..PULSE_COUNT {
        let source_index = pulse_index * 3;
        let row = source_index / 30;
        let col = source_index % 30;
        let object = ObjectId::new((SHAPE_COUNT + pulse_index) as u64);
        let transform = Transform2D {
            translation: Vec2::new(col as f32 * 0.38, -(row as f32) * 0.255),
            ..Transform2D::IDENTITY
        };
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(0.045),
            transform,
            Style::default(),
        ));
        tracks.push(presence_track(next_track, object, 0.10, false, true));
        next_track += 1;
        tracks.push(presence_track(next_track, object, 1.05, true, false));
        next_track += 1;
    }

    assert_eq!(next_track, LEAVING_COUNT * 2 + PULSE_COUNT * 2);

    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    let mut preparer = FramePreparer::new();

    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert_eq!(initial.path_ids.len(), SHAPE_COUNT);
    assert_eq!(initial.circle_ids.len(), 0);
    assert!(initial.stats.geometry_cache_misses > 0);
    assert!(initial.path_geometry_dirty);

    for sample_index in FIRST_SAMPLE_INDEX..FIRST_SAMPLE_INDEX + SAMPLE_COUNT {
        let local_time = sample_index as f64 / SAMPLE_HZ - LIFECYCLE_START;
        instance.advance_to(local_time).unwrap();
        let changes = instance.take_frame_changes();
        let prepared = preparer.prepare_incremental(instance.frame(), &changes);

        let expected_paths = expected_path_count(sample_index);
        let expected_pulses = expected_pulse_count(sample_index);
        let (submitted_paths, submitted_pulses) = submitted_primitive_counts(&prepared);
        assert_eq!(submitted_paths, expected_paths, "sample {sample_index}");
        assert_eq!(submitted_pulses, expected_pulses, "sample {sample_index}");
        assert_eq!(
            submitted_paths + submitted_pulses,
            expected_paths + expected_pulses,
            "sample {sample_index}"
        );
        assert_eq!(
            prepared.stats.instance_count,
            expected_paths + expected_pulses,
            "sample {sample_index}"
        );
        assert_eq!(prepared.stats.full_rebuilds, 0, "sample {sample_index}");
        assert_eq!(
            prepared.stats.geometry_cache_misses, 0,
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
        assert!(!prepared.path_geometry_dirty, "sample {sample_index}");

        let is_membership_transition = matches!(sample_index, 211 | 235 | 241);
        if is_membership_transition {
            assert!(
                prepared.stats.instances_repacked <= MAX_VISIBLE_COUNT,
                "sample {sample_index} repacked more instances than the entire visible working set"
            );
            assert!(
                prepared.stats.dirty_instance_count <= MAX_VISIBLE_COUNT,
                "sample {sample_index} dirtied more instances than the entire visible working set"
            );
            assert!(
                prepared.stats.render_order_positions_visited <= MAX_VISIBLE_COUNT,
                "sample {sample_index} visited render-order positions outside the visible working set"
            );
        } else {
            assert_eq!(
                prepared.stats.instances_repacked, 0,
                "sample {sample_index}"
            );
            assert_eq!(
                prepared.stats.dirty_instance_count, 0,
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
        }
    }
}
