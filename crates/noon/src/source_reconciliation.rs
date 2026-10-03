//! Bounded source-declaration reconciliation through the existing Scene mutation lane.
//!
//! [`SourceCandidate`] is deliberately inert comparison input: it has declarations
//! and a generation, but no semantic store, runtime, playhead, or session authority.

use std::{collections::HashSet, error::Error};

use noon_core::{
    SceneRevision, SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectState, SemanticStoreIdentity, SemanticTransactionNodeRef, SourceIdentity,
};

use crate::{AuthoringError, ExecutionSessionPublicationError, Scene};

/// Ordered source-reexecution result. A reconciler accepts a generation once only
/// after a successful coherent publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceGeneration(u64);
impl SourceGeneration {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A direct, keyed object declaration in a temporary source candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceObjectDeclaration {
    source: SourceIdentity,
    state: SemanticObjectState,
}
impl SourceObjectDeclaration {
    pub fn new(source: SourceIdentity, state: SemanticObjectState) -> Self {
        Self { source, state }
    }
    pub fn source(&self) -> &SourceIdentity {
        &self.source
    }
    pub fn state(&self) -> &SemanticObjectState {
        &self.state
    }
}

/// A non-live, source-keyed declaration list for one Scene root.
///
/// This slice intentionally reconciles direct object declarations only. Families,
/// animation graphs, signals, and host callbacks require their own migration policy
/// rather than becoming a draft Scene or a second runtime authority.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceCandidate {
    generation: SourceGeneration,
    base_scope: SourceScope,
    base_revision: SceneRevision,
    declarations: Vec<SourceObjectDeclaration>,
    sources: HashSet<SourceIdentity>,
}
impl SourceCandidate {
    /// Capture the authoritative Scene revision before source re-execution starts.
    /// A delayed candidate can then never publish over a later source result, even
    /// if it is submitted through a different reconciler instance.
    pub fn new(scene: &Scene, generation: SourceGeneration) -> Self {
        Self {
            generation,
            base_scope: SourceScope {
                store: scene.integration_store().borrow().identity(),
                root: scene.root(),
            },
            base_revision: scene.revision(),
            declarations: Vec::new(),
            sources: HashSet::new(),
        }
    }
    pub const fn generation(&self) -> SourceGeneration {
        self.generation
    }
    pub fn declarations(&self) -> &[SourceObjectDeclaration] {
        &self.declarations
    }
    pub fn declare(
        &mut self,
        declaration: SourceObjectDeclaration,
    ) -> Result<&mut Self, SourceCandidateError> {
        if !self.sources.insert(declaration.source.clone()) {
            return Err(SourceCandidateError::DuplicateSourceIdentity(
                declaration.source,
            ));
        }
        self.declarations.push(declaration);
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceCandidateError {
    DuplicateSourceIdentity(SourceIdentity),
}
impl std::fmt::Display for SourceCandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateSourceIdentity(key) => {
                write!(f, "duplicate source identity in candidate: {key:?}")
            }
        }
    }
}
impl Error for SourceCandidateError {}

