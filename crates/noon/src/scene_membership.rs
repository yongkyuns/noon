use std::{cell::RefCell, rc::Rc};

use noon_core::{
    plan_semantic_scene_membership, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeId, SemanticSceneMembershipRequest, SemanticStore,
};

use crate::MobjectFamilyMember;

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
) -> Result<SemanticMutationTransaction, String> {
    let validate = |member: MobjectFamilyMember<'_>| -> Result<SemanticNodeId, String> {
        if !Rc::ptr_eq(owner, member.store()) {
            return Err("membership target belongs to another scene store".into());
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
    .map_err(|error| error.to_string())
}

pub(crate) fn apply_scene_membership(
    store: &Rc<RefCell<SemanticStore>>,
    transaction: SemanticMutationTransaction,
) -> Result<SemanticMutationTransactionResult, String> {
    transaction
        .apply(&mut store.borrow_mut())
        .map_err(|error| error.to_string())
}
