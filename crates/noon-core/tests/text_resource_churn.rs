use std::sync::Arc;

use noon_core::{
    remove_text_if_current, replace_text_if_current, Rect, TextResource, TextResourceArena,
    TextSourceKind, Vec2,
};

const CHURN_CYCLES: usize = 50_000;

fn text(source: impl Into<Arc<str>>) -> TextResource {
    TextResource {
        source: source.into(),
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

#[test]
fn repeated_text_replace_remove_reinsert_stays_at_one_physical_slot() {
    let mut arena = TextResourceArena::new();
    let initial = arena.insert(text("seed")).unwrap();
    let mut current = initial;
    let mut previous_generation = initial;

    for cycle in 0..CHURN_CYCLES {
        let replacement = text(Arc::<str>::from(format!("replacement-{cycle}")));
        current = replace_text_if_current(&mut arena, current, replacement).unwrap();

        assert_eq!(arena.len(), 1);
        assert_eq!(arena.slot_capacity(), 1);
        assert_eq!(arena.current_handle(current.id), Some(current));
        assert_eq!(arena.stats().live_resources, 1);
        assert_eq!(
            arena.stats().retained_bytes,
            arena.get(current).unwrap().retained_bytes()
        );

        let removed = remove_text_if_current(&mut arena, current).unwrap();
        assert_eq!(removed.source.as_ref(), format!("replacement-{cycle}"));
        assert!(arena.is_empty());
        assert_eq!(arena.slot_capacity(), 1);
        assert_eq!(arena.stats().live_resources, 0);
        assert_eq!(arena.stats().retained_bytes, 0);
        assert_eq!(arena.current_handle(current.id), None);
        assert!(arena.get(current).is_none());

        let next = arena
            .insert(text(Arc::<str>::from(format!("generation-{cycle}"))))
            .unwrap();
        assert_eq!(arena.slot_capacity(), 1);
        assert_eq!(next.version, 0);
        assert_ne!(next.id, current.id);
        assert!(arena.get(current).is_none());
        assert!(arena.get(previous_generation).is_none());

        previous_generation = current;
        current = next;
    }

    assert_eq!(arena.len(), 1);
    assert_eq!(arena.slot_capacity(), 1);
    assert_eq!(arena.current_handle(current.id), Some(current));

    remove_text_if_current(&mut arena, current).unwrap();
    assert!(arena.is_empty());
    assert_eq!(arena.slot_capacity(), 1);
    assert_eq!(arena.stats().live_resources, 0);
    assert_eq!(arena.stats().retained_bytes, 0);
    assert_eq!(arena.stats().glyphs, 0);
    assert_eq!(arena.stats().vectors, 0);
    assert_eq!(arena.stats().parts, 0);
}
