use noon_core::SemanticMutationTransactionResult;

use crate::{
    AuthoringError, LiveSession, LiveSessionError, Mobject, MobjectFamilyMember, Scene,
    SceneMembershipRequest,
};

impl Scene {
    /// Move one object/family projection to the front of painter order.
    ///
    /// ManimCE v0.21 defines `bring_to_front()` through ordinary `add()` semantics,
    /// so this deliberately reuses the same family-aware membership transaction.
    pub fn bring_to_front(&mut self, mobject: &Mobject) -> Result<(), AuthoringError> {
        self.add(mobject)
    }

    /// Move several object/family projections to the front in caller order.
    pub fn bring_to_front_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.add_many(members)
    }

    /// Move one object/family projection to the back of painter order.
    pub fn bring_to_back(&mut self, mobject: &Mobject) -> Result<(), AuthoringError> {
        self.edit_membership(SceneMembershipRequest::BringToBack(&[
            MobjectFamilyMember::Mobject(mobject),
        ]))
        .map(|_| ())
    }

    /// Move several object/family projections to the back in caller order.
    pub fn bring_to_back_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::BringToBack(members))
    }
}

impl<'a> LiveSession<'a> {
    /// Move one live object projection to the front through ordinary add semantics.
    pub fn bring_to_front(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.add(mobject)
    }

    /// Move several live projections to the front in caller order.
    pub fn bring_to_front_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.add_many(members)
    }

    /// Move one live object projection to the back through one atomic publication.
    pub fn bring_to_back(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::BringToBack(&[
            MobjectFamilyMember::Mobject(mobject),
        ]))
    }

    /// Move several live projections to the back in caller order.
    pub fn bring_to_back_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::BringToBack(members))
    }
}
