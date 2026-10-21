//! Direct-child edits for ordinary objects and families use one transaction path.
use crate::{AuthoringError, Mobject, MobjectFamily, MobjectTarget};
use noon_core::SemanticMutationTransaction;

pub(crate) fn replace_members_transaction(
    root: MobjectTarget<'_>,
    members: &[MobjectTarget<'_>],
) -> Result<SemanticMutationTransaction, AuthoringError> {
    root.validate()?;
    for member in members {
        member.require_store(root.integration_store())?;
    }
    let store = root.integration_store().borrow();
    let node = store.semantic_authoring_node_checked(root.node_id())?;
    let current = node.members();
    let desired = members
        .iter()
        .map(MobjectTarget::node_id)
        .collect::<Vec<_>>();
    let mut transaction = SemanticMutationTransaction::new();
    if current == desired && !store.member_container_is_observed(root.node_id()) {
        return Ok(transaction);
    }
    transaction.replace_member_container(root.node_id());
    let current_set = current
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let desired_set = desired
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for member in current
        .iter()
        .copied()
        .filter(|member| !desired_set.contains(member))
    {
        transaction.remove_member(root.node_id(), member);
    }
    for member in desired
        .iter()
        .copied()
        .filter(|member| !current_set.contains(member))
    {
        transaction.add_member(root.node_id(), member);
    }
    let mut before = None;
    for member in desired.iter().rev().copied() {
        transaction.reorder_member(root.node_id(), member, before);
        before = Some(member);
    }
    Ok(transaction)
}

impl MobjectTarget<'_> {
    pub fn add_many(&self, members: &[MobjectTarget<'_>]) -> Result<Vec<bool>, AuthoringError> {
        self.edit_members(members, true)
    }
    pub fn remove_many(&self, members: &[MobjectTarget<'_>]) -> Result<Vec<bool>, AuthoringError> {
        self.edit_members(members, false)
    }
    fn edit_members(
        &self,
        members: &[MobjectTarget<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, AuthoringError> {
        let (transaction, changed) =
            crate::family_authoring::family_membership_transaction(*self, members, adding)?;
        transaction.apply(&mut self.integration_store().borrow_mut())?;
        Ok(changed)
    }
    pub fn prepend_many(&self, members: &[MobjectTarget<'_>]) -> Result<(), AuthoringError> {
        let transaction = crate::family_authoring::family_prepend_transaction(*self, members)?;
        transaction.apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
    pub fn insert_member(
        &self,
        index: usize,
        member: MobjectTarget<'_>,
    ) -> Result<(), AuthoringError> {
        let transaction = crate::family_authoring::family_insert_transaction(*self, index, member)?;
        transaction.apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
    /// Replace the current direct-member container while retained readers keep the old one.
    pub fn replace_members(&self, members: &[MobjectTarget<'_>]) -> Result<(), AuthoringError> {
        let transaction = replace_members_transaction(*self, members)?;
        transaction.apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
}

macro_rules! membership {
    ($target:ty) => {
        impl $target {
            /// Move incoming members to the tail, using their last-occurrence order.
            pub fn add(&self, member: MobjectTarget<'_>) -> Result<bool, AuthoringError> {
                Ok(self.add_many(&[member])?[0])
            }
            pub fn remove(&self, member: MobjectTarget<'_>) -> Result<bool, AuthoringError> {
                Ok(self.remove_many(&[member])?[0])
            }
            pub fn add_many(
                &self,
                members: &[MobjectTarget<'_>],
            ) -> Result<Vec<bool>, AuthoringError> {
                MobjectTarget::from(self).add_many(members)
            }
            pub fn remove_many(
                &self,
                members: &[MobjectTarget<'_>],
            ) -> Result<Vec<bool>, AuthoringError> {
                MobjectTarget::from(self).remove_many(members)
            }
            pub fn prepend_many(
                &self,
                members: &[MobjectTarget<'_>],
            ) -> Result<(), AuthoringError> {
                MobjectTarget::from(self).prepend_many(members)
            }
            pub fn insert_member(
                &self,
                index: usize,
                member: MobjectTarget<'_>,
            ) -> Result<(), AuthoringError> {
                MobjectTarget::from(self).insert_member(index, member)
            }
            pub fn replace_members(
                &self,
                members: &[MobjectTarget<'_>],
            ) -> Result<(), AuthoringError> {
                MobjectTarget::from(self).replace_members(members)
            }
        }
    };
}
membership!(MobjectFamily);
membership!(Mobject);

#[cfg(test)]
mod tests;
