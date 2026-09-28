use super::*;
use crate::{Color, SemanticClickIndicate, SemanticObjectState, StoredGeometry, YELLOW};

fn indicate() -> SemanticClickIndicate {
    SemanticClickIndicate::new(1.2, YELLOW, 0.5)
}

fn object(store: &mut SemanticStore, geometry: StoredGeometry) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(geometry))
}

#[test]
fn click_indicate_is_object_owned_and_clearable() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, StoredGeometry::Circle { radius: 1.0 });
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_click_indicate(target, Some(indicate()));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .click_indicate(),
        Some(indicate())
    );
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ClickIndicate { object: target }]
    );

    let mut clear = SemanticMutationTransaction::new();
    clear.set_click_indicate(target, None);
    clear.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .click_indicate(),
        None
    );
}

#[test]
fn click_indicate_rejects_invalid_payload_content_and_signal_drivers_atomically() {
    let mut store = SemanticStore::new();
    let circle = object(&mut store, StoredGeometry::Circle { radius: 1.0 });
    let line = object(
        &mut store,
        StoredGeometry::Line {
            start: crate::Vec2::ZERO,
            end: crate::Vec2::new(1.0, 0.0),
        },
    );
    let clean_circle = object(&mut store, StoredGeometry::Circle { radius: 1.0 });
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    store
        .bind_semantic_signal(signal, circle, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    for (target, binding) in [
        (circle, indicate()),
        (line, indicate()),
        (
            line,
            SemanticClickIndicate::new(f64::NAN, Color::WHITE, 0.5),
        ),
        (
            clean_circle,
            SemanticClickIndicate::new(f64::from(f32::MAX) * 2.0, Color::WHITE, 0.5),
        ),
        (
            clean_circle,
            SemanticClickIndicate::new(1.2, Color::rgba(1.1, 0.0, 0.0, 1.0), 0.5),
        ),
    ] {
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_indicate(target, Some(binding));
        assert!(matches!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidClickIndicate { .. })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store
                .semantic_object_state_checked(target)
                .unwrap()
                .click_indicate(),
            None
        );
    }
}

#[test]
fn failed_later_click_indicate_keeps_earlier_declaration_unpublished() {
    let mut store = SemanticStore::new();
    let circle = object(&mut store, StoredGeometry::Circle { radius: 1.0 });
    let line = object(
        &mut store,
        StoredGeometry::Line {
            start: crate::Vec2::ZERO,
            end: crate::Vec2::new(1.0, 0.0),
        },
    );
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_click_indicate(circle, Some(indicate()))
        .set_click_indicate(line, Some(indicate()));
    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidClickIndicate { .. })
    ));
    assert_eq!(
        store
            .semantic_object_state_checked(circle)
            .unwrap()
            .click_indicate(),
        None
    );
}

#[test]
fn later_content_replacement_cannot_bypass_click_indicate_target_validation() {
    let mut store = SemanticStore::new();
    let circle = object(&mut store, StoredGeometry::Circle { radius: 1.0 });
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_click_indicate(circle, Some(indicate()))
        .replace_content(
            circle,
            StoredGeometry::Line {
                start: crate::Vec2::ZERO,
                end: crate::Vec2::new(1.0, 0.0),
            },
        );
    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidClickIndicate { .. })
    ));
    let state = store.semantic_object_state_checked(circle).unwrap();
    assert!(matches!(
        state.content.geometry(),
        Some(StoredGeometry::Circle { .. })
    ));
    assert_eq!(state.click_indicate(), None);
}

#[test]
fn click_indicate_metadata_is_pointer_sized_when_absent() {
    assert_eq!(
        std::mem::size_of::<Option<std::sync::Arc<SemanticClickIndicate>>>(),
        std::mem::size_of::<usize>()
    );
}
