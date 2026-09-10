use std::collections::HashSet;

use noon_core::{
    ObjectId, Property, SemanticNodeId, SemanticNodeKind, SemanticObjectProperty,
    SemanticSceneOperationError, SemanticSignalValue, SemanticStore, SemanticTransformInterpolation,
    TrackValues,
};

use super::affine::{
    affine_payload_error, lower_transform_channels, EffectiveAnimationProperties,
    SemanticAffineAnimationTrackError,
};

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

#[derive(Clone, Debug, PartialEq)]
pub enum FamilyTransformCorrespondenceError {
    InvalidNode {
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    RootIsNotFamily(SemanticNodeId),
    UnsupportedNode(SemanticNodeId),
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
            Self::InvalidNode { node, error } => write!(
                formatter,
                "family Transform node {}:{} is invalid: {error}",
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

impl std::error::Error for FamilyTransformCorrespondenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidNode { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Derive Manim-compatible recursive submobject correspondence without changing
/// either authored family. This is the animation-side counterpart to persistent
/// `become`: repeated occurrences are only padding descriptors.
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
        .ok_or_else(|| invalid_node(store, node))?;
    if !matches!(node_ref.kind(), SemanticNodeKind::Family(_)) {
        return Err(FamilyTransformCorrespondenceError::RootIsNotFamily(node));
    }
    Ok(())
}

fn invalid_node(store: &SemanticStore, node: SemanticNodeId) -> FamilyTransformCorrespondenceError {
    let error = store
        .semantic_family_members_checked(node)
        .err()
        .unwrap_or(SemanticSceneOperationError::NotFamily(node));
    FamilyTransformCorrespondenceError::InvalidNode { node, error }
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
            .ok_or_else(|| invalid_node(self.store, source))?;
        let target_kind = self
            .store
            .node(target)
            .map(|node| node.kind())
            .ok_or_else(|| invalid_node(self.store, target))?;

        match (source_kind, target_kind) {
            (SemanticNodeKind::AuthoringObject, SemanticNodeKind::AuthoringObject) => {
                self.store
                    .semantic_object_state_checked(source)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: source,
                        error,
                    })?;
                self.store
                    .semantic_object_state_checked(target)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: target,
                        error,
                    })?;
                self.push_leaf(source, target, source_padding, target_padding)
            }
            (SemanticNodeKind::Family(_), SemanticNodeKind::Family(_)) => {
                let source_members = self
                    .store
                    .semantic_family_members_checked(source)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: source,
                        error,
                    })?
                    .to_vec();
                let target_members = self
                    .store
                    .semantic_family_members_checked(target)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: target,
                        error,
                    })?
                    .to_vec();
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
                self.store
                    .semantic_object_state_checked(source)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: source,
                        error,
                    })?;
                let target_members = self
                    .store
                    .semantic_family_members_checked(target)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: target,
                        error,
                    })?
                    .to_vec();
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
                self.store
                    .semantic_object_state_checked(target)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: target,
                        error,
                    })?;
                let source_members = self
                    .store
                    .semantic_family_members_checked(source)
                    .map_err(|error| FamilyTransformCorrespondenceError::InvalidNode {
                        node: source,
                        error,
                    })?
                    .to_vec();
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
            return Err(FamilyTransformCorrespondenceError::AliasedSourceLeaf(source));
        }
        if !target_padding && !self.seen_target_leaves.insert(target) {
            return Err(FamilyTransformCorrespondenceError::AliasedTargetLeaf(target));
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
            let padding = seen[index];
            seen[index] = true;
            (nodes[index], padding)
        })
        .collect()
}

/// One execution-only channel for a padded source occurrence.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedFamilyTransformChannel {
    pub property: Property,
    pub values: TrackValues,
}

/// Reuse the canonical Transform payload lowering for an execution-only padded
/// occurrence. `source_padding` starts from a fully faded effective copy;
/// `target_padding` ends at a fully faded copy of the target. The returned channels
/// carry no semantic completion because the owning execution display slot is retired
/// at the segment boundary.
#[allow(clippy::too_many_arguments)]
pub fn lower_derived_family_transform_channels(
    store: &SemanticStore,
    animation: SemanticNodeId,
    source: SemanticNodeId,
    target: SemanticNodeId,
    mut from: EffectiveAnimationProperties,
    interpolation: SemanticTransformInterpolation,
    source_padding: bool,
    target_padding: bool,
) -> Result<Vec<DerivedFamilyTransformChannel>, SemanticAffineAnimationTrackError> {
    let source_state = store
        .semantic_object_state_checked(source)
        .map_err(|error| SemanticAffineAnimationTrackError::Target {
            animation,
            node: source,
            error,
        })?;
    let target_state = store
        .semantic_object_state_checked(target)
        .map_err(|error| SemanticAffineAnimationTrackError::Target {
            animation,
            node: target,
            error,
        })?;
    if source_padding {
        from.style.opacity = 0.0;
    }
    let mut channels = lower_transform_channels(
        store,
        source_state,
        target_state,
        from,
        interpolation,
    )
    .map_err(|error| affine_payload_error(animation, source, target, error))?;

    if target_padding {
        if let Some(channel) = channels
            .iter_mut()
            .find(|channel| channel.property == Property::Opacity)
        {
            let TrackValues::Scalar { from, .. } = channel.values else {
                unreachable!("opacity channel must contain scalar values")
            };
            channel.values = TrackValues::Scalar { from, to: 0.0 };
        } else if from.style.opacity != 0.0 {
            channels.push(super::affine::LoweredAffineChannel {
                property: Property::Opacity,
                conflict_property: SemanticObjectProperty::ObjectOpacity,
                completion: super::affine::SemanticAnimationCompletion::Property {
                    property: SemanticObjectProperty::ObjectOpacity,
                    value: SemanticSignalValue::Scalar(0.0),
                },
                values: TrackValues::Scalar {
                    from: from.style.opacity,
                    to: 0.0,
                },
            });
        }
    }

    Ok(channels
        .into_iter()
        .map(|channel| DerivedFamilyTransformChannel {
            property: channel.property,
            values: channel.values,
        })
        .collect())
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
        let source_before = store.semantic_family_members_checked(source).unwrap().to_vec();
        let target_before = store.semantic_family_members_checked(target).unwrap().to_vec();

        let _ = derive_family_transform_correspondence(&store, source, target).unwrap();

        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.semantic_family_members_checked(source).unwrap(), source_before);
        assert_eq!(store.semantic_family_members_checked(target).unwrap(), target_before);
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
