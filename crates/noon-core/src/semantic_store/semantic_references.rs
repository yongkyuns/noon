use std::collections::HashSet;

use crate::{
    SemanticAnimationIntent, SemanticObjectProperty, SemanticSignalExpr, SemanticSignalSource,
};

use super::{
    SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticSceneMembership, SemanticStore,
    SemanticStoreError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticReferenceKind {
    SignalDependency,
    EffectOwner,
    SignalBinding {
        property: SemanticObjectProperty,
    },
    ScopedSignal,
    ForegroundMember,
    Inset2DCameraFrame,
    SpatialAnchorFamily,
    AnimationTarget,
    AnimationTargetState,
    AnimationChild,
    /// Hard authored topology dependency owned by one graph family root.
    GraphDependency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SemanticIncomingReference {
    owner: SemanticNodeId,
    kind: SemanticReferenceKind,
}

impl SemanticIncomingReference {
    const fn new(owner: SemanticNodeId, kind: SemanticReferenceKind) -> Self {
        Self { owner, kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticRemoveNodeEffect {
    NodeRemoved(SemanticNodeId),
    EffectAttachmentChanged {
        owner: SemanticNodeId,
        effect: SemanticNodeId,
    },
    ForegroundMembersChanged {
        scope: SemanticNodeId,
    },
    SubscriptionRemoved {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    ObjectRoleReplaced(SemanticNodeId),
    /// A surviving FixedOrientation object now self-anchors because its shared
    /// family root was removed.
    SpatialAnchorCleared(SemanticNodeId),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SemanticRemoveNodeOutcome {
    effects: Vec<SemanticRemoveNodeEffect>,
    written_slots: HashSet<SemanticNodeId>,
}

impl SemanticRemoveNodeOutcome {
    pub(crate) fn effects(&self) -> &[SemanticRemoveNodeEffect] {
        &self.effects
    }

    pub(crate) fn written_slots(&self) -> &HashSet<SemanticNodeId> {
        &self.written_slots
    }
}

impl SemanticStore {
    /// Return live object owners whose FixedOrientation anchor is one of the
    /// supplied identities. The query follows the retained reverse-reference
    /// index and preserves target/reference order.
    pub(crate) fn spatial_anchor_owners_for_target(
        &self,
        target: SemanticNodeId,
    ) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.incoming_references
            .get(&target)
            .into_iter()
            .flatten()
            .filter(move |reference| {
                reference.kind == SemanticReferenceKind::SpatialAnchorFamily
                    && self.node(reference.owner).is_some_and(|node| {
                        node.semantic_object_state()
                            .is_some_and(|state| state.spatial_anchor_family() == Some(target))
                    })
            })
            .map(|reference| reference.owner)
    }

    /// Derived reverse references keep inset validation local to an edited frame.
    pub(crate) fn inset_displays_for_camera(
        &self,
        camera: SemanticNodeId,
    ) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.incoming_references
            .get(&camera)
            .into_iter()
            .flatten()
            .filter(|reference| reference.kind == SemanticReferenceKind::Inset2DCameraFrame)
            .map(|reference| reference.owner)
    }

    /// Whether this signal participates in any scene execution scope.
    ///
    /// Work is proportional to the signal's direct incoming references; scene
    /// roots and unrelated semantic nodes are never scanned.
    pub fn has_semantic_signal_scope(&self, signal: SemanticNodeId) -> bool {
        self.incoming_references
            .get(&signal)
            .is_some_and(|incoming| {
                incoming
                    .iter()
                    .any(|reference| matches!(reference.kind, SemanticReferenceKind::ScopedSignal))
            })
    }

    /// Whether one exact live signal-to-family scope edge is indexed.
    ///
    /// Stale identities and nodes of another kind have no such edge and return
    /// `false`. Work is proportional only to this signal's direct scope aliases.
    pub fn is_semantic_signal_scoped(&self, scope: SemanticNodeId, signal: SemanticNodeId) -> bool {
        let reference = SemanticIncomingReference::new(scope, SemanticReferenceKind::ScopedSignal);
        self.incoming_references
            .get(&signal)
            .is_some_and(|incoming| incoming.contains(&reference))
    }

    /// Return graph declaration owners whose validity directly depends on
    /// `target`, plus `target` itself when it owns a graph declaration.
    ///
    /// Work is proportional to the target's reverse-reference aliases; unrelated
    /// graph roots and scene nodes are never scanned.
    pub(crate) fn semantic_graph_owners_for_invariant_target(
        &self,
        target: SemanticNodeId,
    ) -> Vec<SemanticNodeId> {
        let mut owners = Vec::new();
        let mut seen = HashSet::new();
        if self
            .node(target)
            .and_then(SemanticNode::graph_declaration)
            .is_some()
            && seen.insert(target)
        {
            owners.push(target);
        }
        if let Some(incoming) = self.incoming_references.get(&target) {
            for reference in incoming.iter().copied() {
                if reference.kind != SemanticReferenceKind::GraphDependency
                    || self.node(reference.owner).is_none()
                    || !self.owner_still_references(
                        reference.owner,
                        target,
                        SemanticReferenceKind::GraphDependency,
                    )
                    || !seen.insert(reference.owner)
                {
                    continue;
                }
                owners.push(reference.owner);
            }
        }
        owners
    }

    pub(crate) fn register_semantic_scoped_signal_reference(
        &mut self,
        scope: SemanticNodeId,
        signal: SemanticNodeId,
    ) {
        let reference = SemanticIncomingReference::new(scope, SemanticReferenceKind::ScopedSignal);
        let incoming = self.incoming_references.entry(signal).or_default();
        if !incoming.contains(&reference) {
            incoming.push(reference);
        }
    }

    /// Replace only this declaration's reverse edges. Do not enumerate this
    /// root's display members, signals, or any unrelated semantic scopes.
    pub(crate) fn replace_semantic_foreground_members(
        &mut self,
        scope: SemanticNodeId,
        members: Vec<SemanticNodeId>,
    ) {
        let previous = std::mem::replace(
            self.node_mut(scope)
                .expect("preflighted foreground scope")
                .foreground_members_mut(),
            members,
        );
        let reference =
            SemanticIncomingReference::new(scope, SemanticReferenceKind::ForegroundMember);
        for member in previous {
            let empty = if let Some(incoming) = self.incoming_references.get_mut(&member) {
                incoming.retain(|candidate| *candidate != reference);
                incoming.is_empty()
            } else {
                false
            };
            if empty {
                self.incoming_references.remove(&member);
            }
        }
        // Split borrows across node storage and the reverse-reference index.
        let declared = &self.slots[scope.slot() as usize]
            .node
            .as_ref()
            .expect("preflighted foreground scope")
            .foreground_members;
        for &member in declared {
            let incoming = self.incoming_references.entry(member).or_default();
            debug_assert!(!incoming.contains(&reference));
            incoming.push(reference);
        }
    }

    /// Register all currently live semantic identities referenced by one owner.
    ///
    /// The reverse index is store metadata rather than authored node payload. Work
    /// is proportional to the owner's direct reference declarations; unrelated
    /// semantic slots are never scanned.
    pub(crate) fn register_semantic_references_for_owner(&mut self, owner: SemanticNodeId) {
        for (target, kind) in self.semantic_outgoing_references(owner) {
            if self.node(target).is_none() {
                // Low-level raw removal is still allowed to leave generation-safe
                // stale declarations. Such references deliberately do not attach
                // themselves to a later slot reuse.
                continue;
            }
            let reference = SemanticIncomingReference::new(owner, kind);
            let incoming = self.incoming_references.entry(target).or_default();
            if !incoming.contains(&reference) {
                incoming.push(reference);
            }
        }
    }

    /// Remove reverse-index entries owned by one node before changing or deleting
    /// its declaration topology.
    pub(crate) fn unregister_semantic_references_for_owner(&mut self, owner: SemanticNodeId) {
        for (target, kind) in self.semantic_outgoing_references(owner) {
            let reference = SemanticIncomingReference::new(owner, kind);
            let remove_key = if let Some(incoming) = self.incoming_references.get_mut(&target) {
                incoming.retain(|candidate| *candidate != reference);
                incoming.is_empty()
            } else {
                false
            };
            if remove_key {
                self.incoming_references.remove(&target);
            }
        }
    }

    fn semantic_outgoing_references(
        &self,
        owner: SemanticNodeId,
    ) -> Vec<(SemanticNodeId, SemanticReferenceKind)> {
        let Some(node) = self.node(owner) else {
            return Vec::new();
        };
        outgoing_references(node)
    }

    /// Compute every declaration that would be removed by the given explicit roots.
    ///
    /// Bindings are soft references and therefore do not add their owner to the
    /// removal set. Derived-signal and animation references are structural
    /// dependencies and do. Work follows only the reverse-reference closure.
    pub(crate) fn semantic_removal_closure(
        &self,
        roots: &HashSet<SemanticNodeId>,
    ) -> HashSet<SemanticNodeId> {
        let mut removed = HashSet::new();
        let mut stack = roots.iter().copied().collect::<Vec<_>>();

        while let Some(id) = stack.pop() {
            if !removed.insert(id) {
                continue;
            }
            let incoming = self
                .incoming_references
                .get(&id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for reference in incoming.iter().copied() {
                if self.node(reference.owner).is_none()
                    || !self.owner_still_references(reference.owner, id, reference.kind)
                {
                    continue;
                }
                match reference.kind {
                    SemanticReferenceKind::SignalBinding { .. }
                    | SemanticReferenceKind::ScopedSignal
                    | SemanticReferenceKind::ForegroundMember
                    | SemanticReferenceKind::Inset2DCameraFrame => {}
                    SemanticReferenceKind::SpatialAnchorFamily => {}
                    SemanticReferenceKind::EffectOwner
                    | SemanticReferenceKind::SignalDependency
                    | SemanticReferenceKind::AnimationTarget
                    | SemanticReferenceKind::AnimationTargetState
                    | SemanticReferenceKind::AnimationChild
                    | SemanticReferenceKind::GraphDependency => stack.push(reference.owner),
                }
            }
        }

        removed
    }

    /// Atomically remove one live node plus semantic declarations that cannot
    /// remain valid without it.
    ///
    /// Signal bindings are unbound in place. Derived signals and authored
    /// animations/compositions that directly reference the removed identity are
    /// themselves removed, which recursively cleans their referrers. Complexity is
    /// proportional to the transitive reverse-reference closure and the ordinary
    /// direct root/family relationships of removed nodes, never total scene size.
    pub(crate) fn remove_node_with_reverse_cleanup(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<super::SemanticRemoveNodeOutcome, SemanticStoreError> {
        if self.node(id).is_none() {
            return Err(SemanticStoreError::UnknownNode(id));
        }

        let mut outcome = SemanticRemoveNodeOutcome::default();
        let mut visiting = HashSet::new();
        self.remove_node_with_reverse_cleanup_inner(id, &mut outcome, &mut visiting)?;
        self.set_last_mutation_writes(outcome.written_slots.len());
        Ok(outcome)
    }

    fn remove_node_with_reverse_cleanup_inner(
        &mut self,
        id: SemanticNodeId,
        outcome: &mut SemanticRemoveNodeOutcome,
        visiting: &mut HashSet<SemanticNodeId>,
    ) -> Result<(), SemanticStoreError> {
        if self.node(id).is_none() {
            return Ok(());
        }
        if !visiting.insert(id) {
            // Signal dependency cycles are already forbidden and animation
            // declarations are append-only today. Keep this guard so corrupted
            // metadata cannot turn deletion into unbounded recursion.
            return Ok(());
        }

        outcome
            .effects
            .push(SemanticRemoveNodeEffect::NodeRemoved(id));
        let incoming = self
            .incoming_references
            .get(&id)
            .cloned()
            .unwrap_or_default();

        for reference in incoming {
            if self.node(reference.owner).is_none()
                || !self.owner_still_references(reference.owner, id, reference.kind)
            {
                continue;
            }

            match reference.kind {
                SemanticReferenceKind::SignalBinding { property } => {
                    let binding_matches = self
                        .node(reference.owner)
                        .and_then(SemanticNode::semantic_object_state)
                        .and_then(|state| {
                            state
                                .signal_bindings()
                                .iter()
                                .find(|binding| binding.property() == property)
                        })
                        .is_some_and(|binding| binding.signal() == id);
                    if binding_matches {
                        self.remove_semantic_signal_binding(reference.owner, property)
                            .expect("indexed semantic binding owner must remain a valid object");
                        outcome.written_slots.insert(reference.owner);
                        outcome
                            .effects
                            .push(SemanticRemoveNodeEffect::SubscriptionRemoved {
                                object: reference.owner,
                                property,
                            });
                    }
                }
                SemanticReferenceKind::ForegroundMember => {
                    let scope = reference.owner;
                    let members = self
                        .node_mut(scope)
                        .expect("indexed foreground owner is live")
                        .foreground_members_mut();
                    let previous_len = members.len();
                    members.retain(|member| *member != id);
                    if members.len() != previous_len {
                        outcome.written_slots.insert(scope);
                        outcome
                            .effects
                            .push(SemanticRemoveNodeEffect::ForegroundMembersChanged { scope });
                    }
                }
                SemanticReferenceKind::Inset2DCameraFrame => {
                    let owner = reference.owner;
                    let matches = self
                        .node(owner)
                        .and_then(SemanticNode::semantic_object_state)
                        .is_some_and(|state| {
                            matches!(
                                state.role(),
                                crate::SemanticObjectRole::Inset2DView(role)
                                    if role.camera_frame == id
                            )
                        });
                    if matches {
                        self.replace_semantic_object_role(
                            owner,
                            crate::SemanticObjectRole::Ordinary,
                        );
                        outcome.written_slots.insert(owner);
                        outcome
                            .effects
                            .push(SemanticRemoveNodeEffect::ObjectRoleReplaced(owner));
                    }
                }
                SemanticReferenceKind::SpatialAnchorFamily => {
                    let owner = reference.owner;
                    let matches = self
                        .node(owner)
                        .and_then(SemanticNode::semantic_object_state)
                        .is_some_and(|state| state.spatial_anchor_family() == Some(id));
                    if matches {
                        self.unregister_semantic_references_for_owner(owner);
                        let state = self
                            .node_mut(owner)
                            .expect("indexed anchor owner is live")
                            .semantic_object_state_mut()
                            .expect("anchor owner remains an object");
                        state
                            .set_spatial_composition_domain_with_anchor(
                                crate::SemanticSpatialCompositionDomain::FixedOrientation,
                                None,
                            )
                            .expect(
                                "clearing an anchor preserves valid fixed-orientation metadata",
                            );
                        self.register_semantic_references_for_owner(owner);
                        outcome.written_slots.insert(owner);
                        outcome
                            .effects
                            .push(SemanticRemoveNodeEffect::SpatialAnchorCleared(owner));
                    }
                }
                SemanticReferenceKind::ScopedSignal => {
                    let scope = reference.owner;
                    let removed = self
                        .node_mut(scope)
                        .is_some_and(|node| node.scoped_signals_mut().remove(&id));
                    if removed {
                        outcome.written_slots.insert(scope);
                    }
                }
                SemanticReferenceKind::EffectOwner
                | SemanticReferenceKind::SignalDependency
                | SemanticReferenceKind::AnimationTarget
                | SemanticReferenceKind::AnimationTargetState
                | SemanticReferenceKind::AnimationChild
                | SemanticReferenceKind::GraphDependency => {
                    self.remove_node_with_reverse_cleanup_inner(
                        reference.owner,
                        outcome,
                        visiting,
                    )?;
                }
            }
        }

        let node = self
            .node(id)
            .expect("node remains live until its reverse referrers are cleaned")
            .clone();
        if let Some(effect) = node.semantic_effect_state() {
            let owner = effect.owner();
            self.unlink_effect(id, owner);
            if self.node(owner).is_some() {
                outcome.written_slots.insert(owner);
                outcome
                    .effects
                    .push(SemanticRemoveNodeEffect::EffectAttachmentChanged { owner, effect: id });
            }
        }
        record_direct_remove_writes(&node, &mut outcome.written_slots);
        self.remove_node(id)?;
        visiting.remove(&id);
        Ok(())
    }

    fn owner_still_references(
        &self,
        owner: SemanticNodeId,
        target: SemanticNodeId,
        kind: SemanticReferenceKind,
    ) -> bool {
        let Some(node) = self.node(owner) else {
            return false;
        };
        match kind {
            SemanticReferenceKind::ForegroundMember => node.foreground_members().contains(&target),
            SemanticReferenceKind::SpatialAnchorFamily => node
                .semantic_object_state()
                .is_some_and(|state| state.spatial_anchor_family() == Some(target)),
            SemanticReferenceKind::GraphDependency => node
                .graph_declaration()
                .is_some_and(|graph| graph.references_node(target)),
            _ => outgoing_references(node)
                .into_iter()
                .any(|candidate| candidate == (target, kind)),
        }
    }
}

fn outgoing_references(node: &SemanticNode) -> Vec<(SemanticNodeId, SemanticReferenceKind)> {
    let mut references = Vec::new();

    if let Some(state) = node.semantic_object_state() {
        references.extend(state.signal_bindings().iter().map(|binding| {
            (
                binding.signal(),
                SemanticReferenceKind::SignalBinding {
                    property: binding.property(),
                },
            )
        }));
        if let crate::SemanticObjectRole::Inset2DView(role) = state.role() {
            references.push((role.camera_frame, SemanticReferenceKind::Inset2DCameraFrame));
        }
        references.extend(
            state
                .spatial_anchor_family()
                .map(|anchor| (anchor, SemanticReferenceKind::SpatialAnchorFamily)),
        );
    }

    references.extend(
        node.scoped_signals()
            .iter()
            .copied()
            .map(|signal| (signal, SemanticReferenceKind::ScopedSignal)),
    );

    references.extend(
        node.foreground_members()
            .iter()
            .copied()
            .map(|member| (member, SemanticReferenceKind::ForegroundMember)),
    );

    if let Some(graph) = node.graph_declaration() {
        references.extend(
            graph
                .referenced_nodes()
                .map(|target| (target, SemanticReferenceKind::GraphDependency)),
        );
    }

    match node.kind() {
        SemanticNodeKind::Effect(state) => {
            references.push((state.owner(), SemanticReferenceKind::EffectOwner))
        }
        SemanticNodeKind::Signal(state) => {
            if let SemanticSignalSource::Derived(expression) = state.source() {
                collect_signal_dependencies(expression, &mut references);
            }
        }
        SemanticNodeKind::Animation(state) => match state.intent() {
            SemanticAnimationIntent::ObjectPropertyTrack { target, values, .. } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
                if let crate::SemanticObjectTrackValues::Object { from, to } = values {
                    references.push((*from, SemanticReferenceKind::AnimationTargetState));
                    references.push((*to, SemanticReferenceKind::AnimationTargetState));
                }
            }
            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
                ..
            } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
                references.push((*target_state, SemanticReferenceKind::AnimationTargetState));
            }
            SemanticAnimationIntent::MoveAlongPath { target, path } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
                references.push((*path, SemanticReferenceKind::AnimationTargetState));
            }
            SemanticAnimationIntent::FamilyTransformTo {
                source,
                target_state,
                ..
            } => {
                references.push((*source, SemanticReferenceKind::AnimationTarget));
                references.push((*target_state, SemanticReferenceKind::AnimationTargetState));
            }
            SemanticAnimationIntent::Rotate { target, .. }
            | SemanticAnimationIntent::WorldTransformTo { target, .. }
            | SemanticAnimationIntent::CameraProfileTo { target, .. }
            | SemanticAnimationIntent::Indicate { target, .. }
            | SemanticAnimationIntent::DrawBorderThenFill { target, .. }
            | SemanticAnimationIntent::PassingFlash { target, .. }
            | SemanticAnimationIntent::SubsetDisplayMember { target, .. }
            | SemanticAnimationIntent::Fade { target, .. }
            | SemanticAnimationIntent::AffineLifecycle { target, .. }
            | SemanticAnimationIntent::Create { target }
            | SemanticAnimationIntent::Add { target } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
            }
            SemanticAnimationIntent::TextGlyph {
                target,
                family_member,
                ..
            } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
                if let Some(member) = family_member {
                    references.push((member.family, SemanticReferenceKind::AnimationTarget));
                }
            }
            SemanticAnimationIntent::SetScalar { signal, .. } => {
                references.push((*signal, SemanticReferenceKind::AnimationTarget));
            }
            SemanticAnimationIntent::Wait => {}
            SemanticAnimationIntent::Composition { children, .. } => {
                references.extend(
                    children
                        .iter()
                        .copied()
                        .map(|child| (child, SemanticReferenceKind::AnimationChild)),
                );
            }
        },
        SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family(_) => {}
    }

    references
}

fn collect_signal_dependencies(
    expression: &SemanticSignalExpr,
    references: &mut Vec<(SemanticNodeId, SemanticReferenceKind)>,
) {
    match expression {
        SemanticSignalExpr::Constant(_) => {}
        SemanticSignalExpr::Signal(signal) => {
            references.push((*signal, SemanticReferenceKind::SignalDependency));
        }
        SemanticSignalExpr::Add(lhs, rhs)
        | SemanticSignalExpr::Sub(lhs, rhs)
        | SemanticSignalExpr::Mul(lhs, rhs) => {
            collect_signal_dependencies(lhs, references);
            collect_signal_dependencies(rhs, references);
        }
        SemanticSignalExpr::Neg(value)
        | SemanticSignalExpr::Sin(value)
        | SemanticSignalExpr::Cos(value) => collect_signal_dependencies(value, references),
    }
}

fn record_direct_remove_writes(node: &SemanticNode, written_slots: &mut HashSet<SemanticNodeId>) {
    written_slots.insert(node.id());
    if let SemanticSceneMembership::Attached { previous, next } = node.scene_membership {
        written_slots.extend(previous);
        written_slots.extend(next);
    }
    written_slots.extend(node.parents().iter().copied());
    written_slots.extend(node.members());
}
