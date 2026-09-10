//! Typed handle for authoritative semantic family membership.

use crate::semantic_mobject::authoring_xy_f64 as semantic_xy_f64;
use crate::AuthoringError;
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticObjectProperty, SemanticStore,
    SemanticVec3,
};
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    rc::Rc,
};

/// Ordered family translation over authoritative shared semantic leaf identity.
///
/// Frontends may retain wrapper trees for language-level identity, but the shared
/// semantic traversal decides which unique leaves are mutated and in what order. The delta is
/// validated once in Rust; all affected leaves are then committed atomically.
#[derive(Clone, Debug)]
#[doc(hidden)]
pub(crate) struct FamilyTranslation {
    source_members: Vec<SemanticNodeId>,
    delta: (f64, f64),
}

impl FamilyTranslation {
    pub fn begin(
        store: &SemanticStore,
        source: SemanticNodeId,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, AuthoringError> {
        let source_members = store
            .ordered_leaf_nodes(source)
            .map_err(AuthoringError::from)?;
        Self::from_members(source_members, delta_x, delta_y)
    }

    pub fn from_members(
        source_members: Vec<SemanticNodeId>,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, AuthoringError> {
        let delta = semantic_xy_f64(delta_x, delta_y)?;
        Ok(Self {
            source_members,
            delta: (delta.x, delta.y),
        })
    }

    /// Apply each selected semantic leaf in one transaction.
    pub fn apply(self, store: &mut SemanticStore) -> Result<(), AuthoringError> {
        self.transaction(store)?
            .apply(store)
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn transaction(
        self,
        store: &SemanticStore,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        translation_transaction(self.into_shifts(), |leaf| {
            store
                .semantic_object_state_checked(leaf)
                .map(|state| state.transform.translation)
                .map_err(AuthoringError::from)
        })
    }

    pub fn into_shifts(self) -> Vec<(SemanticNodeId, f64, f64)> {
        self.source_members
            .into_iter()
            .map(|member| (member, self.delta.0, self.delta.1))
            .collect()
    }
}

pub(crate) fn translation_transaction<F, E>(
    shifts: impl IntoIterator<Item = (SemanticNodeId, f64, f64)>,
    mut authored_translation: F,
) -> Result<SemanticMutationTransaction, E>
where
    F: FnMut(SemanticNodeId) -> Result<SemanticVec3, E>,
{
    // Accumulate staged operations before publishing one final property per
    // identity. A single family translation supplies each semantic leaf once.
    let mut translations = BTreeMap::new();
    for (leaf, x, y) in shifts {
        let translation = match translations.entry(leaf) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(authored_translation(leaf)?),
        };
        translation.x += x;
        translation.y += y;
    }
    let mut transaction = SemanticMutationTransaction::new();
    for (leaf, translation) in translations {
        transaction.set_property(leaf, SemanticObjectProperty::Translation, translation);
    }
    Ok(transaction)
}

/// A semantic family identity in one shared scene store.
///
/// The handle retains no copied membership, schedule, or runtime state. Ordered
/// traversal always reads the authoritative semantic family at use time.
#[derive(Clone, Debug)]
pub struct MobjectFamily {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

/// One borrowed direct member used by authored and live family operations.
#[derive(Clone, Copy)]
pub enum MobjectFamilyMember<'a> {
    Mobject(&'a crate::Mobject),
    Family(&'a MobjectFamily),
}

impl<'a> From<&'a crate::Mobject> for MobjectFamilyMember<'a> {
    fn from(value: &'a crate::Mobject) -> Self {
        Self::Mobject(value)
    }
}

impl<'a> From<&'a MobjectFamily> for MobjectFamilyMember<'a> {
    fn from(value: &'a MobjectFamily) -> Self {
        Self::Family(value)
    }
}

impl MobjectFamilyMember<'_> {
    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), store) {
            return Err(AuthoringError::ForeignStore);
        }
        self.validate()
    }

    pub(crate) fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        match self {
            Self::Mobject(member) => member.integration_store(),
            Self::Family(member) => member.integration_store(),
        }
    }

    pub(crate) fn node_id(&self) -> SemanticNodeId {
        match self {
            Self::Mobject(member) => member.node_id(),
            Self::Family(member) => member.node_id(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), AuthoringError> {
        match self {
            Self::Mobject(member) => member.validate(),
            Self::Family(member) => member.validate(),
        }
    }
}

/// Select the last occurrence of each incoming identity, preserving batch order.
/// This only visits the input batch, never the existing family's siblings.
fn last_occurrences(members: &[MobjectFamilyMember<'_>]) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    let mut indices: Vec<_> = members
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, member)| seen.insert(member.node_id()).then_some(index))
        .collect();
    indices.reverse();
    indices
}

