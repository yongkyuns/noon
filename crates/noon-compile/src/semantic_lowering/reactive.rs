use std::collections::{HashMap, HashSet};

use noon_core::{
    Property, ReactiveError, ReactiveExpr, ReactiveGraphDefinition, ReactiveValue, SemanticNodeId,
    SemanticObjectProperty, SemanticSignalError, SemanticSignalExpr, SemanticSignalSource,
    SemanticSignalValue, SemanticStore, SignalDefinition, SignalId, SignalSource,
};

use super::SemanticExecutionProjection;

/// Execution-facing native-reactive projection derived from authoritative semantic
/// signal identity and object bindings.
///
/// This is not a second authored graph. `SignalId` values are deterministic
/// compatibility encodings used only by the existing native reactive compiler/VM;
/// semantic `SemanticNodeId` remains authoritative.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticReactiveProjection {
    graph: ReactiveGraphDefinition,
    signal_ids: HashMap<SemanticNodeId, SignalId>,
}

impl SemanticReactiveProjection {
    pub fn graph(&self) -> &ReactiveGraphDefinition {
        &self.graph
    }

    pub fn execution_signal_id(&self, semantic_id: SemanticNodeId) -> Option<SignalId> {
        self.signal_ids.get(&semantic_id).copied()
    }

