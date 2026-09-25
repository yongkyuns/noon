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
