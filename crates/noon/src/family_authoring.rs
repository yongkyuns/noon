//! Typed handle for authoritative semantic family membership.

use crate::semantic_mobject::{
    authoring_render_f64 as render_f64, authoring_xy_f64 as semantic_xy_f64,
};
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind,
    SemanticObjectProperty, SemanticStore, SemanticVec3,
};
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    rc::Rc,
};

/// Ordered family translation over authoritative shared semantic leaf identity.
///
/// Frontends may retain wrapper trees for language-level identity, but the shared
/// semantic family decides which leaves are mutated and in what order. The delta is
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
    ) -> Result<Self, String> {
        let source_members = semantic_family_leaf_ids(store, source)?;
        Self::from_members(source_members, delta_x, delta_y)
    }

    pub fn from_members(
        source_members: Vec<SemanticNodeId>,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, String> {
        let delta = semantic_xy_f64(delta_x, delta_y)?;
        Ok(Self {
            source_members,
            delta: (delta.x, delta.y),
        })
    }

    /// Apply every observed leaf occurrence in one semantic transaction.
    pub fn apply(self, store: &mut SemanticStore) -> Result<(), String> {
        let transaction = translation_transaction(self.into_shifts(), |leaf| {
            store
                .semantic_object_state_checked(leaf)
                .map(|state| state.transform.translation)
                .map_err(|error| error.to_string())
        })?;
        transaction
            .apply(store)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn into_shifts(self) -> Vec<(SemanticNodeId, f64, f64)> {
        self.source_members
            .into_iter()
            .map(|member| (member, self.delta.0, self.delta.1))
            .collect()
    }
}

fn translation_transaction<F>(
    shifts: impl IntoIterator<Item = (SemanticNodeId, f64, f64)>,
    mut authored_translation: F,
) -> Result<SemanticMutationTransaction, String>
where
    F: FnMut(SemanticNodeId) -> Result<SemanticVec3, String>,
{
    // A leaf may occur under several direct members. Accumulate those
    // ordered translations before publishing one final property per identity.
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

/// Shared Manim family arrangement over authoritative direct-member identity.
///
/// The semantic store snapshots direct membership/order and recursively resolves the
/// leaf identities each direct member owns. Authored and live Rust callers supply
/// their corresponding bounds; sequencing, buffer math, recentering, and atomic
/// translation construction remain shared here.
#[derive(Clone, Debug)]
#[doc(hidden)]
pub(crate) struct FamilyArrangePlan {
    members: Vec<FamilyArrangeMember>,
    next_member: usize,
}

#[derive(Clone, Debug)]
struct FamilyArrangeMember {
    id: SemanticNodeId,
    leaves: Vec<SemanticNodeId>,
    bounds: Option<Bounds2D64>,
}

impl FamilyArrangePlan {
    pub fn begin(store: &SemanticStore, source: SemanticNodeId) -> Result<Self, String> {
        let direct_members = {
            let source_node = store
                .node(source)
                .ok_or_else(|| format!("unknown family semantic node {source:?}"))?;
            if !matches!(source_node.kind(), SemanticNodeKind::Family) {
                return Err(format!("semantic node {source:?} is not a family"));
            }
            source_node.members().to_vec()
        };

        let mut members = Vec::with_capacity(direct_members.len());
        for id in direct_members {
            let node = store
                .node(id)
                .ok_or_else(|| format!("unknown family arrange member {id:?}"))?;
            let leaves = match node.kind() {
                SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => vec![id],
                SemanticNodeKind::Family => semantic_family_leaf_ids(store, id)?,
                SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {
                    return Err(format!(
                        "family arrange member {id:?} is not an authoring object"
                    ));
                }
            };
            members.push(FamilyArrangeMember {
                id,
                leaves,
                bounds: None,
            });
        }
        Ok(Self {
            members,
            next_member: 0,
        })
    }

    pub fn accept_member_bounds(
        &mut self,
        member: SemanticNodeId,
        bounds: Option<Bounds2D64>,
    ) -> Result<(), String> {
        let expected = self
            .members
            .get(self.next_member)
            .ok_or_else(|| "family arrange received too many direct members".to_owned())?;
        if expected.id != member {
            return Err(format!(
                "family arrange member mismatch at index {}: expected {:?}, got {member:?}",
                self.next_member, expected.id
            ));
        }
        self.members[self.next_member].bounds = bounds;
        self.next_member += 1;
        Ok(())
    }

    pub fn ensure_complete(&self) -> Result<(), String> {
        if self.next_member != self.members.len() {
            return Err(format!(
                "family arrange is incomplete: accepted {} of {} direct members",
                self.next_member,
                self.members.len()
            ));
        }
        Ok(())
    }

    pub fn members(&self) -> impl Iterator<Item = (SemanticNodeId, &[SemanticNodeId])> {
        self.members
            .iter()
            .map(|member| (member.id, member.leaves.as_slice()))
    }

    pub(crate) fn observe_leaf_bounds<F>(&mut self, mut observe: F) -> Result<(), String>
    where
        F: FnMut(SemanticNodeId) -> Result<Option<Bounds2D64>, String>,
    {
        let members = self
            .members()
            .map(|(member, leaves)| (member, leaves.to_vec()))
            .collect::<Vec<_>>();
        for (member, leaves) in members {
            let mut aggregate: Option<Bounds2D64> = None;
            for leaf in leaves {
                let Some(leaf_bounds) = observe(leaf)? else {
                    continue;
                };
                if let Some(total) = &mut aggregate {
                    total.include(leaf_bounds.min_x, leaf_bounds.min_y);
                    total.include(leaf_bounds.max_x, leaf_bounds.max_y);
                } else {
                    aggregate = Some(leaf_bounds);
                }
            }
            self.accept_member_bounds(member, aggregate)?;
        }
        Ok(())
    }

    pub(crate) fn transaction<F>(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
        authored_translation: F,
    ) -> Result<SemanticMutationTransaction, String>
    where
        F: FnMut(SemanticNodeId) -> Result<SemanticVec3, String>,
    {
        let shifts = self
            .finish(direction_x, direction_y, buff, center)?
            .into_iter()
            .flat_map(FamilyTranslation::into_shifts);
        translation_transaction(shifts, authored_translation)
    }

    pub fn finish(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
    ) -> Result<Vec<FamilyTranslation>, String> {
        self.ensure_complete()?;
        let bounds = self
            .members
            .iter()
            .map(|member| member.bounds)
            .collect::<Vec<_>>();
        let deltas = manim_family_arrange_deltas(&bounds, direction_x, direction_y, buff, center)?;
        self.members
            .iter()
            .zip(deltas)
            .map(|(member, delta)| {
                FamilyTranslation::from_members(member.leaves.clone(), delta.0, delta.1)
            })
            .collect()
    }
}

#[doc(hidden)]
pub fn semantic_family_leaf_ids(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, String> {
    fn collect(
        store: &SemanticStore,
        node_id: SemanticNodeId,
        leaves: &mut Vec<SemanticNodeId>,
    ) -> Result<(), String> {
        let node = store
            .node(node_id)
            .ok_or_else(|| format!("unknown semantic family member {node_id:?}"))?;
        match node.kind() {
            SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => {
                leaves.push(node_id);
                Ok(())
            }
            SemanticNodeKind::Family => {
                for member in node.members() {
                    collect(store, member, leaves)?;
                }
                Ok(())
            }
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => Err(format!(
                "family layout member {node_id:?} is not an authoring object"
            )),
        }
    }

    let root = store
        .node(family)
        .ok_or_else(|| format!("unknown family semantic node {family:?}"))?;
    if !matches!(root.kind(), SemanticNodeKind::Family) {
        return Err(format!("semantic node {family:?} is not a family"));
    }

    let mut leaves = Vec::new();
    collect(store, family, &mut leaves)?;
    Ok(leaves)
}

fn manim_family_arrange_deltas(
    member_bounds: &[Option<Bounds2D64>],
    direction_x: f64,
    direction_y: f64,
    buff: f64,
    center: bool,
) -> Result<Vec<(f64, f64)>, String> {
    let direction = semantic_xy_f64(direction_x, direction_y)?;
    let buff = render_f64("buffer", buff)?;
    if member_bounds.is_empty() {
        return Ok(Vec::new());
    }

    let critical = |bounds: Option<Bounds2D64>, x: f64, y: f64| -> (f64, f64) {
        let Some(bounds) = bounds else {
            return (0.0, 0.0);
        };
        let center_x = (bounds.min_x + bounds.max_x) * 0.5;
        let center_y = (bounds.min_y + bounds.max_y) * 0.5;
        (
            if x < 0.0 {
                bounds.min_x
            } else if x > 0.0 {
                bounds.max_x
            } else {
                center_x
            },
            if y < 0.0 {
                bounds.min_y
            } else if y > 0.0 {
                bounds.max_y
            } else {
                center_y
            },
        )
    };

    let mut deltas = vec![(0.0, 0.0); member_bounds.len()];
    for index in 1..member_bounds.len() {
        let source = critical(member_bounds[index], -direction.x, -direction.y);
        let previous = critical(member_bounds[index - 1], direction.x, direction.y);
        deltas[index] = (
            previous.0 + deltas[index - 1].0 - source.0 + direction.x * buff,
            previous.1 + deltas[index - 1].1 - source.1 + direction.y * buff,
        );
    }

    if center {
        let mut arranged_bounds: Option<Bounds2D64> = None;
        for (bounds, delta) in member_bounds.iter().zip(&deltas) {
            let Some(bounds) = bounds else {
                continue;
            };
            let shifted = Bounds2D64 {
                min_x: bounds.min_x + delta.0,
                min_y: bounds.min_y + delta.1,
                max_x: bounds.max_x + delta.0,
                max_y: bounds.max_y + delta.1,
            };
            if let Some(total) = &mut arranged_bounds {
                total.include(shifted.min_x, shifted.min_y);
                total.include(shifted.max_x, shifted.max_y);
            } else {
                arranged_bounds = Some(shifted);
            }
        }
        if let Some(bounds) = arranged_bounds {
            let center_x = (bounds.min_x + bounds.max_x) * 0.5;
            let center_y = (bounds.min_y + bounds.max_y) * 0.5;
            for delta in &mut deltas {
                delta.0 -= center_x;
                delta.1 -= center_y;
            }
        }
    }

    Ok(deltas)
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
    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if !Rc::ptr_eq(self.store(), store) {
            return Err("family members belong to different authoring stores".into());
        }
        self.validate()
    }

    pub(crate) fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        match self {
            Self::Mobject(member) => member.store(),
            Self::Family(member) => member.store(),
        }
    }

    pub(crate) fn node_id(&self) -> SemanticNodeId {
        match self {
            Self::Mobject(member) => member.node_id(),
            Self::Family(member) => member.node_id(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Mobject(member) => member.validate(),
            Self::Family(member) => member.validate(),
        }
    }
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
    String,
> {
    for member in members {
        member.require_store(store)?;
    }
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(noon_core::SemanticNodeCreation::family());
    let mut seen = BTreeSet::new();
    for member in members {
        if seen.insert(member.node_id()) {
            transaction.add_member(family, member.node_id());
        }
    }
    Ok((transaction, family))
}

/// Prepare a local direct-member batch, returning one decision per input wrapper.
pub(crate) fn family_membership_transaction(
    family: &MobjectFamily,
    members: &[MobjectFamilyMember<'_>],
    adding: bool,
) -> Result<(SemanticMutationTransaction, Vec<bool>), String> {
    family.validate()?;
    for member in members {
        member.require_store(family.store())?;
    }
    let store = family.store().borrow();
    let node = store
        .semantic_family_checked(family.node_id())
        .map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    let mut transaction = SemanticMutationTransaction::new();
    let changed = members
        .iter()
        .map(|member| {
            let id = member.node_id();
            let changed = seen.insert(id) && node.contains_member(id) != adding;
            if changed {
                if adding {
                    transaction.add_member(family.node_id(), id);
                } else {
                    transaction.remove_member(family.node_id(), id);
                }
            }
            changed
        })
        .collect();
    Ok((transaction, changed))
}

impl MobjectFamily {
    /// Create a detached family, including empty and nested families, atomically.
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Self, String> {
        let (transaction, family) = family_creation_transaction(&store, members)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(|e| e.to_string())?;
        let node = result
            .resolve(family)
            .expect("committed family token resolves");
        Self::from_node(store, node)
    }

    /// Add one direct member; repeated additions preserve its existing order.
    pub fn add(&self, member: MobjectFamilyMember<'_>) -> Result<bool, String> {
        Ok(self.add_many(&[member])?[0])
    }

    /// Remove one direct member without changing that member's semantic identity.
    pub fn remove(&self, member: MobjectFamilyMember<'_>) -> Result<bool, String> {
        Ok(self.remove_many(&[member])?[0])
    }

    /// Commit a whole direct-member addition before returning per-input decisions.
    pub fn add_many(&self, members: &[MobjectFamilyMember<'_>]) -> Result<Vec<bool>, String> {
        self.edit_members(members, true)
    }

    pub fn remove_many(&self, members: &[MobjectFamilyMember<'_>]) -> Result<Vec<bool>, String> {
        self.edit_members(members, false)
    }

    fn edit_members(
        &self,
        members: &[MobjectFamilyMember<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, String> {
        let (transaction, changed) = family_membership_transaction(self, members, adding)?;
        transaction
            .apply(&mut self.store.borrow_mut())
            .map_err(|e| e.to_string())?;
        Ok(changed)
    }

    pub fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Result<Self, String> {
        store
            .borrow()
            .semantic_family_checked(node)
            .map_err(|error| error.to_string())?;
        Ok(Self { store, node })
    }

    pub fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    pub fn validate(&self) -> Result<(), String> {
        self.store
            .borrow()
            .semantic_family_checked(self.node)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Aggregate the current layout bounds of this family's authoritative leaves.
    pub fn layout_bounds(&self) -> Result<Option<Bounds2D64>, String> {
        Ok(self.layout()?.bounds())
    }

    /// Arrange direct family members from authored layout bounds and publish all
    /// resulting leaf translations in one semantic transaction.
    pub fn arrange(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
    ) -> Result<(), String> {
        self.validate()?;
        let mut plan = FamilyArrangePlan::begin(&self.store.borrow(), self.node)?;
        plan.observe_leaf_bounds(|leaf| {
            crate::Mobject::from_node(Rc::clone(&self.store), leaf)?.layout_bounds()
        })?;
        let transaction = plan.transaction(direction_x, direction_y, buff, center, |leaf| {
            Ok(crate::Mobject::from_node(Rc::clone(&self.store), leaf)?
                .state()?
                .transform
                .translation)
        })?;
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
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
        crate::semantic_mobject::edit_manim_opacity(&mut style, 0.0)?;
        transaction.replace_style(member, style);
    }
    Ok(transaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn family_arrange_preserves_direct_order_spacing_and_recentering() {
        let bounds = [
            Some(Bounds2D64 {
                min_x: -1.0,
                min_y: -0.5,
                max_x: 1.0,
                max_y: 0.5,
            }),
            Some(Bounds2D64 {
                min_x: -0.5,
                min_y: -0.25,
                max_x: 0.5,
                max_y: 0.25,
            }),
        ];
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, second).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, first).unwrap();
        store.add_member(outer, nested).unwrap();

        let mut rejected = FamilyArrangePlan::begin(&store, outer).unwrap();
        assert!(rejected.accept_member_bounds(nested, bounds[1]).is_err());

        let mut plan = FamilyArrangePlan::begin(&store, outer).unwrap();
        plan.accept_member_bounds(first, bounds[0]).unwrap();
        plan.accept_member_bounds(nested, bounds[1]).unwrap();
        let translations = plan.finish(2.0, 0.0, 0.25, true).unwrap();
        assert_eq!(translations.len(), 2);
        assert_eq!(translations[0].source_members, vec![first]);
        assert_eq!(translations[0].delta, (-0.75, 0.0));
        assert_eq!(translations[1].source_members, vec![second]);
        assert_eq!(translations[1].delta, (1.25, 0.0));
    }

    #[test]
    fn ordinary_family_arrange_uses_shared_bounds_and_one_transaction() {
        let scene = Scene::new();
        let first = scene.square(0.4).unwrap();
        let second = scene.circle(0.2).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = scene.store().borrow().scene_revision();

        family.arrange(1.0, 0.0, 0.2, true).unwrap();

        let first_center = first.center().unwrap();
        let second_center = second.center().unwrap();
        assert_eq!(
            scene.store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((second_center.0 - first_center.0 - 0.6).abs() < 1e-6);
        assert!((first_center.0 + second_center.0).abs() < 1e-6);
    }

    #[test]
    fn family_arrange_accumulates_aliases_atomically_and_rejects_invalid_input() {
        let scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let mut second = scene.circle(0.2).unwrap();
        second.shift(2.0, 0.0).unwrap();
        let unrelated = scene.square(1.0).unwrap();
        let unrelated_before = unrelated.state().unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let outer = scene.family(&[(&first).into()]).unwrap();
        scene
            .store()
            .borrow_mut()
            .add_member(outer.node_id(), nested.node_id())
            .unwrap();
        let before = scene.store().borrow().scene_revision();

        assert!(outer.arrange(1.0, 0.0, f64::NAN, true).is_err());
        assert_eq!(scene.store().borrow().scene_revision(), before);
        assert_eq!(first.center().unwrap(), (0.0, 0.0));
        assert_eq!(second.center().unwrap(), (2.0, 0.0));

        outer.arrange(1.0, 0.0, 0.2, true).unwrap();
        assert_eq!(
            scene.store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((first.center().unwrap().0 + 2.0).abs() < 1e-6);
        assert!((second.center().unwrap().0 - 1.3).abs() < 1e-6);
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
            let mut store = scene.store().borrow_mut();
            let nested = store.insert_family();
            store.add_member(nested, first.node_id()).unwrap();
            store.add_member(nested, second.node_id()).unwrap();
            let outer = store.insert_family();
            store.add_member(outer, first.node_id()).unwrap();
            store.add_member(outer, nested).unwrap();
            outer
        };
        let family = MobjectFamily::from_node(Rc::clone(scene.store()), outer).unwrap();

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
