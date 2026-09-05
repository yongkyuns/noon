use std::collections::HashMap;

use super::{
    NativeEventSource, NativeStateSource, SemanticNodeId, SemanticNodeKind, SemanticStore,
    SemanticVec3,
};

/// Stable authored value kind of a semantic signal.
///
/// This is semantic vocabulary, not the execution-layer `ValueKind`. Lowering may
/// specialize `Vec3` or scalar precision for a target runtime without changing the
/// authored signal contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticSignalValueKind {
    Bool,
    Scalar,
    Vec3,
}

impl std::fmt::Display for SemanticSignalValueKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Bool => "bool",
            Self::Scalar => "scalar",
            Self::Vec3 => "vec3",
        })
    }
}

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
    pub const fn value_kind(&self) -> SemanticSignalValueKind {
        match self {
            Self::Bool(_) => SemanticSignalValueKind::Bool,
            Self::Scalar(_) => SemanticSignalValueKind::Scalar,
            Self::Vec3(_) => SemanticSignalValueKind::Vec3,
        }
    }

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

/// Authored native source that drives one semantic input signal.
///
/// Platform hosts collect these language-neutral sources, while lowering maps the
/// owning semantic signal onto private execution `SignalId`s. Native source identity
/// never becomes authored object identity and no platform-specific event type enters
/// the semantic scene.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SemanticNativeInputSource {
    State(NativeStateSource),
    Event(NativeEventSource),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSignalState {
    source: SemanticSignalSource,
    value_kind: SemanticSignalValueKind,
    native_input: Option<SemanticNativeInputSource>,
}

impl SemanticSignalState {
    pub(crate) const fn new(
        source: SemanticSignalSource,
        value_kind: SemanticSignalValueKind,
    ) -> Self {
        Self {
            source,
            value_kind,
            native_input: None,
        }
    }

    pub const fn source(&self) -> &SemanticSignalSource {
        &self.source
    }

    pub const fn value_kind(&self) -> SemanticSignalValueKind {
        self.value_kind
    }

    pub const fn native_input(&self) -> Option<&SemanticNativeInputSource> {
        self.native_input.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSignalError {
    UnknownSignal(SemanticNodeId),
    NotSignal(SemanticNodeId),
    NonFiniteValue,
    DependencyCycle(SemanticNodeId),
    InvalidUnaryExpression {
        operation: &'static str,
        operand: SemanticSignalValueKind,
    },
    InvalidBinaryExpression {
        operation: &'static str,
        lhs: SemanticSignalValueKind,
        rhs: SemanticSignalValueKind,
    },
    SourceTypeMismatch {
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    NativeInputRequiresInputSignal {
        signal: SemanticNodeId,
    },
    NativeInputTypeMismatch {
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
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
            Self::InvalidUnaryExpression { operation, operand } => write!(
                formatter,
                "semantic signal operation {operation} does not accept {operand}"
            ),
            Self::InvalidBinaryExpression {
                operation,
                lhs,
                rhs,
            } => write!(
                formatter,
                "semantic signal operation {operation} does not accept {lhs} and {rhs}"
            ),
            Self::SourceTypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic signal {}:{} has stable kind {expected}, but replacement source is {actual}",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInputRequiresInputSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} must remain an input signal while a native input source is attached",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInputTypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "native input source for semantic signal {}:{} requires {expected}, but the signal is {actual}",
                signal.slot(),
                signal.generation()
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
        let value_kind = validate_value(&value)?;
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Input(value),
            value_kind,
        )))
    }

