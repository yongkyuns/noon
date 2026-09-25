//! Shared admission for one retained object-or-family composite entry.

use crate::{AuthoringError, ExecutionSession, Mobject, MobjectFamily, MobjectTarget};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

/// An owned retained entry root returned by composite display families.
///
/// Construction APIs borrow [`MobjectTarget`] so callers keep their original
/// root identity. Query APIs need an owned handle and use this common form for
/// tables and matrices without flattening family entries.
#[derive(Clone, Debug)]
pub enum CompositeEntryHandle {
    Mobject(Mobject),
    Family(MobjectFamily),
}

impl PartialEq for CompositeEntryHandle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mobject(left), Self::Mobject(right)) => left == right,
            (Self::Family(left), Self::Family(right)) => {
                left.node_id() == right.node_id()
                    && Rc::ptr_eq(left.integration_store(), right.integration_store())
            }
            _ => false,
        }
    }
}

impl CompositeEntryHandle {
    pub fn as_target(&self) -> MobjectTarget<'_> {
        match self {
            Self::Mobject(object) => object.into(),
            Self::Family(family) => family.into(),
        }
    }

    pub(crate) fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Result<Self, AuthoringError> {
        let kind = store.borrow().node(node).map(|value| value.kind().clone());
        match kind {
            Some(noon_core::SemanticNodeKind::AuthoringObject) => {
                Ok(Self::Mobject(Mobject::from_node(Rc::clone(&store), node)?))
            }
            Some(noon_core::SemanticNodeKind::Family(_)) => Ok(Self::Family(
                MobjectFamily::from_node(Rc::clone(&store), node)?,
            )),
            _ => Err(AuthoringError::Semantic(
                noon_core::SemanticSceneOperationError::UnknownNode(node),
            )),
        }
    }
}

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
        let mut result: Option<noon_core::Bounds2D64> = None;
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
    capture_entries_with(store, entries, |object| {
        crate::family_layout::composite_entry_state(store, execution, root, object)
    })
}

/// The shared topology and overlap check, parameterized by the ownership-aware
/// state capture boundary.  Table and matrix admission use their owning Scene
/// or LiveSession here so Scene-owned execution observes effective placement.
pub(crate) fn capture_entries_with(
    store: &Rc<RefCell<SemanticStore>>,
    entries: &[MobjectTarget<'_>],
    mut capture_state: impl FnMut(&Mobject) -> Result<SemanticObjectState, AuthoringError>,
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
                let state = capture_state(&object)?;
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
