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
        if !value.is_finite() {
            return Err(SemanticSignalError::NonFiniteValue);
        }
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Input(value),
        )))
    }

    /// Insert one authored derived signal after validating only its referenced closure.
    ///
    /// Because a new signal cannot reference its not-yet-allocated identity, this
    /// creation-only slice cannot introduce a cycle. Source replacement/cycle
    /// validation belongs with the later semantic mutation transaction work.
    pub fn insert_semantic_derived_signal(
        &mut self,
        expression: SemanticSignalExpr,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        validate_expression(self, &expression)?;
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
}

fn validate_expression(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
) -> Result<(), SemanticSignalError> {
    match expression {
        SemanticSignalExpr::Constant(value) => {
            if value.is_finite() {
                Ok(())
            } else {
                Err(SemanticSignalError::NonFiniteValue)
            }
        }
        SemanticSignalExpr::Signal(id) => {
            store.semantic_signal_state(*id)?;
            Ok(())
        }
        SemanticSignalExpr::Add(lhs, rhs)
        | SemanticSignalExpr::Sub(lhs, rhs)
        | SemanticSignalExpr::Mul(lhs, rhs) => {
            validate_expression(store, lhs)?;
            validate_expression(store, rhs)
        }
        SemanticSignalExpr::Neg(value)
        | SemanticSignalExpr::Sin(value)
        | SemanticSignalExpr::Cos(value) => validate_expression(store, value),
    }
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