    /// Insert one authored derived signal after validating its referenced closure.
    ///
    /// A new signal cannot reference its not-yet-allocated identity, so creation
    /// cannot introduce a cycle. Walking the existing dependency closure rejects
    /// stale/non-signal references and infers one stable semantic result kind.
    pub fn insert_semantic_derived_signal(
        &mut self,
        expression: SemanticSignalExpr,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let mut cache = HashMap::new();
        let value_kind = infer_expression_kind(self, &expression, None, &mut cache)?;
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Derived(expression),
            value_kind,
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

    /// Return the stable authored value kind of one semantic signal in O(1).
    pub fn semantic_signal_value_kind(
        &self,
        id: SemanticNodeId,
    ) -> Result<SemanticSignalValueKind, SemanticSignalError> {
        Ok(self.semantic_signal_state(id)?.value_kind())
    }

    /// Attach or clear the language-neutral native input source for one semantic signal.
    ///
    /// The declaration lives on the authoritative signal node and therefore follows
    /// its generational identity automatically. Validation and mutation are O(1) and
    /// touch exactly that signal slot; no execution identity or scene scan is involved.
    pub fn set_semantic_native_input(
        &mut self,
        id: SemanticNodeId,
        native_input: Option<SemanticNativeInputSource>,
    ) -> Result<bool, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let state = self.semantic_signal_state(id)?;
        let previous = state.native_input().cloned();

        if let Some(native_input) = native_input.as_ref() {
            if !matches!(state.source(), SemanticSignalSource::Input(_)) {
                return Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: id });
            }
            let expected = native_input_signal_kind(native_input);
            let actual = state.value_kind();
            if expected != actual {
                return Err(SemanticSignalError::NativeInputTypeMismatch {
                    signal: id,
                    expected,
                    actual,
                });
            }
        }

        if previous == native_input {
            return Ok(false);
        }

        self.node_mut(id)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("semantic signal existence validated before native input mutation")
            .native_input = native_input;
        self.set_last_mutation_writes(1);
        Ok(true)
    }

    pub fn bind_semantic_native_state_input(
        &mut self,
        id: SemanticNodeId,
        source: NativeStateSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, Some(SemanticNativeInputSource::State(source)))
    }

    pub fn bind_semantic_native_event_input(
        &mut self,
        id: SemanticNodeId,
        source: NativeEventSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, Some(SemanticNativeInputSource::Event(source)))
    }

    pub fn clear_semantic_native_input(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, None)
    }

    /// Replace one signal's authored source while preserving semantic identity and kind.
    ///
    /// Validation completes before the target node is written. Work is proportional
    /// to the new source's dependency closure; unrelated scene nodes are not scanned.
    /// A successful replacement writes exactly the target signal slot, while an
    /// invalid, kind-changing, or identical replacement writes no semantic slots.
    pub fn set_semantic_signal_source(
        &mut self,
        id: SemanticNodeId,
        source: SemanticSignalSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let state = self.semantic_signal_state(id)?;
        let previous = state.source().clone();
        let expected = state.value_kind();
        if state.native_input().is_some() && !matches!(&source, SemanticSignalSource::Input(_)) {
            return Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: id });
        }

        let mut cache = HashMap::new();
        let actual = infer_source_kind(self, &source, Some(id), &mut cache)?;
        if actual != expected {
            return Err(SemanticSignalError::SourceTypeMismatch {
                signal: id,
                expected,
                actual,
            });
        }
        if previous == source {
            return Ok(false);
        }

        self.unregister_semantic_references_for_owner(id);
        self.node_mut(id)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("semantic signal existence validated before mutation")
            .source = source;
        self.register_semantic_references_for_owner(id);
        self.set_last_mutation_writes(1);
        Ok(true)
    }
}

fn native_input_signal_kind(source: &SemanticNativeInputSource) -> SemanticSignalValueKind {
    match source {
        SemanticNativeInputSource::State(
            NativeStateSource::PointerPosition
            | NativeStateSource::ViewportSize
            | NativeStateSource::WheelDelta
            | NativeStateSource::GestureDelta { .. },
        ) => SemanticSignalValueKind::Vec3,
        SemanticNativeInputSource::State(
            NativeStateSource::PointerButton { .. } | NativeStateSource::Key { .. },
        ) => SemanticSignalValueKind::Bool,
        SemanticNativeInputSource::State(NativeStateSource::Control { .. })
        | SemanticNativeInputSource::Event(_) => SemanticSignalValueKind::Scalar,
    }
}

