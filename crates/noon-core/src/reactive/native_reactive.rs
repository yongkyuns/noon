use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{GeometryRef, ObjectId, Property, SceneDefinition, SignalId, ValueKind, Vec2};

mod compute_ir;
pub use compute_ir::*;

/// Minimal execution-target facts needed to validate and classify a native reactive graph.
///
/// This is a compile-time query boundary, not another scene representation. Implementors
/// expose existing object/timeline facts in their own storage order; the reactive compiler
/// neither owns nor mutates that storage.
pub trait ReactiveCompileTarget {
    /// Whether `object` names a live execution object.
    fn contains_object(&self, object: ObjectId) -> bool;

    /// Whether the target already has a timeline driver for this exact property.
    fn has_timeline_driver(&self, object: ObjectId, property: Property) -> bool;

    /// Whether the target has any timeline activity for this object.
    fn has_timeline_activity(&self, object: ObjectId) -> bool;

    /// Visit live execution objects in deterministic target order.
    fn visit_objects(&self, visitor: &mut dyn FnMut(ObjectId));
}

impl ReactiveCompileTarget for SceneDefinition {
    fn contains_object(&self, object: ObjectId) -> bool {
        self.object(object).is_some()
    }

    fn has_timeline_driver(&self, object: ObjectId, property: Property) -> bool {
        self.tracks()
            .iter()
            .any(|track| track.object == object && track.property == property)
    }

    fn has_timeline_activity(&self, object: ObjectId) -> bool {
        self.tracks().iter().any(|track| track.object == object)
    }

    fn visit_objects(&self, visitor: &mut dyn FnMut(ObjectId)) {
        for object in self.objects() {
            visitor(object.id);
        }
    }
}

/// A value that can participate in the native reactive graph.
///
/// Object snapshots are intentionally excluded from this first reactive slice:
/// geometry-changing Transform semantics require a separate incremental geometry
/// plan, while bool/scalar/vector properties can remain cheap and data-oriented.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveValue {
    Bool(bool),
    Scalar(f32),
    Vec2(Vec2),
}

impl ReactiveValue {
    pub const fn value_kind(&self) -> ValueKind {
        match self {
            Self::Bool(_) => ValueKind::Bool,
            Self::Scalar(_) => ValueKind::Scalar,
            Self::Vec2(_) => ValueKind::Vec2,
        }
    }

    pub fn is_finite(&self) -> bool {
        match self {
            Self::Bool(_) => true,
            Self::Scalar(value) => value.is_finite(),
            Self::Vec2(value) => value.x.is_finite() && value.y.is_finite(),
        }
    }
}

impl From<bool> for ReactiveValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f32> for ReactiveValue {
    fn from(value: f32) -> Self {
        Self::Scalar(value)
    }
}

impl From<Vec2> for ReactiveValue {
    fn from(value: Vec2) -> Self {
        Self::Vec2(value)
    }
}

/// Small deterministic expression language for native reactive dependencies.
///
/// This deliberately starts with operations useful for trackers and geometry
/// relationships. It is serializable and language-neutral, so the same graph can
/// later be evaluated by Rust/WASM, SIMD, or GPU lowering without host callbacks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveExpr {
    Constant(ReactiveValue),
    Signal(SignalId),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    Neg(Box<Self>),
    Sin(Box<Self>),
    Cos(Box<Self>),
}

impl ReactiveExpr {
    pub const fn signal(signal: SignalId) -> Self {
        Self::Signal(signal)
    }

    pub fn scalar(value: f32) -> Self {
        Self::Constant(ReactiveValue::Scalar(value))
    }

    pub fn vec2(value: Vec2) -> Self {
        Self::Constant(ReactiveValue::Vec2(value))
    }

