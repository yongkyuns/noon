use std::collections::HashSet;

use super::{SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticVec3};

/// High-precision authored value carried by a semantic signal.
///
/// This is semantic state, not a runtime slot value. Lowering may specialize it
/// to a narrower representation when the target execution plan permits that.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalValue {
    Bool(bool),
    Scalar(f64),
    Vec3(SemanticVec3),
}

impl SemanticSignalValue {
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Bool(_) => true,
            Self::Scalar(value) => value.is_finite(),
            Self::Vec3(value) => value.is_finite(),
        }
    }
}

impl From<bool> for SemanticSignalValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for SemanticSignalValue {
    fn from(value: f64) -> Self {
        Self::Scalar(value)
    }
}

impl From<f32> for SemanticSignalValue {
    fn from(value: f32) -> Self {
        Self::Scalar(value as f64)
    }
}

impl From<SemanticVec3> for SemanticSignalValue {
    fn from(value: SemanticVec3) -> Self {
        Self::Vec3(value)
    }
}

/// Author-authored native reactive expression over semantic signal identity.
///
/// Signal references use the same scene-global generational [`SemanticNodeId`]
/// as every other semantic entity. `SignalId` remains a migration/execution-era
/// identity and is deliberately absent from the target authored model.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalExpr {
    Constant(SemanticSignalValue),
    Signal(SemanticNodeId),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    Neg(Box<Self>),
    Sin(Box<Self>),
    Cos(Box<Self>),
}

impl SemanticSignalExpr {
    pub const fn signal(signal: SemanticNodeId) -> Self {
        Self::Signal(signal)
    }