fn validate_value(
    value: &SemanticSignalValue,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    if value.is_finite() {
        Ok(value.value_kind())
    } else {
        Err(SemanticSignalError::NonFiniteValue)
    }
}

fn infer_source_kind(
    store: &SemanticStore,
    source: &SemanticSignalSource,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    match source {
        SemanticSignalSource::Input(value) => validate_value(value),
        SemanticSignalSource::Derived(expression) => {
            infer_expression_kind(store, expression, cycle_target, cache)
        }
    }
}

fn infer_expression_kind(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    match expression {
        SemanticSignalExpr::Constant(value) => validate_value(value),
        SemanticSignalExpr::Signal(id) => {
            infer_signal_dependency_kind(store, *id, cycle_target, cache)
        }
        SemanticSignalExpr::Add(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Vec3) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "add",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Sub(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Vec3) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "sub",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Mul(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Vec3)
                | (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "mul",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Neg(value) => {
            let operand = infer_expression_kind(store, value, cycle_target, cache)?;
            match operand {
                SemanticSignalValueKind::Scalar | SemanticSignalValueKind::Vec3 => Ok(operand),
                SemanticSignalValueKind::Bool => Err(SemanticSignalError::InvalidUnaryExpression {
                    operation: "neg",
                    operand,
                }),
            }
        }
        SemanticSignalExpr::Sin(value) => {
            infer_scalar_unary_kind(store, value, cycle_target, cache, "sin")
        }
        SemanticSignalExpr::Cos(value) => {
            infer_scalar_unary_kind(store, value, cycle_target, cache, "cos")
        }
    }
}

fn infer_scalar_unary_kind(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
    operation: &'static str,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    let operand = infer_expression_kind(store, expression, cycle_target, cache)?;
    if operand == SemanticSignalValueKind::Scalar {
        Ok(SemanticSignalValueKind::Scalar)
    } else {
        Err(SemanticSignalError::InvalidUnaryExpression { operation, operand })
    }
}

fn infer_signal_dependency_kind(
    store: &SemanticStore,
    id: SemanticNodeId,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    if cycle_target == Some(id) {
        return Err(SemanticSignalError::DependencyCycle(id));
    }
    if let Some(value_kind) = cache.get(&id).copied() {
        return Ok(value_kind);
    }
    let state = store.semantic_signal_state(id)?;
    let value_kind = infer_source_kind(store, state.source(), cycle_target, cache)?;
    debug_assert_eq!(value_kind, state.value_kind());
    cache.insert(id, value_kind);
    Ok(value_kind)
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
        assert_eq!(
            store.semantic_signal_value_kind(signal).unwrap(),
            SemanticSignalValueKind::Scalar
        );
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
        assert_eq!(
            store.semantic_signal_value_kind(derived).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn semantic_native_input_declaration_is_signal_owned_typed_and_local() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let key = store.insert_semantic_input_signal(false).unwrap();
        let viewport = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let event = store.insert_semantic_input_signal(0.0_f64).unwrap();

        let key_source = NativeStateSource::Key {
            code: "Space".to_owned(),
        };
        assert!(store
            .bind_semantic_native_state_input(key, key_source.clone())
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_state(key).unwrap().native_input(),
            Some(&SemanticNativeInputSource::State(key_source))
        );

        assert!(store
            .bind_semantic_native_state_input(viewport, NativeStateSource::ViewportSize)
            .unwrap());
        assert!(store
            .bind_semantic_native_event_input(
                event,
                NativeEventSource::KeyPress {
                    code: "Space".to_owned(),
                },
            )
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        assert!(store.clear_semantic_native_input(key).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_state(key).unwrap().native_input(),
            None
        );
        assert!(!store.clear_semantic_native_input(key).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn semantic_native_input_rejects_derived_and_mismatched_signals_atomically() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let boolean = store.insert_semantic_input_signal(false).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
            .unwrap();

        assert_eq!(
            store.bind_semantic_native_state_input(
                scalar,
                NativeStateSource::Key {
                    code: "KeyA".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputTypeMismatch {
                signal: scalar,
                expected: SemanticSignalValueKind::Bool,
                actual: SemanticSignalValueKind::Scalar,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.bind_semantic_native_event_input(
                boolean,
                NativeEventSource::KeyPress {
                    code: "KeyA".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputTypeMismatch {
                signal: boolean,
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Bool,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.bind_semantic_native_state_input(
                derived,
                NativeStateSource::Control {
                    name: "zoom".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: derived })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn attached_native_input_prevents_silent_conversion_to_a_derived_signal() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_state_input(
                target,
                NativeStateSource::Control {
                    name: "zoom".to_owned(),
                },
            )
            .unwrap();
        let before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(dependency)),
            ),
            Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: target })
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        store.clear_semantic_native_input(target).unwrap();
        assert!(store
            .set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(dependency)),
            )
            .unwrap());
    }

    #[test]
    fn semantic_expression_kinds_match_the_native_reactive_operator_contract() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let boolean = store.insert_semantic_input_signal(true).unwrap();

        assert_eq!(
            store.semantic_signal_value_kind(boolean).unwrap(),
            SemanticSignalValueKind::Bool
        );

        let vector_sum = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(vector)),
                Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                    SemanticVec3::new(4.0, 5.0, 6.0),
                ))),
            ))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(vector_sum).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let scaled = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(scalar)),
                Box::new(SemanticSignalExpr::signal(vector)),
            ))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(scaled).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let negated = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Neg(Box::new(
                SemanticSignalExpr::signal(vector),
            )))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(negated).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let sine = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Sin(Box::new(
                SemanticSignalExpr::signal(scalar),
            )))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(sine).unwrap(),
            SemanticSignalValueKind::Scalar
        );
    }

    #[test]
    fn invalid_semantic_expression_kinds_are_rejected_before_insertion() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let boolean = store.insert_semantic_input_signal(true).unwrap();
        let before = store.len();

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(boolean)),
                Box::new(SemanticSignalExpr::signal(boolean)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "add",
                lhs: SemanticSignalValueKind::Bool,
                rhs: SemanticSignalValueKind::Bool,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(scalar)),
                Box::new(SemanticSignalExpr::signal(vector)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "add",
                lhs: SemanticSignalValueKind::Scalar,
                rhs: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(vector)),
                Box::new(SemanticSignalExpr::signal(vector)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "mul",
                lhs: SemanticSignalValueKind::Vec3,
                rhs: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Sin(Box::new(
                SemanticSignalExpr::signal(vector),
            ))),
            Err(SemanticSignalError::InvalidUnaryExpression {
                operation: "sin",
                operand: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
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
            .insert_semantic_derived_signal(SemanticSignalExpr::Neg(Box::new(
                SemanticSignalExpr::signal(input),
            )))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_value_kind(derived).unwrap(),
            SemanticSignalValueKind::Vec3
        );
    }

    #[test]
    fn signal_source_replacement_preserves_identity_kind_and_one_slot_locality() {
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
        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
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
    fn signal_source_replacement_cannot_change_the_signal_value_kind() {
        let mut store = SemanticStore::new();
        let target = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(vector)),
            ),
            Err(SemanticSignalError::SourceTypeMismatch {
                signal: target,
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &before
        );
        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn stable_signal_kind_allows_recovery_from_a_stale_dependency() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(dependency))
            .unwrap();
        store.remove_node(dependency).unwrap();

        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert!(store
            .set_semantic_signal_source(
                target,
                SemanticSignalSource::Input(SemanticSignalValue::Scalar(7.0)),
            )
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(matches!(
            store.semantic_signal_state(target).unwrap().source(),
            SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) if *value == 7.0
        ));
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
        let mut transaction = super::super::SemanticMutationTransaction::new();
        transaction.add_updater(signal, super::super::HostCallbackId::new(1), 0.0, None);
        assert!(matches!(
            transaction.apply(&mut store),
            Err(super::super::SemanticMutationTransactionError::Family {
                error: super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id),
                ..
            }) if id == signal
        ));
    }
}
