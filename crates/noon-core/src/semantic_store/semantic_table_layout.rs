//! Sparse authored Table layout declarations on ordinary family roots.

use crate::SemanticNodeId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticTableLayout {
    h_buff: f64,
    v_buff: f64,
    label_buff: f64,
    include_outer_lines: bool,
}
impl SemanticTableLayout {
    pub const fn new(h_buff: f64, v_buff: f64, label_buff: f64, include_outer_lines: bool) -> Self {
        Self {
            h_buff,
            v_buff,
            label_buff,
            include_outer_lines,
        }
    }
    pub const fn h_buff(self) -> f64 {
        self.h_buff
    }
    pub const fn v_buff(self) -> f64 {
        self.v_buff
    }
    pub const fn label_buff(self) -> f64 {
        self.label_buff
    }
    pub const fn include_outer_lines(self) -> bool {
        self.include_outer_lines
    }
    pub fn is_valid(self) -> bool {
        [self.h_buff, self.v_buff, self.label_buff]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticMutationTransaction, SemanticNodeCreation, SemanticStore};

    #[test]
    fn invalid_owner_or_value_rolls_back_and_retirement_releases_layout() {
        let mut store = SemanticStore::new();
        let mut tx = SemanticMutationTransaction::new();
        let object = tx.create_node(SemanticNodeCreation::object(
            crate::SemanticObjectState::new(crate::StoredGeometry::Circle { radius: 1.0 }),
        ));
        tx.set_table_layout(object, SemanticTableLayout::new(1.0, 1.0, 0.5, false));
        assert!(tx.apply(&mut store).is_err());

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        tx.set_table_layout(root, SemanticTableLayout::new(f64::NAN, 1.0, 0.5, false));
        assert!(tx.apply(&mut store).is_err());

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        tx.set_table_layout(root, SemanticTableLayout::new(2.0, 1.0, 0.5, true));
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        assert_eq!(
            store.semantic_table_layout(root).unwrap().unwrap().h_buff(),
            2.0
        );
        store.remove_node(root).unwrap();
        assert!(store.semantic_table_layout(root).is_err());
    }
}