/// Retains only generation and provenance validation; it never retains a source
/// candidate or a live Scene/runtime mirror.
#[derive(Debug, Default)]
pub struct SourceReconciler {
    accepted_generation: Option<SourceGeneration>,
    scope: Option<SourceScope>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceScope {
    store: SemanticStoreIdentity,
    root: SemanticNodeId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceReconciliationResult {
    generation: SourceGeneration,
    nodes: Vec<(SourceIdentity, SemanticNodeId)>,
    mutation_impacts: usize,
}
impl SourceReconciliationResult {
    pub const fn generation(&self) -> SourceGeneration {
        self.generation
    }
    pub fn nodes(&self) -> &[(SourceIdentity, SemanticNodeId)] {
        &self.nodes
    }
    pub fn node_for(&self, source: &SourceIdentity) -> Option<SemanticNodeId> {
        self.nodes
            .iter()
            .find_map(|(key, node)| (key == source).then_some(*node))
    }
    pub const fn mutation_impacts(&self) -> usize {
        self.mutation_impacts
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceReconciliationError {
    StaleGeneration {
        candidate: SourceGeneration,
        accepted: SourceGeneration,
    },
    ForeignSceneScope,
    ForeignCandidateScope,
    StaleSceneRevision {
        candidate: SceneRevision,
        actual: SceneRevision,
    },
    UnmanagedScopeMember {
        root: SemanticNodeId,
        member: SemanticNodeId,
    },
    SourceOutsideScope {
        source: SourceIdentity,
        node: SemanticNodeId,
        root: SemanticNodeId,
    },
    SourceIsNotObject {
        source: SourceIdentity,
        node: SemanticNodeId,
    },
    UnsupportedDeclarationChange {
        source: SourceIdentity,
        field: &'static str,
    },
    /// The bounded static slice cannot rebase a persistent source edit beneath
    /// an existing native-signal or host-updater driver.
    ActiveDeclarationDriver {
        source: SourceIdentity,
        kind: &'static str,
    },
    /// Source changes cannot overwrite a transient input interaction; callers must
    /// wait for it to settle or cancel it through its interaction policy first.
    ActiveInteraction,
    /// Existing callback/segment/replay publication barriers are explicit conflicts.
    LivePublication(ExecutionSessionPublicationError),
    Authoring(AuthoringError),
}
impl std::fmt::Display for SourceReconciliationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleGeneration {
                candidate,
                accepted,
            } => write!(
                f,
                "stale source generation {} cannot replace generation {}",
                candidate.get(),
                accepted.get()
            ),
            Self::ForeignSceneScope => {
                f.write_str("source reconciler is bound to another Scene root")
            }
            Self::ForeignCandidateScope => {
                f.write_str("source candidate was built for another Scene root")
            }
            Self::StaleSceneRevision { candidate, actual } => write!(
                f,
                "source candidate expects scene revision {}, but the live Scene is at {}",
                candidate.get(),
                actual.get()
            ),
            Self::UnmanagedScopeMember { root, member } => write!(
                f,
                "root {}:{} has an unmanaged member {}:{}",
                root.slot(),
                root.generation(),
                member.slot(),
                member.generation()
            ),
            Self::SourceOutsideScope { source, .. } => {
                write!(f, "source identity belongs outside this root: {source:?}")
            }
            Self::SourceIsNotObject { source, .. } => {
                write!(f, "source identity is not an object: {source:?}")
            }
            Self::UnsupportedDeclarationChange { source, field } => {
                write!(f, "unsupported {field} declaration change for {source:?}")
            }
            Self::ActiveDeclarationDriver { source, kind } => {
                write!(
                    f,
                    "cannot reconcile {source:?} beneath an active {kind} driver"
                )
            }
            Self::ActiveInteraction => {
                f.write_str("cannot reconcile source while an interaction effect is active")
            }
            Self::LivePublication(error) => error.fmt(f),
            Self::Authoring(error) => error.fmt(f),
        }
    }
}
impl Error for SourceReconciliationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LivePublication(error) => Some(error),
            Self::Authoring(error) => Some(error),
            _ => None,
        }
    }
}

impl SourceReconciler {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn accepted_generation(&self) -> Option<SourceGeneration> {
        self.accepted_generation
    }