    fn dependencies(&self, output: &mut BTreeSet<SignalId>) {
        match self {
            Self::Constant(_) => {}
            Self::Signal(signal) => {
                output.insert(*signal);
            }
            Self::Add(lhs, rhs) | Self::Sub(lhs, rhs) | Self::Mul(lhs, rhs) => {
                lhs.dependencies(output);
                rhs.dependencies(output);
            }
            Self::Neg(value) | Self::Sin(value) | Self::Cos(value) => value.dependencies(output),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    Input(ReactiveValue),
    Derived(ReactiveExpr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalDefinition {
    pub id: SignalId,
    pub source: SignalSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactiveBinding {
    pub signal: SignalId,
    pub object: ObjectId,
    pub property: Property,
}

/// Declarative native-reactive portion of a semantic scene.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReactiveGraphDefinition {
    signals: Vec<SignalDefinition>,
    next_signal_id: u64,
    bindings: Vec<ReactiveBinding>,
}

impl ReactiveGraphDefinition {
    pub const fn new() -> Self {
        Self {
            signals: Vec::new(),
            next_signal_id: 0,
            bindings: Vec::new(),
        }
    }

    pub fn from_parts(
        signals: Vec<SignalDefinition>,
        bindings: Vec<ReactiveBinding>,
    ) -> Result<Self, ReactiveError> {
        let mut ids = BTreeSet::new();
        let mut next_signal_id = 0;
        for signal in &signals {
            if !ids.insert(signal.id) {
                return Err(ReactiveError::DuplicateSignal(signal.id));
            }
            next_signal_id = next_signal_id.max(
                signal
                    .id
                    .get()
                    .checked_add(1)
                    .ok_or(ReactiveError::SignalIdExhausted)?,
            );
        }
        Ok(Self {
            signals,
            next_signal_id,
            bindings,
        })
    }

    pub fn add_input(&mut self, value: impl Into<ReactiveValue>) -> SignalId {
        let id = SignalId::new(self.next_signal_id);
        self.next_signal_id = self
            .next_signal_id
            .checked_add(1)
            .expect("Noon signal ID space exhausted");
        self.signals.push(SignalDefinition {
            id,
            source: SignalSource::Input(value.into()),
        });
        id
    }

    pub fn add_derived(&mut self, expression: ReactiveExpr) -> SignalId {
        let id = SignalId::new(self.next_signal_id);
        self.next_signal_id = self
            .next_signal_id
            .checked_add(1)
            .expect("Noon signal ID space exhausted");
        self.signals.push(SignalDefinition {
            id,
            source: SignalSource::Derived(expression),
        });
        id
    }

    pub fn bind(&mut self, signal: SignalId, object: ObjectId, property: Property) {
        self.bindings.push(ReactiveBinding {
            signal,
            object,
            property,
        });
    }

    pub fn signals(&self) -> &[SignalDefinition] {
        &self.signals
    }

    pub fn bindings(&self) -> &[ReactiveBinding] {
        &self.bindings
    }
}

/// High-level mutable semantic scene.
///
/// `SceneDefinition` remains the normalized deterministic timeline/object program
/// consumed by the existing compiler. `SemanticScene` adds native reactive
/// dependencies without encoding them as fake timeline tracks. Future host callback
/// slots and high-level authoring state can live beside the same definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticScene {
    definition: SceneDefinition,
    reactive: ReactiveGraphDefinition,
}

impl SemanticScene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_definition(definition: SceneDefinition) -> Self {
        Self {
            definition,
            reactive: ReactiveGraphDefinition::new(),
        }
    }

    pub fn definition(&self) -> &SceneDefinition {
        &self.definition
    }

    pub fn definition_mut(&mut self) -> &mut SceneDefinition {
        &mut self.definition
    }

    pub fn reactive(&self) -> &ReactiveGraphDefinition {
        &self.reactive
    }

    pub fn reactive_mut(&mut self) -> &mut ReactiveGraphDefinition {
        &mut self.reactive
    }

    pub fn add(&mut self, geometry: GeometryRef) -> ObjectId {
        self.definition.add(geometry)
    }

    pub fn add_input(&mut self, value: impl Into<ReactiveValue>) -> SignalId {
        self.reactive.add_input(value)
    }

    pub fn add_derived(&mut self, expression: ReactiveExpr) -> SignalId {
        self.reactive.add_derived(expression)
    }

    pub fn bind(&mut self, signal: SignalId, object: ObjectId, property: Property) {
        self.reactive.bind(signal, object, property);
    }

    pub fn compile_reactive(&self) -> Result<ReactiveProgram, ReactiveError> {
        ReactiveProgram::compile(&self.definition, &self.reactive)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectExecutionClass {
    Static,
    Timeline,
    Reactive,
    TimelineAndReactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectExecution {
    pub object: ObjectId,
    pub class: ObjectExecutionClass,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionAnalysis {
    objects: Vec<ObjectExecution>,
    pub static_objects: usize,
    pub timeline_only_objects: usize,
    pub reactive_only_objects: usize,
    pub timeline_and_reactive_objects: usize,
}

impl ExecutionAnalysis {
    pub fn objects(&self) -> &[ObjectExecution] {
        &self.objects
    }

    pub fn class_for(&self, object: ObjectId) -> Option<ObjectExecutionClass> {
        self.objects
            .iter()
            .find(|entry| entry.object == object)
            .map(|entry| entry.class)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CompiledSignal {
    id: SignalId,
    source: SignalSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompiledBinding {
    object: ObjectId,
    property: Property,
}

/// Validated dependency graph ready for incremental native evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactiveProgram {
    signals: Vec<CompiledSignal>,
    signal_indices: BTreeMap<SignalId, usize>,
    topological_order: Vec<usize>,
    topological_rank: Vec<usize>,
    dependents: Vec<Vec<usize>>,
    bindings_by_signal: Vec<Vec<CompiledBinding>>,
    initial_values: Vec<ReactiveValue>,
    analysis: ExecutionAnalysis,
}

impl ReactiveProgram {
    /// Compatibility entry for the migration-era normalized scene definition.
    /// New lowering code should validate against its actual execution target through
    /// [`ReactiveCompileTarget`] instead of reconstructing authored scene state.
    pub fn compile(
        scene: &SceneDefinition,
        graph: &ReactiveGraphDefinition,
    ) -> Result<Self, ReactiveError> {
        Self::compile_for_target(scene, graph)
    }

    /// Compile one native reactive graph against the actual execution target facts.
    pub fn compile_for_target<T: ReactiveCompileTarget + ?Sized>(
        target: &T,
        graph: &ReactiveGraphDefinition,
    ) -> Result<Self, ReactiveError> {
        let mut signal_indices = BTreeMap::new();
        for (index, signal) in graph.signals.iter().enumerate() {
            if signal_indices.insert(signal.id, index).is_some() {
                return Err(ReactiveError::DuplicateSignal(signal.id));
            }
        }

        let signal_count = graph.signals.len();
        let mut indegrees = vec![0usize; signal_count];
        let mut dependents = vec![Vec::new(); signal_count];
        for (index, signal) in graph.signals.iter().enumerate() {
            if let SignalSource::Derived(expression) = &signal.source {
                let mut dependencies = BTreeSet::new();
                expression.dependencies(&mut dependencies);
                indegrees[index] = dependencies.len();
                for dependency in dependencies {
                    let dependency_index = *signal_indices
                        .get(&dependency)
                        .ok_or(ReactiveError::UnknownSignal(dependency))?;
                    dependents[dependency_index].push(index);
                }
            }
        }

        let mut ready = VecDeque::new();
        for (index, indegree) in indegrees.iter().enumerate() {
            if *indegree == 0 {
                ready.push_back(index);
            }
        }
        let mut topological_order = Vec::with_capacity(signal_count);
        while let Some(index) = ready.pop_front() {
            topological_order.push(index);
            for dependent in &dependents[index] {
                indegrees[*dependent] -= 1;
                if indegrees[*dependent] == 0 {
                    ready.push_back(*dependent);
                }
            }
        }
        if topological_order.len() != signal_count {
            return Err(ReactiveError::DependencyCycle);
        }

        let mut topological_rank = vec![0usize; signal_count];
        for (rank, index) in topological_order.iter().copied().enumerate() {
            topological_rank[index] = rank;
        }

        let compiled_signals = graph
            .signals
            .iter()
            .map(|signal| CompiledSignal {
                id: signal.id,
                source: signal.source.clone(),
            })
            .collect::<Vec<_>>();
        let mut initial_values = vec![ReactiveValue::Scalar(0.0); signal_count];
        for index in topological_order.iter().copied() {
            let signal = &compiled_signals[index];
            let value = match &signal.source {
                SignalSource::Input(value) => value.clone(),
                SignalSource::Derived(expression) => {
                    evaluate_expression(expression, &initial_values, &signal_indices, signal.id)?
                }
            };
            validate_reactive_value(signal.id, &value)?;
            initial_values[index] = value;
        }

        let mut bindings_by_signal = vec![Vec::new(); signal_count];
        let mut seen_targets = Vec::<(ObjectId, Property)>::new();
        for binding in &graph.bindings {
            let signal_index = *signal_indices
                .get(&binding.signal)
                .ok_or(ReactiveError::UnknownSignal(binding.signal))?;
            if !target.contains_object(binding.object) {
                return Err(ReactiveError::UnknownObject(binding.object));
            }
            if seen_targets.contains(&(binding.object, binding.property)) {
                return Err(ReactiveError::DuplicateBinding {
                    object: binding.object,
                    property: binding.property,
                });
            }
            if target.has_timeline_driver(binding.object, binding.property) {
                return Err(ReactiveError::ConflictingDriver {
                    object: binding.object,
                    property: binding.property,
                });
            }
            let actual = initial_values[signal_index].value_kind();
            let expected = binding.property.value_kind();
            if expected != actual {
                return Err(ReactiveError::BindingTypeMismatch {
                    signal: binding.signal,
                    property: binding.property,
                    expected,
                    actual,
                });
            }
            seen_targets.push((binding.object, binding.property));
            bindings_by_signal[signal_index].push(CompiledBinding {
                object: binding.object,
                property: binding.property,
            });
        }

        let analysis = build_execution_analysis(target, graph);
        Ok(Self {
            signals: compiled_signals,
            signal_indices,
            topological_order,
            topological_rank,
            dependents,
            bindings_by_signal,
            initial_values,
            analysis,
        })
    }

    pub fn instantiate(&self) -> ReactiveState {
        ReactiveState {
            program: self.clone(),
            values: self.initial_values.clone(),
        }
    }

    pub fn analysis(&self) -> &ExecutionAnalysis {
        &self.analysis
    }

    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    pub fn topological_order(&self) -> impl Iterator<Item = SignalId> + '_ {
        self.topological_order
            .iter()
            .map(|index| self.signals[*index].id)
    }
}

fn build_execution_analysis<T: ReactiveCompileTarget + ?Sized>(
    target: &T,
    graph: &ReactiveGraphDefinition,
) -> ExecutionAnalysis {
    let mut analysis = ExecutionAnalysis::default();
    target.visit_objects(&mut |object| {
        let timeline = target.has_timeline_activity(object);
        let reactive = graph
            .bindings
            .iter()
            .any(|binding| binding.object == object);
        let class = match (timeline, reactive) {
            (false, false) => {
                analysis.static_objects += 1;
                ObjectExecutionClass::Static
            }
            (true, false) => {
                analysis.timeline_only_objects += 1;
                ObjectExecutionClass::Timeline
            }
            (false, true) => {
                analysis.reactive_only_objects += 1;
                ObjectExecutionClass::Reactive
            }
            (true, true) => {
                analysis.timeline_and_reactive_objects += 1;
                ObjectExecutionClass::TimelineAndReactive
            }
        };
        analysis.objects.push(ObjectExecution { object, class });
    });
    analysis
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReactiveEvaluationStats {
    pub derived_signals_evaluated: usize,
    pub bindings_invalidated: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignalChange {
    pub signal: SignalId,
    pub value: ReactiveValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReactivePropertyChange {
    pub object: ObjectId,
    pub property: Property,
    pub value: ReactiveValue,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactiveUpdate {
    signal_changes: Vec<SignalChange>,
    property_changes: Vec<ReactivePropertyChange>,
    stats: ReactiveEvaluationStats,
}

impl ReactiveUpdate {
    pub fn signal_changes(&self) -> &[SignalChange] {
        &self.signal_changes
    }

    pub fn property_changes(&self) -> &[ReactivePropertyChange] {
        &self.property_changes
    }

    pub const fn stats(&self) -> ReactiveEvaluationStats {
        self.stats
    }

    pub fn affected_objects(&self) -> Vec<ObjectId> {
        let mut result = self
            .property_changes
            .iter()
            .map(|change| change.object)
            .collect::<Vec<_>>();
        result.sort_unstable();
        result.dedup();
        result
    }

    pub fn is_empty(&self) -> bool {
        self.signal_changes.is_empty() && self.property_changes.is_empty()
    }
}

/// Mutable values for one execution of a [`ReactiveProgram`].
///
/// Updating an input evaluates only the dependency branch whose upstream value
/// actually changed. If an intermediate derived value recomputes to the same
/// value, propagation stops at that node.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactiveState {
    program: ReactiveProgram,
    values: Vec<ReactiveValue>,
}

impl ReactiveState {
    pub fn value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        let index = self.program.signal_indices.get(&signal)?;
        self.values.get(*index)
    }

    pub fn set_input(
        &mut self,
        signal: SignalId,
        value: impl Into<ReactiveValue>,
    ) -> Result<ReactiveUpdate, ReactiveError> {
        let value = value.into();
        let index = *self
            .program
            .signal_indices
            .get(&signal)
            .ok_or(ReactiveError::UnknownSignal(signal))?;
        if !matches!(self.program.signals[index].source, SignalSource::Input(_)) {
            return Err(ReactiveError::NotInputSignal(signal));
        }
        validate_reactive_value(signal, &value)?;
        let expected = self.values[index].value_kind();
        let actual = value.value_kind();
        if expected != actual {
            return Err(ReactiveError::InputTypeMismatch {
                signal,
                expected,
                actual,
            });
        }
        if self.values[index] == value {
            return Ok(ReactiveUpdate::default());
        }

        self.values[index] = value.clone();
        let mut changed = vec![index];
        let mut pending = BTreeSet::<(usize, usize)>::new();
        for dependent in &self.program.dependents[index] {
            pending.insert((self.program.topological_rank[*dependent], *dependent));
        }

        let mut stats = ReactiveEvaluationStats::default();
        while let Some((rank, current)) = pending.pop_first() {
            debug_assert_eq!(rank, self.program.topological_rank[current]);
            let SignalSource::Derived(expression) = &self.program.signals[current].source else {
                continue;
            };
            stats.derived_signals_evaluated += 1;
            let signal_id = self.program.signals[current].id;
            let next = evaluate_expression(
                expression,
                &self.values,
                &self.program.signal_indices,
                signal_id,
            )?;
            validate_reactive_value(signal_id, &next)?;
            if self.values[current] == next {
                continue;
            }
            self.values[current] = next;
            changed.push(current);
            for dependent in &self.program.dependents[current] {
                pending.insert((self.program.topological_rank[*dependent], *dependent));
            }
        }

        changed.sort_by_key(|index| self.program.topological_rank[*index]);
        let mut signal_changes = Vec::with_capacity(changed.len());
        let mut property_changes = Vec::new();
        for changed_index in changed {
            let changed_signal = self.program.signals[changed_index].id;
            let changed_value = self.values[changed_index].clone();
            signal_changes.push(SignalChange {
                signal: changed_signal,
                value: changed_value.clone(),
            });
            for binding in &self.program.bindings_by_signal[changed_index] {
                property_changes.push(ReactivePropertyChange {
                    object: binding.object,
                    property: binding.property,
                    value: changed_value.clone(),
                });
            }
        }
        stats.bindings_invalidated = property_changes.len();
        Ok(ReactiveUpdate {
            signal_changes,
            property_changes,
            stats,
        })
    }
}

fn evaluate_expression(
    expression: &ReactiveExpr,
    values: &[ReactiveValue],
    indices: &BTreeMap<SignalId, usize>,
    owner: SignalId,
) -> Result<ReactiveValue, ReactiveError> {
    match expression {
        ReactiveExpr::Constant(value) => Ok(value.clone()),
        ReactiveExpr::Signal(signal) => {
            let index = *indices
                .get(signal)
                .ok_or(ReactiveError::UnknownSignal(*signal))?;
            Ok(values[index].clone())
        }
        ReactiveExpr::Add(lhs, rhs) => {
            let lhs = evaluate_expression(lhs, values, indices, owner)?;
            let rhs = evaluate_expression(rhs, values, indices, owner)?;
            match (lhs, rhs) {
                (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs)) => {
                    Ok(ReactiveValue::Scalar(lhs + rhs))
                }
                (ReactiveValue::Vec2(lhs), ReactiveValue::Vec2(rhs)) => {
                    Ok(ReactiveValue::Vec2(lhs + rhs))
                }
                _ => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "add",
                }),
            }
        }
        ReactiveExpr::Sub(lhs, rhs) => {
            let lhs = evaluate_expression(lhs, values, indices, owner)?;
            let rhs = evaluate_expression(rhs, values, indices, owner)?;
            match (lhs, rhs) {
                (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs)) => {
                    Ok(ReactiveValue::Scalar(lhs - rhs))
                }
                (ReactiveValue::Vec2(lhs), ReactiveValue::Vec2(rhs)) => {
                    Ok(ReactiveValue::Vec2(lhs - rhs))
                }
                _ => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "sub",
                }),
            }
        }
        ReactiveExpr::Mul(lhs, rhs) => {
            let lhs = evaluate_expression(lhs, values, indices, owner)?;
            let rhs = evaluate_expression(rhs, values, indices, owner)?;
            match (lhs, rhs) {
                (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs)) => {
                    Ok(ReactiveValue::Scalar(lhs * rhs))
                }
                (ReactiveValue::Scalar(lhs), ReactiveValue::Vec2(rhs)) => {
                    Ok(ReactiveValue::Vec2(lhs * rhs))
                }
                (ReactiveValue::Vec2(lhs), ReactiveValue::Scalar(rhs)) => {
                    Ok(ReactiveValue::Vec2(lhs * rhs))
                }
                _ => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "mul",
                }),
            }
        }
        ReactiveExpr::Neg(value) => {
            let value = evaluate_expression(value, values, indices, owner)?;
            match value {
                ReactiveValue::Scalar(value) => Ok(ReactiveValue::Scalar(-value)),
                ReactiveValue::Vec2(value) => Ok(ReactiveValue::Vec2(-value)),
                ReactiveValue::Bool(_) => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "neg",
                }),
            }
        }
        ReactiveExpr::Sin(value) => {
            let value = evaluate_expression(value, values, indices, owner)?;
            match value {
                ReactiveValue::Scalar(value) => Ok(ReactiveValue::Scalar(value.sin())),
                _ => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "sin",
                }),
            }
        }
        ReactiveExpr::Cos(value) => {
            let value = evaluate_expression(value, values, indices, owner)?;
            match value {
                ReactiveValue::Scalar(value) => Ok(ReactiveValue::Scalar(value.cos())),
                _ => Err(ReactiveError::InvalidExpression {
                    signal: owner,
                    operation: "cos",
                }),
            }
        }
    }
}

