use noon_core::{
    GeometryRef, ObjectContentRef, ObjectId, PathCommand, Style, Transform2D, Vec2, VectorPath,
};
use noon_geometry::PreparedPathInterpolation;
use noon_render_wgpu::{FramePreparer, PreparedFrame};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

fn source() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(6.0, 8.0))
        .line_to(Vec2::new(6.0, 2.0))
        .line_to(Vec2::new(2.0, 2.0))
        .line_to(Vec2::new(2.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close()
}
fn target() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close()
}
fn frame(progress: f32, unrelated: usize) -> FrameState {
    let objects = (0..=unrelated)
        .map(|index| FrameObjectState {
            id: ObjectId::new(index as u64),
            z_index: 0.0,
            content: ObjectContentRef::Geometry(if index == 0 {
                GeometryRef::path(source().with_morph_target(target()))
            } else {
                GeometryRef::circle(0.1)
            }),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style {
                stroke: None,
                stroke_width: 0.0,
                ..Style::default()
            },
            appearance: 1.0,
        })
        .collect();
    let mut morphs = vec![0.0; unrelated + 1];
    morphs[0] = progress;
    FrameState {
        time: f64::from(progress),
        objects,
        presences: vec![true; unrelated + 1],
        reveals: vec![1.0; unrelated + 1],
        morphs,
        render_geometries: vec![None; unrelated + 1],
        render_transforms: vec![None; unrelated + 1],
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
    }
}
fn cross(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn mesh_contains(prepared: &PreparedFrame<'_>, point: [f32; 2]) -> bool {
    let batch = &prepared.path_batches[0];
    prepared.path_indices[batch.index_range.start as usize..batch.index_range.end as usize]
        .as_chunks::<3>()
        .0
        .iter()
        .any(|triangle| {
            let a = prepared.path_vertices[triangle[0] as usize].position;
            let b = prepared.path_vertices[triangle[1] as usize].position;
            let c = prepared.path_vertices[triangle[2] as usize].position;
            if cross(a, b, c).abs() < 1e-8 {
                return false;
            }
            let e = [cross(a, b, point), cross(b, c, point), cross(c, a, point)];
            e.iter().all(|&v| v >= -1e-6) || e.iter().all(|&v| v <= 1e-6)
        })
}
// Independent even-odd polygon coverage oracle. The two fixtures contain only
// straight edges; canonical cubic subdivision preserves those straight segments.
fn polygon_contains(path: &VectorPath, point: [f32; 2]) -> bool {
    let points = path
        .commands()
        .iter()
        .filter_map(|command| match command {
            PathCommand::MoveTo { to }
            | PathCommand::LineTo { to }
            | PathCommand::CubicTo { to, .. }
            | PathCommand::QuadraticTo { to, .. } => Some(*to),
            PathCommand::Close => None,
        })
        .collect::<Vec<_>>();
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a.y > point[1]) != (b.y > point[1])
            && point[0] < (b.x - a.x) * (point[1] - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}
#[test]
fn concave_fill_deforms_at_intermediate_progress_without_disappearing() {
    assert!(noon_geometry::plan_filled_morph_preserving_order(
        &source(),
        &target(),
        noon_geometry::MorphOptions::DEFAULT
    )
    .is_err());
    let interpolation = PreparedPathInterpolation::new(&source(), &target()).unwrap();
    let mut preparer = FramePreparer::new();
    let mut coverages = Vec::new();
    for (step, progress) in [0.0, 0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
        let frame = frame(progress, 0);
        let changes = if step == 0 {
            FrameChanges::all()
        } else {
            FrameChanges::objects(vec![0])
        };
        let prepared = preparer.prepare_incremental(&frame, &changes);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert_eq!(prepared.path_ids, &[ObjectId::new(0)]);
        assert_eq!(prepared.paths[0].style.fill[3], 1.0);
        let expected = interpolation.interpolate(progress).unwrap();
        let mut coverage = Vec::new();
        for y in 0..37 {
            for x in 0..39 {
                let point = [
                    8.0 * (x as f32 + 0.371) / 39.0,
                    8.0 * (y as f32 + 0.613) / 37.0,
                ];
                let actual = mesh_contains(&prepared, point);
                assert_eq!(
                    actual,
                    polygon_contains(&expected, point),
                    "progress={progress}, point={point:?}"
                );
                coverage.push(actual);
            }
        }
        assert!(coverage.iter().any(|&pixel| pixel));
        coverages.push(coverage);
    }
    for pair in coverages.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}
#[test]
fn complex_fill_sampling_is_local_bounded_and_idle_is_clean() {
    let mut preparer = FramePreparer::new();
    let initial = frame(0.0, 1200);
    let order = (0..1201).rev().collect::<Vec<u32>>();
    preparer.set_painter_order(&initial, &order);
    preparer.prepare_incremental(&initial, &FrameChanges::all());
    let meshes = preparer.cached_path_mesh_count();
    for step in 1..=240 {
        let current = frame((step % 121) as f32 / 120.0, 1200);
        let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.instances_repacked, 1);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert!(prepared.stats.render_order_positions_visited <= 64);
        assert!(prepared.stats.render_order_chunks_rebuilt <= 1);
        assert_eq!(prepared.stats.mega_path_indices_repacked, 0);
        assert_eq!(
            preparer.cached_path_mesh_count(),
            meshes,
            "progress must not grow the cache"
        );
        let idle = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
        assert_eq!(idle.stats.geometry_cache_misses, 0);
        assert_eq!(idle.stats.path_vertices_repacked, 0);
        assert_eq!(idle.stats.path_indices_repacked, 0);
        assert_eq!(idle.stats.render_order_positions_visited, 0);
    }
}
#[test]
fn cold_seek_and_forward_playback_realize_identical_filled_geometry() {
    let mut warm = FramePreparer::new();
    warm.prepare(&frame(0.0, 0));
    warm.prepare_incremental(&frame(0.25, 0), &FrameChanges::objects(vec![0]));
    let middle = frame(0.5, 0);
    let actual = warm.prepare_incremental(&middle, &FrameChanges::objects(vec![0]));
    let mut cold = FramePreparer::new();
    let expected = cold.prepare(&middle);
    for y in 0..40 {
        for x in 0..40 {
            let point = [(x as f32 + 0.31) / 5.0, (y as f32 + 0.43) / 5.0];
            assert_eq!(
                mesh_contains(&actual, point),
                mesh_contains(&expected, point)
            );
        }
    }
}
