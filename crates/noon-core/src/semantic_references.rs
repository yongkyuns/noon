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
    SignalBinding { property: SemanticObjectProperty },
    ScopedSignal,
    AnimationTarget,
    AnimationTargetState,
    AnimationChild,
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
    SubscriptionRemoved {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
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
                    | SemanticReferenceKind::ScopedSignal => {}
                    SemanticReferenceKind::SignalDependency
                    | SemanticReferenceKind::AnimationTarget
                    | SemanticReferenceKind::AnimationTargetState
                    | SemanticReferenceKind::AnimationChild => stack.push(reference.owner),
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
                SemanticReferenceKind::ScopedSignal => {
                    let scope = reference.owner;
                    let removed = self
                        .node_mut(scope)
                        .is_some_and(|node| node.scoped_signals_mut().remove(&id));
                    if removed {
                        outcome.written_slots.insert(scope);
                    }
                }
                SemanticReferenceKind::SignalDependency
                | SemanticReferenceKind::AnimationTarget
                | SemanticReferenceKind::AnimationTargetState
                | SemanticReferenceKind::AnimationChild => {
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
        self.semantic_outgoing_references(owner)
            .into_iter()
            .any(|candidate| candidate == (target, kind))
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
    }

    references.extend(
        node.scoped_signals()
            .iter()
            .copied()
            .map(|signal| (signal, SemanticReferenceKind::ScopedSignal)),
    );

    match node.kind() {
        SemanticNodeKind::Signal(state) => {
            if let SemanticSignalSource::Derived(expression) = state.source() {
                collect_signal_dependencies(expression, &mut references);
            }
        }
        SemanticNodeKind::Animation(state) => match state.intent() {
            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
                ..
            } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
                references.push((*target_state, SemanticReferenceKind::AnimationTargetState));
            }
            SemanticAnimationIntent::Rotate { target, .. }
            | SemanticAnimationIntent::Indicate { target, .. }
            | SemanticAnimationIntent::DrawBorderThenFill { target, .. }
            | SemanticAnimationIntent::Fade { target, .. }
            | SemanticAnimationIntent::AffineLifecycle { target, .. }
            | SemanticAnimationIntent::Create { target }
            | SemanticAnimationIntent::Add { target } => {
                references.push((*target, SemanticReferenceKind::AnimationTarget));
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
        SemanticNodeKind::Object(_)
        | SemanticNodeKind::AuthoringObject
        | SemanticNodeKind::Family => {}
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