    /// Emit one local persistent semantic transaction against the authoritative
    /// Scene. Matching keys retain semantic IDs; new IDs appear only at commit.
    /// Existing Scene publication preflights lowering/runtime work before commit,
    /// so every error leaves the previous coherent state visible.
    pub fn reconcile(
        &mut self,
        scene: &mut Scene,
        candidate: &SourceCandidate,
    ) -> Result<SourceReconciliationResult, SourceReconciliationError> {
        if let Some(accepted) = self.accepted_generation.max(scene.source_generation) {
            if candidate.generation <= accepted {
                return Err(SourceReconciliationError::StaleGeneration {
                    candidate: candidate.generation,
                    accepted,
                });
            }
        }
        let root = scene.root();
        let store_identity = scene.integration_store().borrow().identity();
        if candidate.base_scope.root != root || candidate.base_scope.store != store_identity {
            return Err(SourceReconciliationError::ForeignCandidateScope);
        }
        if candidate.base_revision != scene.revision() {
            return Err(SourceReconciliationError::StaleSceneRevision {
                candidate: candidate.base_revision,
                actual: scene.revision(),
            });
        }
        if let Some(scope) = &self.scope {
            if scope.root != root || scope.store != store_identity {
                return Err(SourceReconciliationError::ForeignSceneScope);
            }
        }

        if let Some(execution) = scene.running_execution_mut() {
            let expected = execution.publication_context().scene_revision();
            if expected != candidate.base_revision {
                return Err(SourceReconciliationError::LivePublication(
                    ExecutionSessionPublicationError::StaleSceneRevision {
                        expected,
                        actual: candidate.base_revision,
                    },
                ));
            }
        }
        let mut transaction = SemanticMutationTransaction::new();
        let mut references = Vec::with_capacity(candidate.declarations.len());
        let (stale, mut changed, order_changed) = {
            let store = scene.integration_store().borrow();
            let members = store
                .semantic_family_members_checked(root)
                .expect("Scene root remains a family");
            let member_set = members.iter().copied().collect::<HashSet<_>>();
            let mut current_sources = Vec::with_capacity(members.len());
            for member in &members {
                let Some(source) = store.node(*member).expect("live member").source_identity()
                else {
                    return Err(SourceReconciliationError::UnmanagedScopeMember {
                        root,
                        member: *member,
                    });
                };
                current_sources.push(source.clone());
            }
            let mut matched = HashSet::new();
            let mut changed = false;
            for declaration in &candidate.declarations {
                if let Some(node) = store.node_for_source(&declaration.source) {
                    if !member_set.contains(&node) {
                        return Err(SourceReconciliationError::SourceOutsideScope {
                            source: declaration.source.clone(),
                            node,
                            root,
                        });
                    }
                    let current = store.semantic_object_state_checked(node).map_err(|_| {
                        SourceReconciliationError::SourceIsNotObject {
                            source: declaration.source.clone(),
                            node,
                        }
                    })?;
                    changed |= stage_object_delta(
                        &mut transaction,
                        node,
                        current,
                        store
                            .node(node)
                            .expect("source node remains live during candidate staging")
                            .host_updaters()
                            .is_empty(),
                        &declaration.state,
                        &declaration.source,
                    )?;
                    matched.insert(node);
                    references.push(SemanticTransactionNodeRef::Existing(node));
                } else {
                    let token = transaction.create_node(
                        SemanticNodeCreation::object(declaration.state.clone())
                            .with_source_identity(declaration.source.clone()),
                    );
                    transaction.add_member(root, token);
                    references.push(SemanticTransactionNodeRef::Pending(token));
                    changed = true;
                }
            }
            let stale = members
                .into_iter()
                .filter(|node| !matched.contains(node))
                .collect::<Vec<_>>();
            changed |= !stale.is_empty();
            let expected_sources = candidate
                .declarations
                .iter()
                .map(|declaration| declaration.source.clone())
                .collect::<Vec<_>>();
            (stale, changed, current_sources != expected_sources)
        };
        changed |= order_changed;
        if order_changed {
            for index in (0..references.len()).rev() {
                transaction.reorder_member_ref(
                    root,
                    references[index],
                    references.get(index + 1).copied(),
                );
            }
        }
        // Node deletion is terminal in the shared transaction vocabulary.
        for node in stale {
            transaction.remove_node(node);
        }

        if changed
            && scene
                .running_execution_mut()
                .is_some_and(|execution| execution.interactions_active())
        {
            return Err(SourceReconciliationError::ActiveInteraction);
        }
        // A source generation with identical declarations has nothing to
        // publish. Preserve a running segment, callback barrier, and effective
        // drivers without asking the authored-mutation lane to acquire them.
        // Scope, revision, generation, and declaration validation still apply.
        let impacts = if changed {
            scene
                .apply_semantic_transaction(transaction)
                .map_err(|error| match error {
                    AuthoringError::ExecutionPublication(error) => {
                        SourceReconciliationError::LivePublication(error)
                    }
                    other => SourceReconciliationError::Authoring(other),
                })?
                .impacts()
                .len()
        } else {
            0
        };
        let nodes = {
            let store = scene.integration_store().borrow();
            candidate
                .declarations
                .iter()
                .map(|declaration| {
                    (
                        declaration.source.clone(),
                        store
                            .node_for_source(&declaration.source)
                            .expect("committed declaration resolves"),
                    )
                })
                .collect()
        };
        self.scope.get_or_insert(SourceScope {
            store: store_identity,
            root,
        });
        self.accepted_generation = Some(candidate.generation);
        scene.source_generation = Some(candidate.generation);
        Ok(SourceReconciliationResult {
            generation: candidate.generation,
            nodes,
            mutation_impacts: impacts,
        })
    }
}

