//! Atomic handle edits across a family's authoritative unique leaves.
use crate::{path_editing::*, AuthoringError, MobjectFamily};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore};

pub(crate) fn prepare_anchor_edits(
    store: &SemanticStore,
    states: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState)>,
    smooth: bool,
) -> Result<PreparedPathEdits, AuthoringError> {
    let mut replacements = Vec::new();
    for (node, state) in states {
        if let Some(path) = PathEdit::AnchorMode(smooth).prepare(store, &state)? {
            replacements.push((node, state, path));
        }
    }
    PreparedPathEdits::prepare(store, replacements)
}

impl MobjectFamily {
    pub fn make_smooth(&self) -> Result<(), AuthoringError> {
        self.change_anchor_mode(true)
    }
    pub fn make_jagged(&self) -> Result<(), AuthoringError> {
        self.change_anchor_mode(false)
    }

    fn change_anchor_mode(&self, smooth: bool) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut store = self.integration_store().borrow_mut();
        let states = store
            .ordered_leaf_nodes(self.node_id())?
            .into_iter()
            .map(|node| Ok((node, store.semantic_object_state_checked(node)?.clone())))
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        prepare_anchor_edits(&store, states, smooth)?.publish(&mut store, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}