pub(super) fn validate_reactive_value(
    signal: SignalId,
    value: &ReactiveValue,
) -> Result<(), ReactiveError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ReactiveError::NonFiniteValue(signal))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactiveError {
    DuplicateSignal(SignalId),
    UnknownSignal(SignalId),
    UnknownObject(ObjectId),
    SignalIdExhausted,
    DependencyCycle,
    NonFiniteValue(SignalId),
    NotInputSignal(SignalId),
    InvalidExpression {
        signal: SignalId,
        operation: &'static str,
    },
    InputTypeMismatch {
        signal: SignalId,
        expected: ValueKind,
        actual: ValueKind,
    },
    BindingTypeMismatch {
        signal: SignalId,
        property: Property,
        expected: ValueKind,
        actual: ValueKind,
    },
    DuplicateBinding {
        object: ObjectId,
        property: Property,
    },
    ConflictingDriver {
        object: ObjectId,
        property: Property,
    },
}

impl std::fmt::Display for ReactiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateSignal(signal) => {
                write!(formatter, "duplicate signal id {}", signal.get())
            }
            Self::UnknownSignal(signal) => write!(formatter, "unknown signal {}", signal.get()),
            Self::UnknownObject(object) => write!(formatter, "unknown object {}", object.get()),
            Self::SignalIdExhausted => formatter.write_str("Noon signal ID space exhausted"),
            Self::DependencyCycle => {
                formatter.write_str("reactive dependency graph contains a cycle")
            }
            Self::NonFiniteValue(signal) => write!(
                formatter,
                "signal {} evaluated to a non-finite value",
                signal.get()
            ),
            Self::NotInputSignal(signal) => write!(
                formatter,
                "signal {} is derived and cannot be set directly",
                signal.get()
            ),
            Self::InvalidExpression { signal, operation } => write!(
                formatter,
                "signal {} uses invalid operand types for {operation}",
                signal.get()
            ),
            Self::InputTypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "signal {} input type mismatch: expected {expected:?}, got {actual:?}",
                signal.get()
            ),
            Self::BindingTypeMismatch {
                signal,
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "signal {} cannot drive {property:?}: expected {expected:?}, got {actual:?}",
                signal.get()
            ),
            Self::DuplicateBinding { object, property } => write!(
                formatter,
                "object {} property {property:?} has multiple reactive drivers",
                object.get()
            ),
            Self::ConflictingDriver { object, property } => write!(
                formatter,
                "object {} property {property:?} is driven by both timeline and reactive state",
                object.get()
            ),
        }
    }
}