fn stage_object_delta(
    transaction: &mut SemanticMutationTransaction,
    object: SemanticNodeId,
    current: &SemanticObjectState,
    has_no_host_updaters: bool,
    candidate: &SemanticObjectState,
    source: &SourceIdentity,
) -> Result<bool, SourceReconciliationError> {
    for (field, same) in [
        ("role", current.role() == candidate.role()),
        (
            "text presentation baseline",
            current.text_presentation_baseline() == candidate.text_presentation_baseline(),
        ),
        (
            "decimal number metadata",
            current.decimal_number() == candidate.decimal_number(),
        ),
        (
            "bar metadata",
            current.bar_metadata() == candidate.bar_metadata(),
        ),
        (
            "signal bindings",
            current.signal_bindings() == candidate.signal_bindings(),
        ),
    ] {
        if !same {
            return Err(SourceReconciliationError::UnsupportedDeclarationChange {
                source: source.clone(),
                field,
            });
        }
    }
    let persistent_change = current.content != candidate.content
        || current.transform != candidate.transform
        || current.style != candidate.style
        || current.z_index() != candidate.z_index()
        || current.click_indicate() != candidate.click_indicate();
    if persistent_change && !current.signal_bindings().is_empty() {
        return Err(SourceReconciliationError::ActiveDeclarationDriver {
            source: source.clone(),
            kind: "native signal",
        });
    }
    if persistent_change && !has_no_host_updaters {
        return Err(SourceReconciliationError::ActiveDeclarationDriver {
            source: source.clone(),
            kind: "host updater",
        });
    }
    let mut changed = false;
    if current.content != candidate.content {
        transaction.replace_content(object, candidate.content);
        changed = true;
    }
    if current.transform != candidate.transform {
        transaction.set_object_transform(object, candidate.transform);
        changed = true;
    }
    if current.style != candidate.style {
        transaction.replace_style(object, candidate.style.clone());
        changed = true;
    }
    if current.z_index() != candidate.z_index() {
        transaction.set_z_index(object, candidate.z_index());
        changed = true;
    }
    if current.click_indicate() != candidate.click_indicate() {
        transaction.set_click_indicate(object, candidate.click_indicate());
        changed = true;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{SemanticObjectState, SourceIdentity, StoredGeometry};
    use std::rc::Rc;
    fn key(name: &str) -> SourceIdentity {
        SourceIdentity::ExplicitKey(name.into())
    }
    fn state(x: f64) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        state.transform.translation.x = x;
        state
    }
    fn candidate(
        scene: &Scene,
        generation: u64,
        declarations: impl IntoIterator<Item = (&'static str, f64)>,
    ) -> SourceCandidate {
        let mut candidate = SourceCandidate::new(scene, SourceGeneration::new(generation));
        for (name, x) in declarations {
            candidate
                .declare(SourceObjectDeclaration::new(key(name), state(x)))
                .unwrap();
        }
        candidate
    }
    #[test]
    fn keyed_reorder_preserves_semantic_identities() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("left", -1.0), ("right", 1.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let left = first.node_for(&key("left")).unwrap();
        let right = first.node_for(&key("right")).unwrap();
        let reordered = candidate(&scene, 2, [("right", 1.0), ("left", -1.0)]);
        let second = reconciler.reconcile(&mut scene, &reordered).unwrap();
        assert_eq!(second.node_for(&key("left")), Some(left));
        assert_eq!(second.node_for(&key("right")), Some(right));
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(scene.root())
                .unwrap(),
            vec![right, left]
        );
    }
    #[test]
    fn local_edit_preserves_unrelated_keyed_state_and_identity() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("edited", 0.0), ("stable", 4.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let stable = first.node_for(&key("stable")).unwrap();
        let before = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(stable)
            .unwrap()
            .clone();
        let edited = candidate(&scene, 2, [("edited", 2.0), ("stable", 4.0)]);
        let second = reconciler.reconcile(&mut scene, &edited).unwrap();
        assert_eq!(second.node_for(&key("stable")), Some(stable));
        assert_eq!(second.mutation_impacts(), 1);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(stable)
                .unwrap(),
            &before
        );
    }
    #[test]
    fn stale_generation_is_rejected_without_changing_the_scene() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let accepted = candidate(&scene, 2, [("only", 1.0)]);
        reconciler.reconcile(&mut scene, &accepted).unwrap();
        let revision = scene.revision();
        let stale = candidate(&scene, 1, [("only", 3.0)]);
        assert!(matches!(
            reconciler.reconcile(&mut scene, &stale),
            Err(SourceReconciliationError::StaleGeneration { .. })
        ));
        assert_eq!(scene.revision(), revision);
    }

    #[test]
    fn no_op_generation_rejects_older_results_from_another_reconciler() {
        let mut scene = Scene::new();
        let initial = candidate(&scene, 1, [("only", 1.0)]);
        SourceReconciler::new()
            .reconcile(&mut scene, &initial)
            .unwrap();
        let late = candidate(&scene, 2, [("only", 2.0)]);
        let unchanged = candidate(&scene, 3, [("only", 1.0)]);
        let revision = scene.revision();
        assert_eq!(
            SourceReconciler::new()
                .reconcile(&mut scene, &unchanged)
                .unwrap()
                .mutation_impacts(),
            0
        );
        assert_eq!(scene.revision(), revision);
        assert!(
            matches!(SourceReconciler::new().reconcile(&mut scene, &late),
            Err(SourceReconciliationError::StaleGeneration { candidate, accepted })
                if candidate == SourceGeneration::new(2) && accepted == SourceGeneration::new(3))
        );
        assert_eq!(scene.revision(), revision);
    }

    #[test]
    fn stale_candidate_is_rejected_across_reconcilers_by_its_base_revision() {
        let mut scene = Scene::new();
        let first = candidate(&scene, 1, [("only", 1.0)]);
        let late = candidate(&scene, 2, [("only", 2.0)]);
        SourceReconciler::new()
            .reconcile(&mut scene, &first)
            .unwrap();

        assert!(matches!(
            SourceReconciler::new().reconcile(&mut scene, &late),
            Err(SourceReconciliationError::StaleSceneRevision { .. })
        ));
    }
    #[test]
    fn ambiguous_candidate_or_scope_is_rejected_explicitly() {
        let scene = Scene::new();
        let mut duplicate = SourceCandidate::new(&scene, SourceGeneration::new(1));
        duplicate
            .declare(SourceObjectDeclaration::new(key("same"), state(0.0)))
            .unwrap();
        assert!(matches!(
            duplicate.declare(SourceObjectDeclaration::new(key("same"), state(1.0))),
            Err(SourceCandidateError::DuplicateSourceIdentity(_))
        ));
        let mut scene = Scene::new();
        let unmanaged = scene.circle(1.0).unwrap();
        scene.add(&unmanaged).unwrap();
        let revision = scene.revision();
        let empty = candidate(&scene, 1, []);
        assert!(matches!(
            SourceReconciler::new().reconcile(&mut scene, &empty),
            Err(SourceReconciliationError::UnmanagedScopeMember { .. })
        ));
        assert_eq!(scene.revision(), revision);
    }
    #[test]
    fn failed_reconciliation_is_atomic_and_does_not_consume_generation() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("only", 0.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let node = first.node_for(&key("only")).unwrap();
        let before = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        let revision = scene.revision();
        let mut invalid = SourceCandidate::new(&scene, SourceGeneration::new(2));
        invalid
            .declare(SourceObjectDeclaration::new(
                key("only"),
                SemanticObjectState::new(StoredGeometry::Circle { radius: f32::NAN }),
            ))
            .unwrap();
        assert!(matches!(
            reconciler.reconcile(&mut scene, &invalid),
            Err(SourceReconciliationError::Authoring(_))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(node)
                .unwrap(),
            &before
        );
        let valid = candidate(&scene, 2, [("only", 2.0)]);
        reconciler.reconcile(&mut scene, &valid).unwrap();
    }

    #[test]
    fn unchanged_source_preserves_an_active_segment_without_publication() {
        use noon_core::AnimationOptions;
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("moving", 0.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let node = first.node_for(&key("moving")).unwrap();
        let store = Rc::clone(scene.integration_store());
        let animation = {
            let mut store = store.borrow_mut();
            let endpoint = store.insert_semantic_object(state(2.0));
            store
                .insert_semantic_transform_animation(node, endpoint, AnimationOptions::new())
                .unwrap()
        };
        let mut execution = scene.execution_session().unwrap();
        let segment = execution
            .activate_animation_segment(
                &store.borrow(),
                animation,
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap();
        execution.advance_segment_to(segment, 0.5).unwrap();
        scene.install_execution(execution);
        let publication = scene.owned_execution().publication_context();
        let runtime = scene.owned_execution().runtime_identity();
        let transform = scene.owned_execution().frame().objects[0].transform;
        assert_eq!(transform.translation.x, 1.0);

        let unchanged = candidate(&scene, 2, [("moving", 0.0)]);
        let result = reconciler.reconcile(&mut scene, &unchanged).unwrap();
        assert_eq!(result.mutation_impacts(), 0);
        assert_eq!(result.node_for(&key("moving")), Some(node));
        assert_eq!(scene.owned_execution().runtime_identity(), runtime);
        assert_eq!(scene.owned_execution().publication_context(), publication);
        assert_eq!(
            scene.owned_execution().frame().objects[0].transform,
            transform
        );
        assert_eq!(scene.owned_execution().frame().time, 0.5);
        assert_eq!(
            reconciler.accepted_generation(),
            Some(SourceGeneration::new(2))
        );

        let changed = candidate(&scene, 3, [("moving", 4.0)]);
        assert!(matches!(
            reconciler.reconcile(&mut scene, &changed),
            Err(SourceReconciliationError::LivePublication(
                ExecutionSessionPublicationError::SegmentCompletionPending
            ))
        ));
        assert_eq!(scene.owned_execution().publication_context(), publication);
        assert_eq!(
            reconciler.accepted_generation(),
            Some(SourceGeneration::new(2))
        );
        scene
            .owned_execution_mut()
            .advance_segment_to(segment, 1.0)
            .unwrap();
        scene
            .owned_execution_mut()
            .complete_segment(&mut store.borrow_mut(), segment)
            .unwrap();
        assert_eq!(
            scene.owned_execution().frame().objects[0]
                .transform
                .translation
                .x,
            2.0
        );
    }

    #[test]
    fn source_edit_rejects_an_existing_native_signal_driver() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("driven", 0.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let node = first.node_for(&key("driven")).unwrap();
        let object = crate::Mobject::from_node(Rc::clone(scene.integration_store()), node).unwrap();
        let signal = scene.pointer_position_signal().unwrap();
        scene.bind_native_translation(&object, &signal).unwrap();
        let revision = scene.revision();
        let mut driven = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        driven.transform.translation.x = 2.0;
        let mut edited = SourceCandidate::new(&scene, SourceGeneration::new(2));
        edited
            .declare(SourceObjectDeclaration::new(key("driven"), driven))
            .unwrap();

        assert!(matches!(
            reconciler.reconcile(&mut scene, &edited),
            Err(SourceReconciliationError::ActiveDeclarationDriver {
                kind: "native signal",
                ..
            })
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            reconciler.accepted_generation(),
            Some(SourceGeneration::new(1))
        );
    }

    #[test]
    fn live_reconciliation_preserves_runtime_and_rejects_failed_publication_atomically() {
        let mut scene = Scene::new();
        let mut reconciler = SourceReconciler::new();
        let initial = candidate(&scene, 1, [("edited", 0.0), ("stable", 4.0)]);
        let first = reconciler.reconcile(&mut scene, &initial).unwrap();
        let edited = first.node_for(&key("edited")).unwrap();
        let stable = first.node_for(&key("stable")).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        scene.owned_execution_mut().take_frame_changes();

        let runtime = scene.owned_execution().runtime_identity();
        let publication = scene.owned_execution().publication_context();
        let frame_time = scene.owned_execution().frame().time;
        let stable_object = scene.owned_execution().execution_object_id(stable).unwrap();
        let stable_before = scene
            .owned_execution()
            .frame()
            .objects
            .iter()
            .find(|object| object.id == stable_object)
            .unwrap()
            .transform;
        let revision = scene.revision();

        let mut invalid = SourceCandidate::new(&scene, SourceGeneration::new(2));
        invalid
            .declare(SourceObjectDeclaration::new(
                key("edited"),
                SemanticObjectState::new(StoredGeometry::Circle { radius: f32::NAN }),
            ))
            .unwrap();
        invalid
            .declare(SourceObjectDeclaration::new(
                key("stable"),
                scene
                    .integration_store()
                    .borrow()
                    .semantic_object_state_checked(stable)
                    .unwrap()
                    .clone(),
            ))
            .unwrap();
        assert!(matches!(
            reconciler.reconcile(&mut scene, &invalid),
            Err(SourceReconciliationError::LivePublication(_))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.owned_execution().runtime_identity(), runtime);
        assert_eq!(scene.owned_execution().publication_context(), publication);
        assert_eq!(scene.owned_execution().frame().time, frame_time);
        assert_eq!(
            scene
                .owned_execution()
                .frame()
                .objects
                .iter()
                .find(|object| object.id == stable_object)
                .unwrap()
                .transform,
            stable_before
        );
        assert_eq!(
            reconciler.accepted_generation(),
            Some(SourceGeneration::new(1))
        );

        let edited_candidate = candidate(&scene, 2, [("edited", 2.0), ("stable", 4.0)]);
        let result = reconciler.reconcile(&mut scene, &edited_candidate).unwrap();
        assert_eq!(result.node_for(&key("edited")), Some(edited));
        assert_eq!(scene.owned_execution().runtime_identity(), runtime);
        assert_eq!(scene.owned_execution().frame().time, frame_time);
        assert_eq!(
            scene
                .owned_execution()
                .last_structural_publication_stats()
                .entered_objects,
            0
        );
        assert_eq!(
            scene
                .owned_execution()
                .last_structural_publication_stats()
                .exited_objects,
            0
        );
        let edited_object = scene.owned_execution().execution_object_id(edited).unwrap();
        let edited_index = scene
            .owned_execution()
            .frame()
            .objects
            .iter()
            .position(|object| object.id == edited_object)
            .unwrap();
        assert_eq!(
            scene
                .owned_execution_mut()
                .take_frame_changes()
                .object_indices(),
            &[edited_index]
        );
    }
}
