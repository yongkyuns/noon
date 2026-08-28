use std::sync::Arc;

use noon_core::{
    Rect, TextRenderItem, TextResource, TextResourceArena, TextResourceError, TextResourceStats,
    TextResourceValidationError, TextSourceKind, Vec2,
};

const CHURN_ITERATIONS: u64 = 1_000;

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

fn invalid_text(source: &str) -> TextResource {
    let mut resource = empty_text(source);
    resource.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);
    resource
}

#[test]
fn repeated_invalid_text_insertions_do_not_consume_ids_or_accounting() {
    let mut arena = TextResourceArena::new();

    for iteration in 0..CHURN_ITERATIONS {
        let source = format!("invalid-{iteration}");
        assert_eq!(
            arena.insert(invalid_text(&source)),
            Err(TextResourceValidationError::InvalidRenderItem),
        );
        assert_eq!(arena.stats(), TextResourceStats::default());
        assert!(arena.is_empty());
    }

    let first = arena
        .insert(empty_text("valid"))
        .expect("first valid resource must still receive the first identity");
    assert_eq!(first.id.get(), 0);
    assert_eq!(first.version, 0);
}

#[test]
fn repeated_invalid_text_replacements_preserve_live_version_and_plateau() {
    let mut arena = TextResourceArena::new();
    let initial = arena
        .insert(empty_text("stable"))
        .expect("initial resource must be valid");
    let baseline = arena.stats();

    for iteration in 0..CHURN_ITERATIONS {
        let source = format!("invalid-replacement-{iteration}");
        assert_eq!(
            arena.replace(initial.id, invalid_text(&source)),
            Err(TextResourceError::InvalidResource(
                TextResourceValidationError::InvalidRenderItem,
            )),
        );
        assert_eq!(arena.stats(), baseline);
        assert_eq!(arena.current_handle(initial.id), Some(initial));
        assert_eq!(
            arena
                .get(initial)
                .expect("failed replacement must retain prior resource")
                .source
                .as_ref(),
            "stable",
        );
    }
}
