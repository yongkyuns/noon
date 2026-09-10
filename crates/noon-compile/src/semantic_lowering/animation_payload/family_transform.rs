use std::collections::HashSet;

use noon_core::{SemanticNodeId, SemanticNodeKind, SemanticStore};

/// One leaf-to-leaf visual occurrence after Manim-style family alignment.
///
/// Padding flags describe derived copies only. Both semantic IDs remain authored
/// object identities; callers must never manufacture a Semantic Scene node for a
/// padded occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTransformOccurrence {
    source: SemanticNodeId,
    target: SemanticNodeId,
    source_padding: bool,
    target_padding: bool,
}

impl FamilyTransformOccurrence {
    pub const fn source(self) -> SemanticNodeId {
        self.source
    }

    pub const fn target(self) -> SemanticNodeId {
        self.target
    }

    pub const fn source_is_padding(self) -> bool {
        self.source_padding
    }

    pub const fn target_is_padding(self) -> bool {
        self.target_padding
    }
}

/// Transient family correspondence used only while compiling one Transform.
///
/// This object contains references to authored leaves plus copy markers. It owns no
/// Semantic Scene identity, stable execution identity, authored membership, or
/// persistent topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyTransformCorrespondence {
    occurrences: Vec<FamilyTransformOccurrence>,
}

impl FamilyTransformCorrespondence {
    pub fn occurrences(&self) -> &[FamilyTransformOccurrence] {
        &self.occurrences
    }