/// Construct the same pending semantic family for authored and live publication.
pub(crate) fn family_creation_transaction(
    store: &Rc<RefCell<SemanticStore>>,
    members: &[MobjectFamilyMember<'_>],
) -> Result<
    (
        SemanticMutationTransaction,
        noon_core::SemanticLocalNodeToken,
    ),
    AuthoringError,
> {
    for member in members {
        member.require_store(store)?;
    }
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(noon_core::SemanticNodeCreation::family());
    for index in last_occurrences(members) {
        transaction.add_member(family, members[index].node_id());
    }
    Ok((transaction, family))
}

/// Prepare a local direct-member batch, returning one decision per input wrapper.
pub(crate) fn family_membership_transaction(
    family: &MobjectFamily,
    members: &[MobjectFamilyMember<'_>],
    adding: bool,
) -> Result<(SemanticMutationTransaction, Vec<bool>), AuthoringError> {
    family.validate()?;
    for member in members {
        member.require_store(family.integration_store())?;
    }
    let store = family.integration_store().borrow();
    let node = store
        .semantic_family_checked(family.node_id())
        .map_err(AuthoringError::from)?;
    let mut transaction = SemanticMutationTransaction::new();
    let mut changed = vec![false; members.len()];
    for index in last_occurrences(members) {
        let id = members[index].node_id();
        let present = node.contains_member(id);
        changed[index] = present != adding;
        if adding {
            if present {
                transaction.reorder_member(family.node_id(), id, None);
            } else {
                transaction.add_member(family.node_id(), id);
            }
        } else if present {
            transaction.remove_member(family.node_id(), id);
        }
    }
    Ok((transaction, changed))
}

impl MobjectFamily {
    /// Create a detached family, including empty and nested families, atomically.
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Self, AuthoringError> {
        let (transaction, family) = family_creation_transaction(&store, members)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        let node = result
            .resolve(family)
            .expect("committed family token resolves");
        Self::from_node(store, node)
    }

    /// Add one direct member, moving an existing member to the tail.
    /// Returns whether membership was new; a false result can still reorder it.
    pub fn add(&self, member: MobjectFamilyMember<'_>) -> Result<bool, AuthoringError> {
        Ok(self.add_many(&[member])?[0])
    }

    /// Remove one direct member without changing that member's semantic identity.
    pub fn remove(&self, member: MobjectFamilyMember<'_>) -> Result<bool, AuthoringError> {
        Ok(self.remove_many(&[member])?[0])
    }

    /// Commit a whole direct-member addition before returning per-input decisions.
    pub fn add_many(
        &self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Vec<bool>, AuthoringError> {
        self.edit_members(members, true)
    }

    pub fn remove_many(
        &self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Vec<bool>, AuthoringError> {
        self.edit_members(members, false)
    }

    fn edit_members(
        &self,
        members: &[MobjectFamilyMember<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, AuthoringError> {
        let (transaction, changed) = family_membership_transaction(self, members, adding)?;
        transaction
            .apply(&mut self.store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Ok(changed)
    }

    pub fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Result<Self, AuthoringError> {
        store
            .borrow()
            .semantic_family_checked(node)
            .map_err(AuthoringError::from)?;
        Ok(Self { store, node })
    }

    /// Raw shared arena access for explicit integration, not live mutation.
    ///
    /// External edits can invalidate generational handles and leave an existing
    /// execution session on a stale scene revision. Use `Scene::live` and its
    /// coherent publication operations for edits after lowering. No revision
    /// validation is bypassed by this accessor; see [`crate::integration`].
    pub fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    /// Validate this handle without mutation, preserving typed identity/resource errors.
    pub fn validate(&self) -> Result<(), AuthoringError> {
        self.store
            .borrow()
            .semantic_family_checked(self.node)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Aggregate the current layout bounds of this family's authoritative leaves.
    pub fn layout_bounds(&self) -> Result<Option<Bounds2D64>, AuthoringError> {
        Ok(self.layout()?.bounds())
    }

    /// Atomically hide every direct object member before a shared subset display.
    pub fn prepare_subset_display(&self) -> Result<(), String> {
        let transaction = {
            let store = self.store.borrow();
            prepare_subset_display_transaction(&store, self.node)?
        };
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare_subset_display_transaction(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<SemanticMutationTransaction, String> {
    let members = store
        .semantic_family_members_checked(family)
        .map_err(|error| error.to_string())?;
    if members.is_empty() {
        return Err("subset display requires at least one direct family member".into());
    }
    let mut transaction = SemanticMutationTransaction::new();
    for member in members {
        let mut style = store
            .semantic_object_state_checked(member)
            .map_err(|_| "subset display supports direct object members, not nested families")?
            .style
            .clone();
        crate::semantic_mobject::edit_manim_opacity(&mut style, 0.0)
            .map_err(|error| error.to_string())?;
        transaction.replace_style(member, style);
    }
    Ok(transaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn ordinary_family_arrange_uses_shared_bounds_and_one_transaction() {
        let scene = Scene::new();
        let first = scene.square(0.4).unwrap();
        let second = scene.circle(0.2).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = scene.integration_store().borrow().scene_revision();

        family.arrange(1.0, 0.0, 0.2, true).unwrap();

        let first_center = first.center().unwrap();
        let second_center = second.center().unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((second_center.0 - first_center.0 - 0.6).abs() < 1e-6);
        assert!((first_center.0 + second_center.0).abs() < 1e-6);
    }

    #[test]
    fn family_arrange_moves_shared_leaves_once_and_rejects_invalid_input() {
        let scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let mut second = scene.circle(0.2).unwrap();
        second.shift(2.0, 0.0).unwrap();
        let unrelated = scene.square(1.0).unwrap();
        let unrelated_before = unrelated.state().unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let outer = scene.family(&[(&first).into()]).unwrap();
        scene
            .integration_store()
            .borrow_mut()
            .add_member(outer.node_id(), nested.node_id())
            .unwrap();
        let before = scene.integration_store().borrow().scene_revision();

        assert!(outer.arrange(1.0, 0.0, f64::NAN, true).is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
        assert_eq!(first.center().unwrap(), (0.0, 0.0));
        assert_eq!(second.center().unwrap(), (2.0, 0.0));

        outer.arrange(1.0, 0.0, 0.2, true).unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((first.center().unwrap().0 + 1.0).abs() < 1e-6);
        assert!((second.center().unwrap().0 - 1.0).abs() < 1e-6);
        assert_eq!(unrelated.state().unwrap(), unrelated_before);
    }

    #[test]
    fn family_layout_bounds_follow_nested_and_aliased_semantic_leaves() {
        let scene = Scene::new();
        let mut first = scene.rectangle(2.0, 1.0).unwrap();
        first.shift(-2.0, 0.0).unwrap();
        let mut second = scene.circle(0.5).unwrap();
        second.shift(2.0, 1.0).unwrap();

        let outer = {
            let mut store = scene.integration_store().borrow_mut();
            let nested = store.insert_family();
            store.add_member(nested, first.node_id()).unwrap();
            store.add_member(nested, second.node_id()).unwrap();
            let outer = store.insert_family();
            store.add_member(outer, first.node_id()).unwrap();
            store.add_member(outer, nested).unwrap();
            outer
        };
        let family = MobjectFamily::from_node(Rc::clone(scene.integration_store()), outer).unwrap();

        assert_eq!(
            family.layout_bounds().unwrap(),
            Some(Bounds2D64 {
                min_x: -3.0,
                min_y: -0.5,
                max_x: 2.5,
                max_y: 1.5,
            })
        );
    }
}
