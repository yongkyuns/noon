use std::{cell::RefCell, rc::Rc};

use noon_core::{
    plan_semantic_scene_membership, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeId, SemanticSceneMembershipRequest, SemanticStore,
};

use crate::{AuthoringError, MobjectFamilyMember};

/// One ordered, atomic membership edit over a Scene's authoritative root family.
#[derive(Clone, Copy)]
pub enum SceneMembershipRequest<'a> {
    Add(&'a [MobjectFamilyMember<'a>]),
    Remove(&'a [MobjectFamilyMember<'a>]),
    Clear,
    Replace {
        old: MobjectFamilyMember<'a>,
        new: MobjectFamilyMember<'a>,
    },
}

pub(crate) fn prepare_scene_membership(
    owner: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    request: SceneMembershipRequest<'_>,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let validate = |member: MobjectFamilyMember<'_>| -> Result<SemanticNodeId, AuthoringError> {
        if !Rc::ptr_eq(owner, member.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        member.validate()?;
        Ok(member.node_id())
    };
    let store = owner.borrow();
    match request {
        SceneMembershipRequest::Add(members) => {
            let ids = members
                .iter()
                .copied()
                .map(validate)
                .collect::<Result<Vec<_>, _>>()?;
            plan_semantic_scene_membership(&store, root, SemanticSceneMembershipRequest::Add(&ids))
        }
        SceneMembershipRequest::Remove(members) => {
            let ids = members
                .iter()
                .copied()
                .map(validate)
                .collect::<Result<Vec<_>, _>>()?;
            plan_semantic_scene_membership(
                &store,
                root,
                SemanticSceneMembershipRequest::Remove(&ids),
            )
        }
        SceneMembershipRequest::Clear => {
            plan_semantic_scene_membership(&store, root, SemanticSceneMembershipRequest::Clear)
        }
        SceneMembershipRequest::Replace { old, new } => {
            let old = validate(old)?;
            let new = validate(new)?;
            plan_semantic_scene_membership(
                &store,
                root,
                SemanticSceneMembershipRequest::Replace { old, new },
            )
        }
    }
    .map_err(Into::into)
}

pub(crate) fn apply_scene_membership(
    store: &Rc<RefCell<SemanticStore>>,
    transaction: SemanticMutationTransaction,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    transaction
        .apply(&mut store.borrow_mut())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn transaction_application_keeps_preflight_error_and_commits_nothing() {
        let scene = crate::Scene::new();
        let object = scene.circle(1.0).unwrap();
        let revision = scene.integration_store().borrow().scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(scene.root(), object.node_id());
        transaction.add_member(object.node_id(), scene.root());
        let error = apply_scene_membership(scene.integration_store(), transaction).unwrap_err();
        let AuthoringError::Transaction(cause) = &error else {
            panic!("transaction errors must retain their category")
        };
        assert!(matches!(
            cause,
            noon_core::SemanticMutationTransactionError::Family { index: 1, .. }
        ));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<noon_core::SemanticMutationTransactionError>(),
            Some(cause)
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert!(scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members()
            .is_empty());
    }
}