    pub fn len(&self) -> usize {
        self.occurrences.len()
    }

    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyTransformCorrespondenceError {
    MissingNode(SemanticNodeId),
    RootIsNotFamily(SemanticNodeId),
    UnsupportedNode(SemanticNodeId),
    InvalidObject(SemanticNodeId),
    InvalidFamily(SemanticNodeId),
    EmptyAlignment {
        source: SemanticNodeId,
        target: SemanticNodeId,
    },
    AliasedSourceLeaf(SemanticNodeId),
    AliasedTargetLeaf(SemanticNodeId),
}

impl std::fmt::Display for FamilyTransformCorrespondenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode(node) => write!(
                formatter,
                "family Transform node {}:{} is stale or missing",
                node.slot(),
                node.generation()
            ),
            Self::RootIsNotFamily(node) => write!(
                formatter,
                "family Transform root {}:{} is not a semantic family",
                node.slot(),
                node.generation()
            ),
            Self::UnsupportedNode(node) => write!(
                formatter,
                "family Transform node {}:{} is not an object or family",
                node.slot(),
                node.generation()
            ),
            Self::InvalidObject(node) => write!(
                formatter,
                "family Transform object {}:{} is invalid",
                node.slot(),
                node.generation()
            ),
            Self::InvalidFamily(node) => write!(
                formatter,
                "family Transform family {}:{} is invalid",
                node.slot(),
                node.generation()
            ),
            Self::EmptyAlignment { source, target } => write!(
                formatter,
                "family Transform cannot yet align an empty family side ({}:{} -> {}:{})",
                source.slot(),
                source.generation(),
                target.slot(),
                target.generation()
            ),
            Self::AliasedSourceLeaf(node) => write!(
                formatter,
                "unequal family Transform does not yet support aliased source leaf {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::AliasedTargetLeaf(node) => write!(
                formatter,
                "unequal family Transform does not yet support aliased target leaf {}:{}",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for FamilyTransformCorrespondenceError {}

/// Derive Manim-compatible recursive submobject correspondence without changing
/// either authored family.
///
/// At every family level the shorter side is expanded to the longer side with the
/// same repeat-index rule used by ManimCE's `add_n_more_submobjects`:
/// `floor(position * original_count / aligned_count)`. The first occurrence keeps
/// the authored leaf; later occurrences are marked as transparent derived copies.
/// Alignment then recurses, so nested families preserve the same structural rule.
pub fn derive_family_transform_correspondence(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<FamilyTransformCorrespondence, FamilyTransformCorrespondenceError> {
    require_family_root(store, source)?;
    require_family_root(store, target)?;

    let mut builder = CorrespondenceBuilder {
        store,
        occurrences: Vec::new(),
        seen_source_leaves: HashSet::new(),
        seen_target_leaves: HashSet::new(),
    };
    builder.align_nodes(source, target, false, false)?;
    if builder.occurrences.is_empty() {
        return Err(FamilyTransformCorrespondenceError::EmptyAlignment { source, target });
    }
    Ok(FamilyTransformCorrespondence {
        occurrences: builder.occurrences,
    })
}

fn require_family_root(
    store: &SemanticStore,
    node: SemanticNodeId,
) -> Result<(), FamilyTransformCorrespondenceError> {
    let node_ref = store
        .node(node)
        .ok_or(FamilyTransformCorrespondenceError::MissingNode(node))?;
    if !matches!(node_ref.kind(), SemanticNodeKind::Family(_)) {
        return Err(FamilyTransformCorrespondenceError::RootIsNotFamily(node));
    }
    store
        .semantic_family_members_checked(node)
        .map_err(|_| FamilyTransformCorrespondenceError::InvalidFamily(node))?;
    Ok(())
}

struct CorrespondenceBuilder<'a> {
    store: &'a SemanticStore,
    occurrences: Vec<FamilyTransformOccurrence>,
    seen_source_leaves: HashSet<SemanticNodeId>,
    seen_target_leaves: HashSet<SemanticNodeId>,
}

impl CorrespondenceBuilder<'_> {
    fn align_nodes(
        &mut self,
        source: SemanticNodeId,
        target: SemanticNodeId,
        source_padding: bool,
        target_padding: bool,
    ) -> Result<(), FamilyTransformCorrespondenceError> {
        let source_kind = self
            .store
            .node(source)
            .map(|node| node.kind())
            .ok_or(FamilyTransformCorrespondenceError::MissingNode(source))?;
        let target_kind = self
            .store
            .node(target)
            .map(|node| node.kind())
            .ok_or(FamilyTransformCorrespondenceError::MissingNode(target))?;

        match (source_kind, target_kind) {
            (SemanticNodeKind::AuthoringObject, SemanticNodeKind::AuthoringObject) => {
                self.require_object(source)?;
                self.require_object(target)?;
                self.push_leaf(source, target, source_padding, target_padding)
            }
            (SemanticNodeKind::Family(_), SemanticNodeKind::Family(_)) => {
                let source_members = self.family_members(source)?;
                let target_members = self.family_members(target)?;
                self.align_sequences(
                    source,
                    target,
                    &source_members,
                    &target_members,
                    source_padding,
                    target_padding,
                )
            }
            (SemanticNodeKind::AuthoringObject, SemanticNodeKind::Family(_)) => {
                self.require_object(source)?;
                let target_members = self.family_members(target)?;
                self.align_sequences(
                    source,
                    target,
                    &[source],
                    &target_members,
                    source_padding,
                    target_padding,
                )
            }
            (SemanticNodeKind::Family(_), SemanticNodeKind::AuthoringObject) => {
                self.require_object(target)?;
                let source_members = self.family_members(source)?;
                self.align_sequences(
                    source,
                    target,
                    &source_members,
                    &[target],
                    source_padding,
                    target_padding,
                )
            }
            _ => Err(FamilyTransformCorrespondenceError::UnsupportedNode(source)),
        }
    }

    fn require_object(
        &self,
        node: SemanticNodeId,
    ) -> Result<(), FamilyTransformCorrespondenceError> {
        self.store
            .semantic_object_state_checked(node)
            .map(|_| ())
            .map_err(|_| FamilyTransformCorrespondenceError::InvalidObject(node))
    }

    fn family_members(
        &self,
        node: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, FamilyTransformCorrespondenceError> {
        self.store
            .semantic_family_members_checked(node)
            .map(|members| members.to_vec())
            .map_err(|_| FamilyTransformCorrespondenceError::InvalidFamily(node))
    }

    #[allow(clippy::too_many_arguments)]
    fn align_sequences(
        &mut self,
        source_parent: SemanticNodeId,
        target_parent: SemanticNodeId,
        sources: &[SemanticNodeId],
        targets: &[SemanticNodeId],
        source_padding: bool,
        target_padding: bool,
    ) -> Result<(), FamilyTransformCorrespondenceError> {
        if sources.is_empty() || targets.is_empty() {
            return Err(FamilyTransformCorrespondenceError::EmptyAlignment {
                source: source_parent,
                target: target_parent,
            });
        }

        let count = sources.len().max(targets.len());
        let source_occurrences = expand_sequence(sources, count);
        let target_occurrences = expand_sequence(targets, count);
        for ((source, source_repeat), (target, target_repeat)) in
            source_occurrences.into_iter().zip(target_occurrences)
        {
            self.align_nodes(
                source,
                target,
                source_padding || source_repeat,
                target_padding || target_repeat,
            )?;
        }
        Ok(())
    }

    fn push_leaf(
        &mut self,
        source: SemanticNodeId,
        target: SemanticNodeId,
        source_padding: bool,
        target_padding: bool,
    ) -> Result<(), FamilyTransformCorrespondenceError> {
        if !source_padding && !self.seen_source_leaves.insert(source) {
            return Err(FamilyTransformCorrespondenceError::AliasedSourceLeaf(
                source,
            ));
        }
        if !target_padding && !self.seen_target_leaves.insert(target) {
            return Err(FamilyTransformCorrespondenceError::AliasedTargetLeaf(
                target,
            ));
        }
        self.occurrences.push(FamilyTransformOccurrence {
            source,
            target,
            source_padding,
            target_padding,
        });
        Ok(())
    }
}

fn expand_sequence(nodes: &[SemanticNodeId], target_len: usize) -> Vec<(SemanticNodeId, bool)> {
    debug_assert!(!nodes.is_empty());
    debug_assert!(target_len >= nodes.len());
    if nodes.len() == target_len {
        return nodes.iter().copied().map(|node| (node, false)).collect();
    }

    let mut seen = vec![false; nodes.len()];
    (0..target_len)
        .map(|position| {
            let index = ((position as u128 * nodes.len() as u128) / target_len as u128) as usize;
            let padding = std::mem::replace(&mut seen[index], true);
            (nodes[index], padding)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{SemanticObjectState, StoredGeometry};

    fn object(store: &mut SemanticStore) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }))
    }

    fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
        let family = store.insert_family();
        for &member in members {
            store.add_member(family, member).unwrap();
        }
        family
    }

    #[test]
    fn flat_expansion_uses_manim_repeat_indices() {
        let mut store = SemanticStore::new();
        let s0 = object(&mut store);
        let s1 = object(&mut store);
        let t0 = object(&mut store);
        let t1 = object(&mut store);
        let t2 = object(&mut store);
        let source = family(&mut store, &[s0, s1]);
        let target = family(&mut store, &[t0, t1, t2]);

        let plan = derive_family_transform_correspondence(&store, source, target).unwrap();
        assert_eq!(
            plan.occurrences(),
            &[
                FamilyTransformOccurrence {
                    source: s0,
                    target: t0,
                    source_padding: false,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: s0,
                    target: t1,
                    source_padding: true,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: s1,
                    target: t2,
                    source_padding: false,
                    target_padding: false,
                },
            ]
        );
    }

    #[test]
    fn flat_contraction_fades_the_repeated_target_occurrence() {
        let mut store = SemanticStore::new();
        let s0 = object(&mut store);
        let s1 = object(&mut store);
        let s2 = object(&mut store);
        let t0 = object(&mut store);
        let t1 = object(&mut store);
        let source = family(&mut store, &[s0, s1, s2]);
        let target = family(&mut store, &[t0, t1]);

        let plan = derive_family_transform_correspondence(&store, source, target).unwrap();
        assert_eq!(
            plan.occurrences(),
            &[
                FamilyTransformOccurrence {
                    source: s0,
                    target: t0,
                    source_padding: false,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: s1,
                    target: t0,
                    source_padding: false,
                    target_padding: true,
                },
                FamilyTransformOccurrence {
                    source: s2,
                    target: t1,
                    source_padding: false,
                    target_padding: false,
                },
            ]
        );
    }

    #[test]
    fn nested_alignment_recurses_after_parent_padding() {
        let mut store = SemanticStore::new();
        let a = object(&mut store);
        let b = object(&mut store);
        let c = object(&mut store);
        let d = object(&mut store);
        let e = object(&mut store);
        let f = object(&mut store);
        let bc = family(&mut store, &[b, c]);
        let source = family(&mut store, &[a, bc]);
        let target = family(&mut store, &[d, e, f]);

        let plan = derive_family_transform_correspondence(&store, source, target).unwrap();
        assert_eq!(
            plan.occurrences(),
            &[
                FamilyTransformOccurrence {
                    source: a,
                    target: d,
                    source_padding: false,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: a,
                    target: e,
                    source_padding: true,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: b,
                    target: f,
                    source_padding: false,
                    target_padding: false,
                },
                FamilyTransformOccurrence {
                    source: c,
                    target: f,
                    source_padding: false,
                    target_padding: true,
                },
            ]
        );
    }

    #[test]
    fn correspondence_is_derived_and_never_mutates_authored_membership() {
        let mut store = SemanticStore::new();
        let s0 = object(&mut store);
        let s1 = object(&mut store);
        let t0 = object(&mut store);
        let t1 = object(&mut store);
        let t2 = object(&mut store);
        let source = family(&mut store, &[s0, s1]);
        let target = family(&mut store, &[t0, t1, t2]);
        let revision = store.scene_revision();
        let source_before = store
            .semantic_family_members_checked(source)
            .unwrap()
            .to_vec();
        let target_before = store
            .semantic_family_members_checked(target)
            .unwrap()
            .to_vec();

        let _ = derive_family_transform_correspondence(&store, source, target).unwrap();

        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store.semantic_family_members_checked(source).unwrap(),
            source_before
        );
        assert_eq!(
            store.semantic_family_members_checked(target).unwrap(),
            target_before
        );
    }

    #[test]
    fn empty_side_fails_closed_instead_of_creating_semantic_point_objects() {
        let mut store = SemanticStore::new();
        let source_leaf = object(&mut store);
        let source = family(&mut store, &[source_leaf]);
        let target = family(&mut store, &[]);

        assert!(matches!(
            derive_family_transform_correspondence(&store, source, target),
            Err(FamilyTransformCorrespondenceError::EmptyAlignment { .. })
        ));
    }

    #[test]
    fn unequal_authored_aliases_fail_closed() {
        let mut store = SemanticStore::new();
        let shared = object(&mut store);
        let other = object(&mut store);
        let t0 = object(&mut store);
        let t1 = object(&mut store);
        let left = family(&mut store, &[shared]);
        let right = family(&mut store, &[shared, other]);
        let source = family(&mut store, &[left, right]);
        let target = family(&mut store, &[t0, t1]);

        assert!(matches!(
            derive_family_transform_correspondence(&store, source, target),
            Err(FamilyTransformCorrespondenceError::AliasedSourceLeaf(id)) if id == shared
        ));
    }
}
