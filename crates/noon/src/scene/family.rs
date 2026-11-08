//! Family membership and layout queries through the owning Scene.

use crate::{AuthoringError, EffectiveMobjectLayout, Mobject, MobjectTarget, Scene};

impl Scene {
    /// Add or move direct members to the end of an object or family in one publication.
    ///
    /// Incoming aliases use the shared last-occurrence order. The returned flags
    /// identify newly added members; existing members can move without a new identity.
    pub fn add_family_members<'root>(
        &mut self,
        family: impl Into<MobjectTarget<'root>>,
        members: &[MobjectTarget<'_>],
    ) -> Result<Vec<bool>, AuthoringError> {
        self.edit_family_members(family.into(), members, true)
    }

    /// Remove direct members atomically, preserving their semantic identities.
    pub fn remove_family_members<'root>(
        &mut self,
        family: impl Into<MobjectTarget<'root>>,
        members: &[MobjectTarget<'_>],
    ) -> Result<Vec<bool>, AuthoringError> {
        self.edit_family_members(family.into(), members, false)
    }

    fn edit_family_members(
        &mut self,
        family: MobjectTarget<'_>,
        members: &[MobjectTarget<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, AuthoringError> {
        family.require_store(self.integration_store())?;
        let (transaction, changed) =
            crate::family_authoring::family_membership_transaction(family, members, adding)?;
        self.apply_semantic_transaction(transaction)?;
        Ok(changed)
    }

    /// Read own content and unique descendants from one coherent effective publication.
    ///
    /// Bounds use the shared layout rules without stroke expansion. Cold Scenes
    /// fail explicitly instead of substituting authored geometry for effective state.
    pub fn effective_layout(
        &self,
        object: &Mobject,
    ) -> Result<EffectiveMobjectLayout, AuthoringError> {
        self.require_object(object)?;
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        crate::family_layout::effective_mobject_layout(self.integration_store(), execution, object)
    }
}
