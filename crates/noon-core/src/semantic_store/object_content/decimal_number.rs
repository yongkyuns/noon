//! Retained authored inputs for one numeric text object.
//!
//! This is semantic metadata, rather than a renderer subtype or a frontend
//! cache.  The rendered object remains ordinary retained text.

use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SemanticDecimalNumber {
    value_bits: u64,
    decimal_places: u32,
    include_sign: bool,
    group_with_commas: bool,
    show_ellipsis: bool,
    unit: Option<Arc<str>>,
    font_size_bits: u32,
}

impl SemanticDecimalNumber {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: f64,
        decimal_places: u32,
        include_sign: bool,
        group_with_commas: bool,
        show_ellipsis: bool,
        unit: Option<Arc<str>>,
        font_size: f32,
    ) -> Self {
        Self {
            value_bits: if value == 0.0 { 0 } else { value.to_bits() },
            decimal_places,
            include_sign,
            group_with_commas,
            show_ellipsis,
            unit,
            font_size_bits: font_size.to_bits(),
        }
    }

    pub const fn value(&self) -> f64 {
        f64::from_bits(self.value_bits)
    }
    pub const fn decimal_places(&self) -> u32 {
        self.decimal_places
    }
    pub const fn include_sign(&self) -> bool {
        self.include_sign
    }
    pub const fn group_with_commas(&self) -> bool {
        self.group_with_commas
    }
    pub const fn show_ellipsis(&self) -> bool {
        self.show_ellipsis
    }
    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }
    pub const fn font_size(&self) -> f32 {
        f32::from_bits(self.font_size_bits)
    }

    pub fn is_valid(&self) -> bool {
        self.value().is_finite()
            && self.decimal_places <= 12
            && self.font_size().is_finite()
            && self.font_size() > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticMutationTransaction, SemanticObjectState, SemanticStore, StoredGeometry};

    #[test]
    fn canonicalizes_negative_zero_and_rejects_invalid_inputs() {
        let a = SemanticDecimalNumber::new(-0.0, 2, false, true, false, None, 48.0);
        let b = SemanticDecimalNumber::new(0.0, 2, false, true, false, None, 48.0);
        assert_eq!(a, b);
        assert!(
            !SemanticDecimalNumber::new(f64::NAN, 2, false, true, false, None, 48.0).is_valid()
        );
        assert!(!SemanticDecimalNumber::new(1.0, 13, false, true, false, None, 48.0).is_valid());
    }

    #[test]
    fn numeric_metadata_requires_text_and_rejects_the_whole_transaction() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let before = store.semantic_object_state_checked(object).unwrap().clone();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(object, crate::SemanticObjectProperty::RotationZ, 0.5)
            .replace_decimal_number(
                object,
                SemanticDecimalNumber::new(1.0, 2, false, true, false, None, 48.0),
            );
        assert!(transaction.apply(&mut store).is_err());
        assert_eq!(
            store.semantic_object_state_checked(object).unwrap(),
            &before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn visual_replacement_does_not_retain_receiver_numeric_metadata() {
        let mut receiver = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        receiver.set_decimal_number(Some(SemanticDecimalNumber::new(
            1.0, 2, false, true, false, None, 48.0,
        )));
        let target = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: crate::Vec2::new(2.0, 2.0),
        });
        assert!(receiver
            .with_visual_state_from(&target)
            .decimal_number()
            .is_none());
    }
}
