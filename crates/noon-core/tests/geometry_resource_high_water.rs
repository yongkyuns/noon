use std::sync::Arc;

use noon_core::{GeometryResource, GeometryResourceArena, Vec2, VectorPath};

const CHURN_ITERATIONS: u64 = 1_000;
const LARGE_PATH_SEGMENTS: usize = 1_024;

fn path(segment_count: usize) -> VectorPath {
    let mut path = VectorPath::new().move_to(Vec2::ZERO);
    for index in 0..segment_count {
        path = path.line_to(Vec2::new(index as f32 * 0.01, (index % 17) as f32));
    }
    path
}

#[test]
fn large_temporary_geometry_returns_to_small_retained_byte_plateau() {
    let mut arena = GeometryResourceArena::new();
    let initial = arena.insert_path(path(1));
    let small_plateau = arena.stats();
    let mut current = initial;

    for iteration in 0..CHURN_ITERATIONS {
        let large = arena
            .replace(
                initial.id,
                GeometryResource::VectorPath(Arc::new(path(LARGE_PATH_SEGMENTS))),
            )
            .expect("large temporary geometry must replace the live resource");
        let high_water = arena.stats();

        assert_eq!(high_water.live_resources, 1);
        assert!(high_water.retained_bytes > small_plateau.retained_bytes);
        assert!(high_water.path_command_bytes > small_plateau.path_command_bytes);
        assert!(
            arena.get(current).is_none(),
            "previous handle must be stale"
        );
        assert!(arena.get(large).is_some(), "large handle must resolve");

        let small = arena
            .replace(initial.id, GeometryResource::VectorPath(Arc::new(path(1))))
            .expect("small working set must replace the temporary geometry");
        let recovered = arena.stats();

        assert_eq!(
            recovered, small_plateau,
            "iteration {iteration} must release the temporary high-water payload"
        );
        assert!(arena.get(large).is_none(), "temporary handle must be stale");
        assert!(arena.get(small).is_some(), "small handle must resolve");
        current = small;
    }

    arena
        .remove(initial.id)
        .expect("final resource removal must succeed");
    let released = arena.stats();
    assert_eq!(released.live_resources, 0);
    assert_eq!(released.retained_bytes, 0);
    assert_eq!(released.path_command_bytes, 0);
}