    pub fn scalar(value: f64) -> Self {
        Self::Constant(SemanticSignalValue::Scalar(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalSource {
    Input(SemanticSignalValue),
    Derived(SemanticSignalExpr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSignalState {
    source: SemanticSignalSource,
}

impl SemanticSignalState {
    pub const fn new(source: SemanticSignalSource) -> Self {
        Self { source }
    }

    pub const fn source(&self) -> &SemanticSignalSource {
        &self.source
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSignalError {
    UnknownSignal(SemanticNodeId),
    NotSignal(SemanticNodeId),
    NonFiniteValue,
    DependencyCycle(SemanticNodeId),
}

impl std::fmt::Display for SemanticSignalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSignal(id) => write!(
                formatter,
                "unknown semantic signal {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotSignal(id) => write!(
                formatter,
                "semantic node {}:{} is not a signal",
                id.slot(),
                id.generation()
            ),
            Self::NonFiniteValue => {
                formatter.write_str("semantic signal contains a non-finite value")
            }
            Self::DependencyCycle(id) => write!(
                formatter,
                "semantic signal {}:{} source would create a dependency cycle",
                id.slot(),
                id.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticSignalError {}

impl SemanticStore {
    /// Insert one authored input signal into the authoritative semantic identity space.
    pub fn insert_semantic_input_signal(
        &mut self,
        value: impl Into<SemanticSignalValue>,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let value = value.into();
        validate_value(&value)?;
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Input(value),
        )))
    }

    /// Insert one authored derived signal after validating its referenced closure.
    ///
    /// A new signal cannot reference its not-yet-allocated identity, so creation
    /// cannot introduce a cycle. Walking the existing dependency closure still
    /// rejects stale or non-signal references inherited through another signal.
    pub fn insert_semantic_derived_signal(
        &mut self,
        expression: SemanticSignalExpr,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let mut visited = HashSet::new();
        validate_expression_closure(self, &expression, None, &mut visited)?;
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Derived(expression),
        )))
    }

    pub fn semantic_signal_state(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticSignalState, SemanticSignalError> {
        let node = self
            .node(id)
            .ok_or(SemanticSignalError::UnknownSignal(id))?;
        match node.kind() {
            SemanticNodeKind::Signal(state) => Ok(state),
            _ => Err(SemanticSignalError::NotSignal(id)),
        }
    }

    /// Replace one signal's authored source while preserving semantic identity.
    ///
    /// Validation completes before the target node is written. Work is proportional
    /// to the new source's dependency closure; unrelated scene nodes are not scanned.
    /// A successful replacement writes exactly the target signal slot, while an
    /// invalid or identical replacement writes no semantic slots.
    pub fn set_semantic_signal_source(
        &mut self,
        id: SemanticNodeId,
        source: SemanticSignalSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let previous = self.semantic_signal_state(id)?.source().clone();

        let mut visited = HashSet::new();
        validate_source_closure(self, &source, Some(id), &mut visited)?;
        if previous == source {
            return Ok(false);
        }

        self.node_mut(id)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("semantic signal existence validated before mutation")
            .source = source;
        self.set_last_mutation_writes(1);
        Ok(true)
    }
}

fn validate_value(value: &SemanticSignalValue) -> Result<(), SemanticSignalError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SemanticSignalError::NonFiniteValue)
    }
}

fn validate_source_closure(
    store: &SemanticStore,
    source: &SemanticSignalSource,
    cycle_target: Option<SemanticNodeId>,
    visited: &mut HashSet<SemanticNodeId>,
) -> Result<(), SemanticSignalError> {
    match source {
        SemanticSignalSource::Input(value) => validate_value(value),
        SemanticSignalSource::Derived(expression) => {
            validate_expression_closure(store, expression, cycle_target, visited)
        }
    }
}

fn validate_expression_closure(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
    cycle_target: Option<SemanticNodeId>,
    visited: &mut HashSet<SemanticNodeId>,
) -> Result<(), SemanticSignalError> {
    match expression {
        SemanticSignalExpr::Constant(value) => validate_value(value),
        SemanticSignalExpr::Signal(id) => {
            validate_signal_dependency(store, *id, cycle_target, visited)
        }
        SemanticSignalExpr::Add(lhs, rhs)
        | SemanticSignalExpr::Sub(lhs, rhs)
        | SemanticSignalExpr::Mul(lhs, rhs) => {
            validate_expression_closure(store, lhs, cycle_target, visited)?;
            validate_expression_closure(store, rhs, cycle_target, visited)
        }
        SemanticSignalExpr::Neg(value)
        | SemanticSignalExpr::Sin(value)
        | SemanticSignalExpr::Cos(value) => {
            validate_expression_closure(store, value, cycle_target, visited)
        }
    }
}

fn validate_signal_dependency(
    store: &SemanticStore,
    id: SemanticNodeId,
    cycle_target: Option<SemanticNodeId>,
    visited: &mut HashSet<SemanticNodeId>,
) -> Result<(), SemanticSignalError> {
    if cycle_target == Some(id) {
        return Err(SemanticSignalError::DependencyCycle(id));
    }
    if !visited.insert(id) {
        return Ok(());
    }
    let state = store.semantic_signal_state(id)?;
    validate_source_closure(store, state.source(), cycle_target, visited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticObjectState, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn signals_share_the_scene_global_generational_identity_space() {
        let mut store = SemanticStore::new();
        let object = object(&mut store, 1.0);
        let signal = store.insert_semantic_input_signal(1.25_f64).unwrap();

        assert_ne!(object, signal);
        assert!(matches!(
            store.semantic_signal_state(signal).unwrap().source(),
            SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) if *value == 1.25
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.scene_root_count(), 0);
    }

    #[test]
    fn derived_signal_references_semantic_node_identity() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(input)),
                Box::new(SemanticSignalExpr::scalar(3.0)),
            ))
            .unwrap();

        assert!(matches!(
            store.semantic_signal_state(derived).unwrap().source(),
            SemanticSignalSource::Derived(_)
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn signal_validation_rejects_stale_and_non_signal_dependencies_before_insertion() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 2.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());

        let before = store.len();
        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(stale)),
            Err(SemanticSignalError::UnknownSignal(stale))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(replacement)),
            Err(SemanticSignalError::NotSignal(replacement))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_creation_rejects_stale_transitive_dependency_closure() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(input))
            .unwrap();
        store.remove_node(input).unwrap();

        let before = store.len();
        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(derived)),
            Err(SemanticSignalError::UnknownSignal(input))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn non_finite_signal_values_are_rejected_without_mutation() {
        let mut store = SemanticStore::new();
        assert_eq!(
            store.insert_semantic_input_signal(f64::NAN),
            Err(SemanticSignalError::NonFiniteValue)
        );
        assert_eq!(store.len(), 0);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_creation_cost_does_not_scale_with_unrelated_scene_nodes() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let input = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Sin(Box::new(
                SemanticSignalExpr::signal(input),
            )))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(store.semantic_signal_state(derived).is_ok());
    }

    #[test]
    fn signal_source_replacement_preserves_identity_and_one_slot_locality() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let dependency = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let target = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let source = SemanticSignalSource::Derived(SemanticSignalExpr::Add(
            Box::new(SemanticSignalExpr::signal(dependency)),
            Box::new(SemanticSignalExpr::scalar(3.0)),
        ));
        let before_len = store.len();

        assert!(store
            .set_semantic_signal_source(target, source.clone())
            .unwrap());
        assert_eq!(store.node(target).unwrap().id(), target);
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &source
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        assert!(!store
            .set_semantic_signal_source(target, source.clone())
            .unwrap());
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &source
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_source_replacement_rejects_direct_and_indirect_cycles_atomically() {
        let mut store = SemanticStore::new();
        let a = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let b = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(a))
            .unwrap();
        let c = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(b))
            .unwrap();
        let a_before = store.semantic_signal_state(a).unwrap().source().clone();
        let b_before = store.semantic_signal_state(b).unwrap().source().clone();

        assert_eq!(
            store.set_semantic_signal_source(
                a,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(c)),
            ),
            Err(SemanticSignalError::DependencyCycle(a))
        );
        assert_eq!(store.semantic_signal_state(a).unwrap().source(), &a_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.set_semantic_signal_source(
                b,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(b)),
            ),
            Err(SemanticSignalError::DependencyCycle(b))
        );
        assert_eq!(store.semantic_signal_state(b).unwrap().source(), &b_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_source_replacement_rejects_invalid_dependency_closure_atomically() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let transitive = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(stale))
            .unwrap();
        let target = store.insert_semantic_input_signal(5.0_f64).unwrap();
        let target_before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();
        store.remove_node(stale).unwrap();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(transitive)),
            ),
            Err(SemanticSignalError::UnknownSignal(stale))
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let non_signal = object(&mut store, 2.0);
        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(non_signal)),
            ),
            Err(SemanticSignalError::NotSignal(non_signal))
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Input(SemanticSignalValue::Scalar(f64::NAN)),
            ),
            Err(SemanticSignalError::NonFiniteValue)
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_nodes_are_not_scene_family_or_updater_targets() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(true).unwrap();
        let family = store.insert_family();

        assert!(matches!(
            store.add_semantic_scene_nodes(&[signal]),
            Err(super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id)) if id == signal
        ));
        assert!(matches!(
            store.add_semantic_family_member(family, signal),
            Err(super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id)) if id == signal
        ));
        assert!(matches!(
            store.add_semantic_updater(signal, super::super::HostCallbackId::new(1)),
            Err(super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id)) if id == signal
        ));
    }
}
