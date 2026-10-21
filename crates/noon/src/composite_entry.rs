//! Shared admission for one retained object-or-family composite entry.

use crate::{AuthoringError, Mobject, MobjectFamily, MobjectTarget};
#[cfg(any(feature = "native-text", feature = "latex", test))]
use noon_core::SemanticObjectState;
use noon_core::{SemanticNodeId, SemanticStore};
#[cfg(any(feature = "native-text", feature = "latex", test))]
use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

/// An owned retained entry root returned by composite display families.
///
/// Construction APIs borrow [`MobjectTarget`] so callers keep their original
/// root identity. Query APIs need an owned handle and use this common form for
/// ordinary families, tables and matrices without flattening family entries.
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
    pub(crate) fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        match self {
            Self::Mobject(value) => value.integration_store(),
            Self::Family(value) => value.integration_store(),
        }
    }
    pub(crate) fn node_id(&self) -> SemanticNodeId {
        self.as_target().node_id()
    }
    pub(crate) fn validate(&self) -> Result<(), AuthoringError> {
        self.as_target().validate()
    }
    pub(crate) fn get_num_points(&self) -> Result<usize, AuthoringError> {
        self.as_target().get_num_points()
    }
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
        // Read only the kind tag; a family declaration can contain large data.
        let is_family = store
            .borrow()
            .node(node)
            .and_then(|value| match value.kind() {
                noon_core::SemanticNodeKind::AuthoringObject => Some(false),
                noon_core::SemanticNodeKind::Family(_) => Some(true),
                _ => None,
            });
        match is_family {
            Some(false) => Ok(Self::Mobject(Mobject::from_node(Rc::clone(&store), node)?)),
            Some(true) => Ok(Self::Family(MobjectFamily::from_node(
                Rc::clone(&store),
                node,
            )?)),
            _ => Err(AuthoringError::Semantic(
                noon_core::SemanticSceneOperationError::UnknownNode(node),
            )),
        }
    }
}

/// One supplied table/matrix entry and the leaves that must move with its root.
#[cfg(any(feature = "native-text", feature = "latex", test))]
#[derive(Clone, Debug)]
pub(crate) struct CompositeEntry {
    root: SemanticNodeId,
    leaves: Vec<(Mobject, SemanticObjectState)>,
}

#[cfg(any(feature = "native-text", feature = "latex", test))]
impl CompositeEntry {
    pub(crate) fn root(&self) -> SemanticNodeId {
        self.root
    }
    pub(crate) fn leaves(&self) -> &[(Mobject, SemanticObjectState)] {
        &self.leaves
    }
    #[cfg(any(feature = "latex", test))]
    pub(crate) fn bounds(&self) -> Result<Option<noon_core::Bounds2D64>, AuthoringError> {
        self.measured_bounds(false)
    }
    #[cfg(any(feature = "native-text", feature = "latex"))]
    pub(crate) fn boundary_bounds(&self) -> Result<Option<noon_core::Bounds2D64>, AuthoringError> {
        self.measured_bounds(true)
    }
    fn measured_bounds(
        &self,
        anchors: bool,
    ) -> Result<Option<noon_core::Bounds2D64>, AuthoringError> {
        let mut result: Option<noon_core::Bounds2D64> = None;
        for (object, state) in &self.leaves {
            let measure = if anchors {
                crate::semantic_mobject::boundary_for_content
            } else {
                crate::semantic_mobject::layout_for_content
            };
            if let Some(next) = measure(
                &object.integration_store().borrow(),
                state.content,
                state
                    .transform
                    .as_planar()
                    .ok_or(AuthoringError::NonFiniteObjectState)?,
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

/// The shared topology and overlap check, parameterized by the ownership-aware
/// state capture boundary.  Table and matrix admission use their owning Scene
/// or LiveSession here so Scene-owned execution observes effective placement.
#[cfg(any(feature = "latex", test))]
pub(crate) fn capture_entries_with(
    store: &Rc<RefCell<SemanticStore>>,
    entries: &[MobjectTarget<'_>],
    mut capture_state: impl FnMut(&Mobject) -> Result<SemanticObjectState, AuthoringError>,
) -> Result<Vec<CompositeEntry>, AuthoringError> {
    capture_entries(store, entries, false, &mut capture_state)
}

#[cfg(any(feature = "native-text", feature = "latex"))]
pub(crate) fn capture_entries_with_aliases(
    store: &Rc<RefCell<SemanticStore>>,
    entries: &[MobjectTarget<'_>],
    mut capture_state: impl FnMut(&Mobject) -> Result<SemanticObjectState, AuthoringError>,
) -> Result<Vec<CompositeEntry>, AuthoringError> {
    capture_entries(store, entries, true, &mut capture_state)
}

#[cfg(any(feature = "native-text", feature = "latex", test))]
fn capture_entries(
    store: &Rc<RefCell<SemanticStore>>,
    entries: &[MobjectTarget<'_>],
    allow_aliases: bool,
    capture_state: &mut impl FnMut(&Mobject) -> Result<SemanticObjectState, AuthoringError>,
) -> Result<Vec<CompositeEntry>, AuthoringError> {
    let mut states = BTreeMap::<SemanticNodeId, SemanticObjectState>::new();
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
                if !allow_aliases && states.contains_key(&node) {
                    return Err(AuthoringError::Semantic(
                        noon_core::SemanticSceneOperationError::DuplicateMembershipTarget(node),
                    ));
                }
                let object = Mobject::from_node(Rc::clone(store), node)?;
                let state = match states.get(&node) {
                    Some(state) => state.clone(),
                    None => {
                        let state = capture_state(&object)?;
                        states.insert(node, state.clone());
                        state
                    }
                };
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
        let entries = capture_entries_with(&store, &[(&family).into()], |object| {
            crate::family_layout::composite_entry_state(&store, None, first.node_id(), object)
        })
        .unwrap();
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
        assert!(
            capture_entries_with(&store, &[(&object).into(), (&family).into()], |entry| {
                crate::family_layout::composite_entry_state(&store, None, object.node_id(), entry)
            })
            .is_err()
        );
    }
}
