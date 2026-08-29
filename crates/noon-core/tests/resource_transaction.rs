use std::sync::Arc;

use noon_core::{
    FontResourceArena, GeometryResourceArena, Rect, TextResource, TextResourceArena,
    TextResourceError, TextResourceMutationError, TextResourceMutationTransaction,
    TextResourceMutationTransactionError, TextSourceKind, Vec2,
};

fn text(source: &str) -> TextResource {
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

#[test]
fn recycled_stale_generation_rejects_whole_transaction() {
    let mut texts = TextResourceArena::new();
    let stale = texts.insert(text("old")).expect("initial text must insert");
    let unrelated = texts
        .insert(text("unrelated"))
        .expect("unrelated text must insert");

    texts.remove(stale.id).expect("old text must remove");
    let recycled = texts
        .insert(text("recycled"))
        .expect("replacement text must insert");
    assert_ne!(recycled.id, stale.id);

    let before = texts.stats();
    let geometries = GeometryResourceArena::new();
    let fonts = FontResourceArena::new();
    let mut transaction = TextResourceMutationTransaction::new();
    transaction
        .replace(unrelated, text("must-not-commit"))
        .replace(stale, text("stale"));

    assert_eq!(
        transaction.apply(&mut texts, &geometries, &fonts),
        Err(TextResourceMutationTransactionError::Mutation {
            index: 1,
            error: TextResourceMutationError::Resource(TextResourceError::UnknownResource(
                stale.id,
            )),
        })
    );

    assert_eq!(texts.current_handle(unrelated.id), Some(unrelated));
    assert_eq!(
        texts
            .get(unrelated)
            .map(|resource| resource.source.as_ref()),
        Some("unrelated")
    );
    assert_eq!(texts.current_handle(recycled.id), Some(recycled));
    assert_eq!(
        texts.get(recycled).map(|resource| resource.source.as_ref()),
        Some("recycled")
    );
    assert_eq!(texts.stats(), before);
}