impl std::error::Error for ReactiveError {}

#[cfg(test)]
mod tests {
    use crate::{RateFunction, TrackTiming};

    use super::*;

    #[test]
    fn reactive_update_evaluates_only_affected_dependency_branch() {
        let mut scene = SemanticScene::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::circle(1.0));
        let a = scene.add_input(1.0_f32);
        let b = scene.add_input(10.0_f32);
        let twice_a = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(a)),
            Box::new(ReactiveExpr::scalar(2.0)),
        ));
        let a_branch = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(twice_a)),
            Box::new(ReactiveExpr::scalar(3.0)),
        ));
        let b_branch = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(b)),
            Box::new(ReactiveExpr::scalar(1.0)),
        ));
        scene.bind(a_branch, first, Property::Rotation);
        scene.bind(b_branch, second, Property::Opacity);

        let program = scene.compile_reactive().expect("graph must compile");
        let mut state = program.instantiate();
        let update = state.set_input(a, 2.0_f32).expect("input update must work");

        assert_eq!(update.stats().derived_signals_evaluated, 2);
        assert_eq!(update.stats().bindings_invalidated, 1);
        assert_eq!(update.affected_objects(), vec![first]);
        assert_eq!(
            update.property_changes(),
            &[ReactivePropertyChange {
                object: first,
                property: Property::Rotation,
                value: ReactiveValue::Scalar(7.0),
            }]
        );
        assert_eq!(state.value(b_branch), Some(&ReactiveValue::Scalar(11.0)));
    }

    #[test]
    fn unchanged_derived_value_stops_dirty_propagation() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(1.0_f32);
        let always_zero = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(input)),
            Box::new(ReactiveExpr::scalar(0.0)),
        ));
        let downstream = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(always_zero)),
            Box::new(ReactiveExpr::scalar(1.0)),
        ));
        scene.bind(downstream, object, Property::Opacity);

        let mut state = scene
            .compile_reactive()
            .expect("graph must compile")
            .instantiate();
        let update = state
            .set_input(input, 42.0_f32)
            .expect("input update must work");

        assert_eq!(update.stats().derived_signals_evaluated, 1);
        assert_eq!(update.stats().bindings_invalidated, 0);
        assert!(update.property_changes().is_empty());
        assert_eq!(state.value(downstream), Some(&ReactiveValue::Scalar(1.0)));
    }

    #[test]
    fn execution_analysis_is_local_to_dynamic_objects() {
        let mut scene = SemanticScene::new();
        let static_object = scene.add(GeometryRef::circle(1.0));
        let timeline_object = scene.add(GeometryRef::circle(1.0));
        let reactive_object = scene.add(GeometryRef::circle(1.0));
        let mixed_object = scene.add(GeometryRef::circle(1.0));

        scene
            .definition_mut()
            .animate_scalar(
                timeline_object,
                Property::Rotation,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .expect("timeline track must be valid");
        scene
            .definition_mut()
            .animate_position(
                mixed_object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .expect("timeline track must be valid");

        let reactive = scene.add_input(0.5_f32);
        let mixed = scene.add_input(0.25_f32);
        scene.bind(reactive, reactive_object, Property::Opacity);
        scene.bind(mixed, mixed_object, Property::Rotation);

        let program = scene.compile_reactive().expect("graph must compile");
        let analysis = program.analysis();
        assert_eq!(analysis.static_objects, 1);
        assert_eq!(analysis.timeline_only_objects, 1);
        assert_eq!(analysis.reactive_only_objects, 1);
        assert_eq!(analysis.timeline_and_reactive_objects, 1);
        assert_eq!(
            analysis.class_for(static_object),
            Some(ObjectExecutionClass::Static)
        );
        assert_eq!(
            analysis.class_for(timeline_object),
            Some(ObjectExecutionClass::Timeline)
        );
        assert_eq!(
            analysis.class_for(reactive_object),
            Some(ObjectExecutionClass::Reactive)
        );
        assert_eq!(
            analysis.class_for(mixed_object),
            Some(ObjectExecutionClass::TimelineAndReactive)
        );
    }

    #[test]
    fn same_property_cannot_have_timeline_and_reactive_drivers() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .definition_mut()
            .animate_scalar(
                object,
                Property::Opacity,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .expect("timeline track must be valid");
        let signal = scene.add_input(0.5_f32);
        scene.bind(signal, object, Property::Opacity);

        assert!(matches!(
            scene.compile_reactive(),
            Err(ReactiveError::ConflictingDriver {
                object: conflict_object,
                property: Property::Opacity,
            }) if conflict_object == object
        ));
    }

    #[test]
    fn reactive_cycles_are_rejected_deterministically() {
        let first = SignalId::new(0);
        let second = SignalId::new(1);
        let graph = ReactiveGraphDefinition::from_parts(
            vec![
                SignalDefinition {
                    id: first,
                    source: SignalSource::Derived(ReactiveExpr::signal(second)),
                },
                SignalDefinition {
                    id: second,
                    source: SignalSource::Derived(ReactiveExpr::signal(first)),
                },
            ],
            Vec::new(),
        )
        .expect("transported IDs are unique");

        assert!(matches!(
            ReactiveProgram::compile(&SceneDefinition::new(), &graph),
            Err(ReactiveError::DependencyCycle)
        ));
    }

    #[test]
    fn binding_types_are_checked_before_execution() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let scalar = scene.add_input(1.0_f32);
        scene.bind(scalar, object, Property::Position);

        assert!(matches!(
            scene.compile_reactive(),
            Err(ReactiveError::BindingTypeMismatch {
                signal,
                property: Property::Position,
                expected: ValueKind::Vec2,
                actual: ValueKind::Scalar,
            }) if signal == scalar
        ));
    }
}
