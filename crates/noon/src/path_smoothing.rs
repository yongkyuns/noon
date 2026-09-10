//! Atomic handle edits across a family's authoritative unique leaves.
use crate::{path_editing::*, AuthoringError, MobjectFamily};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectState, SemanticStore,
    StoredGeometry, VectorPath,
};

pub(crate) struct PreparedAnchorEdits {
    states: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    paths: Vec<VectorPath>,
}

impl PreparedAnchorEdits {
    pub(crate) fn prepare(
        store: &SemanticStore,
        states: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState)>,
        smooth: bool,
    ) -> Result<Self, AuthoringError> {
        let mut result = Self {
            states: Vec::new(),
            paths: Vec::new(),
        };
        for (node, captured) in states {
            let before = store.semantic_object_state_checked(node)?;
            let after = path_replacement_state(captured.clone())?;
            let Some(path) = PathEdit::AnchorMode(smooth).prepare(store, &captured)? else {
                continue;
            };
            if path_is_unchanged(store, before, &after, &path) {
                continue;
            }
            result.states.push((node, before.clone(), after));
            result.paths.push(path);
        }
        Ok(result)
    }

    pub(crate) fn publish<T, E>(
        self,
        store: &mut SemanticStore,
        publish: impl FnOnce(&mut SemanticStore, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<noon_core::GeometryResourceError>,
    {
        store.with_geometry_paths(self.paths, |store, handles| {
            let mut transaction = SemanticMutationTransaction::new();
            for ((node, before, mut after), handle) in self.states.into_iter().zip(handles) {
                after.content = StoredGeometry::Resource(*handle).into();
                crate::semantic_mobject::stage_state_changes(
                    &mut transaction,
                    node,
                    &before,
                    &after,
                );
            }
            publish(store, transaction)
        })
    }
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
        PreparedAnchorEdits::prepare(&store, states, smooth)?.publish(
            &mut store,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}
