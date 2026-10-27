//! Root-local copy, clear and append through one semantic transaction.
mod alignment;
use crate::{
    family_copy::{
        prepare_semantic_copy_with_root_membership, CopyRootMembership, PendingSemanticCopy,
    },
    path_editing::{prepare_object_edit, PathEdit, PreparedPathEdits},
    AuthoringError, CompositeEntryHandle, Mobject, MobjectTarget, SemanticCopy,
};
pub(crate) use alignment::alignment_side;
pub use alignment::{PointAlignment, PointAlignmentSide};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNodeId, SemanticObjectState,
    SemanticStore,
};

pub(crate) fn prepare_point_promotion<E: From<AuthoringError>>(
    source: MobjectTarget<'_>,
    references: &[MobjectTarget<'_>],
    mut capture: impl FnMut(&Mobject, bool) -> Result<SemanticObjectState, E>,
    capture_family_style: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        &mut noon_core::SemanticStyle,
    ) -> Result<(), E>,
    copy_effects: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        SemanticLocalNodeToken,
        &mut SemanticMutationTransaction,
    ) -> Result<(), E>,
) -> Result<(PreparedPathEdits, PendingSemanticCopy), E> {
    source.validate()?;
    let root = source.node_id();
    {
        let store = source.integration_store().borrow();
        let node = store.node(root).expect("validated promotion root");
        let structured_table = matches!(node.kind(), noon_core::SemanticNodeKind::Family(_))
            && store
                .semantic_table_layout(root)
                .map_err(AuthoringError::from)?
                .is_some();
        if node.graph_declaration().is_some()
            || structured_table
            || node.semantic_object_state().is_some_and(|state| {
                !matches!(state.role(), noon_core::SemanticObjectRole::Ordinary)
            })
        {
            return Err(AuthoringError::from(
                crate::UnsupportedAuthoringOperation::PointPromotionStructuredRoot,
            )
            .into());
        }
    }
    let mut root_state = None;
    let (mut transaction, pending) = prepare_semantic_copy_with_root_membership(
        source,
        references,
        CopyRootMembership::Omit,
        |object, graph_row| {
            let state = capture(object, graph_row)?;
            if object.node_id() == root {
                root_state = Some(state.clone());
            }
            Ok(state)
        },
        capture_family_style,
        copy_effects,
    )?;
    // CE replaces the public list when appending. Existing readers retain the
    // original member container, while the current root acquires the copy.
    transaction.replace_member_container(root);
    transaction.add_member(root, pending.root_token());
    let store = source.integration_store().borrow();
    let prepared = if let Some(state) = root_state {
        // Clearing an opaque resource must not silently replace it with a path.
        crate::path_queries::content_path(&store, state.content).map_err(E::from)?;
        match prepare_object_edit(&store, root, state, PathEdit::Clear).map_err(E::from)? {
            Some(prepared) => prepared,
            None => PreparedPathEdits::prepare(&store, []).map_err(E::from)?,
        }
    } else {
        PreparedPathEdits::prepare(&store, []).map_err(E::from)?
    };
    Ok((prepared.with_transaction(transaction), pending))
}

impl MobjectTarget<'_> {
    /// Append a child with this root's own points and no direct members, then
    /// clear only this root's points. Existing members and root identity survive.
    /// The returned copy map supports host metadata reconstruction.
    pub fn promote_own_points(&self) -> Result<SemanticCopy<CompositeEntryHandle>, AuthoringError> {
        self.promote_own_points_with_references(&[])
    }

    /// Include additional metadata references in the same atomic copy. Aliases
    /// are copied once but are not added to the promoted child's membership.
    /// Opaque resources and structured roots currently reject before mutation.
    pub fn promote_own_points_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<SemanticCopy<CompositeEntryHandle>, AuthoringError> {
        let (prepared, pending) = prepare_point_promotion(
            *self,
            references,
            |object, _| object.state(),
            |_, _, _| Ok(()),
            |store, source, target, transaction| {
                store
                    .copy_effects_into(source, target, transaction)
                    .map_err(AuthoringError::from)
            },
        )?;
        let result = prepared.publish(
            &mut self.integration_store().borrow_mut(),
            |store, transaction| transaction.apply(store).map_err(AuthoringError::from),
        )?;
        pending.resolve(&result, CompositeEntryHandle::from_node)
    }
}

#[cfg(test)]
mod tests;