    pub fn signal_count(&self) -> usize {
        self.signal_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signal_ids.is_empty() && self.graph.bindings().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticReactiveLoweringError {
    Signal(SemanticSignalError),
    Reactive(ReactiveError),
    NonFiniteSignalValue {
        signal: SemanticNodeId,
    },
    SignalValueOutOfRange {
        signal: SemanticNodeId,
    },
    UnsupportedProperty {
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DependencyCycle(SemanticNodeId),
}

impl From<SemanticSignalError> for SemanticReactiveLoweringError {
    fn from(value: SemanticSignalError) -> Self {
        Self::Signal(value)
    }
}

impl From<ReactiveError> for SemanticReactiveLoweringError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl std::fmt::Display for SemanticReactiveLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::Reactive(error) => error.fmt(formatter),
            Self::NonFiniteSignalValue { signal } => write!(
                formatter,
                "semantic signal {}:{} contains a non-finite execution value",
                signal.slot(),
                signal.generation()
            ),
            Self::SignalValueOutOfRange { signal } => write!(
                formatter,
                "semantic signal {}:{} cannot lower to the current f32 execution domain",
                signal.slot(),
                signal.generation()
            ),
            Self::UnsupportedProperty { target, property } => write!(
                formatter,
                "semantic property {property:?} on object {}:{} has no native reactive execution property yet",
                target.slot(),
                target.generation()
            ),
            Self::DependencyCycle(signal) => write!(
                formatter,
                "semantic signal {}:{} contains a dependency cycle during execution lowering",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticReactiveLoweringError {}

/// Lower only the native-reactive dependency closure reachable from visible object
/// bindings in one semantic execution projection.
///
/// Unbound semantic signals remain authored scene state but do not consume execution
/// VM slots. Dependencies are generation-checked through `SemanticStore`, and the
/// resulting `ReactiveGraphDefinition` is the existing native execution graph rather
/// than a new reactive runtime model.
pub fn lower_semantic_reactive_projection(
    store: &SemanticStore,
    projection: &SemanticExecutionProjection,
) -> Result<SemanticReactiveProjection, SemanticReactiveLoweringError> {
    let mut lowerer = ReactiveLowerer {
        store,
        definitions: Vec::new(),
        bindings: Vec::new(),
        signal_ids: HashMap::new(),
        visiting: HashSet::new(),
    };

    for object in projection.objects() {
        for binding in &object.signal_bindings {
            let property = lower_property(object.semantic_id, binding.property())?;
            let signal = lowerer.lower_signal(binding.signal())?;
            lowerer.bindings.push(noon_core::ReactiveBinding {
                signal,
                object: object.execution_id,
                property,
            });
        }
    }

    let graph = ReactiveGraphDefinition::from_parts(lowerer.definitions, lowerer.bindings)?;
    Ok(SemanticReactiveProjection {
        graph,
        signal_ids: lowerer.signal_ids,
    })
}

struct ReactiveLowerer<'a> {
    store: &'a SemanticStore,
    definitions: Vec<SignalDefinition>,
    bindings: Vec<noon_core::ReactiveBinding>,
    signal_ids: HashMap<SemanticNodeId, SignalId>,
    visiting: HashSet<SemanticNodeId>,
}

impl ReactiveLowerer<'_> {
    fn lower_signal(
        &mut self,
        semantic_id: SemanticNodeId,
    ) -> Result<SignalId, SemanticReactiveLoweringError> {
        if let Some(signal) = self.signal_ids.get(&semantic_id).copied() {
            return Ok(signal);
        }
        if !self.visiting.insert(semantic_id) {
            return Err(SemanticReactiveLoweringError::DependencyCycle(semantic_id));
        }

        let source = self.store.semantic_signal_state(semantic_id)?.source().clone();
        let signal = compatibility_signal_id(semantic_id);
        let source = self.lower_source(semantic_id, &source)?;
        self.visiting.remove(&semantic_id);
        self.signal_ids.insert(semantic_id, signal);
        self.definitions
            .push(SignalDefinition { id: signal, source });
        Ok(signal)
    }

    fn lower_source(
        &mut self,
        owner: SemanticNodeId,
        source: &SemanticSignalSource,
    ) -> Result<SignalSource, SemanticReactiveLoweringError> {
        match source {
            SemanticSignalSource::Input(value) => {
                Ok(SignalSource::Input(lower_value(owner, value)?))
            }
            SemanticSignalSource::Derived(expression) => Ok(SignalSource::Derived(
                self.lower_expression(owner, expression)?,
            )),
        }
    }

    fn lower_expression(
        &mut self,
        owner: SemanticNodeId,
        expression: &SemanticSignalExpr,
    ) -> Result<ReactiveExpr, SemanticReactiveLoweringError> {
        match expression {
            SemanticSignalExpr::Constant(value) => {
                Ok(ReactiveExpr::Constant(lower_value(owner, value)?))
            }
            SemanticSignalExpr::Signal(signal) => {
                Ok(ReactiveExpr::Signal(self.lower_signal(*signal)?))
            }
            SemanticSignalExpr::Add(lhs, rhs) => Ok(ReactiveExpr::Add(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Sub(lhs, rhs) => Ok(ReactiveExpr::Sub(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Mul(lhs, rhs) => Ok(ReactiveExpr::Mul(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Neg(value) => Ok(ReactiveExpr::Neg(Box::new(
                self.lower_expression(owner, value)?,
            ))),
            SemanticSignalExpr::Sin(value) => Ok(ReactiveExpr::Sin(Box::new(
                self.lower_expression(owner, value)?,
            ))),
            SemanticSignalExpr::Cos(value) => Ok(ReactiveExpr::Cos(Box::new(
                self.lower_expression(owner, value)?,
            ))),
        }
    }
}

fn lower_value(
    signal: SemanticNodeId,
    value: &SemanticSignalValue,
) -> Result<ReactiveValue, SemanticReactiveLoweringError> {
    match value {
        SemanticSignalValue::Bool(value) => Ok(ReactiveValue::Bool(*value)),
        SemanticSignalValue::Scalar(value) => {
            Ok(ReactiveValue::Scalar(lower_scalar(signal, *value)?))
        }
        SemanticSignalValue::Vec3(value) => {
            value
                .lower_xy_f32()
                .map(ReactiveValue::Vec2)
                .map_err(|error| match error {
                    noon_core::SemanticLoweringError::NonFiniteVector(_) => {
                        SemanticReactiveLoweringError::NonFiniteSignalValue { signal }
                    }
                    noon_core::SemanticLoweringError::CoordinateOutOfRange(_) => {
                        SemanticReactiveLoweringError::SignalValueOutOfRange { signal }
                    }
                })
        }
    }
}

fn lower_scalar(signal: SemanticNodeId, value: f64) -> Result<f32, SemanticReactiveLoweringError> {
    if !value.is_finite() {
        return Err(SemanticReactiveLoweringError::NonFiniteSignalValue { signal });
    }
    if value.abs() > f32::MAX as f64 {
        return Err(SemanticReactiveLoweringError::SignalValueOutOfRange { signal });
    }
    Ok(value as f32)
}

fn lower_property(
    target: SemanticNodeId,
    property: SemanticObjectProperty,
) -> Result<Property, SemanticReactiveLoweringError> {
    match property {
        SemanticObjectProperty::Translation => Ok(Property::Position),
        SemanticObjectProperty::Scale => Ok(Property::Scale),
        SemanticObjectProperty::RotationZ => Ok(Property::Rotation),
        SemanticObjectProperty::ObjectOpacity => Ok(Property::Opacity),
        SemanticObjectProperty::FillOpacity
        | SemanticObjectProperty::StrokeOpacity
        | SemanticObjectProperty::StrokeWidth => {
            Err(SemanticReactiveLoweringError::UnsupportedProperty { target, property })
        }
    }
}

/// One-to-one compatibility encoding for semantic signal identity at the execution
/// boundary. It introduces no allocator and cannot alias another live generation.
fn compatibility_signal_id(id: SemanticNodeId) -> SignalId {
    let raw = (u64::from(id.generation()) << 32) | u64::from(id.slot());
    SignalId::new(raw)
}

#[cfg(test)]
mod tests {
    use noon_core::{
        ReactiveBinding, SemanticObjectState, SemanticSignalExpr, SemanticVec3, StoredGeometry,
        Vec2,
    };

    use super::*;
    use crate::SemanticExecutionIndex;

    fn visible_circle(store: &mut SemanticStore) -> SemanticNodeId {
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        object
    }

    fn projection(
        store: &SemanticStore,
        index: &mut SemanticExecutionIndex,
    ) -> SemanticExecutionProjection {
        index.lower_scene(store).unwrap()
    }

    #[test]
    fn lowers_only_bound_signal_dependency_closure_into_existing_native_graph() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(input)),
                Box::new(SemanticSignalExpr::scalar(3.0)),
            ))
            .unwrap();
        let unrelated = store.insert_semantic_input_signal(99.0_f64).unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(derived, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();

        assert_eq!(reactive.signal_count(), 2);
        assert!(reactive.execution_signal_id(input).is_some());
        assert!(reactive.execution_signal_id(derived).is_some());
        assert_eq!(reactive.execution_signal_id(unrelated), None);
        assert_eq!(reactive.graph().signals().len(), 2);
        assert_eq!(
            reactive.graph().bindings(),
            &[ReactiveBinding {
                signal: reactive.execution_signal_id(derived).unwrap(),
                object: index.execution_object_id(object).unwrap(),
                property: Property::Opacity,
            }]
        );

        let input_id = reactive.execution_signal_id(input).unwrap();
        let derived_id = reactive.execution_signal_id(derived).unwrap();
        assert!(reactive.graph().signals().iter().any(|signal| {
            signal.id == input_id
                && signal.source == SignalSource::Input(ReactiveValue::Scalar(2.0))
        }));
        assert!(reactive.graph().signals().iter().any(|signal| {
            signal.id == derived_id
                && signal.source
                    == SignalSource::Derived(ReactiveExpr::Add(
                        Box::new(ReactiveExpr::Signal(input_id)),
                        Box::new(ReactiveExpr::Constant(ReactiveValue::Scalar(3.0))),
                    ))
        }));
    }

    #[test]
    fn vector_signals_lower_explicitly_to_current_xy_execution_domain() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::new(4.0, -2.0, 17.0))
            .unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::Translation)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();
        let signal_id = reactive.execution_signal_id(signal).unwrap();

        assert!(reactive.graph().signals().iter().any(|definition| {
            definition.id == signal_id
                && definition.source
                    == SignalSource::Input(ReactiveValue::Vec2(Vec2::new(4.0, -2.0)))
        }));
        assert_eq!(reactive.graph().bindings()[0].property, Property::Position);
    }

    #[test]
    fn unsupported_style_channels_fail_instead_of_collapsing_semantics() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::FillOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        assert_eq!(
            lower_semantic_reactive_projection(&store, &execution).unwrap_err(),
            SemanticReactiveLoweringError::UnsupportedProperty {
                target: object,
                property: SemanticObjectProperty::FillOpacity,
            }
        );
    }

    #[test]
    fn stale_dependency_generation_fails_closed_during_lowering() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(dependency))
            .unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(derived, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        store.remove_node(dependency).unwrap();
        let replacement = store.insert_semantic_input_signal(2.0_f64).unwrap();
        assert_eq!(dependency.slot(), replacement.slot());
        assert_ne!(dependency.generation(), replacement.generation());

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        assert!(matches!(
            lower_semantic_reactive_projection(&store, &execution),
            Err(SemanticReactiveLoweringError::Signal(
                SemanticSignalError::UnknownSignal(id)
            )) if id == dependency
        ));
    }
}
