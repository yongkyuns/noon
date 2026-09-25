//! Shared admission for one retained object-or-family composite entry.

use crate::{AuthoringError, ExecutionSession, Mobject, MobjectTarget};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

/// One supplied table/matrix entry and the leaves that must move with its root.
#[derive(Clone, Debug)]
pub(crate) struct CompositeEntry {
    root: SemanticNodeId,
    leaves: Vec<(Mobject, SemanticObjectState)>,
}

impl CompositeEntry {
    pub(crate) fn root(&self) -> SemanticNodeId {
        self.root
    }
    pub(crate) fn leaves(&self) -> &[(Mobject, SemanticObjectState)] {
        &self.leaves
    }
    pub(crate) fn bounds(&self) -> Result<Option<noon_core::Bounds2D64>, AuthoringError> {
        let mut result = None;
        for (object, state) in &self.leaves {
            if let Some(next) = crate::semantic_mobject::layout_for_content(
                &object.integration_store().borrow(),
                state.content,
                state.transform,
            )? {
                if let Some(bounds) = &mut result {
                    bounds.include(next.min_x, next.min_y);
                    bounds.include(next.max_x, next.max_y);
                } else {
                    result = Some(next);
                }
            }
        }
        Ok(result)
    }
}

/// Capture each entry at one valid authored/effective placement state.
///
/// Families retain their root identity in the caller's topology while every
/// ordered leaf is captured and later translated exactly once.  Descendant
/// overlap is rejected before any transaction is created.
pub(crate) fn capture_entries(
    store: &Rc<RefCell<SemanticStore>>,
    execution: Option<&ExecutionSession>,
    root: SemanticNodeId,
    entries: &[MobjectTarget<'_>],
) -> Result<Vec<CompositeEntry>, AuthoringError> {
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .map(|entry| {
            entry.require_store(store)?;
            let leaves = store
                .borrow()
                .ordered_leaf_nodes(entry.node_id())
                .map_err(AuthoringError::from)?;
            let mut captured = Vec::with_capacity(leaves.len());
            for node in leaves {
                if !seen.insert(node) {
                    return Err(AuthoringError::Semantic(
                        noon_core::SemanticSceneOperationError::DuplicateMembershipTarget(node),
                    ));
                }
                let object = Mobject::from_node(Rc::clone(store), node)?;
                let state =
                    crate::family_layout::composite_entry_state(store, execution, root, &object)?;
                captured.push((object, state));
            }
            Ok(CompositeEntry {
                root: entry.node_id(),
                leaves: captured,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobjectFamily, SemanticObjectState, StoredGeometry};

    fn circle(store: &Rc<RefCell<SemanticStore>>) -> Mobject {
        Mobject::new(
            Rc::clone(store),
            SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
        )
        .unwrap()
    }

    #[test]
    fn family_entry_preserves_root_and_captures_its_ordered_leaves() {
        let store = Rc::new(RefCell::new(SemanticStore::new()));
        let first = circle(&store);
        let second = circle(&store);
        let family =
            MobjectFamily::create(Rc::clone(&store), &[(&first).into(), (&second).into()]).unwrap();
        let entries = capture_entries(&store, None, first.node_id(), &[(&family).into()]).unwrap();
        assert_eq!(entries[0].root(), family.node_id());
        assert_eq!(
            entries[0]
                .leaves()
                .iter()
                .map(|(object, _)| object.clone())
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert!(entries[0].bounds().unwrap().is_some());
    }

    #[test]
    fn overlapping_family_descendants_are_rejected_before_publication() {
        let store = Rc::new(RefCell::new(SemanticStore::new()));
        let object = circle(&store);
        let family = MobjectFamily::create(Rc::clone(&store), &[(&object).into()]).unwrap();
        assert!(capture_entries(
            &store,
            None,
            object.node_id(),
            &[(&object).into(), (&family).into()]
        )
        .is_err());
    }
}
