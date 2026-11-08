//! Typed family authoring through the existing Scene operations.

use super::{ScriptError, ScriptScene};
use crate::{
    EffectiveMobjectLayout, FamilyArrangeOptions, FamilyGridOptions, LayoutAnchor,
    LiveLayoutTarget, ManimNextToArgs, Mobject, MobjectFamily, MobjectTarget,
};

impl ScriptScene {
    /// Construct a detached family from borrowed objects or typed mixed targets.
    ///
    /// A homogeneous collection can be written as `scene.family([&first, &second])`.
    /// Mixed collections use existing `MobjectTarget::Object` and `::Family` values;
    /// arrays, vectors, and borrowed slices of those values are accepted. Membership
    /// order and aliases are resolved by the same shared semantic transaction.
    pub fn family<'a, T>(
        &self,
        members: impl IntoIterator<Item = T>,
    ) -> Result<MobjectFamily, ScriptError>
    where
        T: Into<MobjectTarget<'a>>,
    {
        let members: Vec<_> = members.into_iter().map(Into::into).collect();
        self.with_scene(|scene| Ok(scene.family(&members)?))
    }

    /// Add or reorder direct members through one atomic Scene publication.
    pub fn add_family_members<'root, 'member, T>(
        &self,
        family: impl Into<MobjectTarget<'root>>,
        members: impl IntoIterator<Item = T>,
    ) -> Result<Vec<bool>, ScriptError>
    where
        T: Into<MobjectTarget<'member>>,
    {
        let members: Vec<_> = members.into_iter().map(Into::into).collect();
        self.with_scene(|scene| Ok(scene.add_family_members(family, &members)?))
    }

    /// Remove direct members without destroying their semantic identities.
    pub fn remove_family_members<'root, 'member, T>(
        &self,
        family: impl Into<MobjectTarget<'root>>,
        members: impl IntoIterator<Item = T>,
    ) -> Result<Vec<bool>, ScriptError>
    where
        T: Into<MobjectTarget<'member>>,
    {
        let members: Vec<_> = members.into_iter().map(Into::into).collect();
        self.with_scene(|scene| Ok(scene.remove_family_members(family, &members)?))
    }

    /// Arrange direct members using the shared linear placement semantics.
    ///
    /// The Scene observes authored bounds before execution and coherent effective
    /// bounds after a completed boundary. Unsupported drivers fail before any edit.
    pub fn arrange_family_with_options(
        &self,
        family: &MobjectFamily,
        options: &FamilyArrangeOptions,
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.arrange_family_with_options(family, options)?;
            Ok(())
        })
    }

    /// Arrange direct members in a grid through the same atomic layout publication.
    pub fn arrange_family_in_grid_with_options(
        &self,
        family: &MobjectFamily,
        options: &FamilyGridOptions,
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.arrange_family_in_grid_with_options(family, options)?;
            Ok(())
        })
    }

    /// Read a family's current effective bounds, including its unique descendants.
    /// Cold setup has no effective publication and fails explicitly.
    pub fn effective_family_layout(
        &self,
        family: &MobjectFamily,
    ) -> Result<EffectiveMobjectLayout, ScriptError> {
        self.with_scene(|scene| Ok(scene.effective_family_layout(family)?))
    }

    /// Read an object's own content and descendants from the current publication.
    pub fn effective_layout(
        &self,
        object: &Mobject,
    ) -> Result<EffectiveMobjectLayout, ScriptError> {
        self.with_scene(|scene| Ok(scene.effective_layout(object)?))
    }

    /// Move a family by the shared edge and coordinate-mask placement rules.
    pub fn move_family_to(
        &self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.move_family_to(family, target, edge, mask)?;
            Ok(())
        })
    }

    /// Align a family to a typed point, object, family, or selected layout anchor.
    pub fn align_family_to(
        &self,
        family: &MobjectFamily,
        target: LiveLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.align_family_to(family, target, axis)?;
            Ok(())
        })
    }

    /// Place one selected layout beside another with an explicit alignment anchor.
    pub fn next_layout_to_aligned(
        &self,
        source: &LayoutAnchor,
        target: LiveLayoutTarget<'_>,
        aligner: &LayoutAnchor,
        args: ManimNextToArgs,
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.next_layout_to_aligned(source, target, aligner, args)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
