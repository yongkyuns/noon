//! Atomic admission of ordinary path leaves and their containing family.
//! Geometry resources and semantic nodes share the caller's publication boundary.

use crate::AuthoringError;
use noon_core::{
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticStore, SemanticStyle,
};

pub(crate) fn publish_path_family_with(
    store: &mut SemanticStore,
    paths: Vec<(noon_core::VectorPath, SemanticStyle)>,
    publish: impl FnOnce(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<SemanticNodeId, AuthoringError> {
    let (paths, styles): (Vec<_>, Vec<_>) = paths.into_iter().unzip();
    store.with_geometry_paths(paths, |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        for (handle, style) in handles.iter().zip(styles) {
            let mut state = crate::semantic_mobject::manim_path_resource_state(*handle);
            state.style = style;
            let leaf = transaction.create_node(SemanticNodeCreation::object(state));
            transaction.add_member(family, leaf);
        }
        publish(store, transaction)?
            .resolve(family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family))
    })
}
