use super::*;
use crate::{RateFunction, SemanticClickGlowPulse, SemanticObjectState, StoredGeometry};

fn pulse(intensity: f64, run_time: f64, rate: RateFunction) -> SemanticClickAction {
    SemanticClickAction::GlowPulse(SemanticClickGlowPulse::new(intensity, run_time, rate, true))
}

fn circle(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

#[test]
fn click_glow_pulse_registration_is_inert_replaceable_and_clearable() {
    let mut store = SemanticStore::new();
    let target = circle(&mut store);
    let nodes = store.len();
    let action = pulse(2.0, 0.75, RateFunction::Linear);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_click_action(target, Some(action));
    assert_eq!(
        transaction.apply(&mut store).unwrap().impacts(),
        &[SemanticMutationImpact::ClickAction { object: target }]
    );
    assert_eq!(store.len(), nodes);
    assert!(!store.has_effect_attachments());
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .click_action(),
        Some(action)
    );
    let revision = store.scene_revision();
    let mut identical = SemanticMutationTransaction::new();
    identical.set_click_action(target, Some(action));
    assert!(identical.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);

    let copied_state = store.semantic_object_state_checked(target).unwrap().clone();
    let copied = store.insert_semantic_object(copied_state);
    let mut replace = SemanticMutationTransaction::new();
    replace.set_click_indicate(
        target,
        Some(crate::SemanticClickIndicate::new(1.1, crate::YELLOW, 0.4)),
    );
    replace.apply(&mut store).unwrap();
    assert!(matches!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .click_action(),
        Some(SemanticClickAction::Indicate(_))
    ));
    assert_eq!(
        store
            .semantic_object_state_checked(copied)
            .unwrap()
            .click_action(),
        Some(action)
    );
    let mut clear = SemanticMutationTransaction::new();
    clear.set_click_action(copied, None);
    clear.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(copied)
            .unwrap()
            .click_action(),
        None
    );
    assert!(store
        .semantic_object_state_checked(target)
        .unwrap()
        .click_action()
        .is_some());
    assert!(!store.has_effect_attachments());
}

#[test]
fn click_glow_pulse_rejects_invalid_values_atomically() {
    let mut store = SemanticStore::new();
    let target = circle(&mut store);
    let previous = pulse(1.5, 0.5, RateFunction::ThereAndBack);
    let mut initial = SemanticMutationTransaction::new();
    initial.set_click_action(target, Some(previous));
    initial.apply(&mut store).unwrap();
    let invalid = [
        pulse(f64::NAN, 1.0, RateFunction::Linear),
        pulse(f64::INFINITY, 1.0, RateFunction::Linear),
        pulse(-0.01, 1.0, RateFunction::Linear),
        pulse(8.01, 1.0, RateFunction::Linear),
        pulse(2.0, f64::NAN, RateFunction::Linear),
        pulse(2.0, f64::INFINITY, RateFunction::Linear),
        pulse(2.0, 0.0, RateFunction::Linear),
        pulse(2.0, -0.1, RateFunction::Linear),
        pulse(2.0, 1.0, RateFunction::EaseInBack),
        pulse(2.0, 1.0, RateFunction::EaseOutElastic),
    ];
    for binding in invalid {
        let revision = store.scene_revision();
        let nodes = store.len();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_action(target, Some(binding));
        assert!(matches!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidClickAction { .. })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.len(), nodes);
        assert_eq!(
            store
                .semantic_object_state_checked(target)
                .unwrap()
                .click_action(),
            Some(previous)
        );
        assert!(!store.has_effect_attachments());
    }
}

#[test]
fn click_glow_pulse_preserves_existing_content_and_signal_admission() {
    let mut store = SemanticStore::new();
    let clean = circle(&mut store);
    let driven = circle(&mut store);
    let line = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Line {
        start: crate::Vec2::ZERO,
        end: crate::Vec2::new(1.0, 0.0),
    }));
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    store
        .bind_semantic_signal(signal, driven, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    for target in [line, driven] {
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_click_action(clean, Some(pulse(2.0, 0.5, RateFunction::ThereAndBack)))
            .set_click_action(target, Some(pulse(2.0, 0.5, RateFunction::ThereAndBack)));
        assert!(matches!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidClickAction { .. })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store
                .semantic_object_state_checked(clean)
                .unwrap()
                .click_action(),
            None
        );
        assert!(!store.has_effect_attachments());
    }
}
