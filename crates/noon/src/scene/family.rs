//! Family membership and layout queries through the owning Scene.

use crate::{AuthoringError, EffectiveMobjectLayout, Mobject, MobjectFamily, MobjectTarget, Scene};

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
        let (transaction, changed) = self.prepare_family_edit(family, members, adding)?;
        self.apply_semantic_transaction(transaction)?;
        Ok(changed)
    }

    /// Stage direct member addition for renderer preparation. The change flags
    /// describe the pending edit, which commits only after host acknowledgment.
    pub fn prepare_family_addition<'root>(
        &mut self,
        family: impl Into<MobjectTarget<'root>>,
        members: &[MobjectTarget<'_>],
    ) -> Result<(crate::StagedEffectPublication, Vec<bool>), AuthoringError> {
        let (transaction, changed) = self.prepare_family_edit(family.into(), members, true)?;
        Ok((self.stage_effect_publication(transaction)?, changed))
    }

    fn prepare_family_edit(
        &self,
        family: MobjectTarget<'_>,
        members: &[MobjectTarget<'_>],
        adding: bool,
    ) -> Result<(noon_core::SemanticMutationTransaction, Vec<bool>), AuthoringError> {
        family.require_store(self.integration_store())?;
        crate::family_authoring::family_membership_transaction(family, members, adding)
    }

    /// Move the unique incoming direct members to the front of one family.
    pub fn prepend_family_members<'root>(
        &mut self,
        family: impl Into<MobjectTarget<'root>>,
        members: &[MobjectTarget<'_>],
    ) -> Result<(), AuthoringError> {
        let family = family.into();
        family.require_store(self.integration_store())?;
        let transaction = crate::family_authoring::family_prepend_transaction(family, members)?;
        if !transaction.mutations().is_empty() {
            self.apply_semantic_transaction(transaction)?;
        }
        Ok(())
    }

    /// Insert or reposition one direct family member through one publication.
    pub fn insert_family_member<'root>(
        &mut self,
        family: impl Into<MobjectTarget<'root>>,
        index: usize,
        member: MobjectTarget<'_>,
    ) -> Result<(), AuthoringError> {
        let family = family.into();
        family.require_store(self.integration_store())?;
        let transaction =
            crate::family_authoring::family_insert_transaction(family, index, member)?;
        if !transaction.mutations().is_empty() {
            self.apply_semantic_transaction(transaction)?;
        }
        Ok(())
    }

    /// Reverse a family's order through one authored transaction.
    pub fn invert_family_members(
        &mut self,
        family: &MobjectFamily,
        recursive: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(family).require_store(self.integration_store())?;
        let transaction = crate::family_authoring::family_invert_transaction(family, recursive)?;
        if !transaction.mutations().is_empty() {
            self.apply_semantic_transaction(transaction)?;
        }
        Ok(())
    }

    /// Replace an object or family's direct children in one publication.
    pub fn replace_members(
        &mut self,
        target: MobjectTarget<'_>,
        members: &[MobjectTarget<'_>],
    ) -> Result<(), AuthoringError> {
        target.require_store(self.integration_store())?;
        let transaction = crate::object_membership::replace_members_transaction(target, members)?;
        self.apply_semantic_transaction(transaction)?;
        Ok(())
    }

    /// Publish updater suspension for a detached subtree without changing the callback schedule.
    pub fn set_updating_suspended<'target>(
        &mut self,
        target: impl Into<MobjectTarget<'target>>,
        suspended: bool,
        recursive: bool,
    ) -> Result<(), AuthoringError> {
        let target = target.into();
        target.require_store(self.integration_store())?;
        let transaction =
            target.updater_suspension_transaction(suspended, recursive, Some(self.root()))?;
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Check direct membership in this Scene's root.
    pub fn contains_direct(&self, object: &Mobject) -> Result<bool, AuthoringError> {
        self.require_object(object)?;
        contains_direct(self.integration_store(), self.root(), object)
    }

    /// Check membership through the root or any of its descendants.
    pub fn contains(&self, object: &Mobject) -> Result<bool, AuthoringError> {
        self.require_object(object)?;
        noon_core::semantic_scene_root_contains(
            &self.integration_store().borrow(),
            self.root(),
            object.node_id(),
        )
        .map_err(AuthoringError::from)
    }

    /// Hide direct members atomically before a subset display animation.
    pub fn prepare_family_subset_display(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<noon_core::SemanticMutationTransactionResult, String> {
        MobjectTarget::from(family)
            .require_store(self.integration_store())
            .map_err(|error| error.to_string())?;
        let transaction = crate::family_authoring::prepare_subset_display_transaction(
            &self.integration_store().borrow(),
            family.node_id(),
        )?;
        self.apply_semantic_transaction(transaction)
            .map_err(|error| error.to_string())
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
        let execution = self.execution_for_observation()?;
        crate::family_layout::effective_mobject_layout(self.integration_store(), execution, object)
    }
}

pub(crate) fn contains_direct(
    store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    root: noon_core::SemanticNodeId,
    object: &Mobject,
) -> Result<bool, AuthoringError> {
    store
        .borrow()
        .is_direct_member(root, object.node_id())
        .map_err(noon_core::SemanticSceneOperationError::from)
        .map_err(AuthoringError::from)
}
