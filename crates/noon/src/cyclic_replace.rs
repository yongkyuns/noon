//! Shared Manim-style cyclic target construction for flat semantic families.

use crate::{
    AuthoringError, FamilyCopy, Mobject, MobjectFamily, MobjectTarget,
    UnsupportedAuthoringOperation,
};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind, SemanticObjectProperty,
    SemanticStore,
};
use std::rc::Rc;

pub(crate) fn direct_object_members(
    family: &MobjectFamily,
) -> Result<Vec<SemanticNodeId>, AuthoringError> {
    family.validate()?;
    let store = family.integration_store().borrow();
    let members = store
        .semantic_family_members_checked(family.node_id())
        .map_err(AuthoringError::from)?
        .to_vec();
    if members.is_empty()
        || members.iter().any(|member| {
            !store
                .node(*member)
                .is_some_and(|node| matches!(node.kind(), SemanticNodeKind::AuthoringObject))
        })
    {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::CyclicReplaceFamilyTopology,
        ));
    }
    Ok(members)
}

pub(crate) fn cyclic_target_transaction(
    store: &SemanticStore,
    target_members: &[SemanticNodeId],
    source_centers: &[(f64, f64)],
) -> Result<SemanticMutationTransaction, AuthoringError> {
    if target_members.is_empty() || target_members.len() != source_centers.len() {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::CyclicReplaceFamilyTopology,
        ));
    }
    let mut transaction = SemanticMutationTransaction::new();
    for (index, target) in target_members.iter().copied().enumerate() {
        let mut translation = store
            .semantic_object_state_checked(target)
            .map_err(AuthoringError::from)?
            .transform
            .translation;
        let from = source_centers[index];
        let to = source_centers[(index + 1) % source_centers.len()];
        translation.x += to.0 - from.0;
        translation.y += to.1 - from.1;
        transaction.set_property(target, SemanticObjectProperty::Translation, translation);
    }
    Ok(transaction)
}

impl MobjectFamily {
    /// Copy this flat family and cyclically move each copied member to the next
    /// authored member position, matching ManimCE `CyclicReplace` target creation.
    pub fn cyclic_replace_target(&self) -> Result<FamilyCopy, AuthoringError> {
        self.cyclic_replace_target_with_references(&[])
    }

    /// Preserve detached wrapper metadata references in the same atomic copy used
    /// by ordinary family copying, then apply the shared cyclic placement.
    pub fn cyclic_replace_target_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<FamilyCopy, AuthoringError> {
        let members = direct_object_members(self)?;
        let centers = members
            .iter()
            .map(|member| {
                Mobject::from_node(Rc::clone(self.integration_store()), *member)?.center()
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let copied = self.copy_with_references(references)?;
        let target_members = direct_object_members(copied.root())?;
        let transaction = {
            let store = self.integration_store().borrow();
            cyclic_target_transaction(&store, &target_members, &centers)?
        };
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(AuthoringError::from)?;
        Ok(copied)
    }
}
