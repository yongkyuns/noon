use std::sync::Arc;

use noon_core::{
    FontFaceIdentity, FontResourceArena, GeometryResource, GeometryResourceArena, Rect,
    TextResource, TextResourceArena, TextSourceKind, Vec2, VectorPath,
};

const CHURN_ITERATIONS: u64 = 1_000;

fn path(seed: u64) -> VectorPath {
    let offset = seed as f32 * 0.001;
    VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0 + offset, 0.5))
        .line_to(Vec2::new(0.5, 1.0 + offset))
}

fn empty_text(source: &str) -> TextResource {
    TextResource {
        source: Arc::from(source),
        kind: TextSourceKind::Plain,
        runs: Arc::from([]),
        vector_items: Arc::from([]),
        render_items: Arc::from([]),
        parts: Arc::from([]),
        bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
        baseline: 0.0,
        layout_artifact: None,
    }
}

fn face(variation: &str) -> FontFaceIdentity {
    FontFaceIdentity {
        family: Arc::from("Churn Sans"),
        face_key: Arc::from("churn-sans-v1"),
        face_index: 0,
        variation_key: Arc::from(variation),
    }
}

#[test]
fn geometry_replacement_churn_keeps_one_live_resource_and_stable_accounting() {
    let mut arena = GeometryResourceArena::new();
    let initial = arena.insert_path(path(0));
    let baseline = arena.stats();
    let mut current = initial;

    for iteration in 1..=CHURN_ITERATIONS {
        let next = arena
            .replace(
                initial.id,
                GeometryResource::VectorPath(Arc::new(path(iteration))),
            )
            .expect("bounded replacement must remain valid");

        assert_eq!(next.id, initial.id);
        assert_eq!(next.version, iteration);
        assert!(arena.get(current).is_none(), "old handle must be stale");
        assert!(arena.get(next).is_some(), "new handle must resolve");
        assert_eq!(
            arena.stats(),
            baseline,
            "accounting must stay on its plateau"
        );
        current = next;
    }

    arena
        .remove(initial.id)
        .expect("final removal must succeed");
    let released = arena.stats();
    assert_eq!(released.live_resources, 0);
    assert_eq!(released.retained_bytes, 0);
    assert_eq!(released.path_command_bytes, 0);
}

#[test]
fn text_replacement_churn_keeps_one_live_resource_and_stable_accounting() {
    let mut arena = TextResourceArena::new();
    let initial = arena.insert(empty_text("a")).expect("valid text resource");
    let baseline = arena.stats();
    let mut current = initial;

    for iteration in 1..=CHURN_ITERATIONS {
        let source = if iteration % 2 == 0 { "a" } else { "b" };
        let next = arena
            .replace(initial.id, empty_text(source))
            .expect("bounded replacement must remain valid");

        assert_eq!(next.id, initial.id);
        assert_eq!(next.version, iteration);
        assert!(arena.get(current).is_none(), "old handle must be stale");
        assert!(arena.get(next).is_some(), "new handle must resolve");
        assert_eq!(
            arena.stats(),
            baseline,
            "accounting must stay on its plateau"
        );
        current = next;
    }

    arena
        .remove(initial.id)
        .expect("final removal must succeed");
    let released = arena.stats();
    assert_eq!(released.live_resources, 0);
    assert_eq!(released.retained_bytes, 0);
    assert_eq!(released.glyphs, 0);
    assert_eq!(released.vectors, 0);
    assert_eq!(released.parts, 0);
}

#[test]
fn repeated_font_interning_reuses_one_immutable_buffer() {
    let mut arena = FontResourceArena::new();
    let bytes: Arc<[u8]> = Arc::from([1_u8, 2, 3, 4]);
    let initial = arena
        .intern_face(&face("wght=400"), bytes.clone())
        .expect("initial font must intern");
    let baseline = arena.stats();

    for iteration in 1..=CHURN_ITERATIONS {
        let variation = format!("wght={}", 400 + iteration % 300);
        let handle = arena
            .intern_face(&face(&variation), bytes.clone())
            .expect("identical immutable font bytes must be reusable");

        assert_eq!(handle, initial);
        assert_eq!(
            arena.stats(),
            baseline,
            "font accounting must stay on its plateau"
        );
    }

    assert_eq!(arena.len(), 1);
    assert_eq!(arena.stats().font_bytes, bytes.len());
}
